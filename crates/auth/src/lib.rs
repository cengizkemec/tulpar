//! Authentication, sessions, refresh rotation, and device authorization.

#![forbid(unsafe_code)]

mod email;
mod password;

use std::{cmp::min, net::IpAddr, sync::Arc};

use audit::{AuditError, AuditEvent, AuditService};
use chrono::{DateTime, Duration, Utc};
use common::{
    normalize_email, normalize_password, normalize_text, normalize_username, random_token,
    token_hash, validate_email, validate_username,
};
use config::AppConfig;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use users::{NewUser, User, UserError, UserRecord, UserService};
use uuid::Uuid;

pub use email::{EmailError, EmailSender, LogEmailSender, SmtpEmailSender};
pub use password::PasswordService;

/// Errors returned by authentication operations.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("user error: {0}")]
    User(#[from] UserError),
    #[error("audit error: {0}")]
    Audit(#[from] AuditError),
    #[error("password error: {0}")]
    PasswordHash(String),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("email verification is required")]
    EmailNotVerified,
    #[error("registration is disabled")]
    RegistrationDisabled,
    #[error("username is already taken")]
    UsernameTaken,
    #[error("too many active sessions")]
    TooManySessions,
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid or expired token")]
    InvalidToken,
    #[error("session not found")]
    SessionNotFound,
    #[error("re-authentication is required")]
    ReauthRequired,
    #[error("email delivery failed")]
    Email(#[from] EmailError),
    #[error("internal authentication error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct RegisterInput {
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub locale: String,
}

#[derive(Debug, Clone)]
pub struct RegistrationResult {
    pub accepted: bool,
    pub verification_required: bool,
}

#[derive(Debug, Clone)]
pub struct LoginInput {
    pub login: String,
    pub password: String,
    pub device_name: Option<String>,
    pub platform: String,
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IssuedSession {
    pub tokens: AuthTokens,
    pub user: User,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub session_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessTokenOnly {
    pub access_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub device_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub current: bool,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user: User,
    pub session_id: Uuid,
    pub auth_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Clone)]
pub enum DevicePoll {
    Pending,
    Denied,
    Expired,
    Approved(Box<IssuedSession>),
}

type RefreshTokenRow = (
    Uuid,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
);

type SessionAuthRow = (
    Uuid,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
    DateTime<Utc>,
    i32,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
);

type SessionListRow = (
    Uuid,
    Option<String>,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
    DateTime<Utc>,
    bool,
);

type TokenConsumptionRow = (Uuid, Uuid, DateTime<Utc>, Option<DateTime<Utc>>);

type DeviceAuthorizationRow = (
    Uuid,
    Option<Uuid>,
    String,
    Option<String>,
    String,
    String,
    DateTime<Utc>,
);

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    iss: String,
    sub: Uuid,
    aud: String,
    sid: Uuid,
    ver: i32,
    auth_time: i64,
    iat: i64,
    exp: i64,
    jti: Uuid,
}

#[derive(Clone)]
pub struct AuthService {
    pool: PgPool,
    config: Arc<AppConfig>,
    users: UserService,
    passwords: PasswordService,
    audit: AuditService,
    email: Arc<dyn EmailSender>,
}

impl AuthService {
    pub fn new(
        pool: PgPool,
        config: Arc<AppConfig>,
        email: Arc<dyn EmailSender>,
    ) -> Result<Self, AuthError> {
        Ok(Self {
            users: UserService::new(pool.clone()),
            audit: AuditService::new(pool.clone()),
            passwords: PasswordService::new()?,
            pool,
            config,
            email,
        })
    }

    pub async fn register(&self, input: RegisterInput) -> Result<RegistrationResult, AuthError> {
        if !self.config.auth.registration_enabled {
            return Err(AuthError::RegistrationDisabled);
        }

        let username = normalize_username(&input.username);
        validate_username(&username).map_err(|error| AuthError::InvalidInput(error.to_string()))?;

        let display_name = normalize_text(&input.display_name);
        if !(1..=64).contains(&display_name.chars().count()) {
            return Err(AuthError::InvalidInput(
                "display name must be between 1 and 64 characters".into(),
            ));
        }

        let email = normalize_email(&input.email);
        if !validate_email(&email) {
            return Err(AuthError::InvalidInput("invalid email address".into()));
        }

        validate_password(&input.password, &self.config)?;
        let locale = if input.locale.trim().is_empty() {
            "en".to_string()
        } else {
            normalize_text(&input.locale)
        };

        if self.users.find_by_username(&username).await?.is_some() {
            return Err(AuthError::UsernameTaken);
        }

        let require_verification = self.config.auth.require_email_verification;

        // Do not reveal whether an email is already registered.
        if self.users.find_by_email(&email).await?.is_some() {
            return Ok(RegistrationResult {
                accepted: true,
                verification_required: require_verification,
            });
        }

        let password_hash = self
            .passwords
            .hash(normalize_password(&input.password))
            .await?;

        let user = match self
            .users
            .create_user(NewUser {
                username,
                display_name,
                email: email.clone(),
                password_hash,
                locale,
                email_verified: !require_verification,
            })
            .await
        {
            Ok(user) => user,
            // A concurrent registration or a username collision is handled generically.
            Err(UserError::Conflict) => {
                return Ok(RegistrationResult {
                    accepted: true,
                    verification_required: require_verification,
                })
            }
            Err(error) => return Err(error.into()),
        };

        if require_verification {
            let token = self.create_verification_token(user.id).await?;
            if let Err(error) = self
                .email
                .send_verification(&email, &self.link("verify-email", &token))
                .await
            {
                tracing::error!(%error, "verification email could not be sent");
            }
        }

        self.record(
            AuditEvent::system("auth.registration")
                .user(user.id)
                .target("user", user.id)
                .metadata(json!({ "verification_required": require_verification })),
        )
        .await?;

        Ok(RegistrationResult {
            accepted: true,
            verification_required: require_verification,
        })
    }

    pub async fn login(&self, input: LoginInput) -> Result<IssuedSession, AuthError> {
        if !matches!(
            input.platform.as_str(),
            "web" | "desktop" | "embedded" | "api"
        ) {
            return Err(AuthError::InvalidInput("invalid client platform".into()));
        }

        let login = input.login.trim().to_lowercase();
        let record = self.users.find_by_login(&login).await?;
        let password_hash = record.as_ref().and_then(|user| user.password_hash.clone());
        let password_ok = self.passwords.verify(input.password, password_hash).await;

        let Some(record) = record.filter(|_| password_ok) else {
            self.record(
                AuditEvent::system("auth.login.failed")
                    .request(input.ip, input.user_agent.clone())
                    .metadata(json!({ "reason": "invalid_credentials" })),
            )
            .await?;
            return Err(AuthError::InvalidCredentials);
        };

        if !record.is_active() {
            self.record(
                AuditEvent::system("auth.login.failed")
                    .request(input.ip, input.user_agent.clone())
                    .metadata(json!({ "reason": "inactive_account" })),
            )
            .await?;
            return Err(AuthError::InvalidCredentials);
        }

        if self.config.auth.require_email_verification && record.email_verified_at.is_none() {
            self.record(
                AuditEvent::system("auth.login.verification_required")
                    .user(record.id)
                    .request(input.ip, input.user_agent.clone()),
            )
            .await?;
            return Err(AuthError::EmailNotVerified);
        }

        let active_sessions = self.active_session_count(record.id).await?;
        if active_sessions >= self.config.auth.max_sessions_per_user {
            return Err(AuthError::TooManySessions);
        }

        let issued = self
            .create_session(
                &record,
                input.device_name,
                &input.platform,
                input.ip,
                input.user_agent,
            )
            .await?;

        self.record(
            AuditEvent::system("auth.login.succeeded")
                .user(record.id)
                .target("session", issued.tokens.session_id)
                .request(input.ip, None)
                .metadata(json!({ "platform": input.platform })),
        )
        .await?;

        Ok(issued)
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<AuthTokens, AuthError> {
        let hash = token_hash(refresh_token);
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;

        let token: Option<RefreshTokenRow> = sqlx::query_as(
            r#"
                SELECT session_id, used_at, revoked_at, expires_at
                FROM refresh_tokens
                WHERE token_hash = $1
                FOR UPDATE
                "#,
        )
        .bind(&hash[..])
        .fetch_optional(&mut *tx)
        .await?;

        let Some((session_id, used_at, revoked_at, expires_at)) = token else {
            return Err(AuthError::InvalidToken);
        };

        if used_at.is_some() {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now(), revoked_reason = 'refresh_reuse' WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE refresh_tokens SET revoked_at = now() WHERE session_id = $1 AND revoked_at IS NULL",
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            self.record(
                AuditEvent::system("auth.refresh.reuse_detected")
                    .target("session", session_id)
                    .metadata(json!({ "reason": "refresh_token_reuse" })),
            )
            .await?;
            return Err(AuthError::InvalidToken);
        }

        if revoked_at.is_some() || expires_at <= now {
            return Err(AuthError::InvalidToken);
        }

        let session: Option<SessionAuthRow> = sqlx::query_as(
            r#"
            SELECT s.user_id, s.revoked_at, s.expires_at, s.last_verified_at,
                   u.auth_version, u.disabled_at, u.deleted_at
            FROM sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.id = $1
            FOR UPDATE OF s, u
            "#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((
            user_id,
            session_revoked_at,
            session_expires_at,
            auth_time,
            auth_version,
            disabled_at,
            deleted_at,
        )) = session
        else {
            return Err(AuthError::InvalidToken);
        };

        if session_revoked_at.is_some()
            || session_expires_at <= now
            || disabled_at.is_some()
            || deleted_at.is_some()
        {
            return Err(AuthError::InvalidToken);
        }

        let new_refresh = random_token(32);
        let new_hash = token_hash(&new_refresh);
        let new_id = tulpar_core::new_id();
        let new_expires = min(
            now + Duration::seconds(self.config.auth.refresh_token_ttl_seconds),
            session_expires_at,
        );

        sqlx::query(
            "INSERT INTO refresh_tokens (id, session_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(new_id)
        .bind(session_id)
        .bind(&new_hash[..])
        .bind(new_expires)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE refresh_tokens SET used_at = now(), replaced_by_id = $1 WHERE token_hash = $2",
        )
        .bind(new_id)
        .bind(&hash[..])
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        let access_token = self.issue_access_token(user_id, session_id, auth_time, auth_version)?;
        Ok(AuthTokens {
            access_token,
            refresh_token: new_refresh,
            expires_in: self.config.auth.access_token_ttl_seconds,
            session_id,
        })
    }

    pub async fn logout(&self, refresh_token: &str) -> Result<(), AuthError> {
        let hash = token_hash(refresh_token);
        let mut tx = self.pool.begin().await?;
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT s.id, s.user_id
            FROM refresh_tokens rt
            JOIN sessions s ON s.id = rt.session_id
            WHERE rt.token_hash = $1
            "#,
        )
        .bind(&hash[..])
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((session_id, user_id)) = row {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now(), revoked_reason = 'logout' WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE refresh_tokens SET revoked_at = now() WHERE session_id = $1 AND revoked_at IS NULL",
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            self.record(
                AuditEvent::system("auth.logout")
                    .user(user_id)
                    .target("session", session_id),
            )
            .await?;
        } else {
            tx.rollback().await?;
        }

        Ok(())
    }

    pub async fn logout_session(&self, user_id: Uuid, session_id: Uuid) -> Result<(), AuthError> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = now(), revoked_reason = 'logout' WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AuthError::SessionNotFound);
        }
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = now() WHERE session_id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.record(
            AuditEvent::system("auth.session.revoked")
                .user(user_id)
                .target("session", session_id),
        )
        .await?;
        Ok(())
    }

    pub async fn revoke_other_sessions(
        &self,
        user_id: Uuid,
        current_session_id: Uuid,
    ) -> Result<(), AuthError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE sessions SET revoked_at = now(), revoked_reason = 'logout_all' WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(current_session_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE refresh_tokens SET revoked_at = now()
            WHERE revoked_at IS NULL
              AND session_id IN (
                  SELECT id FROM sessions WHERE user_id = $1 AND id <> $2
              )
            "#,
        )
        .bind(user_id)
        .bind(current_session_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.record(
            AuditEvent::system("auth.sessions.revoked_others")
                .user(user_id)
                .target("user", user_id),
        )
        .await?;
        Ok(())
    }

    pub async fn list_sessions(
        &self,
        user_id: Uuid,
        current_session_id: Uuid,
    ) -> Result<Vec<SessionInfo>, AuthError> {
        let rows: Vec<SessionListRow> = sqlx::query_as(
            r#"
            SELECT id, device_name, platform, created_at, last_seen_at, expires_at,
                   (id = $2) AS current
            FROM sessions
            WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()
            ORDER BY last_seen_at DESC
            "#,
        )
        .bind(user_id)
        .bind(current_session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, device_name, platform, created_at, last_seen_at, expires_at, current)| {
                    SessionInfo {
                        id,
                        device_name,
                        platform,
                        created_at,
                        last_seen_at,
                        expires_at,
                        current,
                    }
                },
            )
            .collect())
    }

    pub async fn authenticate_access_token(
        &self,
        access_token: &str,
    ) -> Result<AuthenticatedUser, AuthError> {
        let claims = self.decode_access_token(access_token)?;
        let record = self.users.get_record(claims.sub).await?;
        if !record.is_active() || record.auth_version != claims.ver {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(AuthenticatedUser {
            session_id: claims.sid,
            auth_time: DateTime::from_timestamp(claims.auth_time, 0)
                .ok_or(AuthError::InvalidToken)?,
            user: record.public(),
        })
    }

    pub async fn verify_email(&self, token: &str) -> Result<(), AuthError> {
        let hash = token_hash(token);
        let mut tx = self.pool.begin().await?;
        let row: Option<TokenConsumptionRow> = sqlx::query_as(
            r#"
            SELECT id, user_id, expires_at, consumed_at
            FROM email_verification_tokens
            WHERE token_hash = $1
            FOR UPDATE
            "#,
        )
        .bind(&hash[..])
        .fetch_optional(&mut *tx)
        .await?;

        let Some((token_id, user_id, expires_at, consumed_at)) = row else {
            return Err(AuthError::InvalidToken);
        };
        if consumed_at.is_some() || expires_at <= Utc::now() {
            return Err(AuthError::InvalidToken);
        }

        sqlx::query("UPDATE email_verification_tokens SET consumed_at = now() WHERE id = $1")
            .bind(token_id)
            .execute(&mut *tx)
            .await?;
        self.users.set_email_verified_tx(&mut tx, user_id).await?;
        tx.commit().await?;
        self.record(
            AuditEvent::system("auth.email.verified")
                .user(user_id)
                .target("user", user_id),
        )
        .await?;
        Ok(())
    }

    pub async fn resend_verification(&self, email: &str) -> Result<(), AuthError> {
        let email = normalize_email(email);
        if let Some(user) = self.users.find_by_email(&email).await? {
            if user.email_verified_at.is_none() {
                let token = self.create_verification_token(user.id).await?;
                if let Err(error) = self
                    .email
                    .send_verification(&email, &self.link("verify-email", &token))
                    .await
                {
                    tracing::error!(%error, "verification email could not be sent");
                }
            }
        }
        Ok(())
    }

    pub async fn request_password_reset(&self, login: &str) -> Result<(), AuthError> {
        if let Some(user) = self.users.find_by_login(login).await? {
            let token = self.create_password_reset_token(user.id, None).await?;
            if let Err(error) = self
                .email
                .send_password_reset(&user.email, &self.link("reset-password", &token))
                .await
            {
                tracing::error!(%error, "password reset email could not be sent");
            }
            self.record(
                AuditEvent::system("auth.password.reset_requested")
                    .user(user.id)
                    .target("user", user.id),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn reset_password(&self, token: &str, new_password: &str) -> Result<(), AuthError> {
        validate_password(new_password, &self.config)?;
        let hash = token_hash(token);
        let new_password = normalize_password(new_password);
        let new_hash = self.passwords.hash(new_password).await?;

        let mut tx = self.pool.begin().await?;
        let row: Option<TokenConsumptionRow> = sqlx::query_as(
            r#"
            SELECT id, user_id, expires_at, consumed_at
            FROM password_reset_tokens
            WHERE token_hash = $1
            FOR UPDATE
            "#,
        )
        .bind(&hash[..])
        .fetch_optional(&mut *tx)
        .await?;

        let Some((token_id, user_id, expires_at, consumed_at)) = row else {
            return Err(AuthError::InvalidToken);
        };
        if consumed_at.is_some() || expires_at <= Utc::now() {
            return Err(AuthError::InvalidToken);
        }

        sqlx::query("UPDATE password_reset_tokens SET consumed_at = now() WHERE id = $1")
            .bind(token_id)
            .execute(&mut *tx)
            .await?;
        self.users
            .update_password_tx(&mut tx, user_id, &new_hash)
            .await?;
        self.revoke_all_sessions_tx(&mut tx, user_id, "password_reset")
            .await?;
        tx.commit().await?;

        self.record(
            AuditEvent::system("auth.password.reset")
                .user(user_id)
                .target("user", user_id),
        )
        .await?;
        Ok(())
    }

    pub async fn change_password(
        &self,
        user_id: Uuid,
        current_session_id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<AccessTokenOnly, AuthError> {
        validate_password(new_password, &self.config)?;
        let record = self.users.get_record(user_id).await?;
        if !self
            .passwords
            .verify(current_password.to_string(), record.password_hash.clone())
            .await
        {
            return Err(AuthError::InvalidCredentials);
        }

        let new_hash = self
            .passwords
            .hash(normalize_password(new_password))
            .await?;
        let mut tx = self.pool.begin().await?;
        self.users
            .update_password_tx(&mut tx, user_id, &new_hash)
            .await?;
        self.revoke_other_sessions_tx(&mut tx, user_id, current_session_id, "password_change")
            .await?;
        sqlx::query("UPDATE sessions SET last_verified_at = now() WHERE id = $1 AND user_id = $2")
            .bind(current_session_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        let updated = self.users.get_record(user_id).await?;
        let access_token = self.issue_access_token(
            user_id,
            current_session_id,
            Utc::now(),
            updated.auth_version,
        )?;
        self.record(
            AuditEvent::system("auth.password.changed")
                .user(user_id)
                .target("user", user_id),
        )
        .await?;
        Ok(AccessTokenOnly {
            access_token,
            expires_in: self.config.auth.access_token_ttl_seconds,
        })
    }

    pub async fn delete_account(&self, user_id: Uuid, password: &str) -> Result<(), AuthError> {
        let record = self.users.get_record(user_id).await?;
        if !record.is_active() {
            return Err(AuthError::InvalidCredentials);
        }
        if !self
            .passwords
            .verify(password.to_string(), record.password_hash.clone())
            .await
        {
            return Err(AuthError::InvalidCredentials);
        }

        let admin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM instance_admins WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
        if admin_count > 0 {
            let total_admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM instance_admins")
                .fetch_one(&self.pool)
                .await?;
            if total_admins <= 1 {
                return Err(AuthError::InvalidInput(
                    "the last instance administrator cannot delete their account".into(),
                ));
            }
        }

        let reserved_until = Utc::now() + Duration::days(90);
        let mut tx = self.pool.begin().await?;
        self.users
            .anonymize_account_tx(&mut tx, user_id, &record.username, reserved_until)
            .await?;
        sqlx::query("DELETE FROM instance_admins WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM email_verification_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM device_authorizations WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        self.revoke_all_sessions_tx(&mut tx, user_id, "account_deleted")
            .await?;
        tx.commit().await?;

        self.record(
            AuditEvent::system("auth.account.deleted")
                .user(user_id)
                .target("user", user_id)
                .metadata(json!({ "retention_days": 30, "username_reserved_days": 90 })),
        )
        .await?;
        Ok(())
    }

    pub async fn reauth(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        password: &str,
    ) -> Result<AccessTokenOnly, AuthError> {
        let record = self.users.get_record(user_id).await?;
        if !self
            .passwords
            .verify(password.to_string(), record.password_hash.clone())
            .await
        {
            return Err(AuthError::InvalidCredentials);
        }

        let result = sqlx::query(
            "UPDATE sessions SET last_verified_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AuthError::SessionNotFound);
        }

        let access_token =
            self.issue_access_token(user_id, session_id, Utc::now(), record.auth_version)?;
        self.record(
            AuditEvent::system("auth.reauthenticated")
                .user(user_id)
                .target("session", session_id),
        )
        .await?;
        Ok(AccessTokenOnly {
            access_token,
            expires_in: self.config.auth.access_token_ttl_seconds,
        })
    }

    pub async fn start_device_authorization(
        &self,
        client_name: &str,
        platform: &str,
        ip: Option<IpAddr>,
    ) -> Result<DeviceCodeResponse, AuthError> {
        if !matches!(platform, "desktop" | "web") {
            return Err(AuthError::InvalidInput("invalid device platform".into()));
        }
        let client_name = normalize_text(client_name);
        if client_name.is_empty() || client_name.chars().count() > 100 {
            return Err(AuthError::InvalidInput("invalid client name".into()));
        }

        let device_code = random_token(32);
        let hash = token_hash(&device_code);
        let user_code = generate_user_code();
        let expires_at = Utc::now() + Duration::seconds(600);

        sqlx::query(
            r#"
            INSERT INTO device_authorizations (
                id, device_code_hash, user_code, client_name, requested_ip, platform, expires_at
            )
            VALUES ($1, $2, $3, $4, $5::inet, $6, $7)
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(&hash[..])
        .bind(&user_code)
        .bind(&client_name)
        .bind(ip.map(|value| value.to_string()))
        .bind(platform)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(DeviceCodeResponse {
            device_code,
            user_code,
            verification_uri: format!(
                "{}/device",
                self.config.instance.public_url.trim_end_matches('/')
            ),
            expires_in: 600,
            interval: 5,
        })
    }

    pub async fn approve_device(&self, user_id: Uuid, user_code: &str) -> Result<(), AuthError> {
        let code = normalize_text(user_code).to_uppercase();
        let result = sqlx::query(
            r#"
            UPDATE device_authorizations
            SET status = 'approved', user_id = $1, approved_at = now()
            WHERE user_code = $2 AND status = 'pending' AND expires_at > now()
            "#,
        )
        .bind(user_id)
        .bind(&code)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AuthError::InvalidToken);
        }
        self.record(
            AuditEvent::system("auth.device.approved")
                .user(user_id)
                .target("device_authorization", user_id),
        )
        .await?;
        Ok(())
    }

    pub async fn deny_device(&self, user_id: Uuid, user_code: &str) -> Result<(), AuthError> {
        let code = normalize_text(user_code).to_uppercase();
        let result = sqlx::query(
            r#"
            UPDATE device_authorizations
            SET status = 'denied', user_id = $1
            WHERE user_code = $2 AND status = 'pending' AND expires_at > now()
            "#,
        )
        .bind(user_id)
        .bind(&code)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AuthError::InvalidToken);
        }
        self.record(
            AuditEvent::system("auth.device.denied")
                .user(user_id)
                .target("device_authorization", user_id),
        )
        .await?;
        Ok(())
    }

    pub async fn poll_device(&self, device_code: &str) -> Result<DevicePoll, AuthError> {
        let hash = token_hash(device_code);
        let mut tx = self.pool.begin().await?;
        let row: Option<DeviceAuthorizationRow> = sqlx::query_as(
            r#"
            SELECT id, user_id, client_name, requested_ip::text, platform, status, expires_at
            FROM device_authorizations
            WHERE device_code_hash = $1
            FOR UPDATE
            "#,
        )
        .bind(&hash[..])
        .fetch_optional(&mut *tx)
        .await?;

        let Some((row_id, user_id, client_name, requested_ip, platform, status, expires_at)) = row
        else {
            return Err(AuthError::InvalidToken);
        };

        if expires_at <= Utc::now() && status != "consumed" {
            sqlx::query("UPDATE device_authorizations SET status = 'expired' WHERE id = $1")
                .bind(row_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(DevicePoll::Expired);
        }

        match status.as_str() {
            "pending" => Ok(DevicePoll::Pending),
            "denied" => Ok(DevicePoll::Denied),
            "consumed" => Err(AuthError::InvalidToken),
            "approved" => {
                let user_id = user_id.ok_or(AuthError::InvalidToken)?;
                sqlx::query("UPDATE device_authorizations SET status = 'consumed' WHERE id = $1")
                    .bind(row_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;

                let record = self.users.get_record(user_id).await?;
                if !record.is_active() {
                    return Err(AuthError::InvalidCredentials);
                }
                let ip = requested_ip.and_then(|value| value.parse().ok());
                let issued = self
                    .create_session(&record, Some(client_name), &platform, ip, None)
                    .await?;
                self.record(
                    AuditEvent::system("auth.device.consumed")
                        .user(user_id)
                        .target("session", issued.tokens.session_id),
                )
                .await?;
                Ok(DevicePoll::Approved(Box::new(issued)))
            }
            _ => Err(AuthError::InvalidToken),
        }
    }

    async fn create_session(
        &self,
        user: &UserRecord,
        device_name: Option<String>,
        platform: &str,
        ip: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<IssuedSession, AuthError> {
        let now = Utc::now();
        let session_id = tulpar_core::new_id();
        let family_id = tulpar_core::new_id();
        let refresh_token = random_token(32);
        let refresh_hash = token_hash(&refresh_token);
        let session_expires_at =
            now + Duration::seconds(self.config.auth.session_absolute_ttl_seconds);
        let refresh_expires_at = min(
            now + Duration::seconds(self.config.auth.refresh_token_ttl_seconds),
            session_expires_at,
        );
        let device_name = device_name.map(|value| truncate(value, 100));
        let user_agent = user_agent.map(|value| truncate(value, 512));

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, user_id, token_family_id, device_name, platform, ip, user_agent,
                expires_at, last_verified_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::inet, $7, $8, $9)
            "#,
        )
        .bind(session_id)
        .bind(user.id)
        .bind(family_id)
        .bind(device_name)
        .bind(platform)
        .bind(ip.map(|value| value.to_string()))
        .bind(user_agent)
        .bind(session_expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO refresh_tokens (id, session_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(tulpar_core::new_id())
        .bind(session_id)
        .bind(&refresh_hash[..])
        .bind(refresh_expires_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE users SET last_seen_at = now() WHERE id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        let access_token = self.issue_access_token(user.id, session_id, now, user.auth_version)?;
        Ok(IssuedSession {
            tokens: AuthTokens {
                access_token,
                refresh_token,
                expires_in: self.config.auth.access_token_ttl_seconds,
                session_id,
            },
            user: user.public(),
        })
    }

    fn issue_access_token(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        auth_time: DateTime<Utc>,
        auth_version: i32,
    ) -> Result<String, AuthError> {
        let now = Utc::now();
        let claims = AccessClaims {
            iss: self
                .config
                .instance
                .public_url
                .trim_end_matches('/')
                .to_string(),
            sub: user_id,
            aud: self.config.auth.audience.clone(),
            sid: session_id,
            ver: auth_version,
            auth_time: auth_time.timestamp(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(self.config.auth.access_token_ttl_seconds)).timestamp(),
            jti: tulpar_core::new_id(),
        };
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(self.config.auth.jwt_kid.clone());
        encode(
            &header,
            &claims,
            &EncodingKey::from_secret(self.config.auth.jwt_secret.as_bytes()),
        )
        .map_err(|error| AuthError::Internal(error.to_string()))
    }

    fn decode_access_token(&self, token: &str) -> Result<AccessClaims, AuthError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[self.config.instance.public_url.trim_end_matches('/')]);
        validation.set_audience(&[self.config.auth.audience.as_str()]);
        validation.leeway = 60;
        decode::<AccessClaims>(
            token,
            &DecodingKey::from_secret(self.config.auth.jwt_secret.as_bytes()),
            &validation,
        )
        .map(|data| data.claims)
        .map_err(|_| AuthError::InvalidToken)
    }

    async fn create_verification_token(&self, user_id: Uuid) -> Result<String, AuthError> {
        let token = random_token(32);
        let hash = token_hash(&token);
        let expires_at =
            Utc::now() + Duration::seconds(self.config.auth.email_verification_ttl_seconds);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE email_verification_tokens SET consumed_at = now() WHERE user_id = $1 AND consumed_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(tulpar_core::new_id())
        .bind(user_id)
        .bind(&hash[..])
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(token)
    }

    async fn create_password_reset_token(
        &self,
        user_id: Uuid,
        ip: Option<IpAddr>,
    ) -> Result<String, AuthError> {
        let token = random_token(32);
        let hash = token_hash(&token);
        let expires_at =
            Utc::now() + Duration::seconds(self.config.auth.password_reset_ttl_seconds);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE password_reset_tokens SET consumed_at = now() WHERE user_id = $1 AND consumed_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO password_reset_tokens (id, user_id, token_hash, requested_ip, expires_at)
            VALUES ($1, $2, $3, $4::inet, $5)
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(user_id)
        .bind(&hash[..])
        .bind(ip.map(|value| value.to_string()))
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(token)
    }

    async fn active_session_count(&self, user_id: Uuid) -> Result<i64, AuthError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    async fn revoke_all_sessions_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        reason: &str,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now(), revoked_reason = $2 WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE refresh_tokens SET revoked_at = now()
            WHERE revoked_at IS NULL
              AND session_id IN (SELECT id FROM sessions WHERE user_id = $1)
            "#,
        )
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn revoke_other_sessions_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        current_session_id: Uuid,
        reason: &str,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now(), revoked_reason = $3 WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(current_session_id)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE refresh_tokens SET revoked_at = now()
            WHERE revoked_at IS NULL
              AND session_id IN (
                  SELECT id FROM sessions WHERE user_id = $1 AND id <> $2
              )
            "#,
        )
        .bind(user_id)
        .bind(current_session_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn record(&self, event: AuditEvent) -> Result<(), AuthError> {
        self.audit.record(event).await?;
        Ok(())
    }

    fn link(&self, path: &str, token: &str) -> String {
        format!(
            "{}/{}#token={}",
            self.config.instance.public_url.trim_end_matches('/'),
            path,
            token
        )
    }
}

fn validate_password(password: &str, config: &AppConfig) -> Result<(), AuthError> {
    let length = password.chars().count();
    if length < config.auth.password_min_length {
        return Err(AuthError::InvalidInput(format!(
            "password must be at least {} characters",
            config.auth.password_min_length
        )));
    }
    if length > config.auth.password_max_length {
        return Err(AuthError::InvalidInput(format!(
            "password must be at most {} characters",
            config.auth.password_max_length
        )));
    }
    if password.trim().is_empty() || password.contains('\0') {
        return Err(AuthError::InvalidInput("invalid password".into()));
    }
    Ok(())
}

fn generate_user_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    let mut code = String::with_capacity(9);
    for index in 0..8 {
        if index == 4 {
            code.push('-');
        }
        let character = ALPHABET[rng.gen_range(0..ALPHABET.len())] as char;
        code.push(character);
    }
    code
}

fn truncate(value: String, max: usize) -> String {
    value.chars().take(max).collect()
}
