use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use users::{AdminUser, User, UserSettings};
use utoipa::ToSchema;
use uuid::Uuid;

use audit::AuditLog;
use channels::{Channel, DirectConversation};
use communities::{BanView, Community, CommunityAccess, Invite, MemberView, Role};
use messaging::{Attachment, Message, Reaction, ReadState};
use notifications::{Notification, NotificationPreferences};
use social::{BlockedUser, FriendUser, FriendshipView};
use voice::{IceServer, VoiceParticipant};

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
pub struct ChannelOverwriteDto {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub role_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

impl From<channels::ChannelOverwrite> for ChannelOverwriteDto {
    fn from(value: channels::ChannelOverwrite) -> Self {
        Self {
            id: value.id,
            channel_id: value.channel_id,
            role_id: value.role_id,
            user_id: value.user_id,
            allow: value
                .allow_permissions()
                .names()
                .into_iter()
                .map(str::to_string)
                .collect(),
            deny: value
                .deny_permissions()
                .names()
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct OverwriteRequest {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelPermissionsDto {
    pub permissions: Vec<String>,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct BanDto {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub moderator_id: Option<Uuid>,
    pub reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<BanView> for BanDto {
    fn from(value: BanView) -> Self {
        Self {
            user_id: value.user_id,
            username: value.username,
            display_name: value.display_name,
            moderator_id: value.moderator_id,
            reason: value.reason,
            expires_at: value.expires_at,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditLogDto {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_username: Option<String>,
    pub actor_type: String,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl From<AuditLog> for AuditLogDto {
    fn from(value: AuditLog) -> Self {
        Self {
            id: value.id,
            actor_user_id: value.actor_user_id,
            actor_username: value.actor_username,
            actor_type: value.actor_type,
            action: value.action,
            target_type: value.target_type,
            target_id: value.target_id,
            metadata: value.metadata,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CommunitySearchQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MemberSearchQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuditLogQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RolePositionRequest {
    pub role_id: Uuid,
    pub position: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReorderRolesRequest {
    pub roles: Vec<RolePositionRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TransferOwnershipRequest {
    pub new_owner_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UserSearchQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttachmentDto {
    pub id: Uuid,
    pub message_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub spoiler: bool,
    pub created_at: DateTime<Utc>,
}

impl From<Attachment> for AttachmentDto {
    fn from(value: Attachment) -> Self {
        Self {
            id: value.id,
            message_id: value.message_id,
            filename: value.filename,
            content_type: value.content_type,
            size_bytes: value.size_bytes,
            spoiler: value.spoiler,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageDto {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub author_id: Option<Uuid>,
    pub content: Option<String>,
    pub reply_to_id: Option<Uuid>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub attachments: Vec<AttachmentDto>,
}

impl MessageDto {
    pub fn from_message(value: Message, attachments: Vec<Attachment>) -> Self {
        Self {
            id: value.id,
            channel_id: value.channel_id,
            author_id: value.author_id,
            content: value.content,
            reply_to_id: value.reply_to_id,
            edited_at: value.edited_at,
            deleted_at: value.deleted_at,
            created_at: value.created_at,
            attachments: attachments.into_iter().map(AttachmentDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UploadedFileDto {
    pub id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReactionDto {
    pub user_id: Uuid,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
}

impl From<Reaction> for ReactionDto {
    fn from(value: Reaction) -> Self {
        Self {
            user_id: value.user_id,
            emoji: value.emoji,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadStateDto {
    pub channel_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub unread_count: i64,
}

impl From<ReadState> for ReadStateDto {
    fn from(value: ReadState) -> Self {
        Self {
            channel_id: value.channel_id,
            last_read_message_id: value.last_read_message_id,
            last_read_at: value.last_read_at,
            unread_count: value.unread_count,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendMessageRequest {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reply_to_id: Option<Uuid>,
    #[serde(default)]
    pub client_nonce: Option<Uuid>,
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditMessageRequest {
    pub content: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageListQuery {
    #[serde(default)]
    pub before: Option<Uuid>,
    #[serde(default)]
    pub after: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageSearchQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReadStateRequest {
    pub message_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationDto {
    pub id: Uuid,
    pub kind: String,
    pub community_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub message_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Notification> for NotificationDto {
    fn from(value: Notification) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            community_id: value.community_id,
            channel_id: value.channel_id,
            message_id: value.message_id,
            actor_user_id: value.actor_user_id,
            payload: value.payload,
            read_at: value.read_at,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationPreferencesDto {
    pub desktop_enabled: bool,
    pub sound_enabled: bool,
    pub mentions_only: bool,
    pub suppress_everyone: bool,
    pub email_enabled: bool,
    pub updated_at: DateTime<Utc>,
}

impl From<NotificationPreferences> for NotificationPreferencesDto {
    fn from(value: NotificationPreferences) -> Self {
        Self {
            desktop_enabled: value.desktop_enabled,
            sound_enabled: value.sound_enabled,
            mentions_only: value.mentions_only,
            suppress_everyone: value.suppress_everyone,
            email_enabled: value.email_enabled,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NotificationListQuery {
    #[serde(default)]
    pub before: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkNotificationsReadRequest {
    #[serde(default)]
    pub ids: Option<Vec<Uuid>>,
    #[serde(default)]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNotificationPreferencesRequest {
    #[serde(default)]
    pub desktop_enabled: Option<bool>,
    #[serde(default)]
    pub sound_enabled: Option<bool>,
    #[serde(default)]
    pub mentions_only: Option<bool>,
    #[serde(default)]
    pub suppress_everyone: Option<bool>,
    #[serde(default)]
    pub email_enabled: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarkNotificationsReadResponse {
    pub updated: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GatewayTicketDto {
    pub ticket: String,
    pub expires_in: u64,
    pub gateway_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncCommunityDto {
    pub community: CommunityDto,
    pub roles: Vec<RoleDto>,
    pub channels: Vec<ChannelDto>,
    pub read_states: Vec<ReadStateDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncInstanceDto {
    pub name: String,
    pub public_url: String,
    pub features: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncGatewayDto {
    pub url: String,
    pub heartbeat_interval_ms: u64,
    pub protocol_version: u16,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncResponse {
    pub server_time: DateTime<Utc>,
    pub instance: SyncInstanceDto,
    pub user: SelfUserDto,
    pub settings: UserSettingsDto,
    pub communities: Vec<SyncCommunityDto>,
    pub friends: Vec<FriendUserDto>,
    pub friend_requests: Vec<FriendshipDto>,
    pub blocks: Vec<BlockedUserDto>,
    pub dms: Vec<DirectConversationDto>,
    pub unread_counts: std::collections::HashMap<Uuid, i64>,
    pub notifications_unread: i64,
    pub gateway: SyncGatewayDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncInstanceResponse {
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub public_url: String,
    pub features: serde_json::Value,
    pub registration_enabled: bool,
    pub require_email_verification: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoiceTokenRequest {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IceServerDto {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

impl From<IceServer> for IceServerDto {
    fn from(value: IceServer) -> Self {
        Self {
            urls: value.urls,
            username: value.username,
            credential: value.credential,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VoiceTokenDto {
    pub url: String,
    pub room: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub ice_servers: Vec<IceServerDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VoiceParticipantDto {
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub display_name: String,
    pub joined_at: DateTime<Utc>,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub server_mute: bool,
    pub server_deaf: bool,
    pub tracks: Vec<VoiceTrackDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VoiceTrackDto {
    pub sid: String,
    pub source: String,
    pub kind: String,
    pub muted: bool,
}

impl From<VoiceParticipant> for VoiceParticipantDto {
    fn from(value: VoiceParticipant) -> Self {
        Self {
            user_id: value.user_id,
            channel_id: value.channel_id,
            display_name: value.display_name,
            joined_at: value.joined_at,
            self_mute: value.self_mute,
            self_deaf: value.self_deaf,
            server_mute: value.server_mute,
            server_deaf: value.server_deaf,
            tracks: value
                .tracks
                .into_iter()
                .map(|track| VoiceTrackDto {
                    sid: track.sid,
                    source: track.source,
                    kind: track.kind,
                    muted: track.muted,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoicePasswordRequest {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoiceModerationRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoiceMoveRequest {
    pub channel_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LiveKitWebhookAck {
    pub received: bool,
    pub duplicate: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOverviewDto {
    pub users: i64,
    pub communities: i64,
    pub messages: i64,
    pub active_sessions: i64,
    pub files: i64,
    pub voice_channels: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminUserDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub email_verified: bool,
    pub is_bot: bool,
    pub disabled_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub is_instance_admin: bool,
}

impl From<AdminUser> for AdminUserDto {
    fn from(value: AdminUser) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            email: value.email,
            email_verified: value.email_verified_at.is_some(),
            is_bot: value.is_bot,
            disabled_at: value.disabled_at,
            deleted_at: value.deleted_at,
            created_at: value.created_at,
            is_instance_admin: value.is_instance_admin,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminUsersQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminUpdateUserRequest {
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub instance_admin: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminAuditQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmbedAppDto {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub public_key: String,
    pub allowed_origins: Vec<String>,
    pub widgets: Vec<String>,
    pub allow_authenticated: bool,
    pub allow_message_preview: bool,
    pub public_channel_id: Option<Uuid>,
    pub token_version: i32,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEmbedAppRequest {
    pub name: String,
    pub allowed_origins: Vec<String>,
    pub widgets: Vec<String>,
    #[serde(default)]
    pub allow_authenticated: bool,
    #[serde(default)]
    pub allow_message_preview: bool,
    #[serde(default)]
    pub public_channel_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEmbedAppRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub allowed_origins: Option<Vec<String>>,
    #[serde(default)]
    pub widgets: Option<Vec<String>>,
    #[serde(default)]
    pub allow_authenticated: Option<bool>,
    #[serde(default)]
    pub allow_message_preview: Option<bool>,
    #[serde(default)]
    pub public_channel_id: Option<Uuid>,
    #[serde(default)]
    pub revoked: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmbedAuthorizeRequest {
    pub public_key: String,
    pub origin: String,
    pub widget: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmbedAuthorizeResponse {
    pub code: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmbedTokenRequest {
    pub public_key: String,
    pub origin: String,
    pub widget: String,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmbedTokenResponse {
    pub token: String,
    pub expires_in: i64,
    pub widget: String,
    pub community_id: Uuid,
    pub authenticated: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmbedCommunityDto {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i64,
    pub voice_channel_count: i64,
    pub public_url: String,
    pub authenticated: bool,
    pub messages: Vec<EmbedMessageDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmbedMessageDto {
    pub id: Uuid,
    pub author: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyDto {
    pub id: Uuid,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub bot_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub api_key: String,
    pub key: ApiKeyDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookDto {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWebhookRequest {
    pub url: String,
    pub events: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateWebhookResponse {
    pub webhook: WebhookDto,
    pub secret: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BotDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBotRequest {
    pub username: String,
    pub display_name: String,
}
