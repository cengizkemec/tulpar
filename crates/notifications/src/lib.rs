//! Notification records, preferences, and durable dispatch events.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("notification not found")]
    NotFound,
    #[error("invalid notification: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub community_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub message_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub payload: Value,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct NotificationPreferences {
    pub user_id: Uuid,
    pub desktop_enabled: bool,
    pub sound_enabled: bool,
    pub mentions_only: bool,
    pub suppress_everyone: bool,
    pub email_enabled: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewNotification {
    pub user_id: Uuid,
    pub kind: String,
    pub community_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub message_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub payload: Value,
}

#[derive(Clone)]
pub struct NotificationService {
    pool: PgPool,
}

impl NotificationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a notification and its durable dispatch event in one transaction.
    pub async fn create(&self, input: NewNotification) -> Result<Notification, NotificationError> {
        if input.kind.trim().is_empty() {
            return Err(NotificationError::Invalid("kind is required".into()));
        }
        let mut tx = self.pool.begin().await?;
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            INSERT INTO notifications (
                id, user_id, kind, community_id, channel_id, message_id, actor_user_id, payload
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, user_id, kind, community_id, channel_id, message_id, actor_user_id,
                      payload, read_at, created_at
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(input.user_id)
        .bind(input.kind)
        .bind(input.community_id)
        .bind(input.channel_id)
        .bind(input.message_id)
        .bind(input.actor_user_id)
        .bind(&input.payload)
        .fetch_one(&mut *tx)
        .await?;

        let event_payload = serde_json::to_value(&notification)
            .map_err(|error| NotificationError::Invalid(error.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO outbox_events (
                event_id, topic, aggregate_type, aggregate_id, community_id, payload
            )
            VALUES ($1, 'notification.created', 'notification', $2, $3, $4)
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(notification.id)
        .bind(notification.community_id)
        .bind(event_payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(notification)
    }

    /// Respect channel/community mute settings before creating a notification.
    pub async fn should_notify(
        &self,
        user_id: Uuid,
        community_id: Option<Uuid>,
        channel_id: Option<Uuid>,
    ) -> Result<bool, NotificationError> {
        self.preferences(user_id).await?;
        if let Some(channel_id) = channel_id {
            let setting: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
                "SELECT level, muted_until FROM channel_notification_settings WHERE user_id = $1 AND channel_id = $2",
            )
            .bind(user_id)
            .bind(channel_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((level, muted_until)) = setting {
                let muted = muted_until.is_some_and(|until| until > Utc::now());
                if muted || level == "none" {
                    return Ok(false);
                }
            }
        }
        if let Some(community_id) = community_id {
            let setting: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
                "SELECT level, muted_until FROM community_notification_settings WHERE user_id = $1 AND community_id = $2",
            )
            .bind(user_id)
            .bind(community_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((level, muted_until)) = setting {
                let muted = muted_until.is_some_and(|until| until > Utc::now());
                if muted || level == "none" {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub async fn list(
        &self,
        user_id: Uuid,
        before: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Notification>, NotificationError> {
        let notifications = sqlx::query_as::<_, Notification>(
            r#"
            SELECT id, user_id, kind, community_id, channel_id, message_id, actor_user_id,
                   payload, read_at, created_at
            FROM notifications
            WHERE user_id = $1 AND ($2::uuid IS NULL OR id < $2)
            ORDER BY id DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(before)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        Ok(notifications)
    }

    pub async fn unread_count(&self, user_id: Uuid) -> Result<i64, NotificationError> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Mark specific notifications read. `None` marks all unread notifications.
    pub async fn mark_read(
        &self,
        user_id: Uuid,
        ids: Option<&[Uuid]>,
    ) -> Result<u64, NotificationError> {
        let result = match ids {
            Some(ids) => {
                sqlx::query(
                    "UPDATE notifications SET read_at = now()
                     WHERE user_id = $1 AND read_at IS NULL AND id = ANY($2)",
                )
                .bind(user_id)
                .bind(ids)
                .execute(&self.pool)
                .await?
            }
            None => sqlx::query(
                "UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL",
            )
            .bind(user_id)
            .execute(&self.pool)
            .await?,
        };
        Ok(result.rows_affected())
    }

    pub async fn preferences(
        &self,
        user_id: Uuid,
    ) -> Result<NotificationPreferences, NotificationError> {
        sqlx::query(
            "INSERT INTO notification_preferences (user_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        let preferences = sqlx::query_as::<_, NotificationPreferences>(
            r#"
            SELECT user_id, desktop_enabled, sound_enabled, mentions_only,
                   suppress_everyone, email_enabled, updated_at
            FROM notification_preferences WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(preferences)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_preferences(
        &self,
        user_id: Uuid,
        desktop_enabled: Option<bool>,
        sound_enabled: Option<bool>,
        mentions_only: Option<bool>,
        suppress_everyone: Option<bool>,
        email_enabled: Option<bool>,
    ) -> Result<NotificationPreferences, NotificationError> {
        sqlx::query(
            "INSERT INTO notification_preferences (user_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"
            UPDATE notification_preferences
            SET desktop_enabled = COALESCE($2, desktop_enabled),
                sound_enabled = COALESCE($3, sound_enabled),
                mentions_only = COALESCE($4, mentions_only),
                suppress_everyone = COALESCE($5, suppress_everyone),
                email_enabled = COALESCE($6, email_enabled),
                updated_at = now()
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .bind(desktop_enabled)
        .bind(sound_enabled)
        .bind(mentions_only)
        .bind(suppress_everyone)
        .bind(email_enabled)
        .execute(&self.pool)
        .await?;
        self.preferences(user_id).await
    }
}
