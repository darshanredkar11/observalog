package com.observalog;

public final class Err {
    public final String kind;
    public final String code;
    public final String message;
    public final boolean retryable;

    public Err(String kind, String code, String message, boolean retryable) {
        this.kind = kind;
        this.code = code;
        this.message = message;
        this.retryable = retryable;
    }
}
