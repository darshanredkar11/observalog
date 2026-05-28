package middleware

import (
	"net/http"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/google/uuid"
	log "github.com/yourorg/observalog-go"
)

var (
	routeStages = make(map[string]string)
	mu           sync.RWMutex
)

// Middleware wraps an HTTP handler to inject trace context.
func Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()

		// Propagate or create trace_id.
		traceID := r.Header.Get("X-Trace-Id")
		if traceID != "" {
			ctx = log.WithTraceID(ctx, traceID)
		}

		// Generate new span_id for this service.
		spanID := "spn_" + uuid.New().String()[:3]
		ctx = log.WithSpanID(ctx, spanID)

		// Propagate parent span if present.
		if parentSpan := r.Header.Get("X-Parent-Span-Id"); parentSpan != "" {
			ctx = log.WithParentSpan(ctx, parentSpan)
		}

		// Propagate user_id.
		if userID := r.Header.Get("X-User-Id"); userID != "" {
			ctx = log.WithUserID(ctx, userID)
		}

		// Derive and set journey_stage.
		journeyStage := deriveJourneyStage(r)
		ctx = log.WithJourneyStage(ctx, journeyStage)

		// Create fresh seq counter for this request.
		ctx = log.WithSeq(ctx, new(atomic.Uint32))

		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// deriveJourneyStage looks up the route in config or auto-derives from request path.
func deriveJourneyStage(r *http.Request) string {
	// Try config lookup first.
	mu.RLock()
	if stage, ok := routeStages[r.Method+" "+r.URL.Path]; ok {
		mu.RUnlock()
		return stage
	}
	mu.RUnlock()

	// Auto-derive from path: e.g., /api/v1/docs → api.docs
	path := strings.TrimPrefix(strings.TrimPrefix(r.URL.Path, "/api"), "/v1")
	path = strings.Trim(path, "/")
	parts := strings.Split(path, "/")
	if len(parts) == 0 || parts[0] == "" {
		return "http.root"
	}
	// Join with dots: /users/{id}/edit → http.users.edit
	return "http." + strings.Join(parts, ".")
}

// LoadRouteStages loads a route → journey_stage mapping (stub for YAML file).
// For now, this is a placeholder. In production, parse route_stages.yaml.
func LoadRouteStages(path string) error {
	// TODO: Parse YAML and populate routeStages map.
	return nil
}
