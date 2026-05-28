package log

// F is the developer-supplied context map attached to every log call.
type F map[string]interface{}

// Err carries typed error metadata. Never pass a raw error string.
type Err struct {
	Kind      string // error class: "RateLimitExceeded", "NetworkError"
	Code      string // machine-readable constant: "PROVIDER_QUOTA_EXCEEDED"
	Message   string // err.Error() value
	Retryable bool
}

// Outcome represents the result of a decision point or operation exit.
type Outcome string

const (
	Success Outcome = "success"
	Failure Outcome = "failure"
	Partial Outcome = "partial"
	Pending Outcome = "pending"
)

// Level controls which log calls are emitted vs filtered.
// Ordered: Debug < Info < Warn < Error — comparable with <.
type Level uint8

const (
	LevelDebug Level = iota
	LevelInfo
	LevelWarn
	LevelError
)

func (l Level) String() string {
	switch l {
	case LevelDebug:
		return "debug"
	case LevelInfo:
		return "info"
	case LevelWarn:
		return "warn"
	case LevelError:
		return "error"
	default:
		return "unknown"
	}
}
