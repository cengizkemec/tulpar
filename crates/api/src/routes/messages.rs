use axum::{
    extract::{Path, Query, State},
    Json,
};
use channels::Channel;
use communities::CommunityAccess;
use messaging::{Message, NewMessage};
use permissions::Permissions;
use uuid::Uuid;

use crate::{
    dto::{
        EditMessageRequest, MessageDto, MessageListQuery, MessageSearchQuery, ReactionDto,
        ReadStateDto, ReadStateRequest, SendMessageRequest,
    },
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
};

struct ChannelAuthorization {
    channel: Channel,
    community: Option<CommunityAccess>,
    permissions: Option<Permissions>,
}

impl ChannelAuthorization {
    fn require(&self, permission: Permissions) -> Result<(), ApiError> {
        if let Some(permissions) = &self.permissions {
            if !permissions.contains(permission) {
                return Err(ApiError::forbidden(
                    "permission_denied",
                    "You do not have permission to perform this action.",
                ));
            }
        }
        Ok(())
    }

    fn can(&self, permission: Permissions) -> bool {
        self.permissions
            .as_ref()
            .is_none_or(|permissions| permissions.contains(permission))
    }

    fn community_id(&self) -> Option<Uuid> {
        self.community.as_ref().map(|access| access.community.id)
    }
}

async fn authorize_channel(
    state: &AppState,
    user_id: Uuid,
    channel_id: Uuid,
) -> Result<ChannelAuthorization, ApiError> {
    let channel = state.channels.get(channel_id).await?;
    if channel.scope == "direct" {
        let other = state
            .channels
            .direct_other_user(channel_id, user_id)
            .await?;
        if other.is_none() {
            return Err(ApiError::not_found("Channel not found."));
        }
        return Ok(ChannelAuthorization {
            channel,
            community: None,
            permissions: None,
        });
    }

    let (channel, access, permissions) =
        super::channels::channel_access(state, user_id, channel_id).await?;
    if !permissions.contains(Permissions::VIEW_CHANNEL) {
        return Err(ApiError::not_found("Channel not found."));
    }
    Ok(ChannelAuthorization {
        channel,
        community: Some(access),
        permissions: Some(permissions),
    })
}

async fn authorize_message(
    state: &AppState,
    user_id: Uuid,
    message_id: Uuid,
) -> Result<(Message, ChannelAuthorization), ApiError> {
    let message = state.messages.get(message_id).await?;
    let authorization = authorize_channel(state, user_id, message.channel_id).await?;
    Ok((message, authorization))
}

