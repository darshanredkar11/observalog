package com.observalog;

public enum Outcome {
    SUCCESS("success", 1),
    FAILURE("failure", 2),
    PARTIAL("partial", 3),
    PENDING("pending", 4);

    final String wire;
    final int code;

    Outcome(String wire, int code) {
        this.wire = wire;
        this.code = code;
    }

    @Override
    public String toString() {
        return wire;
    }
}
