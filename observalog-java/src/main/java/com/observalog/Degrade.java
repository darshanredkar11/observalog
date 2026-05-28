package com.observalog;

import java.util.UUID;

// Decision 5: never panic on missing context. Background jobs are legitimate emitters.
// The sys_ prefix signals the brain without requiring special handling in the emitting code.
public final class Degrade {
    private Degrade() {}

    public static void applyGracefulDegradation(LogContext ctx) {
        if (ctx.getTraceId().isEmpty()) {
            // sys_ + 10 hex chars = 14 chars total, fits CHAR(14).
            String uid = UUID.randomUUID().toString().replace("-", "");
            ctx.setTraceId("sys_" + uid.substring(0, 10));
        }
        if (ctx.getUserId().isEmpty()) {
            ctx.setUserId("system");
        }
        if (ctx.getJourneyStage().isEmpty()) {
            ctx.setJourneyStage("system.background.untraced");
        }
    }
}
