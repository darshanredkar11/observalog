use super::parser::parse_entry;
use super::writer::write_entry;
use anyhow::Result;
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    message::Message as KafkaMessage,
    ClientConfig,
};
use sqlx::PgPool;
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

pub struct KafkaConfig {
    pub brokers: String,
    pub topic: String,
    pub group_id: String,
}

impl KafkaConfig {
    pub fn from_env() -> Result<Self> {
        Ok(KafkaConfig {
            brokers: std::env::var("KAFKA_BROKERS")
                .unwrap_or_else(|_| "localhost:9092".to_string()),
            topic: std::env::var("KAFKA_TOPIC")
                .unwrap_or_else(|_| "observalog".to_string()),
            group_id: std::env::var("KAFKA_GROUP_ID")
                .unwrap_or_else(|_| "observalog-brain".to_string()),
        })
    }
}

/// Start the Kafka consumer loop.
/// Reads from Kafka, parses two-line wire format, writes to TimescaleDB.
/// Commits offsets only after successful DB write (Decision 9 / Gap 9).
pub async fn run(pool: PgPool, cfg: KafkaConfig) -> Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &cfg.brokers)
        .set("group.id", &cfg.group_id)
        .set("enable.auto.commit", "false") // manual commit after DB write
        .set("auto.offset.reset", "earliest")
        .set("session.timeout.ms", "30000")
        .create()?;

    consumer.subscribe(&[&cfg.topic])?;
    info!(topic = %cfg.topic, brokers = %cfg.brokers, "Kafka consumer started");

    let mut stream = consumer.stream();

    while let Some(result) = stream.next().await {
        match result {
            Err(e) => {
                error!("Kafka error: {}", e);
            }
            Ok(msg) => {
                let payload = match msg.payload_view::<str>() {
                    Some(Ok(s)) => s,
                    _ => {
                        warn!("Non-UTF8 Kafka message, skipping");
                        continue;
                    }
                };

                // Each Kafka message contains two NDJSON lines: Part A + Part B.
                let mut lines = payload.lines();
                let line_a = match lines.next() {
                    Some(l) => l,
                    None => {
                        warn!("Empty Kafka message, skipping");
                        continue;
                    }
                };
                let line_b = match lines.next() {
                    Some(l) => l,
                    None => {
                        warn!("Kafka message missing Part B line, skipping");
                        continue;
                    }
                };

                match parse_entry(line_a, line_b) {
                    Err(e) => {
                        warn!(error = %e, "parse failure, skipping message");
                    }
                    Ok(entry) => {
                        if let Err(e) = write_entry(&pool, &entry).await {
                            error!(trace_id = %entry.trace_id, error = %e, "DB write failure");
                        } else {
                            // Commit only after successful write (Gap 9: resume from offset on restart).
                            if let Err(e) = consumer.commit_message(&msg, rdkafka::consumer::CommitMode::Async) {
                                warn!("offset commit failed: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
