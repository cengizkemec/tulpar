//! User accounts, profiles, settings, and privacy preferences.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use common::{normalize_email, normalize_username};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

/// Errors returned by user operations.
#[derive(Debug, Error)]
pub enum UserError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("user not found")]
    NotFound,
    #[error("username or email already exists")]
    Conflict,
    #[error("invalid user data: {0}")]
    Invalid(String),
}

/// Public user profile data. Never contains a password hash.
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub avatar_file_id: Option<Uuid>,
    pub bio: Option<String>,
    pub locale: String,
    pub preferred_status: String,
    pub is_bot: bool,
    pub disabled_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Internal user record used only by authentication.
#[derive(Debug, Clone, FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub password_hash: Option<String>,
    pub avatar_file_id: Option<Uuid>,
    pub bio: Option<String>,
    pub locale: String,
    pub preferred_status: String,
    pub is_bot: bool,
    pub auth_version: i32,
    pub disabled_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<UserRecord> for User {
    fn from(value: UserRecord) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            email: value.email,
            email_verified_at: value.email_verified_at,
            avatar_file_id: value.avatar_file_id,
            bio: value.bio,
            locale: value.locale,
            preferred_status: value.preferred_status,
            is_bot: value.is_bot,
            disabled_at: value.disabled_at,
            deleted_at: value.deleted_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl UserRecord {
    pub fn is_active(&self) -> bool {
        self.disabled_at.is_none() && self.deleted_at.is_none()
    }

    pub fn public(&self) -> User {
        self.clone().into()
    }
}

/// User settings returned to the client.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct UserSettings {
    pub theme: String,
    pub accent_color: String,
    pub compact_messages: bool,
    pub show_typing_indicators: bool,
    pub allow_friend_requests_from: String,
    pub allow_dms_from: String,
    pub presence_visibility: String,
    pub read_receipts: bool,
    pub custom_status: Option<String>,
    pub custom_status_emoji: Option<String>,
}

/// Input for a new account.
pub struct NewUser {
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password_hash: String,
    pub locale: String,
    pub email_verified: bool,
}

#[derive(Debug, Default)]
pub struct ProfileUpdate {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub locale: Option<String>,
    pub preferred_status: Option<String>,
}

#[derive(Debug, Default)]
pub struct SettingsUpdate {
    pub theme: Option<String>,
    pub accent_color: Option<String>,
    pub compact_messages: Option<bool>,
    pub show_typing_indicators: Option<bool>,
    pub allow_friend_requests_from: Option<String>,
    pub allow_dms_from: Option<String>,
    pub presence_visibility: Option<String>,
    pub read_receipts: Option<bool>,
    pub custom_status: Option<String>,
    pub custom_status_emoji: Option<String>,
}

const USER_COLUMNS: &str = r#"
    id, username::text AS username, display_name, email::text AS email,
    email_verified_at, password_hash, avatar_file_id, bio, locale,
    preferred_status, is_bot, auth_version, disabled_at, deleted_at,
    created_at, updated_at
"#;

/// User repository and service.
#[derive(Clone)]
pub struct UserService {
    pool: PgPool,
}

