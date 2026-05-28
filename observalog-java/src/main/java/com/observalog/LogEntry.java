package com.observalog;

import java.util.Map;

// All fields needed to produce one two-line wire emission.
public final class LogEntry {
    // Part A — fixed-position header
    public String traceId;
    public String spanId;
    public String parentSpan;
    public int    seq;
    public byte   serviceCode;
    public int    levelCode;
    public int    outcomeCode;
    public long   tsMs;

    // Part B — structural
    public String  event;
    public String  message;
    public Long    durationMs;
    public Outcome outcome;
    public Err     error;

    // Part B — ambient
    public String userId;
    public String journeyStage;

    // Part B — developer-supplied context (minus extracted structural keys)
    public Map<String, Object> ctx;
}
