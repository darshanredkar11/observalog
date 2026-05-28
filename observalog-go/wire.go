package log

import (
	"encoding/json"
	"fmt"
)

// LogEntry holds every field needed to produce one two-line wire emission.
type LogEntry struct {
	// Part A — fixed-position header
	TraceID     string
	SpanID      string
	ParentSpan  string
	Seq         uint32
	ServiceCode uint8
	LevelCode   uint8
	OutcomeCode uint8
	TsMs        int64

	// Part B — top-level structural
	Event      string
	Message    string
	DurationMs *int64
	Outcome    Outcome
	Error      *Err
	Fingerprint int64

	// Part B — ambient (from context, not developer F)
	UserID       string
	JourneyStage string

	// Part B — developer-supplied ctx (F minus extracted structural keys)
	Ctx F
}

// EncodePartA writes the compact fixed-position header line.
// Output after "A:" is always exactly PartAByteLen bytes, followed by '\n'.
func EncodePartA(entry LogEntry) []byte {
	parentSpan := entry.ParentSpan
	if parentSpan == "" {
		parentSpan = "-------"
	}
	// seq encoded as 2 hex chars: always 2 chars for values 0-255.
	line := fmt.Sprintf("A:%d|%-14s|%-7s|%-7s|%02x|%d|%d|%d|%013d\n",
		PartASchemaVersion,
		truncate(entry.TraceID, 14),
		truncate(entry.SpanID, 7),
		truncate(parentSpan, 7),
		entry.Seq&0xFF,
		entry.ServiceCode,
		entry.LevelCode,
		entry.OutcomeCode,
		entry.TsMs,
	)
	return []byte(line)
}

// EncodePartB writes the abbreviated JSON payload line.
// Keys are shortened using the dict from dict.go.
func EncodePartB(entry LogEntry) []byte {
	out := make(map[string]interface{}, 8)
	out["e"] = entry.Event
	out["m"] = entry.Message
	if entry.DurationMs != nil {
		out["ms"] = *entry.DurationMs
	}
	if entry.Outcome != "" {
		out["o"] = string(entry.Outcome)
	}
	if entry.Error != nil {
		out["er"] = map[string]interface{}{
			"ek": entry.Error.Kind,
			"ec": entry.Error.Code,
			"em": entry.Error.Message,
			"rt": entry.Error.Retryable,
		}
	}
	if len(entry.Ctx) > 0 {
		abbrev := make(map[string]interface{}, len(entry.Ctx))
		for k, v := range entry.Ctx {
			if short, ok := KeyToAbbrev[k]; ok {
				abbrev[short] = v
			} else {
				abbrev[k] = v
			}
		}
		out["c"] = abbrev
	}
	if entry.UserID != "" {
		out["ui"] = entry.UserID
	}
	if entry.JourneyStage != "" {
		out["js"] = entry.JourneyStage
	}
	b, _ := json.Marshal(out)
	return append(b, '\n')
}

// truncate returns s trimmed to at most n bytes (left-padded with spaces if shorter
// would misalign fixed offsets — we left-justify instead via fmt %-Ns).
func truncate(s string, n int) string {
	if len(s) > n {
		return s[:n]
	}
	return s
}
