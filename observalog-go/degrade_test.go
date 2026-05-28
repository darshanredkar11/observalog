package log

import (
	"context"
	"strings"
	"testing"
)

func TestDegradationInjectsMissingFields(t *testing.T) {
	ctx := context.Background()

	// Apply degradation to empty context.
	ctx = ApplyGracefulDegradation(ctx)

	// trace_id should be injected with sys_ prefix.
	traceID := TraceIDFromCtx(ctx)
	if !strings.HasPrefix(traceID, "sys_") {
		t.Errorf("missing trace_id should be injected with sys_ prefix, got %q", traceID)
	}
	if len(traceID) != 14 {
		t.Errorf("sys_ trace_id should be CHAR(14), got len=%d", len(traceID))
	}

	// user_id should be "system".
	userID := UserIDFromCtx(ctx)
	if userID != "system" {
		t.Errorf("missing user_id should be 'system', got %q", userID)
	}

	// journey_stage should be "system.background.untraced".
	journeyStage := JourneyStageFromCtx(ctx)
	if journeyStage != "system.background.untraced" {
		t.Errorf("missing journey_stage should be 'system.background.untraced', got %q", journeyStage)
	}
}

func TestDegradationPreservesExistingFields(t *testing.T) {
	ctx := context.Background()
	ctx = WithTraceID(ctx, "trc_existing00")
	ctx = WithUserID(ctx, "user456")

	// Apply degradation.
	ctx = ApplyGracefulDegradation(ctx)

	// Existing fields should be preserved.
	if TraceIDFromCtx(ctx) != "trc_existing00" {
		t.Error("trace_id should be preserved")
	}
	if UserIDFromCtx(ctx) != "user456" {
		t.Error("user_id should be preserved")
	}
}
