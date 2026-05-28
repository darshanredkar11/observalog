package com.observalog.middleware;

import com.observalog.LogContext;
import org.springframework.http.HttpRequest;
import org.springframework.http.client.ClientHttpRequestExecution;
import org.springframework.http.client.ClientHttpRequestInterceptor;
import org.springframework.http.client.ClientHttpResponse;

import java.io.IOException;

/**
 * Spring RestTemplate interceptor that injects ObservaLog trace context headers
 * into every outgoing HTTP request.
 *
 * <p>Register once when configuring your RestTemplate:
 * <pre>{@code
 * @Bean
 * public RestTemplate restTemplate() {
 *     RestTemplate rt = new RestTemplate();
 *     rt.getInterceptors().add(new TracedRestTemplateInterceptor());
 *     return rt;
 * }
 * }</pre>
 *
 * <p>Headers injected:
 * <ul>
 *   <li>X-Trace-Id       — continues the trace in the downstream service</li>
 *   <li>X-Parent-Span-Id — this service's span_id, becomes parent_span downstream</li>
 *   <li>X-User-Id        — forwarded user identity</li>
 *   <li>X-Journey-Stage  — forwarded journey stage</li>
 * </ul>
 */
public class TracedRestTemplateInterceptor implements ClientHttpRequestInterceptor {

    @Override
    public ClientHttpResponse intercept(
            HttpRequest request,
            byte[] body,
            ClientHttpRequestExecution execution) throws IOException {

        LogContext ctx = LogContext.get();

        if (!ctx.getTraceId().isEmpty()) {
            request.getHeaders().set("X-Trace-Id", ctx.getTraceId());
        }
        // Our span becomes the downstream parent_span — links the span tree.
        if (!ctx.getSpanId().isEmpty()) {
            request.getHeaders().set("X-Parent-Span-Id", ctx.getSpanId());
        }
        if (!ctx.getUserId().isEmpty()) {
            request.getHeaders().set("X-User-Id", ctx.getUserId());
        }
        if (!ctx.getJourneyStage().isEmpty()) {
            request.getHeaders().set("X-Journey-Stage", ctx.getJourneyStage());
        }

        return execution.execute(request, body);
    }
}
