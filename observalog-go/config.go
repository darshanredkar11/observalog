package log

import (
	"fmt"
	"os"
	"strconv"
	"strings"
)

type Config struct {
	ServiceName string
	Version     string
	Env         string
	Level       string // "debug", "info", "warn", "error"
	BufferSize  int
	Host        string
}

// ConfigFromEnv reads config from environment variables.
// VERSION must be passed as parameter (typically from ldflags).
func ConfigFromEnv(version string) Config {
	bufSize := 10000
	if b := os.Getenv("LOG_BUFFER_SIZE"); b != "" {
		if n, err := strconv.Atoi(b); err == nil {
			bufSize = n
		}
	}

	host := os.Getenv("HOST")
	if host == "" {
		host, _ = os.Hostname()
	}

	return Config{
		ServiceName: os.Getenv("SERVICE_NAME"),
		Version:     version,
		Env:         os.Getenv("ENV"),
		Level:       strings.ToLower(os.Getenv("LOG_LEVEL")),
		BufferSize:  bufSize,
		Host:        host,
	}
}

// Init validates config, starts the drain goroutine, and sets package globals.
// Must be called before any log calls.
//
// Returns an error — it no longer panics. Collect all missing vars in one
// error so callers can surface a useful message rather than a series of
// panics:
//
//	if err := log.Init(log.ConfigFromEnv("v1.0.0")); err != nil {
//	    log.Fatalf("observalog: %v", err)
//	}
//
// Use MustInit if you prefer panic-on-error behaviour (e.g. in test helpers).
func Init(cfg Config) error {
	// Collect all missing required fields in one pass.
	var missing []string
	if cfg.ServiceName == "" {
		missing = append(missing, "SERVICE_NAME")
	}
	if cfg.Env == "" {
		missing = append(missing, "ENV")
	}
	if cfg.Level == "" {
		missing = append(missing, "LOG_LEVEL")
	}
	if len(missing) > 0 {
		return fmt.Errorf("observalog Init: missing required config: %s", strings.Join(missing, ", "))
	}

	// Parse and set minLevel.
	switch cfg.Level {
	case "debug":
		minLevel = LevelDebug
	case "info":
		minLevel = LevelInfo
	case "warn":
		minLevel = LevelWarn
	case "error":
		minLevel = LevelError
	default:
		return fmt.Errorf("observalog Init: invalid LOG_LEVEL=%q (want debug|info|warn|error)", cfg.Level)
	}

	// Set service code based on name.
	switch cfg.ServiceName {
	case "auth":
		serviceCode = ServiceAuth
	case "doc":
		serviceCode = ServiceDoc
	case "provider":
		serviceCode = ServiceProvider
	default:
		serviceCode = ServiceSystem
	}

	// Validate dict collisions.
	ValidateNoDictCollisions()

	// Start drain goroutine to stdout.
	StartDrainGoroutine(os.Stdout, cfg.BufferSize)

	return nil
}

// MustInit calls Init and panics if it returns an error.
// Prefer Init in production code; use MustInit in test helpers or main()
// where a missing config is a programmer error, not a runtime condition.
func MustInit(cfg Config) {
	if err := Init(cfg); err != nil {
		panic(err)
	}
}

// Shutdown drains the buffer and closes the channel.
// Blocks until all logs are written.
func Shutdown() {
	FlushAndClose()
}
