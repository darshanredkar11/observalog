package com.observalog;

import java.io.IOException;
import java.time.Instant;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

public final class ObservaLog {

    private static volatile Level minLevel   = Level.INFO;
    private static volatile byte  serviceCode = WireContract.SERVICE_SYSTEM;

    private ObservaLog() {}

    public static void init(ObservaLogConfig cfg) {
        if (cfg.serviceName.isEmpty()) throw new IllegalStateException("SERVICE_NAME required");
        if (cfg.env.isEmpty())         throw new IllegalStateException("ENV required");

        minLevel = switch (cfg.level) {
            case "debug" -> Level.DEBUG;
            case "info"  -> Level.INFO;
            case "warn"  -> Level.WARN;
            case "error" -> Level.ERROR;
            default      -> throw new IllegalStateException("Invalid LOG_LEVEL=" + cfg.level);
        };

        serviceCode = switch (cfg.serviceName) {
            case "auth"     -> WireContract.SERVICE_AUTH;
            case "doc"      -> WireContract.SERVICE_DOC;
            case "provider" -> WireContract.SERVICE_PROVIDER;
            default         -> WireContract.SERVICE_SYSTEM;
        };

        AsyncDrain.start(System.out, cfg.bufferSize);
    }

    public static void shutdown() {
        AsyncDrain.shutdown();
    }

    public static long droppedLogCount() {
        return AsyncDrain.droppedCount();
    }

    // ── Emit functions ────────────────────────────────────────────────────────

    public static void debug(String event, String message, Map<String, Object> fields) {
        emitLine(Level.DEBUG, event, message, fields);
    }

    public static void info(String event, String message, Map<String, Object> fields) {
        emitLine(Level.INFO, event, message, fields);
    }

    public static void warn(String event, String message, Map<String, Object> fields) {
        emitLine(Level.WARN, event, message, fields);
    }

    public static void error(String event, String message, Map<String, Object> fields) {
        emitLine(Level.ERROR, event, message, fields);
    }

    // Convenience overload for no-field emits
    public static void info(String event, String message) {
        emitLine(Level.INFO, event, message, Collections.emptyMap());
    }

    // ── Core ──────────────────────────────────────────────────────────────────

    private static void emitLine(Level level, String event, String message, Map<String, Object> fields) {
        if (level.code < minLevel.code) return;

        LogContext ctx = LogContext.get();
        Degrade.applyGracefulDegradation(ctx);

        // Extract structural fields from developer map
        Outcome outcome    = null;
        Long    durationMs = null;
        Err     errField   = null;
        Map<String, Object> devCtx = new LinkedHashMap<>();

        for (Map.Entry<String, Object> entry : fields.entrySet()) {
            switch (entry.getKey()) {
                case "outcome"     -> { if (entry.getValue() instanceof Outcome o) outcome = o; }
                case "duration_ms" -> { if (entry.getValue() instanceof Long l)    durationMs = l;
                                        else if (entry.getValue() instanceof Number n) durationMs = n.longValue(); }
                case "error"       -> { if (entry.getValue() instanceof Err e)     errField = e; }
                default            -> devCtx.put(entry.getKey(), entry.getValue());
            }
        }

        // Fingerprint (computed but not emitted — brain computes it on ingest)
        if (errField != null) {
            String ctxKey = devCtx.containsKey("ctx_primary_key")
                ? String.valueOf(devCtx.get("ctx_primary_key")) : "";
            Fingerprint.compute(serviceCode, event, errField.code, ctxKey); // side-effect free
        }

        LogEntry logEntry = new LogEntry();
        logEntry.traceId      = ctx.getTraceId();
        logEntry.spanId       = ctx.getSpanId();
        logEntry.parentSpan   = ctx.getParentSpan();
        logEntry.seq          = ctx.nextSeq();
        logEntry.serviceCode  = serviceCode;
        logEntry.levelCode    = level.code;
        logEntry.outcomeCode  = outcome != null ? outcome.code : 0;
        logEntry.tsMs         = Instant.now().toEpochMilli();
        logEntry.event        = event;
        logEntry.message      = message;
        logEntry.durationMs   = durationMs;
        logEntry.outcome      = outcome;
        logEntry.error        = errField;
        logEntry.userId       = ctx.getUserId();
        logEntry.journeyStage = ctx.getJourneyStage();
        logEntry.ctx          = devCtx;

        try {
            AsyncDrain.send(Wire.encodePartA(logEntry));
            AsyncDrain.send(Wire.encodePartB(logEntry));
        } catch (IOException ignored) {}
    }
}
