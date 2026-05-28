package com.observalog;

// Wire format contract — READ ONLY. Constants only.
// Any change here MUST be mirrored in observalog-brain/src/ingest/wire_contract.rs
// and observalog-go/wire_contract.go.
//
// Part A layout (byte positions 0-indexed after the "A:" prefix):
//   [0]     schema_version
//   [1]     |
//   [2-15]  trace_id  (14 chars)
//   [16]    |
//   [17-23] span_id   (7 chars)
//   [24]    |
//   [25-31] parent_span (7 chars, or "-------" if absent)
//   [32]    |
//   [33-34] seq (2 chars, zero-padded hex uint8)
//   [35]    |
//   [36]    svc (1 char)
//   [37]    |
//   [38]    lvl (1 char)
//   [39]    |
//   [40]    out (1 char)
//   [41]    |
//   [42-54] ts_ms (13 chars, unix milliseconds)
public final class WireContract {
    private WireContract() {}

    public static final int PART_A_SCHEMA_VERSION = 1;
    public static final int PART_A_BYTE_LEN = 55;

    public static final int TRACE_ID_OFFSET    = 2;
    public static final int SPAN_ID_OFFSET     = 17;
    public static final int PARENT_SPAN_OFFSET = 25;
    public static final int SEQ_OFFSET         = 33;
    public static final int SERVICE_CODE_OFFSET = 36;
    public static final int LEVEL_CODE_OFFSET  = 38;
    public static final int OUTCOME_CODE_OFFSET = 40;
    public static final int TS_MS_OFFSET       = 42;

    // Service codes
    public static final byte SERVICE_SYSTEM   = 0;
    public static final byte SERVICE_AUTH     = 1;
    public static final byte SERVICE_DOC      = 2;
    public static final byte SERVICE_PROVIDER = 3;

    public static final String PARENT_SPAN_ABSENT = "-------";
}
