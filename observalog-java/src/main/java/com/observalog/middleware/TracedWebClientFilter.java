package com.observalog.middleware;

import com.observalog.LogContext;
import org.springframework.web.reactive.function.client.ClientRequest;
import org.springframework.web.reactive.function.client.ClientResponse;
import org.springframework.web.reactive.function.client.ExchangeFilterFunction;
import org.springframework.web.reactive.function.client.ExchangeFunction;
import reactor.core.publisher.Mono;

/**
 * Spring WebClient filter that injects ObservaLog trace context headers into
 * every outgoing reactive HTTP request.
 *
 * <p>Register once when building your WebClient:
 * <pre>{@code
 * @Bean
 * public WebClient webClient() {
 *     return WebClient.builder()
 *         .filter(TracedWebClientFilter.create())
 *         .build();
 * }
 * }</pre>
 *
 * <p>Note: WebClient is typically used in reactive (non-blocking) contexts.
 * This filter reads the trace context from ThreadLocal at subscription time.
 * For fully reactive stacks, consider propagating context via Reactor's
 * {@code Context} and subscribing within a scoped context.
 *
 * <p>Headers injected:
 * <ul>
 *   <li>X-Trace-Id       — continues the trace in the downstream service</li>
 *   <li>X-Parent-Span-Id — this service's span_id, becomes parent_span downstream</li>
 *   <li>X-User-Id        — forwarded user identity</li>
 *   <li>X-Journey-Stage  — forwarded journey stage</li>
 * </ul>
 */
public class TracedWebClientFilter implements ExchangeFilterFunction {

    public static TracedWebClientFilter create() {
        return new TracedWebClientFilter();
    }

    @Override
    public Mono<ClientResponse> filter(ClientRequest request, ExchangeFunction next) {
        LogContext ctx = LogContext.get();

        ClientRequest.Builder builder = ClientRequest.from(request);

        if (!ctx.getTraceId().isEmpty()) {
            builder.header("X-Trace-Id", ctx.getTraceId());
        }
        if (!ctx.getSpanId().isEmpty()) {
            builder.header("X-Parent-Span-Id", ctx.getSpanId());
        }
        if (!ctx.getUserId().isEmpty()) {
            builder.header("X-User-Id", ctx.getUserId());
        }
        if (!ctx.getJourneyStage().isEmpty()) {
            builder.header("X-Journey-Stage", ctx.getJourneyStage());
        }

        return next.exchange(builder.build());
    }
}
