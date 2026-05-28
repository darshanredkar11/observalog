package log

import "github.com/cespare/xxhash/v2"

// ComputeFingerprint produces a 64-bit hash for deduplication.
// Decision 9: xxHash64(serviceCode | event | error_code | ctx_primary_key).
// Decision 9: Must include ctx_primary_key to prevent semantic collisions across
// different domain objects with the same event+error_code (Gap 3).
func ComputeFingerprint(serviceCode uint8, event, errorCode, ctxPrimaryKey string) int64 {
	// Concatenate components with pipe delimiters.
	data := string(serviceCode) + "|" + event + "|" + errorCode + "|" + ctxPrimaryKey
	return int64(xxhash.Sum64String(data))
}
