//! Friendships, blocks, and direct-message privacy rules.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SocialError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("friendship not found")]
    FriendshipNotFound,
    #[error("a friendship or request already exists")]
    AlreadyExists,
    #[error("cannot add yourself")]
    SelfAction,
    #[error("the target does not accept friend requests")]
    RequestsDisabled,
    #[error("you cannot send a direct message to this user")]
    DmDisabled,
    #[error("user is blocked")]
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct FriendUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FriendshipView {
    pub id: Uuid,
    pub user: FriendUser,
    pub status: String,
    pub direction: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockedUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct FriendshipRow {
    id: Uuid,
    status: String,
    created_at: DateTime<Utc>,
    requested_by: Uuid,
    user_id: Uuid,
    username: String,
    display_name: String,
    avatar_file_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct SocialService {
    pool: PgPool,
}

impl SocialService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn send_request(
        &self,
        requester_id: Uuid,
        addressee_id: Uuid,
    ) -> Result<(), SocialError> {
        if requester_id == addressee_id {
            return Err(SocialError::SelfAction);
        }
        if self.is_blocked(requester_id, addressee_id).await? {
            return Err(SocialError::Blocked);
        }
        if !self
            .accepts_friend_requests(requester_id, addressee_id)
            .await?
        {
            return Err(SocialError::RequestsDisabled);
        }

        let (a, b) = ordered_pair(requester_id, addressee_id);
        let result = sqlx::query(
            r#"
            INSERT INTO friendships (id, user_a_id, user_b_id, requested_by)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_a_id, user_b_id) DO NOTHING
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(a)
        .bind(b)
        .bind(requester_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(SocialError::AlreadyExists);
        }
        Ok(())
    }

    pub async fn accept_request(
        &self,
        user_id: Uuid,
        friendship_id: Uuid,
    ) -> Result<(), SocialError> {
        let result = sqlx::query(
            r#"
            UPDATE friendships
            SET status = 'accepted', responded_at = now()
            WHERE id = $1
              AND status = 'pending'
              AND requested_by <> $2
              AND (user_a_id = $2 OR user_b_id = $2)
            "#,
        )
        .bind(friendship_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(SocialError::FriendshipNotFound);
        }
        Ok(())
    }

    pub async fn decline_request(
        &self,
        user_id: Uuid,
        friendship_id: Uuid,
    ) -> Result<(), SocialError> {
        let result = sqlx::query(
            r#"
            UPDATE friendships
            SET status = 'declined', responded_at = now()
            WHERE id = $1
              AND status = 'pending'
              AND requested_by <> $2
              AND (user_a_id = $2 OR user_b_id = $2)
            "#,
        )
        .bind(friendship_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(SocialError::FriendshipNotFound);
        }
        Ok(())
    }

    pub async fn remove_friend(&self, user_id: Uuid, friend_id: Uuid) -> Result<(), SocialError> {
        if user_id == friend_id {
            return Err(SocialError::SelfAction);
        }
        let (a, b) = ordered_pair(user_id, friend_id);
        sqlx::query("DELETE FROM friendships WHERE user_a_id = $1 AND user_b_id = $2")
            .bind(a)
            .bind(b)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_friends(&self, user_id: Uuid) -> Result<Vec<FriendUser>, SocialError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, Option<Uuid>)>(
            r#"
            SELECT u.id, u.username::text, u.display_name, u.avatar_file_id
            FROM friendships f
            JOIN users u
              ON u.id = CASE WHEN f.user_a_id = $1 THEN f.user_b_id ELSE f.user_a_id END
            WHERE (f.user_a_id = $1 OR f.user_b_id = $1)
              AND f.status = 'accepted'
              AND u.deleted_at IS NULL
            ORDER BY u.display_name, u.id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, username, display_name, avatar_file_id)| FriendUser {
                id,
                username,
                display_name,
                avatar_file_id,
            })
            .collect())
    }

    pub async fn list_requests(&self, user_id: Uuid) -> Result<Vec<FriendshipView>, SocialError> {
        let rows = sqlx::query_as::<_, FriendshipRow>(
            r#"
            SELECT f.id, f.status, f.created_at, f.requested_by,
                   u.id AS user_id, u.username::text AS username, u.display_name, u.avatar_file_id
            FROM friendships f
            JOIN users u
              ON u.id = CASE WHEN f.user_a_id = $1 THEN f.user_b_id ELSE f.user_a_id END
            WHERE (f.user_a_id = $1 OR f.user_b_id = $1)
              AND f.status = 'pending'
            ORDER BY f.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| FriendshipView {
                id: row.id,
                user: FriendUser {
                    id: row.user_id,
                    username: row.username,
                    display_name: row.display_name,
                    avatar_file_id: row.avatar_file_id,
                },
                status: row.status,
                direction: if row.requested_by == user_id {
                    "outgoing".to_string()
                } else {
                    "incoming".to_string()
                },
                created_at: row.created_at,
            })
            .collect())
    }

    pub async fn block(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<(), SocialError> {
        if blocker_id == blocked_id {
            return Err(SocialError::SelfAction);
        }
        let (a, b) = ordered_pair(blocker_id, blocked_id);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(blocker_id)
        .bind(blocked_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM friendships WHERE user_a_id = $1 AND user_b_id = $2")
            .bind(a)
            .bind(b)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn unblock(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<(), SocialError> {
        sqlx::query("DELETE FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2")
            .bind(blocker_id)
            .bind(blocked_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_blocks(&self, user_id: Uuid) -> Result<Vec<BlockedUser>, SocialError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, Option<Uuid>, DateTime<Utc>)>(
            r#"
            SELECT u.id, u.username::text, u.display_name, u.avatar_file_id, b.created_at
            FROM user_blocks b
            JOIN users u ON u.id = b.blocked_id
            WHERE b.blocker_id = $1 AND u.deleted_at IS NULL
            ORDER BY b.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, username, display_name, avatar_file_id, created_at)| BlockedUser {
                    id,
                    username,
                    display_name,
                    avatar_file_id,
                    created_at,
                },
            )
            .collect())
    }

    pub async fn is_blocked(&self, first: Uuid, second: Uuid) -> Result<bool, SocialError> {
        let blocked: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM user_blocks
                WHERE (blocker_id = $1 AND blocked_id = $2)
                   OR (blocker_id = $2 AND blocked_id = $1)
            )
            "#,
        )
        .bind(first)
        .bind(second)
        .fetch_one(&self.pool)
        .await?;
        Ok(blocked)
    }

    pub async fn can_dm(&self, sender_id: Uuid, recipient_id: Uuid) -> Result<(), SocialError> {
        if sender_id == recipient_id {
            return Err(SocialError::SelfAction);
        }
        if self.is_blocked(sender_id, recipient_id).await? {
            return Err(SocialError::Blocked);
        }
        let policy: String =
            sqlx::query_scalar("SELECT allow_dms_from FROM user_settings WHERE user_id = $1")
                .bind(recipient_id)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or_else(|| "friends".to_string());

        let allowed = match policy.as_str() {
            "everyone" => true,
            "nobody" => false,
            "friends" => self.are_friends(sender_id, recipient_id).await?,
            "mutual_communities" => self.mutual_communities(sender_id, recipient_id).await?,
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(SocialError::DmDisabled)
        }
    }

    pub async fn are_friends(&self, first: Uuid, second: Uuid) -> Result<bool, SocialError> {
        let (a, b) = ordered_pair(first, second);
        let friends: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM friendships WHERE user_a_id = $1 AND user_b_id = $2 AND status = 'accepted')",
        )
        .bind(a)
        .bind(b)
        .fetch_one(&self.pool)
        .await?;
        Ok(friends)
    }

    pub async fn mutual_communities(&self, first: Uuid, second: Uuid) -> Result<bool, SocialError> {
        let mutual: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM community_members a
                JOIN community_members b ON b.community_id = a.community_id
                WHERE a.user_id = $1 AND b.user_id = $2
            )
            "#,
        )
        .bind(first)
        .bind(second)
        .fetch_one(&self.pool)
        .await?;
        Ok(mutual)
    }

    async fn accepts_friend_requests(
        &self,
        _requester_id: Uuid,
        addressee_id: Uuid,
    ) -> Result<bool, SocialError> {
        let policy: String = sqlx::query_scalar(
            "SELECT allow_friend_requests_from FROM user_settings WHERE user_id = $1",
        )
        .bind(addressee_id)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or_else(|| "everyone".to_string());
        match policy.as_str() {
            "everyone" => Ok(true),
            "nobody" => Ok(false),
            "mutual_communities" => self.mutual_communities(_requester_id, addressee_id).await,
            _ => Ok(false),
        }
    }
}

fn ordered_pair(first: Uuid, second: Uuid) -> (Uuid, Uuid) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}
