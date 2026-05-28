package middleware

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	log "github.com/darshanredkar11/observalog/observalog-go"
)

func TestHttpMiddlewareTraceIdPropagation(t *testing.T) {
	handler := Middleware(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()
		traceID := log.TraceIDFromCtx(ctx)
		if traceID != "trc_testtrace01" {
			t.Errorf("trace_id not propagated, got %q", traceID)
		}
		w.WriteHeader(http.StatusOK)
	}))

	req := httptest.NewRequest("GET", "/test", nil)
	req.Header.Set("X-Trace-Id", "trc_testtrace01")

	rr := httptest.NewRecorder()
	handler.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Errorf("handler returned %d, want %d", rr.Code, http.StatusOK)
	}
}

func TestHttpMiddlewareJourneyStageDerivation(t *testing.T) {
	handler := Middleware(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()
		journeyStage := log.JourneyStageFromCtx(ctx)

		// Should auto-derive to http.users.list based on /api/v1/users
		if !strings.HasPrefix(journeyStage, "http.") {
			t.Errorf("journey_stage should start with 'http.', got %q", journeyStage)
		}
		w.WriteHeader(http.StatusOK)
	}))

	req := httptest.NewRequest("GET", "/api/v1/users", nil)
	rr := httptest.NewRecorder()
	handler.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Errorf("handler returned %d, want %d", rr.Code, http.StatusOK)
	}
}

func TestHttpMiddlewareGeneratesSpanId(t *testing.T) {
	var capturedSpanID string

	handler := Middleware(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()
		capturedSpanID = log.SpanIDFromCtx(ctx)
		w.WriteHeader(http.StatusOK)
	}))

	req := httptest.NewRequest("GET", "/test", nil)
	rr := httptest.NewRecorder()
	handler.ServeHTTP(rr, req)

	if !strings.HasPrefix(capturedSpanID, "spn_") {
		t.Errorf("span_id should start with 'spn_', got %q", capturedSpanID)
	}
	if len(capturedSpanID) != 7 {
		t.Errorf("span_id should be 7 chars total, got len=%d", len(capturedSpanID))
	}
}
