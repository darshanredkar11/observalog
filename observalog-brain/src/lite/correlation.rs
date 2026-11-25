use super::{
    model::CanonicalEvent,
    queue::{self, WorkItem},
};
use anyhow::{Context, Result};
use chrono::Duration;
use serde_json::{json, Value};
use sqlx::{pool::PoolConnection, FromRow, PgPool, Postgres};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration as StdDuration, Instant},
};
use tokio::time::{interval, sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

const LEADER_LOCK_KEY: i64 = 0x4F42534C49544533;
const WINDOW: Duration = Duration::hours(2);
const RESIDENT_TTL: StdDuration = StdDuration::from_secs(2 * 60 * 60);
const MAX_RESIDENT_EVENTS: usize = 20_000;
const MAX_EVENTS_PER_IDENTITY: usize = 256;
const MAX_CANDIDATES_PER_EVENT: usize = 64;
const MAX_ADJACENCY_EDGES_PER_EVENT: usize = 64;
const RECOVERY_LIMIT: i64 = 20_000;
const RECOVERY_EDGE_LIMIT: i64 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IdentityKind {
    Trace,
    Request,
    BusinessEntity,
    Message,
    SpanReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IdentityKey {
    kind: IdentityKind,
    value: String,
}

#[derive(Debug)]
struct EventNode {
    event: CanonicalEvent,
    inserted_at: Instant,
    keys: Vec<IdentityKey>,
}

#[derive(Debug, Clone)]
struct Relationship {
    other_id: i64,
    relation_type: &'static str,
    confidence: f64,
    evidence: Value,
}

#[derive(Debug, Default)]
struct CorrelationPlan {
    relationships: Vec<Relationship>,
    trace_conflicts: Vec<Value>,
}

#[derive(Debug, Default)]
struct CorrelationEngine {
    nodes: HashMap<i64, EventNode>,
    identity_index: HashMap<IdentityKey, VecDeque<i64>>,
    adjacency: HashMap<i64, Vec<(i64, String)>>,
    insertion_order: VecDeque<i64>,
}

#[derive(Debug, FromRow)]
struct PayloadRow {
    canonical: Value,
    quality: Value,
}

#[derive(Debug, FromRow)]
struct RecoveryRow {
    id: i64,
    canonical: Value,
    quality: Value,
}

#[derive(Debug, FromRow)]
struct EdgeRow {
    event_a: i64,
    event_b: i64,
    relation_type: String,
}

pub(super) fn usable_trace_id(value: &str) -> bool {
    !value.starts_with("sys_") && value.trim().len() >= 8
}

fn eligible(event: &CanonicalEvent, field: &str, value: Option<&String>) -> bool {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return false;
    };
    if field == "trace_id" && !usable_trace_id(value) {
        return false;
    }
    match event.quality["correlation_eligibility"][field].as_str() {
        Some("eligible") => true,
        Some(_) => false,
        None if field == "trace_id" => usable_trace_id(value),
        None => true,
    }
}

fn tenant_key(event: &CanonicalEvent) -> Option<&str> {
    event.tenant_id.as_deref().filter(|value| !value.trim().is_empty())
}

fn tenant_conflict(left: &CanonicalEvent, right: &CanonicalEvent) -> bool {
    tenant_key(left) != tenant_key(right)
}

fn flow_scopes(event: &CanonicalEvent) -> Vec<String> {
    let mut scopes = Vec::with_capacity(2);
    if eligible(event, "trace_id", event.trace_id.as_ref()) {
        if let Some(value) = event.trace_id.as_ref() {
            scopes.push(format!("trace:{value}"));
        }
    }
    if eligible(event, "request_id", event.request_id.as_ref()) {
        if let Some(value) = event.request_id.as_ref() {
            scopes.push(format!("request:{value}"));
        }
    }
    scopes
}

fn push_key(keys: &mut Vec<IdentityKey>, kind: IdentityKind, value: Option<&String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        keys.push(IdentityKey { kind, value: value.clone() });
    }
}

