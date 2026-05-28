import { getContext } from '../context';
import { PART_A_SCHEMA_VERSION } from '../wire-contract';

export interface KafkaProducerMessage {
    topic:   string;
    key?:    string | Buffer | null;
    value:   string | Buffer | null;
    headers: Record<string, string>;
}

// Creates a Kafka message with trace context headers from the current AsyncLocalStorage context.
// Downstream consumers call kafkaConsumerMiddleware to continue the trace.
export function newTracedMessage(
    topic: string,
    key: string | Buffer | null,
    value: string | Buffer | null,
): KafkaProducerMessage {
    const ctx = getContext();
    return {
        topic,
        key,
        value,
        headers: {
            'X-Trace-Id':       ctx?.traceId      ?? '',
            'X-Span-Id':        ctx?.spanId        ?? '',
            'X-Parent-Span-Id': ctx?.parentSpan    ?? '',
            'X-User-Id':        ctx?.userId        ?? '',
            'X-Journey-Stage':  ctx?.journeyStage  ?? '',
            'X-Schema-Version': String(PART_A_SCHEMA_VERSION),
        },
    };
}
