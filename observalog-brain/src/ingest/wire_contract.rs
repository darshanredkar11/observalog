/// Wire format contract — READ ONLY. Constants only. No functions.
/// MUST stay in sync with observalog-go/wire_contract.go.
/// Any change here requires a matching change in the Go file.
///
/// Part A layout (0-indexed after the "A:" prefix, total PART_A_BYTE_LEN bytes):
///   [0]     schema_version (1 char digit)
///   [1]     |
///   [2-15]  trace_id (14 chars)
///   [16]    |
///   [17-23] span_id (7 chars)
///   [24]    |
///   [25-31] parent_span (7 chars, "-------" when absent)
///   [32]    |
///   [33-34] seq (2 hex chars, 0-255 as 00–ff)
///   [35]    |
///   [36]    svc (1 char: 0=system 1=auth 2=doc 3=provider)
///   [37]    |
///   [38]    lvl (1 char: 0=debug 1=info 2=warn 3=error)
///   [39]    |
///   [40]    out (1 char: 0=none 1=success 2=failure 3=partial 4=pending)
///   [41]    |
///   [42-54] ts_ms (13 chars, unix milliseconds)
pub const PART_A_SCHEMA_VERSION: u8 = 1;
pub const PART_A_BYTE_LEN: usize = 55;

// Byte offsets (0-indexed after "A:").
pub const SCHEMA_VERSION_OFFSET: usize = 0;
pub const TRACE_ID_OFFSET: usize = 2;
pub const TRACE_ID_LEN: usize = 14;
pub const SPAN_ID_OFFSET: usize = 17;
pub const SPAN_ID_LEN: usize = 7;
pub const PARENT_SPAN_OFFSET: usize = 25;
pub const PARENT_SPAN_LEN: usize = 7;
pub const SEQ_OFFSET: usize = 33;
pub const SEQ_LEN: usize = 2;
pub const SERVICE_CODE_OFFSET: usize = 36;
pub const LEVEL_CODE_OFFSET: usize = 38;
pub const OUTCOME_CODE_OFFSET: usize = 40;
pub const TS_MS_OFFSET: usize = 42;
pub const TS_MS_LEN: usize = 13;

pub const PARENT_SPAN_ABSENT: &str = "-------";

// Service codes.
pub const SERVICE_SYSTEM: u8 = 0;
pub const SERVICE_AUTH: u8 = 1;
pub const SERVICE_DOC: u8 = 2;
pub const SERVICE_PROVIDER: u8 = 3;

// Level codes.
pub const LEVEL_DEBUG: u8 = 0;
pub const LEVEL_INFO: u8 = 1;
pub const LEVEL_WARN: u8 = 2;
pub const LEVEL_ERROR: u8 = 3;

// Outcome codes.
pub const OUTCOME_NONE: u8 = 0;
pub const OUTCOME_SUCCESS: u8 = 1;
pub const OUTCOME_FAILURE: u8 = 2;
pub const OUTCOME_PARTIAL: u8 = 3;
pub const OUTCOME_PENDING: u8 = 4;
