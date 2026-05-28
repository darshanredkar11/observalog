package log

import (
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

// Init validates config, starts the drain goroutine, and sets globals.
// Must be called before any log calls. Panics on invalid config.
func Init(cfg Config) error {
	// Validate required fields.
	if cfg.ServiceName == "" {
		panic("Init: SERVICE_NAME env var required")
	}
	if cfg.Env == "" {
		panic("Init: ENV env var required")
	}
	if cfg.Level == "" {
		panic("Init: LOG_LEVEL env var required")
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
		panic("Init: invalid LOG_LEVEL=" + cfg.Level)
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

// Shutdown drains the buffer and closes the channel.
// Blocks until all logs are written.
func Shutdown() {
	FlushAndClose()
}