fn identity_keys(event: &CanonicalEvent) -> Vec<IdentityKey> {
    let mut keys = Vec::with_capacity(8);
    if eligible(event, "trace_id", event.trace_id.as_ref()) {
        push_key(&mut keys, IdentityKind::Trace, event.trace_id.as_ref());
    }
    if eligible(event, "request_id", event.request_id.as_ref()) {
        push_key(&mut keys, IdentityKind::Request, event.request_id.as_ref());
    }
    if eligible(event, "business_entity_id", event.business_entity_id.as_ref()) {
        push_key(&mut keys, IdentityKind::BusinessEntity, event.business_entity_id.as_ref());
    }
    if eligible(event, "message_id", event.message_id.as_ref()) {
        push_key(&mut keys, IdentityKind::Message, event.message_id.as_ref());
    }
    for scope in flow_scopes(event) {
        if eligible(event, "span_id", event.span_id.as_ref()) {
            if let Some(value) = event.span_id.as_ref() {
                keys.push(IdentityKey {
                    kind: IdentityKind::SpanReference,
                    value: format!("{scope}:{value}"),
                });
            }
        }
        if eligible(event, "parent_span_id", event.parent_span_id.as_ref()) {
            if let Some(value) = event.parent_span_id.as_ref() {
                keys.push(IdentityKey {
                    kind: IdentityKind::SpanReference,
                    value: format!("{scope}:{value}"),
                });
            }
        }
    }
    keys.sort_by(|left, right| {
        (left.kind as u8, &left.value).cmp(&(right.kind as u8, &right.value))
    });
    keys.dedup();
    keys
}

impl CorrelationEngine {
    fn candidates(&self, event: &CanonicalEvent) -> Vec<i64> {
        let mut unique = HashSet::new();
        for key in identity_keys(event) {
            if let Some(ids) = self.identity_index.get(&key) {
                unique.extend(ids.iter().copied());
            }
        }
        let mut candidates: Vec<i64> = unique
            .into_iter()
            .filter(|id| self.nodes.contains_key(id))
            .collect();
        candidates.sort_by_key(|id| {
            let other = &self.nodes[id].event;
            let distance = event
                .event_ts
                .signed_duration_since(other.event_ts)
                .num_milliseconds()
                .unsigned_abs();
            (distance, *id)
        });
        candidates.truncate(MAX_CANDIDATES_PER_EVENT);
        candidates
    }

