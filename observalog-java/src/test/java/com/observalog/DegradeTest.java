package com.observalog;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class DegradeTest {

    @Test
    void missing_trace_id_gets_sys_prefix() {
        LogContext ctx = new LogContext();
        Degrade.applyGracefulDegradation(ctx);
        assertTrue(ctx.getTraceId().startsWith("sys_"),
            "Missing trace_id must get sys_ prefix");
        assertEquals(14, ctx.getTraceId().length(),
            "sys_ trace_id must be 14 chars to fit CHAR(14)");
    }

    @Test
    void missing_user_id_becomes_system() {
        LogContext ctx = new LogContext();
        Degrade.applyGracefulDegradation(ctx);
        assertEquals("system", ctx.getUserId());
    }

    @Test
    void missing_journey_stage_gets_background_default() {
        LogContext ctx = new LogContext();
        Degrade.applyGracefulDegradation(ctx);
        assertEquals("system.background.untraced", ctx.getJourneyStage());
    }

    @Test
    void existing_values_are_not_overwritten() {
        LogContext ctx = new LogContext();
        ctx.setTraceId("trc_existing123");
        ctx.setUserId("user-999");
        ctx.setJourneyStage("http.doc.upload");
        Degrade.applyGracefulDegradation(ctx);
        assertEquals("trc_existing123", ctx.getTraceId());
        assertEquals("user-999", ctx.getUserId());
        assertEquals("http.doc.upload", ctx.getJourneyStage());
    }
}
