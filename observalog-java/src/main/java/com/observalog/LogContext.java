package com.observalog;

import java.util.concurrent.atomic.AtomicInteger;

// Request-scoped context stored in ThreadLocal.
// Java equivalent of Go's context.Context carrying observalog keys.
// Set by HttpFilter or KafkaConsumerMiddleware; cleared in finally block.
public final class LogContext {

    private String traceId      = "";
    private String spanId       = "";
    private String parentSpan   = "";
    private String userId       = "";
    private String journeyStage = "";
    // Per-request atomic counter — Decision 1: never global, never shared across requests.
    private final AtomicInteger seq = new AtomicInteger(0);

    private static final ThreadLocal<LogContext> CURRENT = ThreadLocal.withInitial(LogContext::new);

    public static LogContext get()             { return CURRENT.get(); }
    public static void set(LogContext ctx)     { CURRENT.set(ctx); }
    public static void clear()                 { CURRENT.remove(); }

    public String getTraceId()                 { return traceId; }
    public void setTraceId(String v)           { traceId = v; }

    public String getSpanId()                  { return spanId; }
    public void setSpanId(String v)            { spanId = v; }

    public String getParentSpan()              { return parentSpan; }
    public void setParentSpan(String v)        { parentSpan = v; }

    public String getUserId()                  { return userId; }
    public void setUserId(String v)            { userId = v; }

    public String getJourneyStage()            { return journeyStage; }
    public void setJourneyStage(String v)      { journeyStage = v; }

    public int nextSeq()                       { return seq.incrementAndGet(); }
    public void resetSeq()                     { seq.set(0); }
}
