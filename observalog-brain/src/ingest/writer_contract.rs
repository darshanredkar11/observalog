/// Write contract — READ ONLY. Invariants for the two-table write path.
/// Any change to writer.rs must verify these invariants are preserved.
///
/// INVARIANT 1 — Write order (Decision 8 / Gap 4):
///   log_payload is written BEFORE log_index.
///   If brain crashes between the two writes, the orphaned payload has no
///   index row pointing to it. Orphan cleanup handles this at startup.
///   NEVER reverse the order.
///
/// INVARIANT 2 — payload_id validity:
///   log_index.payload_id MUST reference an existing log_payload.id.
///   No foreign key constraint exists (performance), so writer.rs enforces this.
///
/// INVARIANT 3 — ts bound on all trace_id queries (Gap 1 / Decision 10):
///   Every query on log_index WHERE trace_id = X MUST include
///   AND ts BETWEEN $from AND $to.
///   Without the ts bound, TimescaleDB scans all chunks instead of 1.
///   See: db/queries.rs fetch_journey(), fetch_seq_chain().
///
/// INVARIANT 4 — fingerprint computation (Decision 9 / Gap 3):
///   Fingerprint = xxHash64(service_byte | event | error_code | ctx_primary_key).
///   service is a raw byte (NOT its ASCII digit), matching Go string(uint8) semantics.
///   See: ingest/parser.rs compute_fingerprint().
///
/// INVARIANT 5 — JOIN scale threshold (Gap 10):
///   When fetching payloads for a trace, fetch_payloads() should only be
///   called when the result set is <= 15 rows. Above that, flag for later.
pub const MAX_PAYLOAD_FETCH_ROWS: usize = 15;
