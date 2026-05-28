package log

import (
	"context"
	"os"
	"sync/atomic"
	"testing"
)

func TestEmitAllShapes(t *testing.T) {
	os.Setenv("SERVICE_NAME", "test")
	os.Setenv("ENV", "test")
	os.Setenv("LOG_LEVEL", "debug")
	Init(ConfigFromEnv("test-v1"))

	ctx := context.Background()
	ctx = WithTraceID(ctx, "trc_1234567890ab")
	ctx = WithSpanID(ctx, "spn_001")
	ctx = WithUserID(ctx, "user123")
	ctx = WithJourneyStage(ctx, "auth.login")
	ctx = WithSeq(ctx, new(atomic.Uint32))

	// Shape 1: Informational
	Info(ctx, "doc.storage.saved", "Document written to storage", F{
		"doc_id":  "doc123",
		"bytes":   1024,
		"backend": "postgres",
	})

	// Shape 2: Decision point
	Info(ctx, "auth.permission.checked", "Permission granted", F{
		"doc_id":      "doc123",
		"permission":  "doc:send",
		"outcome":     Success,
		"duration_ms": int64(42),
	})

	// Shape 3: Failure
	Error(ctx, "provider.send.rejected", "Provider rejected send", F{
		"doc_id":      "doc123",
		"provider":    "sendgrid",
		"http_status": 429,
		"outcome":     Failure,
		"duration_ms": int64(87),
		"error": &Err{
			Kind:      "RateLimitExceeded",
			Code:      "PROVIDER_QUOTA_EXCEEDED",
			Message:   "rate limit exceeded",
			Retryable: true,
		},
	})

	Shutdown()
}

func TestAmbientFieldInjection(t *testing.T) {
	ctx := context.Background()
	ctx = WithTraceID(ctx, "trc_test000001")
	ctx = WithUserID(ctx, "testuser")

	ctx2 := ApplyGracefulDegradation(ctx)

	if TraceIDFromCtx(ctx2) != "trc_test000001" {
		t.Error("trace_id not preserved")
	}
	if UserIDFromCtx(ctx2) != "testuser" {
		t.Error("user_id not preserved")
	}
	if !isSystemJourneyStage(JourneyStageFromCtx(ctx2)) {
		t.Error("journey_stage should be injected")
	}
}

func isSystemJourneyStage(s string) bool {
	return s == "system.background.untraced"
}
