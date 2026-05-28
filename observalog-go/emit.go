package log

import (
	"context"
	"sync/atomic"
	"time"
)

var (
	minLevel   Level = LevelInfo
	serviceCode uint8
)

// GetServiceCode returns the service code set at Init().
func GetServiceCode() uint8 {
	return serviceCode
}

func Info(ctx context.Context, event, message string, fields F) {
	emitLine(ctx, LevelInfo, event, message, fields)
}

func Warn(ctx context.Context, event, message string, fields F) {
	emitLine(ctx, LevelWarn, event, message, fields)
}

func Error(ctx context.Context, event, message string, fields F) {
	emitLine(ctx, LevelError, event, message, fields)
}

func Debug(ctx context.Context, event, message string, fields F) {
	emitLine(ctx, LevelDebug, event, message, fields)
}

func emitLine(ctx context.Context, level Level, event, message string, fields F) {
	// Level check: return if below threshold.
	if level < minLevel {
		return
	}

	// Apply graceful degradation for missing ambient fields.
	ctx = ApplyGracefulDegradation(ctx)

	// Extract ambient fields from context.
	traceID := TraceIDFromCtx(ctx)
	spanID := SpanIDFromCtx(ctx)
	parentSpan := ParentSpanFromCtx(ctx)
	userID := UserIDFromCtx(ctx)
	journeyStage := JourneyStageFromCtx(ctx)

	// Increment seq.
	seq := SeqFromCtx(ctx)
	if seq == nil {
		seq = new(atomic.Uint32)
		ctx = WithSeq(ctx, seq)
	}
	seqVal := seq.Add(1)

	// Extract and remove structural fields from F.
	outcome := Outcome("")
	durationMs := (*int64)(nil)
	var errField *Err
	developerCtx := make(F, len(fields))

	for k, v := range fields {
		switch k {
		case "outcome":
			if o, ok := v.(Outcome); ok {
				outcome = o
			} else if s, ok := v.(string); ok {
				outcome = Outcome(s)
			}
		case "duration_ms":
			if ms, ok := v.(int64); ok {
				durationMs = &ms
			}
		case "error":
			if e, ok := v.(*Err); ok {
				errField = e
			}
		default:
			developerCtx[k] = v
		}
	}

	// Compute fingerprint if error present.
	var fingerprint int64
	if errField != nil {
		// Extract ctx_primary_key from developer context if available.
		ctxKey := ""
		if pk, ok := developerCtx["ctx_primary_key"].(string); ok {
			ctxKey = pk
		}
		fingerprint = ComputeFingerprint(GetServiceCode(), event, errField.Code, ctxKey)
	}

	// Convert level to wire code.
	levelCode := uint8(level)

	// Convert outcome to wire code.
	outcomeCode := uint8(OutcomeCodeNone)
	switch outcome {
	case Success:
		outcomeCode = OutcomeCodeSuccess
	case Failure:
		outcomeCode = OutcomeCodeFailure
	case Partial:
		outcomeCode = OutcomeCodePartial
	case Pending:
		outcomeCode = OutcomeCodePending
	}

	// Create log entry.
	entry := LogEntry{
		TraceID:      traceID,
		SpanID:       spanID,
		ParentSpan:   parentSpan,
		Seq:          seqVal,
		ServiceCode:  GetServiceCode(),
		LevelCode:    levelCode,
		OutcomeCode:  outcomeCode,
		TsMs:         time.Now().UnixMilli(),
		Event:        event,
		Message:      message,
		DurationMs:   durationMs,
		Outcome:      outcome,
		Error:        errField,
		Fingerprint:  fingerprint,
		UserID:       userID,
		JourneyStage: journeyStage,
		Ctx:          developerCtx,
	}

	// Encode and send.
	partA := EncodePartA(entry)
	partB := EncodePartB(entry)
	SendToChannel(partA)
	SendToChannel(partB)
}
