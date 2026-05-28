package log

import (
	"context"
	"sync/atomic"
)

// Unexported key types prevent collision with any other package's context keys.
type (
	traceIDKey      struct{}
	spanIDKey       struct{}
	parentSpanKey   struct{}
	userIDKey       struct{}
	tenantIDKey     struct{}
	journeyStageKey struct{}
	seqKey          struct{}
)

func WithTraceID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, traceIDKey{}, id)
}

func WithSpanID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, spanIDKey{}, id)
}

func WithParentSpan(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, parentSpanKey{}, id)
}

func WithUserID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, userIDKey{}, id)
}

func WithTenantID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, tenantIDKey{}, id)
}

func WithJourneyStage(ctx context.Context, stage string) context.Context {
	return context.WithValue(ctx, journeyStageKey{}, stage)
}

// WithSeq injects a service-local atomic counter. Call once per service entry
// with new(atomic.Uint32). Decision 1: never a global, Redis, or mutex counter.
func WithSeq(ctx context.Context, seq *atomic.Uint32) context.Context {
	return context.WithValue(ctx, seqKey{}, seq)
}

func TraceIDFromCtx(ctx context.Context) string {
	v, _ := ctx.Value(traceIDKey{}).(string)
	return v
}

func SpanIDFromCtx(ctx context.Context) string {
	v, _ := ctx.Value(spanIDKey{}).(string)
	return v
}

func ParentSpanFromCtx(ctx context.Context) string {
	v, _ := ctx.Value(parentSpanKey{}).(string)
	return v
}

func UserIDFromCtx(ctx context.Context) string {
	v, _ := ctx.Value(userIDKey{}).(string)
	return v
}

func TenantIDFromCtx(ctx context.Context) string {
	v, _ := ctx.Value(tenantIDKey{}).(string)
	return v
}

func JourneyStageFromCtx(ctx context.Context) string {
	v, _ := ctx.Value(journeyStageKey{}).(string)
	return v
}

// SeqFromCtx returns the atomic counter pointer, or nil if not in context.
func SeqFromCtx(ctx context.Context) *atomic.Uint32 {
	v, _ := ctx.Value(seqKey{}).(*atomic.Uint32)
	return v
}
