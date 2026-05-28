package log

import (
	"strings"
	"testing"
)

func TestPartABytePositions(t *testing.T) {
	entry := LogEntry{
		TraceID:     "trc_7f2a1b9e4d",
		SpanID:      "spn_004",
		ParentSpan:  "spn_001",
		Seq:         4,
		ServiceCode: ServiceDoc,
		LevelCode:   LevelCodeInfo,
		OutcomeCode: OutcomeCodeSuccess,
		TsMs:        1748268153812,
	}

	partA := string(EncodePartA(entry))
	// Remove trailing newline for position testing.
	partA = strings.TrimSuffix(partA, "\n")

	// Verify "A:" prefix.
	if !strings.HasPrefix(partA, "A:") {
		t.Errorf("Part A missing 'A:' prefix: %s", partA)
	}

	// Test against wire_contract offsets (0-indexed after "A:").
	content := partA[2:] // Skip "A:"

	// [0] schema version
	if content[SchemaVersionOffset] != '1' {
		t.Errorf("schema version at [%d] = %c, want '1'", SchemaVersionOffset, content[SchemaVersionOffset])
	}

	// [2-15] trace_id (14 chars)
	traceID := content[TraceIDOffset : TraceIDOffset+14]
	if traceID != "trc_7f2a1b9e4d" {
		t.Errorf("trace_id at [%d:%d] = %q, want 'trc_7f2a1b9e4d'", TraceIDOffset, TraceIDOffset+14, traceID)
	}

	// [17-23] span_id (7 chars)
	spanID := content[SpanIDOffset : SpanIDOffset+7]
	if spanID != "spn_004" {
		t.Errorf("span_id at [%d:%d] = %q, want 'spn_004'", SpanIDOffset, SpanIDOffset+7, spanID)
	}

	// [25-31] parent_span (7 chars)
	parentSpan := content[ParentSpanOffset : ParentSpanOffset+7]
	if parentSpan != "spn_001" {
		t.Errorf("parent_span at [%d:%d] = %q, want 'spn_001'", ParentSpanOffset, ParentSpanOffset+7, parentSpan)
	}

	// [42-54] ts_ms (13 chars)
	tsMs := content[TsMsOffset : TsMsOffset+13]
	if tsMs != "1748268153812" {
		t.Errorf("ts_ms at [%d:%d] = %q, want '1748268153812'", TsMsOffset, TsMsOffset+13, tsMs)
	}

	// Verify total length is exactly PartAByteLen.
	if len(content) != PartAByteLen {
		t.Errorf("Part A length = %d, want %d", len(content), PartAByteLen)
	}
}

func TestPartBJson(t *testing.T) {
	entry := LogEntry{
		Event:      "auth.jwt.validated",
		Message:    "Token validated successfully",
		DurationMs: ptrInt64(42),
		Outcome:    Success,
		UserID:     "user123",
		JourneyStage: "auth.login",
		Ctx: F{
			"doc_id": "abc123",
		},
	}

	partB := string(EncodePartB(entry))
	partB = strings.TrimSuffix(partB, "\n")

	// Should be valid JSON with abbreviated keys.
	if !strings.Contains(partB, "\"e\"") {
		t.Error("Part B missing abbreviated 'e' key for event")
	}
	if !strings.Contains(partB, "\"m\"") {
		t.Error("Part B missing abbreviated 'm' key for message")
	}
	if !strings.Contains(partB, "\"ui\"") {
		t.Error("Part B missing abbreviated 'ui' key for user_id")
	}
	if !strings.Contains(partB, "\"c\"") {
		t.Error("Part B missing abbreviated 'c' key for ctx")
	}
}

func ptrInt64(v int64) *int64 {
	return &v
}
