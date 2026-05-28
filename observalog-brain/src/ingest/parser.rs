use super::wire_contract::*;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use xxhash_rust::xxh64::xxh64;

/// All fields decoded from one two-line wire emission (Part A + Part B).
#[derive(Debug, Clone)]
pub struct ParsedEntry {
    // Part A — fixed-position
    pub schema_version: u8,
    pub trace_id: String,        // CHAR(14)
    pub span_id: String,         // CHAR(7)
    pub parent_span: Option<String>, // None when wire contains "-------"
    pub seq: u8,
    pub service: u8,
    pub level: u8,
    pub outcome: Option<u8>,     // None when OUTCOME_NONE
    pub ts: DateTime<Utc>,

    // Part B — decoded from abbreviated JSON
    pub event: String,
    pub message: String,
    pub duration_ms: Option<i64>,
    pub user_id: Option<String>,
    pub journey_stage: Option<String>,
    pub error: Option<ParsedError>,
    pub ctx: Option<Value>,      // "c" object, stored verbatim in log_payload

    // Computed fingerprint (Decision 9) — None when no error present
    pub fingerprint: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ParsedError {
    pub kind: String,    // "ek"
    pub code: String,    // "ec"
    pub message: String, // "em"
    pub retryable: bool, // "rt"
}

/// Parse a complete two-line wire emission.
/// line_a must start with "A:"; line_b must be valid JSON.
pub fn parse_entry(line_a: &str, line_b: &str) -> Result<ParsedEntry> {
    let entry_a = parse_part_a(line_a)?;

    // Unknown schema version → cannot parse at fixed offsets.
    if entry_a.schema_version != PART_A_SCHEMA_VERSION {
        bail!(
            "unknown Part A schema version {}, expected {}",
            entry_a.schema_version,
            PART_A_SCHEMA_VERSION
        );
    }

    let (event, message, duration_ms, user_id, journey_stage, error, ctx) =
        parse_part_b(line_b)?;

    // Compute fingerprint identically to ComputeFingerprint in fingerprint.go.
    // Decision 9: xxHash64(service_byte | event | error_code | ctx_primary_key).
    // CRITICAL: service_code is a raw byte, not its string representation.
    let fingerprint = error.as_ref().map(|e| {
        let ctx_primary_key = ctx
            .as_ref()
            .and_then(|c| c.get("ctx_primary_key"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        compute_fingerprint(entry_a.service, &event, &e.code, ctx_primary_key)
    });

    Ok(ParsedEntry {
        schema_version: entry_a.schema_version,
        trace_id: entry_a.trace_id,
        span_id: entry_a.span_id,
        parent_span: entry_a.parent_span,
        seq: entry_a.seq,
        service: entry_a.service,
        level: entry_a.level,
        outcome: entry_a.outcome,
        ts: entry_a.ts,
        event,
        message,
        duration_ms,
        user_id,
        journey_stage,
        error,
        ctx,
        fingerprint,
    })
}

// ─── Part A ──────────────────────────────────────────────────────────────────

struct PartA {
    schema_version: u8,
    trace_id: String,
    span_id: String,
    parent_span: Option<String>,
    seq: u8,
    service: u8,
    level: u8,
    outcome: Option<u8>,
    ts: DateTime<Utc>,
}

fn parse_part_a(line: &str) -> Result<PartA> {
    let line = line.trim_end_matches('\n').trim_end_matches('\r');

    if !line.starts_with("A:") {
        bail!("Part A line must start with 'A:', got: {:?}", &line[..20.min(line.len())]);
    }

    let content = &line[2..]; // skip "A:"

    if content.len() < PART_A_BYTE_LEN {
        bail!(
            "Part A too short: {} bytes (need {})",
            content.len(),
            PART_A_BYTE_LEN
        );
    }

    let bytes = content.as_bytes();

    // [0] schema_version
    let schema_version = bytes[SCHEMA_VERSION_OFFSET] - b'0';

    // [2-15] trace_id (14 chars)
    let trace_id = str_slice(content, TRACE_ID_OFFSET, TRACE_ID_LEN)
        .context("trace_id slice")?
        .to_string();

    // [17-23] span_id (7 chars)
    let span_id = str_slice(content, SPAN_ID_OFFSET, SPAN_ID_LEN)
        .context("span_id slice")?
        .to_string();

    // [25-31] parent_span (7 chars, "-------" = absent)
    let parent_span_raw = str_slice(content, PARENT_SPAN_OFFSET, PARENT_SPAN_LEN)
        .context("parent_span slice")?;
    let parent_span = if parent_span_raw == PARENT_SPAN_ABSENT {
        None
    } else {
        Some(parent_span_raw.to_string())
    };

    // [33-34] seq (2 hex chars)
    let seq_str = str_slice(content, SEQ_OFFSET, SEQ_LEN).context("seq slice")?;
    let seq = u8::from_str_radix(seq_str, 16)
        .with_context(|| format!("seq hex parse: {:?}", seq_str))?;

    // [36] service code
    let service = bytes[SERVICE_CODE_OFFSET] - b'0';

    // [38] level code
    let level = bytes[LEVEL_CODE_OFFSET] - b'0';

    // [40] outcome code
    let out_code = bytes[OUTCOME_CODE_OFFSET] - b'0';
    let outcome = if out_code == OUTCOME_NONE { None } else { Some(out_code) };

    // [42-54] ts_ms (13 chars, unix millis)
    let ts_str = str_slice(content, TS_MS_OFFSET, TS_MS_LEN).context("ts_ms slice")?;
    let ts_ms = ts_str
        .parse::<i64>()
        .with_context(|| format!("ts_ms parse: {:?}", ts_str))?;
    let ts = DateTime::from_timestamp_millis(ts_ms)
        .context("ts_ms out of range")?
        .with_timezone(&Utc);

    Ok(PartA { schema_version, trace_id, span_id, parent_span, seq, service, level, outcome, ts })
}

// ─── Part B ──────────────────────────────────────────────────────────────────

type PartBFields = (
    String,        // event
    String,        // message
    Option<i64>,   // duration_ms
    Option<String>,// user_id
    Option<String>,// journey_stage
    Option<ParsedError>,
    Option<Value>, // ctx
);

fn parse_part_b(line: &str) -> Result<PartBFields> {
    let line = line.trim_end_matches('\n').trim_end_matches('\r');
    let v: Value = serde_json::from_str(line)
        .with_context(|| format!("Part B JSON parse failed: {:?}", &line[..80.min(line.len())]))?;

    let obj = v.as_object().context("Part B must be a JSON object")?;

    let event = obj
        .get("e")
        .and_then(|v| v.as_str())
        .context("Part B missing 'e' (event)")?
        .to_string();

    let message = obj
        .get("m")
        .and_then(|v| v.as_str())
        .context("Part B missing 'm' (message)")?
        .to_string();

    let duration_ms = obj.get("ms").and_then(|v| v.as_i64());

    let user_id = obj.get("ui").and_then(|v| v.as_str()).map(|s| s.to_string());
    let journey_stage = obj.get("js").and_then(|v| v.as_str()).map(|s| s.to_string());

    let error = obj.get("er").map(|er| {
        ParsedError {
            kind: er.get("ek").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            code: er.get("ec").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            message: er.get("em").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            retryable: er.get("rt").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    });

    // "c" is the developer ctx object — stored verbatim, including any ctx_primary_key.
    let ctx = obj.get("c").cloned();

    Ok((event, message, duration_ms, user_id, journey_stage, error, ctx))
}

// ─── Fingerprint ──────────────────────────────────────────────────────────────

/// Compute fingerprint matching observalog-go's ComputeFingerprint exactly.
/// Decision 9: xxHash64(service_byte | "|" | event | "|" | error_code | "|" | ctx_primary_key).
/// The service code is a raw byte (NOT its ASCII numeral), matching Go's string(uint8) semantics.
pub fn compute_fingerprint(service: u8, event: &str, error_code: &str, ctx_primary_key: &str) -> i64 {
    let mut data: Vec<u8> = Vec::with_capacity(
        1 + 1 + event.len() + 1 + error_code.len() + 1 + ctx_primary_key.len(),
    );
    data.push(service);    // raw byte — matches Go string(uint8(service))
    data.push(b'|');
    data.extend_from_slice(event.as_bytes());
    data.push(b'|');
    data.extend_from_slice(error_code.as_bytes());
    data.push(b'|');
    data.extend_from_slice(ctx_primary_key.as_bytes());
    xxh64(&data, 0) as i64
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn str_slice(s: &str, offset: usize, len: usize) -> Result<&str> {
    let end = offset + len;
    if s.len() < end {
        bail!("slice [{offset}..{end}] out of bounds (len={})", s.len());
    }
    Ok(&s[offset..end])
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Reference emission from TestEmitAllShapes output.
    const PART_A: &str = "A:1|trc_1234567890|spn_001|-------|01|0|1|0|1779942532087\n";
    const PART_B: &str = r#"{"c":{"backend":"postgres","bytes":1024,"di":"doc123"},"e":"doc.storage.saved","js":"auth.login","m":"Document written to storage","ui":"user123"}"#;

    #[test]
    fn test_parse_part_a_byte_positions() {
        let a = parse_part_a(PART_A).unwrap();
        assert_eq!(a.schema_version, 1);
        assert_eq!(a.trace_id, "trc_1234567890");
        assert_eq!(a.span_id, "spn_001");
        assert_eq!(a.parent_span, None); // "-------"
        assert_eq!(a.seq, 1);
        assert_eq!(a.service, 0);  // "0" = ServiceSystem
        assert_eq!(a.level, 1);    // "1" = LevelInfo
        assert_eq!(a.outcome, None); // "0" = OutcomeNone
    }

    #[test]
    fn test_parse_part_b_fields() {
        let (event, message, _, user_id, journey_stage, error, ctx) =
            parse_part_b(PART_B).unwrap();
        assert_eq!(event, "doc.storage.saved");
        assert_eq!(message, "Document written to storage");
        assert_eq!(user_id.as_deref(), Some("user123"));
        assert_eq!(journey_stage.as_deref(), Some("auth.login"));
        assert!(error.is_none());
        assert!(ctx.is_some());
    }

    #[test]
    fn test_parse_full_entry() {
        let entry = parse_entry(PART_A, PART_B).unwrap();
        assert_eq!(entry.trace_id, "trc_1234567890");
        assert_eq!(entry.event, "doc.storage.saved");
        assert!(entry.fingerprint.is_none()); // no error field
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let fp1 = compute_fingerprint(1, "auth.jwt.failed", "JWT_INVALID", "user_123");
        let fp2 = compute_fingerprint(1, "auth.jwt.failed", "JWT_INVALID", "user_123");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprint_ctx_primary_key_prevents_collision() {
        // Gap 3: same event + error_code, different ctx_primary_key → different fingerprint.
        let fp1 = compute_fingerprint(3, "provider.send.rejected", "QUOTA_EXCEEDED", "doc_001");
        let fp2 = compute_fingerprint(3, "provider.send.rejected", "QUOTA_EXCEEDED", "doc_002");
        assert_ne!(fp1, fp2);
    }
}
