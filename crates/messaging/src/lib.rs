//! Messages, reactions, mentions, pins, read states, and message search.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

/// Errors returned by the messaging layer.
#[derive(Debug, Error)]
pub enum MessageError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("message not found")]
    NotFound,
    #[error("invalid message: {0}")]
    Invalid(String),
    #[error("reply target must belong to the same channel")]
    ReplyTargetInvalid,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub author_id: Option<Uuid>,
    pub kind: String,
    pub content: Option<String>,
    pub reply_to_id: Option<Uuid>,
    pub client_nonce: Option<Uuid>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Reaction {
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Attachment {
    pub id: Uuid,
    pub message_id: Uuid,
    pub file_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub spoiler: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AttachmentDownload {
    pub attachment: Attachment,
    pub object_key: String,
}

#[derive(FromRow)]
struct AttachmentDownloadRow {
    id: Uuid,
    message_id: Uuid,
    file_id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
    spoiler: bool,
    created_at: DateTime<Utc>,
    object_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadState {
    pub channel_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub unread_count: i64,
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub channel_id: Uuid,
    pub author_id: Uuid,
    pub content: Option<String>,
    pub reply_to_id: Option<Uuid>,
    pub client_nonce: Option<Uuid>,
    pub attachment_ids: Vec<Uuid>,
}

const MESSAGE_COLUMNS: &str = r#"
    id, channel_id, author_id, kind, content, reply_to_id, client_nonce,
    edited_at, deleted_at, deleted_by, created_at, updated_at
"#;

#[derive(Clone)]
pub struct MessageService {
    pool: PgPool,
}

impl MessageService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Persist a message. The boolean is `false` when an idempotent nonce replay returned an existing message.
    pub async fn create(&self, input: NewMessage) -> Result<(Message, bool), MessageError> {
        let content = input
            .content
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(content) = content {
            if content.chars().count() > 8000 {
                return Err(MessageError::Invalid(
                    "message content must be at most 8000 characters".into(),
                ));
            }
        }
        if content.is_none() && input.attachment_ids.is_empty() {
            return Err(MessageError::Invalid(
                "a message must contain text or at least one attachment".into(),
            ));
        }
        if input.attachment_ids.len() > 10 {
            return Err(MessageError::Invalid(
                "a message may contain at most 10 attachments".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for attachment_id in &input.attachment_ids {
            if !seen.insert(*attachment_id) {
                return Err(MessageError::Invalid(
                    "duplicate attachment in message".into(),
                ));
            }
        }

        let mut tx = self.pool.begin().await?;
        if let Some(reply_to_id) = input.reply_to_id {
            let reply_channel: Option<Uuid> =
                sqlx::query_scalar("SELECT channel_id FROM messages WHERE id = $1")
                    .bind(reply_to_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            match reply_channel {
                Some(channel_id) if channel_id == input.channel_id => {}
                Some(_) => return Err(MessageError::ReplyTargetInvalid),
                None => return Err(MessageError::NotFound),
            }
        }

        let mut files = Vec::with_capacity(input.attachment_ids.len());
        for file_id in &input.attachment_ids {
            let file: Option<(String, String, i64)> = sqlx::query_as(
                r#"
                SELECT original_filename, content_type, size_bytes
                FROM files
                WHERE id = $1 AND owner_user_id = $2 AND status = 'ready'
                  AND purpose = 'attachment' AND deleted_at IS NULL
                "#,
            )
            .bind(file_id)
            .bind(input.author_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((filename, content_type, size_bytes)) = file else {
                return Err(MessageError::Invalid(
                    "an attachment is missing, not ready, or owned by another user".into(),
                ));
            };
            files.push((*file_id, filename, content_type, size_bytes));
        }

        let inserted = sqlx::query_as::<_, Message>(&format!(
            r#"
            INSERT INTO messages (id, channel_id, author_id, content, reply_to_id, client_nonce)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (channel_id, author_id, client_nonce)
                WHERE client_nonce IS NOT NULL AND author_id IS NOT NULL
            DO NOTHING
            RETURNING {MESSAGE_COLUMNS}
            "#
        ))
        .bind(tulpar_core::new_id())
        .bind(input.channel_id)
        .bind(input.author_id)
        .bind(content)
        .bind(input.reply_to_id)
        .bind(input.client_nonce)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(message) = inserted else {
            // The only conflict target is the idempotency index.
            let existing = sqlx::query_as::<_, Message>(&format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages
                 WHERE channel_id = $1 AND author_id = $2 AND client_nonce = $3"
            ))
            .bind(input.channel_id)
            .bind(input.author_id)
            .bind(input.client_nonce)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok((existing, false));
        };

        for (file_id, filename, content_type, size_bytes) in files {
            sqlx::query(
                r#"
                INSERT INTO attachments (id, message_id, file_id, filename, content_type, size_bytes)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(tulpar_core::new_id())
            .bind(message.id)
            .bind(file_id)
            .bind(filename)
            .bind(content_type)
            .bind(size_bytes)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            "UPDATE channels SET last_message_id = $2, last_message_at = $3, updated_at = now() WHERE id = $1",
        )
        .bind(input.channel_id)
        .bind(message.id)
        .bind(message.created_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((message, true))
    }

    pub async fn get(&self, message_id: Uuid) -> Result<Message, MessageError> {
        sqlx::query_as::<_, Message>(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = $1"
        ))
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(MessageError::NotFound)
    }

    pub async fn attachments(&self, message_id: Uuid) -> Result<Vec<Attachment>, MessageError> {
        let attachments = sqlx::query_as::<_, Attachment>(
            r#"
            SELECT id, message_id, file_id, filename, content_type, size_bytes, spoiler, created_at
            FROM attachments
            WHERE message_id = $1
            ORDER BY created_at, id
            "#,
        )
        .bind(message_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(attachments)
    }

    pub async fn attachments_for_messages(
        &self,
        message_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, MessageError> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let attachments = sqlx::query_as::<_, Attachment>(
            r#"
            SELECT id, message_id, file_id, filename, content_type, size_bytes, spoiler, created_at
            FROM attachments
            WHERE message_id = ANY($1)
            ORDER BY created_at, id
            "#,
        )
        .bind(message_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(attachments)
    }

    pub async fn attachment_download(
        &self,
        message_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<AttachmentDownload, MessageError> {
        let row = sqlx::query_as::<_, AttachmentDownloadRow>(
            r#"
            SELECT a.id, a.message_id, a.file_id, a.filename, a.content_type,
                   a.size_bytes, a.spoiler, a.created_at, f.object_key
            FROM attachments a
            JOIN files f ON f.id = a.file_id
            WHERE a.message_id = $1 AND a.id = $2 AND f.deleted_at IS NULL
            "#,
        )
        .bind(message_id)
        .bind(attachment_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(MessageError::NotFound)?;
        Ok(AttachmentDownload {
            attachment: Attachment {
                id: row.id,
                message_id: row.message_id,
                file_id: row.file_id,
                filename: row.filename,
                content_type: row.content_type,
                size_bytes: row.size_bytes,
                spoiler: row.spoiler,
                created_at: row.created_at,
            },
            object_key: row.object_key,
        })
    }

    pub async fn add_user_mention(
        &self,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), MessageError> {
        sqlx::query(
            "INSERT INTO message_mentions (id, message_id, kind, user_id) VALUES ($1, $2, 'user', $3)",
        )
        .bind(tulpar_core::new_id())
        .bind(message_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_role_mention(
        &self,
        message_id: Uuid,
        role_id: Uuid,
    ) -> Result<(), MessageError> {
        sqlx::query(
            "INSERT INTO message_mentions (id, message_id, kind, role_id) VALUES ($1, $2, 'role', $3)",
        )
        .bind(tulpar_core::new_id())
        .bind(message_id)
        .bind(role_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_everyone_mention(&self, message_id: Uuid) -> Result<(), MessageError> {
        sqlx::query(
            "INSERT INTO message_mentions (id, message_id, kind) VALUES ($1, $2, 'everyone')",
        )
        .bind(tulpar_core::new_id())
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Newest-first keyset pagination. `before` and `after` are message IDs.
    pub async fn list(
        &self,
        channel_id: Uuid,
        before: Option<Uuid>,
        after: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Message>, MessageError> {
        let messages = sqlx::query_as::<_, Message>(&format!(
            r#"
            SELECT {MESSAGE_COLUMNS}
            FROM messages
            WHERE channel_id = $1
              AND deleted_at IS NULL
              AND ($2::uuid IS NULL OR id < $2)
              AND ($3::uuid IS NULL OR id > $3)
            ORDER BY id DESC
            LIMIT $4
            "#
        ))
        .bind(channel_id)
        .bind(before)
        .bind(after)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        Ok(messages)
    }

    pub async fn last_authored_at(
        &self,
        channel_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, MessageError> {
        let created_at = sqlx::query_scalar(
            "SELECT created_at FROM messages
             WHERE channel_id = $1 AND author_id = $2 AND deleted_at IS NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(channel_id)
        .bind(author_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(created_at)
    }

    pub async fn edit(&self, message_id: Uuid, content: &str) -> Result<Message, MessageError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(MessageError::Invalid(
                "message content must not be empty".into(),
            ));
        }
        if content.chars().count() > 8000 {
            return Err(MessageError::Invalid(
                "message content must be at most 8000 characters".into(),
            ));
        }
        let message = sqlx::query_as::<_, Message>(&format!(
            r#"
            UPDATE messages
            SET content = $2, edited_at = now(), updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING {MESSAGE_COLUMNS}
            "#
        ))
        .bind(message_id)
        .bind(content)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(MessageError::NotFound)?;
        Ok(message)
    }

    pub async fn soft_delete(
        &self,
        message_id: Uuid,
        deleted_by: Uuid,
    ) -> Result<Message, MessageError> {
        let mut tx = self.pool.begin().await?;
        let message = sqlx::query_as::<_, Message>(&format!(
            r#"
            UPDATE messages
            SET content = NULL, deleted_at = now(), deleted_by = $2, updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING {MESSAGE_COLUMNS}
            "#
        ))
        .bind(message_id)
        .bind(deleted_by)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MessageError::NotFound)?;

        sqlx::query(
            r#"
            UPDATE channels
            SET last_message_id = (
                    SELECT id FROM messages
                    WHERE channel_id = $1 AND deleted_at IS NULL
                    ORDER BY id DESC LIMIT 1
                ),
                last_message_at = (
                    SELECT created_at FROM messages
                    WHERE channel_id = $1 AND deleted_at IS NULL
                    ORDER BY id DESC LIMIT 1
                ),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(message.channel_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(message)
    }

    pub async fn add_reaction(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
    ) -> Result<(), MessageError> {
        let emoji = emoji.trim();
        if emoji.is_empty() || emoji.chars().count() > 64 {
            return Err(MessageError::Invalid(
                "emoji must be between 1 and 64 characters".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO message_reactions (message_id, user_id, emoji) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(message_id)
        .bind(user_id)
        .bind(emoji)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_reaction(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
    ) -> Result<(), MessageError> {
        sqlx::query(
            "DELETE FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
        )
        .bind(message_id)
        .bind(user_id)
        .bind(emoji)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_reactions(&self, message_id: Uuid) -> Result<Vec<Reaction>, MessageError> {
        let reactions = sqlx::query_as::<_, Reaction>(
            "SELECT message_id, user_id, emoji, created_at FROM message_reactions WHERE message_id = $1 ORDER BY created_at, user_id",
        )
        .bind(message_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(reactions)
    }

    pub async fn set_read_state(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
        message_id: Uuid,
    ) -> Result<ReadState, MessageError> {
        let message_channel: Uuid =
            sqlx::query_scalar("SELECT channel_id FROM messages WHERE id = $1")
                .bind(message_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(MessageError::NotFound)?;
        if message_channel != channel_id {
            return Err(MessageError::Invalid(
                "the read cursor must belong to the requested channel".into(),
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO read_states (user_id, channel_id, last_read_message_id, last_read_at, updated_at)
            VALUES ($1, $2, $3, now(), now())
            ON CONFLICT (user_id, channel_id) DO UPDATE
            SET last_read_message_id = EXCLUDED.last_read_message_id,
                last_read_at = EXCLUDED.last_read_at,
                updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        self.read_state(user_id, channel_id).await
    }

    pub async fn read_state(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> Result<ReadState, MessageError> {
        let row: Option<(Option<Uuid>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT last_read_message_id, last_read_at FROM read_states WHERE user_id = $1 AND channel_id = $2",
        )
        .bind(user_id)
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await?;
        let (last_read_message_id, last_read_at) = row.unwrap_or((None, None));
        let unread_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM messages
            WHERE channel_id = $1 AND deleted_at IS NULL
              AND ($2::uuid IS NULL OR id > $2)
            "#,
        )
        .bind(channel_id)
        .bind(last_read_message_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(ReadState {
            channel_id,
            last_read_message_id,
            last_read_at,
            unread_count,
        })
    }

    pub async fn pin(
        &self,
        channel_id: Uuid,
        message_id: Uuid,
        pinned_by: Uuid,
    ) -> Result<(), MessageError> {
        let message_channel: Uuid = sqlx::query_scalar(
            "SELECT channel_id FROM messages WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(MessageError::NotFound)?;
        if message_channel != channel_id {
            return Err(MessageError::Invalid(
                "the message must belong to the requested channel".into(),
            ));
        }
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pinned_messages WHERE channel_id = $1")
                .bind(channel_id)
                .fetch_one(&self.pool)
                .await?;
        let already_pinned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pinned_messages WHERE channel_id = $1 AND message_id = $2)",
        )
        .bind(channel_id)
        .bind(message_id)
        .fetch_one(&self.pool)
        .await?;
        if !already_pinned && count >= 50 {
            return Err(MessageError::Invalid(
                "a channel may have at most 50 pinned messages".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO pinned_messages (channel_id, message_id, pinned_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(channel_id)
        .bind(message_id)
        .bind(pinned_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unpin(&self, channel_id: Uuid, message_id: Uuid) -> Result<(), MessageError> {
        sqlx::query("DELETE FROM pinned_messages WHERE channel_id = $1 AND message_id = $2")
            .bind(channel_id)
            .bind(message_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_pins(&self, channel_id: Uuid) -> Result<Vec<Message>, MessageError> {
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT m.id, m.channel_id, m.author_id, m.kind, m.content, m.reply_to_id,
                   m.client_nonce, m.edited_at, m.deleted_at, m.deleted_by, m.created_at, m.updated_at
            FROM pinned_messages p
            JOIN messages m ON m.id = p.message_id
            WHERE p.channel_id = $1 AND m.deleted_at IS NULL
            ORDER BY p.pinned_at DESC
            "#,
        )
        .bind(channel_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(messages)
    }

    pub async fn search(
        &self,
        channel_id: Uuid,
        query: &str,
        limit: i64,
    ) -> Result<Vec<Message>, MessageError> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Err(MessageError::Invalid(
                "search query must be at least 2 characters".into(),
            ));
        }
        let messages = sqlx::query_as::<_, Message>(&format!(
            r#"
            SELECT {MESSAGE_COLUMNS}
            FROM messages
            WHERE channel_id = $1
              AND deleted_at IS NULL
              AND search_vector @@ plainto_tsquery('simple', $2)
            ORDER BY id DESC
            LIMIT $3
            "#
        ))
        .bind(channel_id)
        .bind(query)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        Ok(messages)
    }
}
