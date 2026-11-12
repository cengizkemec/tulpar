use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use users::{User, UserSettings};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
pub struct SelfUserDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub email_verified: bool,
    pub avatar_file_id: Option<Uuid>,
    pub bio: Option<String>,
    pub locale: String,
    pub preferred_status: String,
    pub is_bot: bool,
    pub created_at: DateTime<Utc>,
}

impl From<&User> for SelfUserDto {
    fn from(value: &User) -> Self {
        Self {
            id: value.id,
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            email: value.email.clone(),
            email_verified: value.email_verified_at.is_some(),
            avatar_file_id: value.avatar_file_id,
            bio: value.bio.clone(),
            locale: value.locale.clone(),
            preferred_status: value.preferred_status.clone(),
            is_bot: value.is_bot,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicUserDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
    pub bio: Option<String>,
    pub is_bot: bool,
}

impl From<&User> for PublicUserDto {
    fn from(value: &User) -> Self {
        Self {
            id: value.id,
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            avatar_file_id: value.avatar_file_id,
            bio: value.bio.clone(),
            is_bot: value.is_bot,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserSettingsDto {
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

impl From<UserSettings> for UserSettingsDto {
    fn from(value: UserSettings) -> Self {
        Self {
            theme: value.theme,
            accent_color: value.accent_color,
            compact_messages: value.compact_messages,
            show_typing_indicators: value.show_typing_indicators,
            allow_friend_requests_from: value.allow_friend_requests_from,
            allow_dms_from: value.allow_dms_from,
            presence_visibility: value.presence_visibility,
            read_receipts: value.read_receipts,
            custom_status: value.custom_status,
            custom_status_emoji: value.custom_status_emoji,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionDto {
    pub id: Uuid,
    pub device_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub current: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserExport {
    pub exported_at: DateTime<Utc>,
    pub user: SelfUserDto,
    pub settings: UserSettingsDto,
    pub sessions: Vec<SessionDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub session_id: Uuid,
    pub user: SelfUserDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeviceTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub session_id: Uuid,
    pub user: SelfUserDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegistrationResponse {
    pub accepted: bool,
    pub verification_required: bool,
    pub login_required: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeviceCodeResponseDto {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VersionResponse {
    pub name: String,
    pub version: String,
    pub api_version: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LivenessResponse {
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub checks: HealthChecks,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthChecks {
    pub database: String,
    pub redis: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub client: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogoutRequest {
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmailRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub login: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReauthRequest {
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteAccountRequest {
    pub password: String,
    pub confirmation: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProfileUpdateRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub preferred_status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SettingsUpdateRequest {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub compact_messages: Option<bool>,
    #[serde(default)]
    pub show_typing_indicators: Option<bool>,
    #[serde(default)]
    pub allow_friend_requests_from: Option<String>,
    #[serde(default)]
    pub allow_dms_from: Option<String>,
    #[serde(default)]
    pub presence_visibility: Option<String>,
    #[serde(default)]
    pub read_receipts: Option<bool>,
    #[serde(default)]
    pub custom_status: Option<String>,
    #[serde(default)]
    pub custom_status_emoji: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeviceCodeRequest {
    pub client_name: String,
    #[serde(default = "default_desktop")]
    pub platform: String,
}

fn default_desktop() -> String {
    "desktop".to_string()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeviceApproveRequest {
    pub user_code: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeviceTokenRequest {
    pub device_code: String,
}
