package log

import (
	"context"

	"github.com/google/uuid"
)

// ApplyGracefulDegradation injects missing ambient fields.
// Decision 5: Never panic. Background jobs and crons are legitimate.
func ApplyGracefulDegradation(ctx context.Context) context.Context {
	if TraceIDFromCtx(ctx) == "" {
		// sys_ prefix signals the brain this is untraced (Decision 5).
		// Truncate uuid to 10 chars to fit CHAR(14): "sys_" + 10.
		u := uuid.New().String()
		ctx = WithTraceID(ctx, "sys_"+u[:10])
	}
	if UserIDFromCtx(ctx) == "" {
		ctx = WithUserID(ctx, "system")
	}
	if JourneyStageFromCtx(ctx) == "" {
		ctx = WithJourneyStage(ctx, "system.background.untraced")
	}
	return ctx
}