    fn plan(&self, current_id: i64, current: &CanonicalEvent) -> CorrelationPlan {
        let mut plan = CorrelationPlan::default();
        let mut seen = HashSet::new();
        for other_id in self.candidates(current) {
            if other_id == current_id {
                continue;
            }
            let Some(other_node) = self.nodes.get(&other_id) else { continue; };
            let other = &other_node.event;
            let delta = current.event_ts.signed_duration_since(other.event_ts);
            if delta > WINDOW || delta < -WINDOW {
                continue;
            }

            let tenant_mismatch = tenant_conflict(current, other);
            let request_eligible = eligible(current, "request_id", current.request_id.as_ref())
                && eligible(other, "request_id", other.request_id.as_ref());
            let entity_eligible = eligible(current, "business_entity_id", current.business_entity_id.as_ref())
                && eligible(other, "business_entity_id", other.business_entity_id.as_ref());
            let message_eligible = eligible(current, "message_id", current.message_id.as_ref())
                && eligible(other, "message_id", other.message_id.as_ref());
            let request_conflict = request_eligible && current.request_id != other.request_id;
            let entity_conflict = entity_eligible && current.business_entity_id != other.business_entity_id;
            let same_trace = eligible(current, "trace_id", current.trace_id.as_ref())
                && eligible(other, "trace_id", other.trace_id.as_ref())
                && current.trace_id == other.trace_id;

            if same_trace && (request_conflict || entity_conflict || tenant_mismatch) {
                plan.trace_conflicts.push(json!({
                    "other_event_id": other_id,
                    "trace_id": current.trace_id,
                    "request_conflict": request_conflict,
                    "entity_conflict": entity_conflict,
                    "tenant_conflict": tenant_mismatch,
                    "current_request_id": current.request_id,
                    "other_request_id": other.request_id,
                    "current_entity_id": current.business_entity_id,
                    "other_entity_id": other.business_entity_id,
                    "current_tenant_id": current.tenant_id,
                    "other_tenant_id": other.tenant_id
                }));
            }

            if !tenant_mismatch && same_trace && !request_conflict && !entity_conflict
                && seen.insert((other_id, "same_trace"))
            {
                plan.relationships.push(Relationship {
                    other_id, relation_type: "same_trace", confidence: 0.99,
                    evidence: json!({"trace_id": current.trace_id}),
                });
            }
            if !tenant_mismatch && request_eligible && current.request_id == other.request_id
                && seen.insert((other_id, "same_request"))
            {
                plan.relationships.push(Relationship {
                    other_id, relation_type: "same_request", confidence: 0.98,
                    evidence: json!({"request_id": current.request_id}),
                });
            }
            if !tenant_mismatch && entity_eligible && current.business_entity_id == other.business_entity_id
                && seen.insert((other_id, "same_business_entity"))
            {
                plan.relationships.push(Relationship {
                    other_id, relation_type: "same_business_entity", confidence: 0.94,
                    evidence: json!({"business_entity_id": current.business_entity_id}),
                });
            }
            if !tenant_mismatch && message_eligible && current.message_id == other.message_id
                && seen.insert((other_id, "same_message"))
            {
                plan.relationships.push(Relationship {
                    other_id, relation_type: "same_message", confidence: 0.97,
                    evidence: json!({"message_id": current.message_id}),
                });
            }

            let same_request = request_eligible && current.request_id == other.request_id;
            let same_flow = !tenant_mismatch
                && ((same_trace && !request_conflict && !entity_conflict) || same_request);
            if same_flow {
                if eligible(current, "parent_span_id", current.parent_span_id.as_ref())
                    && eligible(other, "span_id", other.span_id.as_ref())
                    && current.parent_span_id == other.span_id
                    && seen.insert((other_id, "parent_child"))
                {
                    plan.relationships.push(Relationship {
                        other_id, relation_type: "parent_child", confidence: 0.995,
                        evidence: json!({
                            "parent_event_id": other_id,
                            "child_event_id": current_id,
                            "parent_span_id": other.span_id,
                            "child_span_id": current.span_id
                        }),
                    });
                } else if eligible(other, "parent_span_id", other.parent_span_id.as_ref())
                    && eligible(current, "span_id", current.span_id.as_ref())
                    && other.parent_span_id == current.span_id
                    && seen.insert((other_id, "parent_child"))
                {
                    plan.relationships.push(Relationship {
                        other_id, relation_type: "parent_child", confidence: 0.995,
                        evidence: json!({
                            "parent_event_id": current_id,
                            "child_event_id": other_id,
                            "parent_span_id": current.span_id,
                            "child_span_id": other.span_id
                        }),
                    });
                }
            }
        }
        plan
    }

    fn insert(&mut self, id: i64, event: CanonicalEvent) {
        if self.nodes.contains_key(&id) { return; }
        let keys = identity_keys(&event);
        self.nodes.insert(id, EventNode {
            event, inserted_at: Instant::now(), keys: keys.clone(),
        });
        self.insertion_order.push_back(id);
        for key in keys {
            let bucket = self.identity_index.entry(key).or_default();
            bucket.push_back(id);
            while bucket.len() > MAX_EVENTS_PER_IDENTITY { bucket.pop_front(); }
        }
        self.evict();
    }

