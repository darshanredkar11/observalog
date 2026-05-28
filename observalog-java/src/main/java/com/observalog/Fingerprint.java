package com.observalog;

import net.openhft.hashing.LongHashFunction;
import java.nio.charset.StandardCharsets;

public final class Fingerprint {
    private static final LongHashFunction XX64 = LongHashFunction.xx(0);

    private Fingerprint() {}

    // Compute the xxHash64 fingerprint matching Go's ComputeFingerprint and brain's compute_fingerprint.
    // Input bytes: serviceCode (raw byte) | event | errorCode | ctxPrimaryKey joined with '|'.
    // serviceCode is a raw byte — NOT the ASCII digit. Matches Go: string(uint8(serviceCode)).
    public static long compute(byte serviceCode, String event, String errorCode, String ctxPrimaryKey) {
        byte[] eventB    = event.getBytes(StandardCharsets.UTF_8);
        byte[] errCodeB  = errorCode.getBytes(StandardCharsets.UTF_8);
        byte[] ctxKeyB   = ctxPrimaryKey.getBytes(StandardCharsets.UTF_8);

        int len = 1 + 1 + eventB.length + 1 + errCodeB.length + 1 + ctxKeyB.length;
        byte[] data = new byte[len];
        int pos = 0;
        data[pos++] = serviceCode;
        data[pos++] = '|';
        System.arraycopy(eventB,   0, data, pos, eventB.length);   pos += eventB.length;
        data[pos++] = '|';
        System.arraycopy(errCodeB, 0, data, pos, errCodeB.length); pos += errCodeB.length;
        data[pos++] = '|';
        System.arraycopy(ctxKeyB,  0, data, pos, ctxKeyB.length);

        return XX64.hashBytes(data);
    }
}
