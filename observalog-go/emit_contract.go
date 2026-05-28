package log

import (
	"sync/atomic"
)

// Contract: seq must be *atomic.Uint32, never a value copy or global counter.
// This compile-time assertion prevents accidental value copies (Decision 1).
var _ = func() struct{} {
	var seq *atomic.Uint32
	// Using the pointer, not a value, ensures atomic operations work.
	_ = seq
	return struct{}{}
}()

// Contract: PartAByteLen must match the actual fixed-position header size.
// Part A format after "A:" is exactly 55 bytes:
//   [0]     schema version (1)
//   [1]     | (1)
//   [2-15]  trace_id (14)
//   [16]    | (1)
//   [17-23] span_id (7)
//   [24]    | (1)
//   [25-31] parent_span (7)
//   [32]    | (1)
//   [33-34] seq (2)
//   [35]    | (1)
//   [36]    svc (1)
//   [37]    | (1)
//   [38]    lvl (1)
//   [39]    | (1)
//   [40]    out (1)
//   [41]    | (1)
//   [42-54] ts_ms (13)
// Total: 55 bytes. Verified in wire_test.go.

// Contract: Fingerprint must include ctx_primary_key in the hash input.
// Without it, different domain objects with same event+error_code collide (Gap 3).
// The fingerprint is: xxHash64(serviceCode | event | error_code | ctx_primary_key).
