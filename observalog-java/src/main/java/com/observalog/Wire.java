package com.observalog;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;

public final class Wire {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    private Wire() {}

    // EncodePartA produces the 55-byte fixed-position header line.
    // Matches Go: fmt.Sprintf("A:%d|%-14s|%-7s|%-7s|%02x|%d|%d|%d|%013d\n", ...)
    public static byte[] encodePartA(LogEntry entry) {
        String parentSpan = entry.parentSpan.isEmpty() ? WireContract.PARENT_SPAN_ABSENT : entry.parentSpan;
        String line = String.format(Locale.US,
            "A:%d|%-14s|%-7s|%-7s|%02x|%d|%d|%d|%013d\n",
            WireContract.PART_A_SCHEMA_VERSION,
            truncate(entry.traceId, 14),
            truncate(entry.spanId, 7),
            truncate(parentSpan, 7),
            entry.seq & 0xFF,
            entry.serviceCode,
            entry.levelCode,
            entry.outcomeCode,
            entry.tsMs
        );
        return line.getBytes(java.nio.charset.StandardCharsets.UTF_8);
    }

    // EncodePartB produces the abbreviated JSON payload line.
    public static byte[] encodePartB(LogEntry entry) throws IOException {
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("e", entry.event);
        out.put("m", entry.message);
        if (entry.durationMs != null)          out.put("ms", entry.durationMs);
        if (entry.outcome != null)             out.put("o", entry.outcome.wire);

        if (entry.error != null) {
            Map<String, Object> err = new LinkedHashMap<>();
            err.put("ek", entry.error.kind);
            err.put("ec", entry.error.code);
            err.put("em", entry.error.message);
            err.put("rt", entry.error.retryable);
            out.put("er", err);
        }

        if (!entry.ctx.isEmpty()) {
            Map<String, Object> ctx = new LinkedHashMap<>();
            for (Map.Entry<String, Object> e : entry.ctx.entrySet()) {
                String abbrev = Dict.KEY_TO_ABBREV.get(e.getKey());
                ctx.put(abbrev != null ? abbrev : e.getKey(), e.getValue());
            }
            out.put("c", ctx);
        }

        if (!entry.userId.isEmpty())       out.put("ui", entry.userId);
        if (!entry.journeyStage.isEmpty()) out.put("js", entry.journeyStage);

        byte[] json = MAPPER.writeValueAsBytes(out);
        byte[] line = new byte[json.length + 1];
        System.arraycopy(json, 0, line, 0, json.length);
        line[json.length] = '\n';
        return line;
    }

    private static String truncate(String s, int n) {
        return s.length() > n ? s.substring(0, n) : s;
    }
}
