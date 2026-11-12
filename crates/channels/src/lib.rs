//! Community channels, categories, and direct conversations.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use permissions::Permissions;
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("channel not found")]
    NotFound,
    #[error("invalid channel data: {0}")]
    Invalid(String),
    #[error("direct conversation not found")]
    ConversationNotFound,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Channel {
    pub id: Uuid,
    pub scope: String,
    pub community_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub kind: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: i32,
    pub nsfw: bool,
    pub slowmode_seconds: i32,
    pub user_limit: Option<i32>,
    pub bitrate: Option<i32>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectConversation {
    pub channel: Channel,
    pub other_user_id: Uuid,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ChannelOverwrite {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub role_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub allow: i64,
    pub deny: i64,
}

impl ChannelOverwrite {
    pub fn allow_permissions(&self) -> Permissions {
        Permissions::from_bits_truncate(self.allow as u64)
    }

    pub fn deny_permissions(&self) -> Permissions {
        Permissions::from_bits_truncate(self.deny as u64)
    }
}

#[derive(Clone)]
pub struct ChannelService {
    pool: PgPool,
}

impl ChannelService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_community_channel(
        &self,
        community_id: Uuid,
        kind: &str,
        name: &str,
        parent_id: Option<Uuid>,
        position: i32,
        topic: Option<String>,
    ) -> Result<Channel, ChannelError> {
        if !matches!(
            kind,
            "category" | "text" | "announcement" | "voice" | "forum"
        ) {
            return Err(ChannelError::Invalid("invalid channel kind".into()));
        }
        if kind == "category" && parent_id.is_some() {
            return Err(ChannelError::Invalid(
                "categories cannot have a parent".into(),
            ));
        }
        let channel = sqlx::query_as::<_, Channel>(
            r#"
            INSERT INTO channels (id, scope, community_id, parent_id, kind, name, topic, position)
            VALUES ($1, 'community', $2, $3, $4, $5, $6, $7)
            RETURNING id, scope, community_id, parent_id, kind, name, topic, position, nsfw,
                      slowmode_seconds, user_limit, bitrate, last_message_at, last_message_id,
                      archived_at, created_at, updated_at
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(community_id)
        .bind(parent_id)
        .bind(kind)
        .bind(name.trim())
        .bind(topic)
        .bind(position)
        .fetch_one(&self.pool)
        .await?;
        Ok(channel)
    }

    pub async fn create_default_channels(
        &self,
        community_id: Uuid,
    ) -> Result<(Channel, Channel), ChannelError> {
        let category = self
            .create_community_channel(community_id, "category", "General", None, 0, None)
            .await?;
        let general = self
            .create_community_channel(
                community_id,
                "text",
                "general",
                Some(category.id),
                0,
                Some("Welcome to the community.".to_string()),
            )
            .await?;
        Ok((category, general))
    }

    pub async fn get(&self, channel_id: Uuid) -> Result<Channel, ChannelError> {
        sqlx::query_as::<_, Channel>(
            r#"
            SELECT id, scope, community_id, parent_id, kind, name, topic, position, nsfw,
                   slowmode_seconds, user_limit, bitrate, last_message_at, last_message_id,
                   archived_at, created_at, updated_at
            FROM channels
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ChannelError::NotFound)
    }

    pub async fn list_for_community(
        &self,
        community_id: Uuid,
    ) -> Result<Vec<Channel>, ChannelError> {
        let channels = sqlx::query_as::<_, Channel>(
            r#"
            SELECT id, scope, community_id, parent_id, kind, name, topic, position, nsfw,
                   slowmode_seconds, user_limit, bitrate, last_message_at, last_message_id,
                   archived_at, created_at, updated_at
            FROM channels
            WHERE community_id = $1 AND deleted_at IS NULL
            ORDER BY parent_id NULLS FIRST, position ASC, created_at ASC
            "#,
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(channels)
    }

    pub async fn create_direct(
        &self,
        user_a_id: Uuid,
        user_b_id: Uuid,
        created_by: Uuid,
    ) -> Result<Channel, ChannelError> {
        if user_a_id == user_b_id {
            return Err(ChannelError::Invalid(
                "a direct conversation requires two different users".into(),
            ));
        }
        let (a, b) = if user_a_id < user_b_id {
            (user_a_id, user_b_id)
        } else {
            (user_b_id, user_a_id)
        };

        if let Some(channel) = self.find_direct(a, b).await? {
            return Ok(channel);
        }

        let mut tx = self.pool.begin().await?;
        let channel = sqlx::query_as::<_, Channel>(
            r#"
            INSERT INTO channels (id, scope, kind)
            VALUES ($1, 'direct', 'direct')
            RETURNING id, scope, community_id, parent_id, kind, name, topic, position, nsfw,
                      slowmode_seconds, user_limit, bitrate, last_message_at, last_message_id,
                      archived_at, created_at, updated_at
            "#,
        )
        .bind(tulpar_core::new_id())
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO direct_conversations (channel_id, user_a_id, user_b_id, created_by) VALUES ($1, $2, $3, $4)",
        )
        .bind(channel.id)
        .bind(a)
        .bind(b)
        .bind(created_by)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(channel)
    }

    async fn find_direct(&self, a: Uuid, b: Uuid) -> Result<Option<Channel>, ChannelError> {
        let channel = sqlx::query_as::<_, Channel>(
            r#"
            SELECT c.id, c.scope, c.community_id, c.parent_id, c.kind, c.name, c.topic,
                   c.position, c.nsfw, c.slowmode_seconds, c.user_limit, c.bitrate,
                   c.last_message_at, c.last_message_id, c.archived_at, c.created_at, c.updated_at
            FROM direct_conversations d
            JOIN channels c ON c.id = d.channel_id
            WHERE d.user_a_id = $1 AND d.user_b_id = $2 AND c.deleted_at IS NULL
            "#,
        )
        .bind(a)
        .bind(b)
        .fetch_optional(&self.pool)
        .await?;
        Ok(channel)
    }

    pub async fn list_direct_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<DirectConversation>, ChannelError> {
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT d.channel_id,
                   CASE WHEN d.user_a_id = $1 THEN d.user_b_id ELSE d.user_a_id END AS other_user_id
            FROM direct_conversations d
            JOIN channels c ON c.id = d.channel_id
            WHERE (d.user_a_id = $1 OR d.user_b_id = $1) AND c.deleted_at IS NULL
            ORDER BY c.last_message_at DESC NULLS LAST, c.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut conversations = Vec::with_capacity(rows.len());
        for (channel_id, other_user_id) in rows {
            let channel = self.get(channel_id).await?;
            conversations.push(DirectConversation {
                channel,
                other_user_id,
            });
        }
        Ok(conversations)
    }

    pub async fn voice_password_hash(
        &self,
        channel_id: Uuid,
    ) -> Result<Option<String>, ChannelError> {
        let hash: Option<Option<String>> = sqlx::query_scalar(
            "SELECT voice_password_hash FROM channels WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await?;
        hash.ok_or(ChannelError::NotFound)
    }

    pub async fn set_voice_password(
        &self,
        channel_id: Uuid,
        password_hash: Option<String>,
    ) -> Result<Channel, ChannelError> {
        let channel = sqlx::query_as::<_, Channel>(
            r#"
            UPDATE channels
            SET voice_password_hash = $2, updated_at = now()
            WHERE id = $1 AND kind = 'voice' AND deleted_at IS NULL
            RETURNING id, scope, community_id, parent_id, kind, name, topic, position, nsfw,
                      slowmode_seconds, user_limit, bitrate, last_message_at, last_message_id,
                      archived_at, created_at, updated_at
            "#,
        )
        .bind(channel_id)
        .bind(password_hash)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ChannelError::NotFound)?;
        Ok(channel)
    }

    pub async fn direct_other_user(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Uuid>, ChannelError> {
        let other: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT CASE WHEN user_a_id = $2 THEN user_b_id ELSE user_a_id END
            FROM direct_conversations
            WHERE channel_id = $1 AND (user_a_id = $2 OR user_b_id = $2)
            "#,
        )
        .bind(channel_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(other)
    }

    pub async fn list_overwrites(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<ChannelOverwrite>, ChannelError> {
        let overwrites = sqlx::query_as::<_, ChannelOverwrite>(
            r#"
            SELECT id, channel_id, role_id, user_id, allow, deny
            FROM channel_permission_overwrites
            WHERE channel_id = $1
            ORDER BY id
            "#,
        )
        .bind(channel_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(overwrites)
    }

    pub async fn set_overwrite(
        &self,
        channel_id: Uuid,
        role_id: Option<Uuid>,
        user_id: Option<Uuid>,
        allow: Permissions,
        deny: Permissions,
    ) -> Result<ChannelOverwrite, ChannelError> {
        if role_id.is_some() == user_id.is_some() {
            return Err(ChannelError::Invalid(
                "an overwrite must target exactly one role or member".into(),
            ));
        }
        if !(allow & deny).is_empty() {
            return Err(ChannelError::Invalid(
                "allow and deny permissions must not overlap".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let existing: Option<Uuid> = if let Some(role_id) = role_id {
            sqlx::query_scalar(
                "SELECT id FROM channel_permission_overwrites WHERE channel_id = $1 AND role_id = $2",
            )
            .bind(channel_id)
            .bind(role_id)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT id FROM channel_permission_overwrites WHERE channel_id = $1 AND user_id = $2",
            )
            .bind(channel_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?
        };

        let id = existing.unwrap_or_else(tulpar_core::new_id);
        if existing.is_some() {
            sqlx::query(
                "UPDATE channel_permission_overwrites SET allow = $2, deny = $3, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(allow.bits() as i64)
            .bind(deny.bits() as i64)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO channel_permission_overwrites (id, channel_id, role_id, user_id, allow, deny) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(channel_id)
            .bind(role_id)
            .bind(user_id)
            .bind(allow.bits() as i64)
            .bind(deny.bits() as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(ChannelOverwrite {
            id,
            channel_id,
            role_id,
            user_id,
            allow: allow.bits() as i64,
            deny: deny.bits() as i64,
        })
    }

    pub async fn delete_overwrite(
        &self,
        channel_id: Uuid,
        role_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<(), ChannelError> {
        if role_id.is_some() == user_id.is_some() {
            return Err(ChannelError::Invalid(
                "an overwrite must target exactly one role or member".into(),
            ));
        }
        if let Some(role_id) = role_id {
            sqlx::query(
                "DELETE FROM channel_permission_overwrites WHERE channel_id = $1 AND role_id = $2",
            )
            .bind(channel_id)
            .bind(role_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM channel_permission_overwrites WHERE channel_id = $1 AND user_id = $2",
            )
            .bind(channel_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}
