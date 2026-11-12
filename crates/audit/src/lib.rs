//! Append-only audit records.

#![forbid(unsafe_code)]

use std::net::IpAddr;

use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

/// Errors returned by the audit layer.
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// A security-relevant event. Metadata must never contain secrets or message bodies.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub actor_user_id: Option<Uuid>,
    pub actor_type: String,
    pub action: String,
    pub community_id: Option<Uuid>,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub metadata: Value,
}

impl AuditEvent {
    pub fn system(action: impl Into<String>) -> Self {
        Self {
            actor_user_id: None,
            actor_type: "system".to_string(),
            action: action.into(),
            community_id: None,
            target_type: None,
            target_id: None,
            ip: None,
            user_agent: None,
            metadata: Value::Object(Default::default()),
        }
    }

    pub fn user(mut self, user_id: Uuid) -> Self {
        self.actor_user_id = Some(user_id);
        self.actor_type = "user".to_string();
        self
    }

    pub fn target(mut self, target_type: impl Into<String>, target_id: Uuid) -> Self {
        self.target_type = Some(target_type.into());
        self.target_id = Some(target_id);
        self
    }

    pub fn request(mut self, ip: Option<IpAddr>, user_agent: Option<String>) -> Self {
        self.ip = ip;
        self.user_agent = user_agent;
        self
    }

    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Audit repository.
#[derive(Clone)]
pub struct AuditService {
    pool: PgPool,
}

impl AuditService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        sqlx::query(
            r#"
            INSERT INTO audit_logs (
                id, actor_user_id, actor_type, action, community_id,
                target_type, target_id, ip, user_agent, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::inet, $9, $10)
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(event.actor_user_id)
        .bind(event.actor_type)
        .bind(event.action)
        .bind(event.community_id)
        .bind(event.target_type)
        .bind(event.target_id)
        .bind(event.ip.map(|value| value.to_string()))
        .bind(event.user_agent)
        .bind(event.metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