    fn evict(&mut self) {
        while let Some(oldest_id) = self.insertion_order.front().copied() {
            let expired = self.nodes.get(&oldest_id)
                .is_some_and(|node| node.inserted_at.elapsed() >= RESIDENT_TTL);
            let over_capacity = self.nodes.len() > MAX_RESIDENT_EVENTS;
            if !expired && !over_capacity { break; }
            self.insertion_order.pop_front();
            self.remove(oldest_id);
        }
    }

    fn remove(&mut self, id: i64) {
        let Some(node) = self.nodes.remove(&id) else { return; };
        for key in node.keys {
            let empty = if let Some(bucket) = self.identity_index.get_mut(&key) {
                bucket.retain(|candidate| *candidate != id);
                bucket.is_empty()
            } else { false };
            if empty { self.identity_index.remove(&key); }
        }
        if let Some(neighbours) = self.adjacency.remove(&id) {
            for (other, relation) in neighbours {
                let empty = if let Some(edges) = self.adjacency.get_mut(&other) {
                    edges.retain(|(candidate, kind)| *candidate != id || *kind != relation);
                    edges.is_empty()
                } else { false };
                if empty { self.adjacency.remove(&other); }
            }
        }
    }

    fn add_adjacency(&mut self, left_id: i64, right_id: i64, relation_type: &str) {
        let left_has = self.adjacency.get(&left_id).is_some_and(|edges| {
            edges.iter().any(|(other, kind)| *other == right_id && kind == relation_type)
        });
        let right_has = self.adjacency.get(&right_id).is_some_and(|edges| {
            edges.iter().any(|(other, kind)| *other == left_id && kind == relation_type)
        });
        let left_room = self.adjacency.get(&left_id)
            .is_none_or(|edges| edges.len() < MAX_ADJACENCY_EDGES_PER_EVENT);
        let right_room = self.adjacency.get(&right_id)
            .is_none_or(|edges| edges.len() < MAX_ADJACENCY_EDGES_PER_EVENT);

        if !left_has && left_room {
            self.adjacency.entry(left_id).or_default()
                .push((right_id, relation_type.to_string()));
        }
        if !right_has && right_room {
            self.adjacency.entry(right_id).or_default()
                .push((left_id, relation_type.to_string()));
        }
    }

    fn apply(&mut self, id: i64, event: CanonicalEvent, relationships: &[Relationship]) {
        self.insert(id, event);
        for relationship in relationships {
            self.add_adjacency(id, relationship.other_id, relationship.relation_type);
        }
    }

    fn restore_edge(&mut self, edge: EdgeRow) {
        if !self.nodes.contains_key(&edge.event_a) || !self.nodes.contains_key(&edge.event_b) {
            return;
        }
        self.add_adjacency(edge.event_a, edge.event_b, &edge.relation_type);
    }
}

async fn recover(pool: &PgPool, engine: &mut CorrelationEngine) -> Result<()> {
    let mut rows: Vec<RecoveryRow> = sqlx::query_as(
        r#"
        SELECT p.id, p.canonical, p.quality
        FROM lite_event_payload p
        JOIN lite_event_index i ON i.id = p.id
        JOIN lite_gate_work q ON q.raw_id = p.raw_id AND q.gate_name = 'quality' AND q.state = 'done'
        WHERE i.observed_at >= now() - INTERVAL '2 hours'
        ORDER BY i.observed_at DESC, p.id DESC
        LIMIT $1
        "#
    )
    .bind(RECOVERY_LIMIT)
    .fetch_all(pool).await?;
    rows.reverse();
    for row in rows {
        let mut event: CanonicalEvent = serde_json::from_value(row.canonical)?;
        event.quality = row.quality;
        engine.insert(row.id, event);
    }

    let edges: Vec<EdgeRow> = sqlx::query_as(
        r#"
        SELECT event_a, event_b, relation_type FROM lite_event_links
        WHERE created_at >= now() - INTERVAL '2 hours'
        ORDER BY created_at DESC LIMIT $1
        "#
    )
    .bind(RECOVERY_EDGE_LIMIT)
    .fetch_all(pool).await?;
    for edge in edges { engine.restore_edge(edge); }

    info!(
        recovered_events = engine.nodes.len(),
        recovered_identity_buckets = engine.identity_index.len(),
        "correlation memory rebuilt from durable Timescale journal"
    );
    Ok(())
}

