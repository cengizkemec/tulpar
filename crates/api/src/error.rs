use std::time::Duration;

use auth::AuthError;
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use channels::ChannelError;
use communities::CommunityError;
use messaging::MessageError;
use notifications::NotificationError;
use serde::Serialize;
use social::SocialError;
use storage::StorageError;
use users::UserError;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldError>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// API error mapped to an RFC 9457 problem+json response.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub fields: Option<Vec<FieldError>>,
    pub retry_after: Option<Duration>,
}

impl ApiError {
    pub fn new(
        status: StatusCode,
        code: &'static str,
        title: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code,
            title,
            detail: detail.into(),
            fields: None,
            retry_after: None,
        }
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "Invalid request",
            detail,
        )
    }

    pub fn unauthorized(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            code,
            "Authentication required",
            detail,
        )
    }

    pub fn forbidden(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, "Permission denied", detail)
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "Not found", detail)
    }

    pub fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, "Conflict", detail)
    }

    pub fn rate_limited(retry_after: Duration) -> Self {
        let mut error = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many requests",
            "Try again later.",
        );
        error.retry_after = Some(retry_after);
        error
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            detail,
        )
    }

    pub fn dependency_unavailable(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "dependency_unavailable",
            "Service temporarily unavailable",
            detail,
        )
    }

    pub fn with_fields(mut self, fields: Vec<FieldError>) -> Self {
        self.fields = Some(fields);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ProblemDetails {
            problem_type: format!("urn:tulpar:error:{}", self.code),
            title: self.title.to_string(),
            status: self.status.as_u16(),
            detail: self.detail,
            code: self.code.to_string(),
            errors: self.fields,
            request_id: None,
        };

        let mut response = (self.status, axum::Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if let Some(retry_after) = self.retry_after {
            if let Ok(value) = HeaderValue::from_str(&retry_after.as_secs().to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidCredentials => {
                Self::unauthorized("invalid_credentials", "The credentials are invalid.")
            }
            AuthError::EmailNotVerified => Self::forbidden(
                "email_verification_required",
                "Verify your email address before signing in.",
            ),
            AuthError::RegistrationDisabled => {
                Self::forbidden("registration_disabled", "Registration is disabled.")
            }
            AuthError::UsernameTaken => {
                Self::conflict("username_taken", "That username is already in use.")
            }
            AuthError::TooManySessions => Self::conflict(
                "too_many_sessions",
                "Too many active sessions. Revoke one before signing in.",
            ),
            AuthError::InvalidToken => {
                Self::unauthorized("invalid_token", "The token is invalid or expired.")
            }
            AuthError::SessionNotFound => Self::not_found("Session not found."),
            AuthError::ReauthRequired => Self::forbidden(
                "reauth_required",
                "Confirm your password before performing this action.",
            ),
            AuthError::InvalidInput(message) => Self::validation(message),
            AuthError::Email(error) => {
                tracing::error!(%error, "email delivery failed");
                Self::dependency_unavailable("Email delivery is temporarily unavailable.")
            }
            AuthError::User(UserError::NotFound) => Self::not_found("User not found."),
            AuthError::User(UserError::Conflict) => {
                Self::conflict("conflict", "The resource already exists.")
            }
            AuthError::User(UserError::Invalid(message)) => Self::validation(message),
            AuthError::User(UserError::Database(error)) => {
                tracing::error!(%error, "user database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
            AuthError::Database(error) => {
                tracing::error!(%error, "database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
            AuthError::Audit(error) => {
                tracing::error!(%error, "audit database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
            AuthError::PasswordHash(error) => {
                tracing::error!(%error, "password hashing error");
                Self::internal("An internal error occurred.")
            }
            AuthError::Internal(error) => {
                tracing::error!(%error, "internal authentication error");
                Self::internal("An internal error occurred.")
            }
        }
    }
}

impl From<UserError> for ApiError {
    fn from(error: UserError) -> Self {
        match error {
            UserError::NotFound => Self::not_found("User not found."),
            UserError::Conflict => Self::conflict("conflict", "The resource already exists."),
            UserError::Invalid(message) => Self::validation(message),
            UserError::Database(error) => {
                tracing::error!(%error, "user database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
        }
    }
}

impl From<CommunityError> for ApiError {
    fn from(error: CommunityError) -> Self {
        match error {
            CommunityError::NotFound
            | CommunityError::MemberNotFound
            | CommunityError::RoleNotFound => Self::not_found("Community resource not found."),
            CommunityError::InviteInvalid => Self::new(
                StatusCode::GONE,
                "invite_expired",
                "Invite no longer valid",
                "The invite is expired, revoked, or exhausted.",
            ),
            CommunityError::AlreadyMember => {
                Self::conflict("already_member", "You are already a member.")
            }
            CommunityError::Banned => {
                Self::forbidden("banned", "You are banned from this community.")
            }
            CommunityError::RoleConflict => {
                Self::conflict("role_conflict", "A role with that name already exists.")
            }
            CommunityError::Invalid(message) => Self::validation(message),
            CommunityError::Database(error) => {
                tracing::error!(%error, "community database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
        }
    }
}

impl From<ChannelError> for ApiError {
    fn from(error: ChannelError) -> Self {
        match error {
            ChannelError::NotFound | ChannelError::ConversationNotFound => {
                Self::not_found("Channel not found.")
            }
            ChannelError::Invalid(message) => Self::validation(message),
            ChannelError::Database(error) => {
                tracing::error!(%error, "channel database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
        }
    }
}

impl From<NotificationError> for ApiError {
    fn from(error: NotificationError) -> Self {
        match error {
            NotificationError::NotFound => Self::not_found("Notification not found."),
            NotificationError::Invalid(message) => Self::validation(message),
            NotificationError::Database(error) => {
                tracing::error!(%error, "notification database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
        }
    }
}

impl From<StorageError> for ApiError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Invalid(message) => Self::validation(message),
            StorageError::NotFound => Self::not_found("File not found."),
            StorageError::Io(error) => {
                tracing::error!(%error, "storage I/O error");
                Self::dependency_unavailable("Storage is temporarily unavailable.")
            }
        }
    }
}

impl From<MessageError> for ApiError {
    fn from(error: MessageError) -> Self {
        match error {
            MessageError::NotFound => Self::not_found("Message not found."),
            MessageError::Invalid(message) => Self::validation(message),
            MessageError::ReplyTargetInvalid => {
                Self::validation("The reply target must belong to the same channel.")
            }
            MessageError::Database(error) => {
                tracing::error!(%error, "message database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
        }
    }
}

impl From<SocialError> for ApiError {
    fn from(error: SocialError) -> Self {
        match error {
            SocialError::FriendshipNotFound => Self::not_found("Friend request not found."),
            SocialError::AlreadyExists => {
                Self::conflict("already_exists", "A friendship or request already exists.")
            }
            SocialError::SelfAction => {
                Self::validation("You cannot perform that action on yourself.")
            }
            SocialError::RequestsDisabled => Self::forbidden(
                "friend_requests_disabled",
                "This user does not accept friend requests.",
            ),
            SocialError::DmDisabled => Self::forbidden(
                "dm_disabled",
                "This user does not accept direct messages from you.",
            ),
            SocialError::Blocked => Self::forbidden("blocked", "This user is blocked."),
            SocialError::Database(error) => {
                tracing::error!(%error, "social database error");
                Self::dependency_unavailable("The database is temporarily unavailable.")
            }
        }
    }
}

pub fn log_and_internal(message: &str, error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, "{message}");
    ApiError::internal("An internal error occurred.")
}