#[utoipa::path(
    post,
    path = "/api/v1/channels/{channel_id}/messages",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    request_body = SendMessageRequest,
    responses((status = 200, body = MessageDto))
)]
pub async fn send_message(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<Json<MessageDto>, ApiError> {
    let authorization = authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::SEND_MESSAGES)?;

    if authorization.community.is_some() {
        let bypass = authorization.can(Permissions::BYPASS_SLOWMODE)
            || authorization.can(Permissions::MANAGE_MESSAGES);
        if !bypass {
            if let Some(last) = state
                .messages
                .last_authored_at(channel_id, current.user().id)
                .await?
            {
                let elapsed = (chrono::Utc::now() - last).num_seconds().max(0);
                let remaining = authorization.channel.slowmode_seconds as i64 - elapsed;
                if remaining > 0 {
                    return Err(ApiError::rate_limited(std::time::Duration::from_secs(
                        remaining as u64,
                    )));
                }
            }
        }
    }

    let (message, _created) = state
        .messages
        .create(NewMessage {
            channel_id,
            author_id: current.user().id,
            content: payload.content,
            reply_to_id: payload.reply_to_id,
            client_nonce: payload.client_nonce,
        })
        .await?;
    Ok(Json(message.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/messages",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("before" = Option<Uuid>, Query, description = "Return messages older than this ID"),
        ("after" = Option<Uuid>, Query, description = "Return messages newer than this ID"),
        ("limit" = Option<i64>, Query, description = "Maximum messages, 1-100")
    ),
    responses((status = 200, body = Vec<MessageDto>))
)]
pub async fn list_messages(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    Query(params): Query<MessageListQuery>,
) -> Result<Json<Vec<MessageDto>>, ApiError> {
    let authorization = authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::READ_MESSAGE_HISTORY)?;
    let messages = state
        .messages
        .list(
            channel_id,
            params.before,
            params.after,
            params.limit.unwrap_or(50),
        )
        .await?;
    Ok(Json(messages.into_iter().map(MessageDto::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/messages/search",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("query" = Option<String>, Query, description = "Full-text search query"),
        ("limit" = Option<i64>, Query, description = "Maximum results, 1-100")
    ),
    responses((status = 200, body = Vec<MessageDto>))
)]
pub async fn search_messages(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    Query(params): Query<MessageSearchQuery>,
) -> Result<Json<Vec<MessageDto>>, ApiError> {
    let authorization = authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::READ_MESSAGE_HISTORY)?;
    let query = params
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| value.chars().count() >= 2)
        .ok_or_else(|| ApiError::validation("Search query must be at least 2 characters."))?;
    let messages = state
        .messages
        .search(channel_id, query, params.limit.unwrap_or(25))
        .await?;
    Ok(Json(messages.into_iter().map(MessageDto::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/messages/{message_id}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("message_id" = Uuid, Path, description = "Message identifier")),
    responses((status = 200, body = MessageDto))
)]
pub async fn get_message(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<MessageDto>, ApiError> {
    let (message, _) = authorize_message(&state, current.user().id, message_id).await?;
    Ok(Json(message.into()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/messages/{message_id}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("message_id" = Uuid, Path, description = "Message identifier")),
    request_body = EditMessageRequest,
    responses((status = 200, body = MessageDto))
)]
pub async fn edit_message(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(message_id): Path<Uuid>,
    Json(payload): Json<EditMessageRequest>,
) -> Result<Json<MessageDto>, ApiError> {
    let (message, _) = authorize_message(&state, current.user().id, message_id).await?;
    if message.author_id != Some(current.user().id) {
        return Err(ApiError::forbidden(
            "not_author",
            "Only the author can edit this message.",
        ));
    }
    let message = state.messages.edit(message_id, &payload.content).await?;
    Ok(Json(message.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/messages/{message_id}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("message_id" = Uuid, Path, description = "Message identifier")),
    responses((status = 204, description = "Message deleted"))
)]
pub async fn delete_message(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(message_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    let (message, authorization) = authorize_message(&state, current.user().id, message_id).await?;
    if message.author_id != Some(current.user().id) {
        authorization.require(Permissions::MANAGE_MESSAGES)?;
    }
    state
        .messages
        .soft_delete(message_id, current.user().id)
        .await?;
    if let Some(community_id) = authorization.community_id() {
        state
            .record_audit(
                Some(current.user().id),
                "message.delete",
                Some(community_id),
                Some(("message", message_id)),
            )
            .await;
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/messages/{message_id}/reactions",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("message_id" = Uuid, Path, description = "Message identifier")),
    responses((status = 200, body = Vec<ReactionDto>))
)]
pub async fn list_reactions(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<Vec<ReactionDto>>, ApiError> {
    let (_, authorization) = authorize_message(&state, current.user().id, message_id).await?;
    authorization.require(Permissions::READ_MESSAGE_HISTORY)?;
    let reactions = state.messages.list_reactions(message_id).await?;
    Ok(Json(reactions.into_iter().map(ReactionDto::from).collect()))
}

#[utoipa::path(
    put,
    path = "/api/v1/messages/{message_id}/reactions/{emoji}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("message_id" = Uuid, Path, description = "Message identifier"),
        ("emoji" = String, Path, description = "Reaction emoji")
    ),
    responses((status = 204, description = "Reaction added"))
)]
pub async fn add_reaction(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((message_id, emoji)): Path<(Uuid, String)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let (message, authorization) = authorize_message(&state, current.user().id, message_id).await?;
    if message.deleted_at.is_some() {
        return Err(ApiError::not_found("Message not found."));
    }
    authorization.require(Permissions::ADD_REACTIONS)?;
    state
        .messages
        .add_reaction(message_id, current.user().id, &emoji)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/messages/{message_id}/reactions/{emoji}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("message_id" = Uuid, Path, description = "Message identifier"),
        ("emoji" = String, Path, description = "Reaction emoji")
    ),
    responses((status = 204, description = "Reaction removed"))
)]
pub async fn remove_reaction(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((message_id, emoji)): Path<(Uuid, String)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let (_, _) = authorize_message(&state, current.user().id, message_id).await?;
    state
        .messages
        .remove_reaction(message_id, current.user().id, &emoji)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/read-state",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    responses((status = 200, body = ReadStateDto))
)]
pub async fn get_read_state(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<ReadStateDto>, ApiError> {
    authorize_channel(&state, current.user().id, channel_id).await?;
    let read_state = state
        .messages
        .read_state(current.user().id, channel_id)
        .await?;
    Ok(Json(read_state.into()))
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/read-state",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    request_body = ReadStateRequest,
    responses((status = 200, body = ReadStateDto))
)]
pub async fn set_read_state(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    Json(payload): Json<ReadStateRequest>,
) -> Result<Json<ReadStateDto>, ApiError> {
    authorize_channel(&state, current.user().id, channel_id).await?;
    let read_state = state
        .messages
        .set_read_state(current.user().id, channel_id, payload.message_id)
        .await?;
    Ok(Json(read_state.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/pins",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    responses((status = 200, body = Vec<MessageDto>))
)]
pub async fn list_pins(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<MessageDto>>, ApiError> {
    let authorization = authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::READ_MESSAGE_HISTORY)?;
    let messages = state.messages.list_pins(channel_id).await?;
    Ok(Json(messages.into_iter().map(MessageDto::from).collect()))
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/pins/{message_id}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("message_id" = Uuid, Path, description = "Message identifier")
    ),
    responses((status = 204, description = "Message pinned"))
)]
pub async fn pin_message(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let authorization = authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::PIN_MESSAGES)?;
    state
        .messages
        .pin(channel_id, message_id, current.user().id)
        .await?;
    if let Some(community_id) = authorization.community_id() {
        state
            .record_audit(
                Some(current.user().id),
                "message.pin",
                Some(community_id),
                Some(("message", message_id)),
            )
            .await;
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/pins/{message_id}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("message_id" = Uuid, Path, description = "Message identifier")
    ),
    responses((status = 204, description = "Message unpinned"))
)]
pub async fn unpin_message(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let authorization = authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::PIN_MESSAGES)?;
    state.messages.unpin(channel_id, message_id).await?;
    if let Some(community_id) = authorization.community_id() {
        state
            .record_audit(
                Some(current.user().id),
                "message.unpin",
                Some(community_id),
                Some(("message", message_id)),
            )
            .await;
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}
