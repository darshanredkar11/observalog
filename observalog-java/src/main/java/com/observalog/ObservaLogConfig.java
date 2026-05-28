package com.observalog;

public final class ObservaLogConfig {
    public final String serviceName;
    public final String version;
    public final String env;
    public final String level;    // "debug" | "info" | "warn" | "error"
    public final int    bufferSize;

    private ObservaLogConfig(Builder b) {
        this.serviceName = b.serviceName;
        this.version     = b.version;
        this.env         = b.env;
        this.level       = b.level;
        this.bufferSize  = b.bufferSize;
    }

    // Reads SERVICE_NAME, ENV, LOG_LEVEL, LOG_BUFFER_SIZE from environment.
    // version is passed as a parameter (typically from a build constant or manifest).
    public static ObservaLogConfig fromEnv(String version) {
        String level = System.getenv("LOG_LEVEL");
        if (level == null || level.isEmpty()) level = "info";

        int bufferSize = 10_000;
        String buf = System.getenv("LOG_BUFFER_SIZE");
        if (buf != null && !buf.isEmpty()) {
            try { bufferSize = Integer.parseInt(buf.trim()); } catch (NumberFormatException ignored) {}
        }

        return new Builder()
            .serviceName(getEnv("SERVICE_NAME", ""))
            .version(version)
            .env(getEnv("ENV", ""))
            .level(level.toLowerCase())
            .bufferSize(bufferSize)
            .build();
    }

    private static String getEnv(String key, String defaultValue) {
        String v = System.getenv(key);
        return (v != null && !v.isEmpty()) ? v : defaultValue;
    }

    public static final class Builder {
        private String serviceName = "";
        private String version     = "";
        private String env         = "";
        private String level       = "info";
        private int    bufferSize  = 10_000;

        public Builder serviceName(String v) { this.serviceName = v; return this; }
        public Builder version(String v)     { this.version = v;     return this; }
        public Builder env(String v)         { this.env = v;         return this; }
        public Builder level(String v)       { this.level = v;       return this; }
        public Builder bufferSize(int v)     { this.bufferSize = v;  return this; }
        public ObservaLogConfig build()      { return new ObservaLogConfig(this); }
    }
}
