// Wire format contract — READ ONLY. Constants only.
// Any change here MUST be mirrored in observalog-brain/src/ingest/wire_contract.rs
// and observalog-go/wire_contract.go.
export const PART_A_SCHEMA_VERSION = 1;
export const PART_A_BYTE_LEN       = 55;

export const TRACE_ID_OFFSET     = 2;
export const SPAN_ID_OFFSET      = 17;
export const PARENT_SPAN_OFFSET  = 25;
export const SEQ_OFFSET          = 33;
export const SERVICE_CODE_OFFSET = 36;
export const LEVEL_CODE_OFFSET   = 38;
export const OUTCOME_CODE_OFFSET = 40;
export const TS_MS_OFFSET        = 42;

export const SERVICE_SYSTEM   = 0;
export const SERVICE_AUTH     = 1;
export const SERVICE_DOC      = 2;
export const SERVICE_PROVIDER = 3;

export const PARENT_SPAN_ABSENT = '-------';
