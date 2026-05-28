package com.observalog.middleware;

import com.observalog.LogContext;
import org.apache.kafka.clients.consumer.ConsumerRecord;

import java.nio.charset.StandardCharsets;

// Extracts trace context from Kafka record headers and populates LogContext.
// Call wrap() before processing each record; call LogContext.clear() in finally.
//
// Usage:
//   for (ConsumerRecord<?, ?> record : records) {
//       KafkaConsumerMiddleware.wrap(record);
//       try {
//           processRecord(record);
//       } finally {
//           LogContext.clear();
//       }
//   }
public final class KafkaConsumerMiddleware {

    private KafkaConsumerMiddleware() {}

    public static void wrap(ConsumerRecord<?, ?> record) {
        LogContext ctx = new LogContext();
        ctx.setTraceId(header(record, "X-Trace-Id"));
        ctx.setSpanId(header(record, "X-Span-Id"));
        ctx.setParentSpan(header(record, "X-Parent-Span-Id"));
        ctx.setUserId(header(record, "X-User-Id"));
        ctx.setJourneyStage(header(record, "X-Journey-Stage"));
        LogContext.set(ctx);
    }

    private static String header(ConsumerRecord<?, ?> record, String key) {
        org.apache.kafka.common.header.Header h = record.headers().lastHeader(key);
        return h != null ? new String(h.value(), StandardCharsets.UTF_8) : "";
    }
}
