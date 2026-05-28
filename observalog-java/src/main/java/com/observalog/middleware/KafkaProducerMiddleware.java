package com.observalog.middleware;

import com.observalog.LogContext;
import com.observalog.WireContract;
import org.apache.kafka.clients.producer.ProducerRecord;
import org.apache.kafka.common.header.internals.RecordHeader;

import java.nio.charset.StandardCharsets;

// Injects trace context from LogContext into outgoing Kafka record headers.
// Downstream consumers call KafkaConsumerMiddleware.wrap() to continue the trace.
public final class KafkaProducerMiddleware {

    private KafkaProducerMiddleware() {}

    public static <K, V> ProducerRecord<K, V> traced(String topic, K key, V value) {
        LogContext ctx = LogContext.get();
        ProducerRecord<K, V> record = new ProducerRecord<>(topic, key, value);

        addHeader(record, "X-Trace-Id",       ctx.getTraceId());
        addHeader(record, "X-Span-Id",        ctx.getSpanId());
        addHeader(record, "X-Parent-Span-Id", ctx.getParentSpan());
        addHeader(record, "X-User-Id",        ctx.getUserId());
        addHeader(record, "X-Journey-Stage",  ctx.getJourneyStage());
        addHeader(record, "X-Schema-Version", String.valueOf(WireContract.PART_A_SCHEMA_VERSION));

        return record;
    }

    private static <K, V> void addHeader(ProducerRecord<K, V> record, String key, String value) {
        if (value != null && !value.isEmpty()) {
            record.headers().add(new RecordHeader(key, value.getBytes(StandardCharsets.UTF_8)));
        }
    }
}
