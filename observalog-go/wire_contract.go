package log

// Wire format contract — READ ONLY. No functions. Constants only.
// Any change here MUST be mirrored in observalog-brain/src/ingest/wire_contract.rs.
//
// Part A layout (byte positions 0-indexed after the "A:" prefix):
//   [0]     schema_version
//   [1]     |
//   [2-15]  trace_id (14 chars)
//   [16]    |
//   [17-23] span_id (7 chars)
//   [24]    |
//   [25-31] parent_span (7 chars, or "-------" if absent)
//   [32]    |
//   [33-34] seq (2 chars, zero-padded uint8)
//   [35]    |
//   [36]    svc (1 char)
//   [37]    |
//   [38]    lvl (1 char)
//   [39]    |
//   [40]    out (1 char)
//   [41]    |
//   [42-54] ts_ms (13 chars, unix milliseconds)

const PartASchemaVersion = 1
const PartAByteLen = 55

// Byte offset constants for Rust brain fixed-offset reads.
const (
	SchemaVersionOffset = 0
	TraceIDOffset       = 2
	SpanIDOffset        = 17
	ParentSpanOffset    = 25
	SeqOffset           = 33
	ServiceCodeOffset   = 36
	LevelCodeOffset     = 38
	OutcomeCodeOffset   = 40
	TsMsOffset          = 42
)

// Service code constants (1 char on wire).
const (
	ServiceSystem   uint8 = 0
	ServiceAuth     uint8 = 1
	ServiceDoc      uint8 = 2
	ServiceProvider uint8 = 3
)

// Level code constants — must match Level iota order in fields.go.
const (
	LevelCodeDebug uint8 = 0
	LevelCodeInfo  uint8 = 1
	LevelCodeWarn  uint8 = 2
	LevelCodeError uint8 = 3
)

// Outcome code constants.
const (
	OutcomeCodeNone    uint8 = 0
	OutcomeCodeSuccess uint8 = 1
	OutcomeCodeFailure uint8 = 2
	OutcomeCodePartial uint8 = 3
	OutcomeCodePending uint8 = 4
)
