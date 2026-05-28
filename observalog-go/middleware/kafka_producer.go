package middleware

import (
	"context"
	"fmt"

	log "github.com/yourorg/observalog-go"
)

// KafkaMessage wraps a Kafka message with trace headers injected.
type KafkaMessage struct {
	Topic   string
	Value   []byte
	Headers map[string]string
}

// NewTracedMessage creates a Kafka message with trace context injected into headers.
func NewTracedMessage(ctx context.Context, topic string, value []byte) KafkaMessage {
	headers := make(map[string]string)

	// Inject trace context.
	if traceID := log.TraceIDFromCtx(ctx); traceID != "" {
		headers["X-Trace-Id"] = traceID
	}
	if spanID := log.SpanIDFromCtx(ctx); spanID != "" {
		headers["X-Span-Id"] = spanID
	}
	if userID := log.UserIDFromCtx(ctx); userID != "" {
		headers["X-User-Id"] = userID
	}
	if journeyStage := log.JourneyStageFromCtx(ctx); journeyStage != "" {
		headers["X-Journey-Stage"] = journeyStage
	}

	// Inject service info.
	headers["X-Service"] = fmt.Sprintf("%d", log.GetServiceCode())

	// Inject seq for message ordering.
	if seq := log.SeqFromCtx(ctx); seq != nil {
		headers["X-Service-Seq"] = fmt.Sprintf("%d", seq.Load())
	}

	// Inject schema version for wire format.
	headers["X-Schema-Version"] = fmt.Sprintf("%d", log.PartASchemaVersion)

	return KafkaMessage{
		Topic:   topic,
		Value:   value,
		Headers: headers,
	}
}
