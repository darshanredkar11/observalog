package com.observalog;

import org.junit.jupiter.api.Test;
import java.io.IOException;
import java.util.Collections;

import static org.junit.jupiter.api.Assertions.*;

class WireTest {

    private LogEntry minimalEntry() {
        LogEntry e = new LogEntry();
        e.traceId      = "trc_7f2a1b9e4d";
        e.spanId       = "spn_004";
        e.parentSpan   = "spn_001";
        e.seq          = 4;
        e.serviceCode  = WireContract.SERVICE_DOC;
        e.levelCode    = 1; // INFO
        e.outcomeCode  = 1; // success
        e.tsMs         = 1748268153812L;
        e.event        = "doc.storage.saved";
        e.message      = "Document written to storage";
        e.userId       = "";
        e.journeyStage = "";
        e.ctx          = Collections.emptyMap();
        return e;
    }

    @Test
    void partA_prefix() {
        byte[] partA = Wire.encodePartA(minimalEntry());
        String line = new String(partA);
        assertTrue(line.startsWith("A:1|"), "Part A must start with A:1|");
    }

    @Test
    void partA_traceId_at_offset_2() {
        byte[] partA = Wire.encodePartA(minimalEntry());
        // Skip "A:" prefix (2 bytes)
        String payload = new String(partA).substring(2);
        String traceId = payload.substring(
            WireContract.TRACE_ID_OFFSET - 2,
            WireContract.SPAN_ID_OFFSET - 2 - 1  // -1 for separator
        ).stripTrailing();
        assertEquals("trc_7f2a1b9e4d", traceId);
    }

    @Test
    void partA_seq_hex_encoded() {
        LogEntry e = minimalEntry();
        e.seq = 255;
        String line = new String(Wire.encodePartA(e));
        // seq field is at positions 33-34 after the "A:" prefix
        String seqField = line.substring(2 + WireContract.SEQ_OFFSET, 2 + WireContract.SEQ_OFFSET + 2);
        assertEquals("ff", seqField);
    }

    @Test
    void partA_absent_parent_span_uses_dashes() {
        LogEntry e = minimalEntry();
        e.parentSpan = "";
        String line = new String(Wire.encodePartA(e));
        // parent span at offsets 25-31 after "A:" prefix
        String parentField = line.substring(2 + WireContract.PARENT_SPAN_OFFSET,
                                             2 + WireContract.PARENT_SPAN_OFFSET + 7);
        assertEquals("-------", parentField);
    }

    @Test
    void partA_length_is_55_plus_newline() {
        byte[] partA = Wire.encodePartA(minimalEntry());
        // Content is 55 chars + "A:" prefix (2) + newline (1) = 58
        assertEquals(58, partA.length);
    }

    @Test
    void partB_has_abbreviated_event_key() throws IOException {
        LogEntry e = minimalEntry();
        String partB = new String(Wire.encodePartB(e));
        assertTrue(partB.contains("\"e\":"), "event must be abbreviated to 'e'");
        assertFalse(partB.contains("\"event\":"), "full key 'event' must not appear");
    }

    @Test
    void partB_error_struct_uses_abbreviated_keys() throws IOException {
        LogEntry e = minimalEntry();
        e.error = new Err("RateLimitExceeded", "QUOTA", "limit hit", true);
        String partB = new String(Wire.encodePartB(e));
        assertTrue(partB.contains("\"ek\""), "error.kind → ek");
        assertTrue(partB.contains("\"ec\""), "error.code → ec");
        assertTrue(partB.contains("\"rt\""), "error.retryable → rt");
    }
}
