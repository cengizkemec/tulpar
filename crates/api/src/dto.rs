use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use users::{User, UserSettings};
use utoipa::ToSchema;
use uuid::Uuid;

use channels::{Channel, DirectConversation};
use communities::{Community, CommunityAccess, Invite, MemberView, Role};
use social::{BlockedUser, FriendUser, FriendshipView};

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

#[derive(Debug, Serialize, ToSchema)]
pub struct CommunityDto {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub icon_file_id: Option<Uuid>,
    pub banner_file_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub visibility: String,
    pub default_notifications: String,
    pub created_at: DateTime<Utc>,
}

impl From<Community> for CommunityDto {
    fn from(value: Community) -> Self {
        Self {
            id: value.id,
            name: value.name,
            slug: value.slug,
            description: value.description,
            icon_file_id: value.icon_file_id,
            banner_file_id: value.banner_file_id,
            owner_user_id: value.owner_user_id,
            visibility: value.visibility,
            default_notifications: value.default_notifications,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoleDto {
    pub id: Uuid,
    pub name: String,
    pub color: i32,
    pub position: i32,
    pub permissions: Vec<String>,
    pub is_default: bool,
    pub is_managed: bool,
    pub mentionable: bool,
}

impl From<Role> for RoleDto {
    fn from(value: Role) -> Self {
        Self {
            id: value.id,
            name: value.name,
            color: value.color,
            position: value.position,
            permissions: value
                .permissions
                .names()
                .into_iter()
                .map(str::to_string)
                .collect(),
            is_default: value.is_default,
            is_managed: value.is_managed,
            mentionable: value.mentionable,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberDto {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
    pub nickname: Option<String>,
    pub joined_at: DateTime<Utc>,
    pub communication_disabled_until: Option<DateTime<Utc>>,
}

impl From<MemberView> for MemberDto {
    fn from(value: MemberView) -> Self {
        Self {
            user_id: value.user_id,
            username: value.username,
            display_name: value.display_name,
            avatar_file_id: value.avatar_file_id,
            nickname: value.nickname,
            joined_at: value.joined_at,
            communication_disabled_until: value.communication_disabled_until,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelDto {
    pub id: Uuid,
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
}

impl From<Channel> for ChannelDto {
    fn from(value: Channel) -> Self {
        Self {
            id: value.id,
            community_id: value.community_id,
            parent_id: value.parent_id,
            kind: value.kind,
            name: value.name,
            topic: value.topic,
            position: value.position,
            nsfw: value.nsfw,
            slowmode_seconds: value.slowmode_seconds,
            user_limit: value.user_limit,
            bitrate: value.bitrate,
            last_message_at: value.last_message_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InviteDto {
    pub code: String,
    pub community_id: Uuid,
    pub channel_id: Option<Uuid>,
    pub max_uses: Option<i32>,
    pub uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Invite> for InviteDto {
    fn from(value: Invite) -> Self {
        Self {
            code: value.code,
            community_id: value.community_id,
            channel_id: value.channel_id,
            max_uses: value.max_uses,
            uses: value.uses,
            expires_at: value.expires_at,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommunityDetailsDto {
    pub community: CommunityDto,
    pub channels: Vec<ChannelDto>,
    pub roles: Vec<RoleDto>,
    pub member_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommunityPermissionsDto {
    pub permissions: Vec<String>,
    pub is_owner: bool,
    pub highest_role_position: i32,
    pub timed_out_until: Option<DateTime<Utc>>,
}

impl From<CommunityAccess> for CommunityPermissionsDto {
    fn from(value: CommunityAccess) -> Self {
        Self {
            permissions: value
                .permissions
                .names()
                .into_iter()
                .map(str::to_string)
                .collect(),
            is_owner: value.is_owner,
            highest_role_position: value.highest_role_position,
            timed_out_until: value.member.communication_disabled_until,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitePreviewDto {
    pub invite: InviteDto,
    pub community: CommunityDto,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCommunityRequest {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_private")]
    pub visibility: String,
}

fn default_private() -> String {
    "private".to_string()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCommunityRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRoleRequest {
    pub name: String,
    #[serde(default)]
    pub color: Option<i32>,
    #[serde(default)]
    pub position: Option<i32>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub mentionable: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateRoleRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<i32>,
    #[serde(default)]
    pub position: Option<i32>,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    #[serde(default)]
    pub mentionable: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMemberRequest {
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateChannelRequest {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub position: Option<i32>,
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInviteRequest {
    #[serde(default)]
    pub channel_id: Option<Uuid>,
    #[serde(default)]
    pub max_uses: Option<i32>,
    #[serde(default)]
    pub expires_in_hours: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BanMemberRequest {
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FriendUserDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
}

impl From<FriendUser> for FriendUserDto {
    fn from(value: FriendUser) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            avatar_file_id: value.avatar_file_id,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FriendshipDto {
    pub id: Uuid,
    pub user: FriendUserDto,
    pub status: String,
    pub direction: String,
    pub created_at: DateTime<Utc>,
}

impl From<FriendshipView> for FriendshipDto {
    fn from(value: FriendshipView) -> Self {
        Self {
            id: value.id,
            user: value.user.into(),
            status: value.status,
            direction: value.direction,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BlockedUserDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl From<BlockedUser> for BlockedUserDto {
    fn from(value: BlockedUser) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            avatar_file_id: value.avatar_file_id,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendFriendRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDmRequest {
    pub recipient_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DirectConversationDto {
    pub channel_id: Uuid,
    pub other_user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

impl From<DirectConversation> for DirectConversationDto {
    fn from(value: DirectConversation) -> Self {
        Self {
            channel_id: value.channel.id,
            other_user_id: value.other_user_id,
            created_at: value.channel.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinResponse {
    pub community_id: Uuid,
    pub already_member: bool,
}
