package middleware

import (
	"net/http"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/google/uuid"
	log "github.com/darshanredkar11/observalog/observalog-go"
)

var (
	routeStages = make(map[string]string)
	mu           sync.RWMutex
)

// Middleware wraps an HTTP handler to inject trace context.
//
// Inbound propagation: if an upstream service (or API gateway) passes
// X-Trace-Id / X-Parent-Span-Id headers the trace chain is continued.
// If there are no incoming headers this is a fresh entry point and a new
// trc_<id> is generated so that ALL log calls within this request share
// the same trace_id (relying on ApplyGracefulDegradation would generate a
// different sys_<uuid> for every emitLine call).
func Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()

		// Continue an existing trace OR start a fresh one at the entry point.
		traceID := r.Header.Get("X-Trace-Id")
		if traceID == "" {
			// "trc_" (4) + 10 hex chars = 14 bytes — fits the Part A trace_id slot.
			traceID = "trc_" + uuid.New().String()[:10]
		}
		ctx = log.WithTraceID(ctx, traceID)

		// Generate a new span_id for this service hop.
		spanID := "spn_" + uuid.New().String()[:3]
		ctx = log.WithSpanID(ctx, spanID)

		// The upstream span becomes our parent_span (shows call hierarchy in the brain).
		if parentSpan := r.Header.Get("X-Parent-Span-Id"); parentSpan != "" {
			ctx = log.WithParentSpan(ctx, parentSpan)
		}

		// Propagate user identity across the chain.
		if userID := r.Header.Get("X-User-Id"); userID != "" {
			ctx = log.WithUserID(ctx, userID)
		}

		// journey_stage: prefer inherited value, fall back to auto-derive.
		journeyStage := r.Header.Get("X-Journey-Stage")
		if journeyStage == "" {
			journeyStage = deriveJourneyStage(r)
		}
		ctx = log.WithJourneyStage(ctx, journeyStage)

		// Fresh seq counter — resets to 1 for this service hop.
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
