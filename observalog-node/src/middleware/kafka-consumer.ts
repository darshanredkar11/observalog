import { runWithContextAsync } from '../context';

export interface KafkaMessage {
    headers?: Record<string, Buffer | string | undefined>;
}

// Wraps a KafkaJS message handler with trace context extracted from message headers.
//
// Usage:
//   consumer.run({
//     eachMessage: kafkaConsumerMiddleware(async ({ message }) => {
//         log.info('provider.message.received', 'Message received', { topic });
//     }),
//   });
export function kafkaConsumerMiddleware<T extends { message: KafkaMessage }>(
    handler: (payload: T) => Promise<void>,
): (payload: T) => Promise<void> {
    return (payload: T): Promise<void> => {
        const h = (name: string): string => {
            const v = payload.message.headers?.[name];
            if (!v) return '';
            return Buffer.isBuffer(v) ? v.toString('utf8') : String(v);
        };

        const ctx = {
            traceId:      h('X-Trace-Id'),
            spanId:       h('X-Span-Id'),
            parentSpan:   h('X-Parent-Span-Id'),
            userId:       h('X-User-Id'),
            journeyStage: h('X-Journey-Stage'),
            seq:          { value: 0 },
        };

        return runWithContextAsync(ctx, () => handler(payload));
    };
}
