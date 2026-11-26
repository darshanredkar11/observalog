use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, FromRow};
use tracing::warn;

#[derive(Debug, Clone, FromRow)]
pub struct WorkItem {
    pub work_id: i64,
    pub raw_id: String,
    pub raw_text: String,
    pub source_metadata: Value,
    pub received_at: DateTime<Utc>,
    pub payload_id: Option<i64>,
    pub event_ts: Option<DateTime<Utc>>,
    pub attempts: i32,
}

pub async fn claim(pool: &PgPool, gate: &str) -> Result<Option<WorkItem>> {
    let mut tx = pool.begin().await?;
    let item = sqlx::query_as::<_, WorkItem>(
        r#"
        SELECT w.id AS work_id, r.id AS raw_id, r.raw_text, r.source_metadata, r.received_at,
               w.payload_id, w.event_ts, w.attempts
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
        "UPDATE lite_gate_work SET state='done', lease_until=NULL, last_error=NULL, payload_id=COALESCE($2,payload_id), event_ts=COALESCE($3,event_ts) WHERE id=$1"
    )
    .bind(work.work_id)
    .bind(payload_id)
    .bind(work.event_ts)
    .execute(&mut *tx)
    .await?;

    if let Some(gate) = next_gate {
        sqlx::query(
            "INSERT INTO lite_gate_work (raw_id, gate_name, payload_id, event_ts) VALUES ($1,$2,$3,$4) ON CONFLICT (raw_id,gate_name) DO NOTHING"
        )
        .bind(&work.raw_id)
        .bind(gate)
        .bind(payload_id.or(work.payload_id))
        .bind(work.event_ts)
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


#[cfg(test)]
mod gate4_database_recovery_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool(database_url: &str, schema: &str) -> PgPool {
        let schema = schema.to_string();
        PgPoolOptions::new().max_connections(4)
            .after_connect(move |conn, _| {
                let schema = schema.clone();
                Box::pin(async move {
                    sqlx::query(&format!("SET search_path TO {}", schema))
                        .execute(conn).await?;
                    Ok(())
                })
            })
            .connect(database_url).await.expect("connect isolated test pool")
    }

    #[tokio::test]
    async fn gate4_persistent_queue_recovery_and_single_leader() {
        let Ok(database_url) = env::var("GATE4_DATABASE_URL") else {
            eprintln!("Gate 4 integration skipped: GATE4_DATABASE_URL is not set");
            return;
        };
        let admin = PgPoolOptions::new().max_connections(2).connect(&database_url)
            .await.expect("connect to PostgreSQL");
        let schema = format!("gate4_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {}", schema)).execute(&admin).await.expect("create test schema");
        let pool1 = test_pool(&database_url, &schema).await;
        sqlx::raw_sql(r#"
            CREATE TABLE lite_raw_records (
                id TEXT PRIMARY KEY, raw_text TEXT NOT NULL,
                source_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
                received_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE TABLE lite_gate_work (
                id BIGSERIAL PRIMARY KEY, raw_id TEXT NOT NULL REFERENCES lite_raw_records(id) ON DELETE CASCADE,
                gate_name TEXT NOT NULL, payload_id BIGINT, event_ts TIMESTAMPTZ,
                state TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
                available_at TIMESTAMPTZ NOT NULL DEFAULT now(), lease_until TIMESTAMPTZ,
                last_error TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE(raw_id,gate_name), CHECK (state IN ('pending','leased','done','dead'))
            );
        "#).execute(&pool1).await.expect("create queue schema");
        sqlx::query("INSERT INTO lite_raw_records (id,raw_text) VALUES ('raw-1','{}')")
            .execute(&pool1).await.expect("seed raw event");
        sqlx::query("INSERT INTO lite_gate_work (raw_id,gate_name) VALUES ('raw-1','normalize')")
            .execute(&pool1).await.expect("seed work");

        let first = claim(&pool1, "normalize").await.expect("claim").expect("pending work");
        assert_eq!(first.attempts, 0, "claim returns attempts before increment");
        let count: i32 = sqlx::query_scalar("SELECT attempts FROM lite_gate_work WHERE id=$1")
            .bind(first.work_id).fetch_one(&pool1).await.expect("check attempt count");
        assert_eq!(count, 1);
        sqlx::query("UPDATE lite_gate_work SET lease_until=now()-INTERVAL '1 second' WHERE id=$1")
            .bind(first.work_id).execute(&pool1).await.expect("expire lease");
        pool1.close().await;

        // New pool represents a restarted worker/process; queue state survives.
        let pool2 = test_pool(&database_url, &schema).await;
        let recovered = claim(&pool2, "normalize").await.expect("reclaim lease").expect("expired lease");
        assert_eq!(recovered.work_id, first.work_id);
        let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM lite_gate_work WHERE id=$1")
            .bind(recovered.work_id).fetch_one(&pool2).await.expect("retry count");
        assert_eq!(attempts, 2);

        complete(&pool2, &recovered, Some("quality"), Some(42)).await.expect("complete work");
        complete(&pool2, &recovered, Some("quality"), Some(42)).await.expect("idempotent completion");
        let downstream: i64 = sqlx::query_scalar("SELECT count(*) FROM lite_gate_work WHERE raw_id='raw-1' AND gate_name='quality'")
            .fetch_one(&pool2).await.expect("count downstream");
        assert_eq!(downstream, 1);
        let state: String = sqlx::query_scalar("SELECT state FROM lite_gate_work WHERE id=$1")
            .bind(recovered.work_id).fetch_one(&pool2).await.expect("completion state");
        assert_eq!(state, "done");

        // PostgreSQL session advisory locks enforce a single correlation leader.
        let lock_key: i64 = 0x4F42534C49544533;
        let mut leader = pool2.acquire().await.expect("leader session");
        let got_lock: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(lock_key)
            .fetch_one(&mut *leader).await.expect("leader lock");
        assert!(got_lock);
        let mut contender = pool2.acquire().await.expect("contender session");
        let second: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(lock_key)
            .fetch_one(&mut *contender).await.expect("contender lock");
        assert!(!second);
        sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key).execute(&mut *leader)
            .await.expect("unlock leader");
        let after_release: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(lock_key)
            .fetch_one(&mut *contender).await.expect("lock after leader release");
        assert!(after_release);
        sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key).execute(&mut *contender)
            .await.expect("unlock contender");

        pool2.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", schema)).execute(&admin).await.expect("drop test schema");
        admin.close().await;
    }
}
