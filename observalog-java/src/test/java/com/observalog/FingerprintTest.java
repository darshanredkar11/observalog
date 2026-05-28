package com.observalog;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class FingerprintTest {

    @Test
    void same_inputs_produce_same_hash() {
        long a = Fingerprint.compute((byte) 2, "doc.storage.saved", "IO_ERROR", "doc123");
        long b = Fingerprint.compute((byte) 2, "doc.storage.saved", "IO_ERROR", "doc123");
        assertEquals(a, b);
    }

    @Test
    void different_ctx_primary_key_prevents_collision() {
        long a = Fingerprint.compute((byte) 2, "doc.storage.saved", "IO_ERROR", "doc123");
        long b = Fingerprint.compute((byte) 2, "doc.storage.saved", "IO_ERROR", "doc456");
        assertNotEquals(a, b, "ctx_primary_key must disambiguate fingerprints");
    }

    @Test
    void different_service_codes_differ() {
        long a = Fingerprint.compute((byte) 1, "auth.jwt.expired", "TOKEN_EXPIRED", "");
        long b = Fingerprint.compute((byte) 2, "auth.jwt.expired", "TOKEN_EXPIRED", "");
        assertNotEquals(a, b);
    }

    @Test
    void service_code_is_raw_byte_not_ascii_digit() {
        // Service code 1 as raw byte (0x01) differs from ASCII '1' (0x31)
        long rawByte   = Fingerprint.compute((byte) 1,  "e", "c", "");
        long asciiChar = Fingerprint.compute((byte) '1', "e", "c", "");
        assertNotEquals(rawByte, asciiChar,
            "service code must be raw byte matching Go's string(uint8(code))");
    }
}
