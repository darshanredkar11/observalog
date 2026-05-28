package middleware

import (
	"fmt"
	"net/http"

	log "github.com/darshanredkar11/observalog/observalog-go"
)

// TracedTransport is an http.RoundTripper that automatically injects
// ObservaLog trace context headers into every outgoing HTTP request.
//
// Use it when Service A calls Service B over HTTP so that Service B's
// Middleware can continue the same trace rather than starting a fresh one.
//
//	Headers injected:
//	  X-Trace-Id       — the current trace_id (Service B continues the chain)
//	  X-Parent-Span-Id — this service's span_id (becomes parent_span in Service B)
//	  X-User-Id        — user identity, forwarded unchanged
//	  X-Journey-Stage  — journey stage established at the original entry point
//	  X-Service        — numeric service code of the caller
//
// Usage:
//
//	// Create once, reuse for all calls.
//	client := &http.Client{
//	    Transport: middleware.NewTracedTransport(nil), // nil → http.DefaultTransport
//	}
//
//	// Service A calling Service B — trace context propagates automatically.
//	req, _ := http.NewRequestWithContext(ctx, "GET", "http://service-b/health", nil)
//	resp, err := client.Do(req)
type TracedTransport struct {
	base http.RoundTripper
}

// NewTracedTransport wraps base. Pass nil to wrap http.DefaultTransport.
func NewTracedTransport(base http.RoundTripper) *TracedTransport {
	if base == nil {
		base = http.DefaultTransport
	}
	return &TracedTransport{base: base}
}

// RoundTrip injects trace headers then delegates to the wrapped transport.
// The context is read from req.Context() — always use
// http.NewRequestWithContext(ctx, ...) so the context is present.
func (t *TracedTransport) RoundTrip(req *http.Request) (*http.Response, error) {
	// Clone the request before mutating headers (RoundTrip must not modify the
	// original per the http.RoundTripper contract).
	r := req.Clone(req.Context())
	ctx := req.Context()

	if id := log.TraceIDFromCtx(ctx); id != "" {
		r.Header.Set("X-Trace-Id", id)
	}
	// Our span_id becomes the downstream parent_span — links the span tree.
	if span := log.SpanIDFromCtx(ctx); span != "" {
		r.Header.Set("X-Parent-Span-Id", span)
	}
	if uid := log.UserIDFromCtx(ctx); uid != "" {
		r.Header.Set("X-User-Id", uid)
	}
	if stage := log.JourneyStageFromCtx(ctx); stage != "" {
		r.Header.Set("X-Journey-Stage", stage)
	}
	r.Header.Set("X-Service", fmt.Sprintf("%d", log.GetServiceCode()))

	return t.base.RoundTrip(r)
}

// DefaultTracedClient is a convenience *http.Client backed by TracedTransport.
// Use this when you don't need a custom base transport.
var DefaultTracedClient = &http.Client{
	Transport: NewTracedTransport(nil),
}
