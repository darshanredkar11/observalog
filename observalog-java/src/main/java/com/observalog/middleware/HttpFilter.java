package com.observalog.middleware;

import com.observalog.LogContext;
import jakarta.servlet.*;
import jakarta.servlet.http.HttpServletRequest;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import java.util.stream.Collectors;

// Servlet filter that injects trace context into LogContext (ThreadLocal).
// Register this filter for "/*" in web.xml or via @WebFilter.
public class HttpFilter implements Filter {

    @Override
    public void doFilter(ServletRequest request, ServletResponse response, FilterChain chain)
            throws IOException, ServletException {
        HttpServletRequest req = (HttpServletRequest) request;

        // Continue an existing trace OR start a fresh one at the entry point.
        // Never rely on graceful degradation here — that would generate a
        // different sys_<uuid> for every ObservaLog.info() call in the same request.
        String traceId = header(req, "X-Trace-Id");
        if (traceId.isEmpty()) {
            // "trc_" (4) + 10 hex chars = 14 bytes — fits the Part A trace_id slot.
            traceId = "trc_" + UUID.randomUUID().toString().replace("-", "").substring(0, 10);
        }

        // The upstream span becomes parent_span; generate a fresh one for this hop.
        String parentSpan = header(req, "X-Parent-Span-Id");
        String spanId     = "spn_" + UUID.randomUUID().toString().substring(0, 3);

        // Prefer inherited journey_stage; fall back to auto-derive.
        String journeyStage = header(req, "X-Journey-Stage");
        if (journeyStage.isEmpty()) {
            journeyStage = deriveJourneyStage(req.getRequestURI());
        }

        LogContext ctx = new LogContext();
        ctx.setTraceId(traceId);
        ctx.setSpanId(spanId);
        ctx.setParentSpan(parentSpan);
        ctx.setUserId(header(req, "X-User-Id"));
        ctx.setJourneyStage(journeyStage);

        LogContext.set(ctx);
        try {
            chain.doFilter(request, response);
        } finally {
            LogContext.clear();
        }
    }

    // Auto-derives journey_stage from request URI.
    // /api/v1/docs/123 → http.docs (strips /api and /v1 prefixes, drops IDs)
    private static String deriveJourneyStage(String uri) {
        String path = uri;
        for (String strip : List.of("/api", "/v1")) {
            if (path.startsWith(strip)) path = path.substring(strip.length());
        }
        path = path.replaceAll("^/|/$", "");
        if (path.isEmpty()) return "http.root";

        String[] parts = path.split("/");
        // Drop UUID/numeric segments (positional IDs)
        String joined = Arrays.stream(parts)
            .filter(p -> !p.isEmpty() && !p.matches("[0-9a-f\\-]{8,}|\\d+"))
            .collect(Collectors.joining("."));

        return joined.isEmpty() ? "http.root" : "http." + joined;
    }

    private static String header(HttpServletRequest req, String name) {
        String v = req.getHeader(name);
        return (v != null) ? v : "";
    }
}
