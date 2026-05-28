package com.observalog;

public enum Level {
    DEBUG(0), INFO(1), WARN(2), ERROR(3);

    final int code;

    Level(int code) {
        this.code = code;
    }
}