async fn correlate_one(pool: &PgPool, engine: &mut CorrelationEngine, work: &WorkItem) -> Result<()> {
    let payload_id = work.payload_id.context("correlation work has no payload_id")?;
    let payload: PayloadRow = sqlx::query_as(
        "SELECT canonical, quality FROM lite_event_payload WHERE id = $1"
    )
    .bind(payload_id).fetch_one(pool).await?;
    let mut event: CanonicalEvent = serde_json::from_value(payload.canonical)?;
    event.quality = payload.quality;
    if let Some(work_timestamp) = work.event_ts {
        if work_timestamp != event.event_ts {
            anyhow::bail!("correlation work timestamp does not match canonical payload");
        }
    }

    // Resolve candidates in memory. Apply the mutation only after the durable
    // links, completion cursor and downstream work all commit.
    let plan = engine.plan(payload_id, &event);
    let mut tx = pool.begin().await?;
    if !plan.trace_conflicts.is_empty() {
        let evidence = json!({"trace_id": event.trace_id, "conflicts": plan.trace_conflicts});
        sqlx::query(
            "INSERT INTO lite_signals (event_id,event_ts,signal_type,severity,evidence) VALUES ($1,$2,'trace_identity_conflict','high',$3) ON CONFLICT (event_id,signal_type) DO NOTHING"
        )
        .bind(payload_id).bind(event.event_ts).bind(evidence)
        .execute(&mut *tx).await?;
    }

    for relationship in &plan.relationships {
        let (event_a, event_b) = if payload_id < relationship.other_id {
            (payload_id, relationship.other_id)
        } else { (relationship.other_id, payload_id) };
        sqlx::query(
            "INSERT INTO lite_event_links (event_a,event_b,relation_type,confidence,evidence) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
        )
        .bind(event_a).bind(event_b).bind(relationship.relation_type)
        .bind(relationship.confidence).bind(&relationship.evidence)
        .execute(&mut *tx).await?;
    }

    sqlx::query(
        "UPDATE lite_gate_work SET state='done', lease_until=NULL, payload_id=$2, event_ts=$3, last_error=NULL WHERE id=$1"
    )
    .bind(work.work_id).bind(payload_id).bind(event.event_ts)
    .execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO lite_gate_work (raw_id,gate_name,payload_id,event_ts) VALUES ($1,'detect',$2,$3) ON CONFLICT (raw_id,gate_name) DO NOTHING"
    )
    .bind(&work.raw_id).bind(payload_id).bind(event.event_ts)
    .execute(&mut *tx).await?;
    tx.commit().await?;

    engine.apply(payload_id, event, &plan.relationships);
    debug!(work_id=work.work_id, event_id=payload_id,
        relationship_count=plan.relationships.len(),
        "event correlated from bounded memory state");
    Ok(())
}

async fn acquire_leadership(pool: &PgPool) -> Result<Option<PoolConnection<Postgres>>> {
    let mut connection = pool.acquire().await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(LEADER_LOCK_KEY).fetch_one(&mut *connection).await?;
    if acquired { Ok(Some(connection)) } else { Ok(None) }
}

