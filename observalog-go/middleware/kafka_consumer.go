package middleware

import (
	"context"
	"sync/atomic"

	log "github.com/darshanredkar11/observalog-go"
)

// Message represents a Kafka message with headers.
type Message interface {
	Headers() map[string]string
}

// KafkaMiddleware wraps a Kafka message handler to inject trace context.
// The handler signature is handler(ctx context.Context, msg Message) error.
func KafkaMiddleware(handler func(context.Context, Message) error) func(context.Context, Message) error {
	return func(ctx context.Context, msg Message) error {
		headers := msg.Headers()

		// Extract or propagate trace_id.
		if traceID, ok := headers["X-Trace-Id"]; ok {
			ctx = log.WithTraceID(ctx, traceID)
		}

		// Extract parent span_id.
		if parentSpan, ok := headers["X-Span-Id"]; ok {
			ctx = log.WithParentSpan(ctx, parentSpan)
		}

		// Extract user_id.
		if userID, ok := headers["X-User-Id"]; ok {
			ctx = log.WithUserID(ctx, userID)
		}

		// Extract journey_stage.
		if journeyStage, ok := headers["X-Journey-Stage"]; ok {
			ctx = log.WithJourneyStage(ctx, journeyStage)
		}

		// Create fresh seq counter for this message.
		ctx = log.WithSeq(ctx, new(atomic.Uint32))

		return handler(ctx, msg)
	}
}