impl UserService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a user and default settings in one transaction.
    pub async fn create_user(&self, new: NewUser) -> Result<User, UserError> {
        let username = normalize_username(&new.username);
        let email = normalize_email(&new.email);
        let id = tulpar_core::new_id();
        let email_verified_at = new.email_verified.then(Utc::now);

        let mut tx = self.pool.begin().await?;

        let inserted = sqlx::query_as::<_, UserRecord>(&format!(
            r#"
            INSERT INTO users (
                id, username, display_name, email, email_verified_at, password_hash, locale
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING {USER_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(&username)
        .bind(&new.display_name)
        .bind(&email)
        .bind(email_verified_at)
        .bind(&new.password_hash)
        .bind(&new.locale)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_user_error)?;

        sqlx::query(
            "INSERT INTO user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(inserted.public())
    }

    pub async fn find_by_login(&self, login: &str) -> Result<Option<UserRecord>, UserError> {
        let normalized = login.trim().to_lowercase();
        let record = sqlx::query_as::<_, UserRecord>(&format!(
            r#"
            SELECT {USER_COLUMNS}
            FROM users
            WHERE (username = $1 OR email = $1) AND deleted_at IS NULL
            LIMIT 1
            "#
        ))
        .bind(normalized)
        .fetch_optional(&self.pool)
        .await?;
        Ok(record)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<UserRecord>, UserError> {
        let normalized = normalize_email(email);
        let record = sqlx::query_as::<_, UserRecord>(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE email = $1 AND deleted_at IS NULL LIMIT 1"
        ))
        .bind(normalized)
        .fetch_optional(&self.pool)
        .await?;
        Ok(record)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<UserRecord>, UserError> {
        let normalized = normalize_username(username);
        let record = sqlx::query_as::<_, UserRecord>(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE username = $1 AND deleted_at IS NULL LIMIT 1"
        ))
        .bind(normalized)
        .fetch_optional(&self.pool)
        .await?;
        Ok(record)
    }

    pub async fn get_record(&self, id: Uuid) -> Result<UserRecord, UserError> {
        let record = sqlx::query_as::<_, UserRecord>(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE id = $1 LIMIT 1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UserError::NotFound)?;
        Ok(record)
    }

    pub async fn get_user(&self, id: Uuid) -> Result<User, UserError> {
        Ok(self.get_record(id).await?.public())
    }

    /// Search active users by username or display name.
    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<User>, UserError> {
        let pattern = format!("%{}%", query.trim().replace(['%', '_'], ""));
        let rows = sqlx::query_as::<_, UserRecord>(&format!(
            "SELECT {USER_COLUMNS} FROM users
             WHERE deleted_at IS NULL AND disabled_at IS NULL
               AND (username::text ILIKE $1 OR display_name ILIKE $1)
             ORDER BY display_name, id
             LIMIT $2"
        ))
        .bind(pattern)
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(User::from).collect())
    }

    pub async fn set_email_verified(&self, id: Uuid) -> Result<(), UserError> {
        let result = sqlx::query(
            "UPDATE users SET email_verified_at = now(), updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(UserError::NotFound);
        }
        Ok(())
    }

    pub async fn update_password(&self, id: Uuid, password_hash: &str) -> Result<(), UserError> {
        let mut tx = self.pool.begin().await?;
        self.update_password_tx(&mut tx, id, password_hash).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn update_password_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        password_hash: &str,
    ) -> Result<(), UserError> {
        let result = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $2, auth_version = auth_version + 1, updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(password_hash)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(UserError::NotFound);
        }
        Ok(())
    }

    pub async fn set_email_verified_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<(), UserError> {
        let result = sqlx::query(
            "UPDATE users SET email_verified_at = now(), updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(UserError::NotFound);
        }
        Ok(())
    }

    pub async fn update_profile(&self, id: Uuid, update: ProfileUpdate) -> Result<User, UserError> {
        let record = sqlx::query_as::<_, UserRecord>(&format!(
            r#"
            UPDATE users
            SET display_name = COALESCE($2, display_name),
                bio = CASE WHEN $3::text IS NULL THEN bio WHEN $3 = '' THEN NULL ELSE $3 END,
                locale = COALESCE($4, locale),
                preferred_status = COALESCE($5, preferred_status),
                updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING {USER_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(update.display_name)
        .bind(update.bio)
        .bind(update.locale)
        .bind(update.preferred_status)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UserError::NotFound)?;
        Ok(record.public())
    }

    pub async fn get_settings(&self, user_id: Uuid) -> Result<UserSettings, UserError> {
        let settings = sqlx::query_as::<_, UserSettings>(
            r#"
            SELECT theme, accent_color, compact_messages, show_typing_indicators,
                   allow_friend_requests_from, allow_dms_from, presence_visibility,
                   read_receipts, custom_status, custom_status_emoji
            FROM user_settings
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UserError::NotFound)?;
        Ok(settings)
    }

    pub async fn update_settings(
        &self,
        user_id: Uuid,
        update: SettingsUpdate,
    ) -> Result<UserSettings, UserError> {
        let settings = sqlx::query_as::<_, UserSettings>(
            r#"
            UPDATE user_settings
            SET theme = COALESCE($2, theme),
                accent_color = COALESCE($3, accent_color),
                compact_messages = COALESCE($4, compact_messages),
                show_typing_indicators = COALESCE($5, show_typing_indicators),
                allow_friend_requests_from = COALESCE($6, allow_friend_requests_from),
                allow_dms_from = COALESCE($7, allow_dms_from),
                presence_visibility = COALESCE($8, presence_visibility),
                read_receipts = COALESCE($9, read_receipts),
                custom_status = CASE
                    WHEN $10::text IS NULL THEN custom_status
                    WHEN $10 = '' THEN NULL
                    ELSE $10
                END,
                custom_status_emoji = CASE
                    WHEN $11::text IS NULL THEN custom_status_emoji
                    WHEN $11 = '' THEN NULL
                    ELSE $11
                END,
                updated_at = now()
            WHERE user_id = $1
            RETURNING theme, accent_color, compact_messages, show_typing_indicators,
                      allow_friend_requests_from, allow_dms_from, presence_visibility,
                      read_receipts, custom_status, custom_status_emoji
            "#,
        )
        .bind(user_id)
        .bind(update.theme)
        .bind(update.accent_color)
        .bind(update.compact_messages)
        .bind(update.show_typing_indicators)
        .bind(update.allow_friend_requests_from)
        .bind(update.allow_dms_from)
        .bind(update.presence_visibility)
        .bind(update.read_receipts)
        .bind(update.custom_status)
        .bind(update.custom_status_emoji)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UserError::NotFound)?;
        Ok(settings)
    }

    pub async fn is_instance_admin(&self, user_id: Uuid) -> Result<bool, UserError> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_admins WHERE user_id = $1)")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(exists)
    }

    /// Anonymize a deleted account inside a transaction.
    ///
    /// This Phase 1 version covers the tables that exist now. Later phases must
    /// extend it for communities, messages, DMs, and files.
    pub async fn anonymize_account_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        original_username: &str,
        reserved_until: DateTime<Utc>,
    ) -> Result<(), UserError> {
        let deleted_username = format!("deleted_{}", &user_id.simple().to_string()[..16]);
        let deleted_email = format!("deleted+{user_id}@invalid.local");

        let result = sqlx::query(
            r#"
            UPDATE users
            SET username = $2,
                display_name = 'Deleted user',
                email = $3,
                email_verified_at = NULL,
                password_hash = NULL,
                avatar_file_id = NULL,
                bio = NULL,
                locale = 'en',
                preferred_status = 'invisible',
                disabled_at = now(),
                disabled_reason = 'account_deleted',
                deleted_at = now(),
                auth_version = auth_version + 1,
                updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(deleted_username)
        .bind(deleted_email)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(UserError::NotFound);
        }

        sqlx::query(
            r#"
            INSERT INTO username_reservations (username, reserved_until)
            VALUES ($1, $2)
            ON CONFLICT (username) DO UPDATE SET reserved_until = EXCLUDED.reserved_until
            "#,
        )
        .bind(original_username)
        .bind(reserved_until)
        .execute(&mut **tx)
        .await?;

        sqlx::query("DELETE FROM user_settings WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }
}

fn map_user_error(error: sqlx::Error) -> UserError {
    if let sqlx::Error::Database(ref database_error) = error {
        if database_error.is_unique_violation() {
            return UserError::Conflict;
        }
    }
    UserError::Database(error)
}