async fn run_leader(
    pool: &PgPool,
    connection: &mut PoolConnection<Postgres>,
    engine: &mut CorrelationEngine,
) -> Result<()> {
    let mut poll = interval(TokioDuration::from_millis(200));
    let mut heartbeat = interval(TokioDuration::from_secs(1));
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let _: i32 = sqlx::query_scalar("SELECT 1")
                    .fetch_one(&mut **connection)
                    .await
                    .context("correlation leader connection was lost")?;
            }
            _ = poll.tick() => {
                match queue::claim(pool, "correlate").await {
                    Ok(Some(work)) => {
                        if let Err(err) = correlate_one(pool, engine, &work).await {
                            error!(work_id=work.work_id, attempts=work.attempts + 1,
                                error=%err, "correlation gate processing failed");
                            queue::fail(pool, &work, &err).await;
                        }
                    }
                    Ok(None) => {}
                    Err(err) => warn!(error=%err, "correlation queue read failed"),
                }
            }
        }
    }
}

pub async fn run(pool: PgPool) {
    loop {
        match acquire_leadership(&pool).await {
            Ok(Some(mut connection)) => {
                let mut engine = CorrelationEngine::default();
                match recover(&pool, &mut engine).await {
                    Ok(()) => {
                        info!("single-owner in-memory correlation worker acquired leadership");
                        if let Err(err) = run_leader(&pool, &mut connection, &mut engine).await {
                            warn!(error=%err, "correlation leader stopped; rebuilding state");
                        }
                    }
                    Err(err) => error!(error=%err, "could not recover correlation memory from Timescale"),
                }
                // Session advisory locks survive returning a healthy connection to
                // SQLx's pool. Always release explicitly before yielding it.
                let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
                    .bind(LEADER_LOCK_KEY)
                    .execute(&mut *connection)
                    .await;
                drop(connection);
            }
            Ok(None) => sleep(TokioDuration::from_secs(1)).await,
            Err(err) => {
                error!(error=%err, "could not acquire correlation leadership");
                sleep(TokioDuration::from_secs(1)).await;
            }
        }
    }
}

