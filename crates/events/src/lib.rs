//! Versioned event contracts and the transactional outbox dispatcher.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EventError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid event: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, FromRow)]
pub struct OutboxEvent {
    pub id: i64,
    pub event_id: Uuid,
    pub topic: String,
    pub aggregate_type: String,
    pub aggregate_id: Option<Uuid>,
    pub community_id: Option<Uuid>,
    pub payload: Value,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct OutboxService {
    pool: PgPool,
}

impl OutboxService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(
        &self,
        topic: &str,
        aggregate_type: &str,
        aggregate_id: Option<Uuid>,
        community_id: Option<Uuid>,
        payload: Value,
    ) -> Result<Uuid, EventError> {
        if topic.trim().is_empty() || aggregate_type.trim().is_empty() {
            return Err(EventError::Invalid(
                "topic and aggregate_type are required".into(),
            ));
        }
        let event_id = tulpar_core::new_id();
        sqlx::query(
            r#"
            INSERT INTO outbox_events (
                event_id, topic, aggregate_type, aggregate_id, community_id, payload
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(event_id)
        .bind(topic)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(community_id)
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(event_id)
    }

    /// Fetch pending events. The single-process dispatcher is the only caller in v1.
    pub async fn pending(&self, limit: i64) -> Result<Vec<OutboxEvent>, EventError> {
        let events = sqlx::query_as::<_, OutboxEvent>(
            r#"
            SELECT id, event_id, topic, aggregate_type, aggregate_id, community_id,
                   payload, attempts, created_at
            FROM outbox_events
            WHERE processed_at IS NULL AND available_at <= now()
            ORDER BY id
            LIMIT $1
            "#,
        )
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    pub async fn mark_processed(&self, id: i64) -> Result<(), EventError> {
        sqlx::query(
            "UPDATE outbox_events SET processed_at = now(), last_error = NULL WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_failed(
        &self,
        id: i64,
        error: &str,
        retry_after_seconds: i64,
    ) -> Result<(), EventError> {
        sqlx::query(
            r#"
            UPDATE outbox_events
            SET attempts = attempts + 1,
                last_error = $2,
                available_at = now() + make_interval(secs => $3)
            WHERE id = $1 AND processed_at IS NULL
            "#,
        )
        .bind(id)
        .bind(error)
        .bind(retry_after_seconds.max(1))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn pending_count(&self) -> Result<i64, EventError> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE processed_at IS NULL")
                .fetch_one(&self.pool)
                .await?,
        )
    }
}
