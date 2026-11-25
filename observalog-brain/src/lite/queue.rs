use anyhow::Result;
use serde_json::Value;
use sqlx::{PgPool, FromRow};
use tracing::warn;

#[derive(Debug, Clone, FromRow)]
pub struct WorkItem {
    pub work_id: i64,
    pub raw_id: String,
    pub raw_text: String,
    pub source_metadata: Value,
    pub payload_id: Option<i64>,
    pub attempts: i32,
}

pub async fn claim(pool: &PgPool, gate: &str) -> Result<Option<WorkItem>> {
    let mut tx = pool.begin().await?;
    let item = sqlx::query_as::<_, WorkItem>(
        r#"
        SELECT w.id AS work_id, r.id AS raw_id, r.raw_text, r.source_metadata,
               w.payload_id, w.attempts
        FROM lite_gate_work w
        JOIN lite_raw_records r ON r.id = w.raw_id
        WHERE w.gate_name = $1
          AND ((w.state = 'pending' AND w.available_at <= now())
            OR (w.state = 'leased' AND w.lease_until < now()))
        ORDER BY w.id
        FOR UPDATE OF w SKIP LOCKED
        LIMIT 1
        "#
    )
    .bind(gate)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(ref work) = item {
        sqlx::query(
            "UPDATE lite_gate_work SET state='leased', attempts=attempts+1, lease_until=now()+INTERVAL '5 minutes' WHERE id=$1"
        )
        .bind(work.work_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(item)
}

pub async fn complete(pool: &PgPool, work: &WorkItem, next_gate: Option<&str>, payload_id: Option<i64>) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE lite_gate_work SET state='done', lease_until=NULL, last_error=NULL, payload_id=COALESCE($2,payload_id) WHERE id=$1"
    )
    .bind(work.work_id)
    .bind(payload_id)
    .execute(&mut *tx)
    .await?;

    if let Some(gate) = next_gate {
        sqlx::query(
            "INSERT INTO lite_gate_work (raw_id, gate_name, payload_id) VALUES ($1,$2,$3) ON CONFLICT (raw_id,gate_name) DO NOTHING"
        )
        .bind(&work.raw_id)
        .bind(gate)
        .bind(payload_id.or(work.payload_id))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn fail(pool: &PgPool, work: &WorkItem, error: &anyhow::Error) {
    let message = error.to_string();
    let delay_seconds = (work.attempts.max(1) * 2).min(60);
    let result = sqlx::query(
        r#"
        UPDATE lite_gate_work
        SET state = CASE WHEN attempts >= 10 THEN 'dead' ELSE 'pending' END,
            available_at = now() + ($2 * INTERVAL '1 second'),
            lease_until = NULL,
            last_error = $3
        WHERE id = $1
        "#
    )
    .bind(work.work_id)
    .bind(delay_seconds)
    .bind(&message)
    .execute(pool)
    .await;
    if let Err(db_error) = result {
        warn!(work_id=work.work_id, error=%db_error, "could not reschedule failed gate work");
    }
}