#[cfg(test)]
mod gate3_correlation_tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde_json::json;

    fn event(
        seconds: i64, trace: Option<&str>, request: Option<&str>, entity: Option<&str>,
        tenant: Option<&str>, span: Option<&str>, parent: Option<&str>, message_id: Option<&str>,
    ) -> CanonicalEvent {
        let timestamp = DateTime::parse_from_rfc3339("2026-11-25T12:00:00Z")
            .unwrap().with_timezone(&Utc) + Duration::seconds(seconds);
        let mut eligibility = serde_json::Map::new();
        for field in ["trace_id", "request_id", "business_entity_id", "message_id", "span_id", "parent_span_id"] {
            eligibility.insert(field.to_string(), Value::String("eligible".to_string()));
        }
        CanonicalEvent {
            source_name: Some("test".into()),
            source_event_id: Some(format!("event-{seconds}")),
            event_ts: timestamp.clone(),
            observed_at: timestamp,
            event_ts_inferred: false,
            service: Some("svc".into()),
            instance: Some("instance-1".into()),
            version: Some("1".into()),
            environment: Some("test".into()),
            region: None,
            tenant_id: tenant.map(str::to_string),
            severity: Some("INFO".into()),
            trace_id: trace.map(str::to_string),
            span_id: span.map(str::to_string),
            parent_span_id: parent.map(str::to_string),
            request_id: request.map(str::to_string),
            business_entity_id: entity.map(str::to_string),
            message_id: message_id.map(str::to_string),
            dependency_name: None,
            event_name: None,
            message: Some("message".into()),
            outcome: None,
            duration_ms: None,
            error_kind: None,
            error_code: None,
            error_message: None,
            attributes: json!({}),
            provenance: json!({}),
            quality: json!({"correlation_eligibility": eligibility}),
        }
    }

    #[test]
    fn identity_families_do_not_collide_on_equal_raw_values() {
        let mut engine = CorrelationEngine::default();
        engine.insert(1, event(0, None, Some("same-value"), None, None, None, None, None));
        let plan = engine.plan(2, &event(1, None, None, Some("same-value"), None, None, None, None));
        assert!(plan.relationships.is_empty());
    }

    #[test]
    fn tenant_mismatch_never_creates_a_relationship() {
        let mut engine = CorrelationEngine::default();
        engine.insert(1, event(0, Some("trace-12345678"), Some("req-1"), None, Some("tenant-a"), None, None, None));
        let plan = engine.plan(2, &event(1, Some("trace-12345678"), Some("req-1"), None, Some("tenant-b"), None, None, None));
        assert!(plan.relationships.is_empty());
        assert_eq!(plan.trace_conflicts.len(), 1);
    }

    #[test]
    fn temporal_proximity_alone_does_not_create_a_relationship() {
        let mut engine = CorrelationEngine::default();
        engine.insert(1, event(0, None, None, None, None, None, None, None));
        assert!(engine.plan(2, &event(1, None, None, None, None, None, None, None)).relationships.is_empty());
    }

    #[test]
    fn late_parent_arrival_resolves_explicit_span_reference() {
        let mut engine = CorrelationEngine::default();
        engine.insert(2, event(2, Some("trace-12345678"), Some("req-1"), Some("order-1"),
            None, Some("child-span"), Some("parent-span"), None));
        let plan = engine.plan(1, &event(1, Some("trace-12345678"), Some("req-1"), Some("order-1"),
            None, Some("parent-span"), None, None));
        let relation = plan.relationships.iter().find(|edge| edge.relation_type == "parent_child").unwrap();
        assert_eq!(relation.other_id, 2);
        assert_eq!(relation.evidence["parent_event_id"], 1);
        assert_eq!(relation.evidence["child_event_id"], 2);
    }

    #[test]
    fn out_of_order_timestamps_do_not_turn_context_into_causality() {
        let mut engine = CorrelationEngine::default();
        engine.insert(1, event(10, Some("trace-12345678"), Some("req-1"), Some("order-1"),
            None, None, None, None));
        let plan = engine.plan(2, &event(-10, Some("trace-12345678"), Some("req-1"), Some("order-1"),
            None, None, None, None));
        assert!(plan.relationships.iter().any(|edge| edge.relation_type == "same_trace"));
        assert!(!plan.relationships.iter().any(|edge| edge.relation_type == "parent_child"));
    }

    #[test]
    fn candidate_search_and_identity_buckets_are_bounded() {
        let mut engine = CorrelationEngine::default();
        for id in 0..(MAX_EVENTS_PER_IDENTITY as i64 + 80) {
            engine.insert(id, event(id, Some("trace-12345678"), None, None, None, None, None, None));
        }
        let current = event(500, Some("trace-12345678"), None, None, None, None, None, None);
        assert!(engine.candidates(&current).len() <= MAX_CANDIDATES_PER_EVENT);
        assert!(engine.identity_index.values().all(|bucket| bucket.len() <= MAX_EVENTS_PER_IDENTITY));
    }

    #[test]
    fn adjacency_degree_is_bounded() {
        let mut engine = CorrelationEngine::default();
        for other_id in 2..200 {
            engine.add_adjacency(1, other_id, "same_trace");
        }
        assert!(engine.adjacency[&1].len() <= MAX_ADJACENCY_EDGES_PER_EVENT);
    }

    #[test]
    fn duplicate_event_insert_is_idempotent() {
        let mut engine = CorrelationEngine::default();
        let first = event(0, Some("trace-12345678"), None, None, None, None, None, None);
        engine.insert(1, first.clone());
        engine.insert(1, first);
        assert_eq!(engine.nodes.len(), 1);
        assert_eq!(engine.identity_index.values().map(VecDeque::len).sum::<usize>(), 1);
    }

    #[test]
    fn system_and_short_trace_ids_are_not_indexed() {
        let mut engine = CorrelationEngine::default();
        engine.insert(1, event(0, Some("sys_generated_123456"), None, None, None, None, None, None));
        engine.insert(2, event(1, Some("tiny"), None, None, None, None, None, None));
        assert!(engine.identity_index.keys().all(|key| key.kind != IdentityKind::Trace));
    }
}
