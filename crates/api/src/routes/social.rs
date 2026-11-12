use std::time::Duration as StdDuration;

use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::{
    dto::{
        BlockedUserDto, CreateDmRequest, DirectConversationDto, FriendUserDto, FriendshipDto,
        SendFriendRequest,
    },
    error::ApiError,
    extract::{enforce_rate_limit, CurrentUser},
    state::AppState,
};

async fn limit_social(
    state: &AppState,
    action: &str,
    user_id: Uuid,
    limit: u64,
) -> Result<(), ApiError> {
    enforce_rate_limit(
        state,
        &format!("social:{action}:{user_id}"),
        limit,
        StdDuration::from_secs(3600),
    )
    .await
}

#[utoipa::path(
    get,
    path = "/api/v1/friends",
    tag = "social",
    security(("bearer_auth" = [])),
    responses((status = 200, body = Vec<FriendUserDto>))
)]
pub async fn list_friends(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<FriendUserDto>>, ApiError> {
    let friends = state.social.list_friends(current.user().id).await?;
    Ok(Json(friends.into_iter().map(FriendUserDto::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/friends/requests",
    tag = "social",
    security(("bearer_auth" = [])),
    responses((status = 200, body = Vec<FriendshipDto>))
)]
pub async fn list_requests(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<FriendshipDto>>, ApiError> {
    let requests = state.social.list_requests(current.user().id).await?;
    Ok(Json(
        requests.into_iter().map(FriendshipDto::from).collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/friends/requests",
    tag = "social",
    security(("bearer_auth" = [])),
    request_body = SendFriendRequest,
    responses((status = 204, description = "Friend request sent"))
)]
pub async fn send_request(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<SendFriendRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let target = state.users.get_user(payload.user_id).await?;
    if target.deleted_at.is_some() || target.disabled_at.is_some() {
        return Err(ApiError::not_found("User not found."));
    }
    limit_social(&state, "friend-request", current.user().id, 60).await?;
    state
        .social
        .send_request(current.user().id, payload.user_id)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "friend.request",
            None,
            Some(("user", payload.user_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/friends/requests/{request_id}/accept",
    tag = "social",
    security(("bearer_auth" = [])),
    params(("request_id" = Uuid, Path, description = "Friend request identifier")),
    responses((status = 204, description = "Friend request accepted"))
)]
pub async fn accept_request(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(request_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .social
        .accept_request(current.user().id, request_id)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "friend.accept",
            None,
            Some(("friendship", request_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/friends/requests/{request_id}/decline",
    tag = "social",
    security(("bearer_auth" = [])),
    params(("request_id" = Uuid, Path, description = "Friend request identifier")),
    responses((status = 204, description = "Friend request declined"))
)]
pub async fn decline_request(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(request_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .social
        .decline_request(current.user().id, request_id)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "friend.decline",
            None,
            Some(("friendship", request_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/friends/{user_id}",
    tag = "social",
    security(("bearer_auth" = [])),
    params(("user_id" = Uuid, Path, description = "Friend identifier")),
    responses((status = 204, description = "Friend removed"))
)]
pub async fn remove_friend(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .social
        .remove_friend(current.user().id, user_id)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "friend.remove",
            None,
            Some(("user", user_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/blocks",
    tag = "social",
    security(("bearer_auth" = [])),
    responses((status = 200, body = Vec<BlockedUserDto>))
)]
pub async fn list_blocks(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<BlockedUserDto>>, ApiError> {
    let blocks = state.social.list_blocks(current.user().id).await?;
    Ok(Json(blocks.into_iter().map(BlockedUserDto::from).collect()))
}

#[utoipa::path(
    put,
    path = "/api/v1/blocks/{user_id}",
    tag = "social",
    security(("bearer_auth" = [])),
    params(("user_id" = Uuid, Path, description = "User identifier")),
    responses((status = 204, description = "User blocked"))
)]
pub async fn block_user(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.users.get_user(user_id).await?;
    state.social.block(current.user().id, user_id).await?;
    state
        .record_audit(
            Some(current.user().id),
            "user.block",
            None,
            Some(("user", user_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/blocks/{user_id}",
    tag = "social",
    security(("bearer_auth" = [])),
    params(("user_id" = Uuid, Path, description = "User identifier")),
    responses((status = 204, description = "User unblocked"))
)]
pub async fn unblock_user(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.social.unblock(current.user().id, user_id).await?;
    state
        .record_audit(
            Some(current.user().id),
            "user.unblock",
            None,
            Some(("user", user_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/dms",
    tag = "social",
    security(("bearer_auth" = [])),
    responses((status = 200, body = Vec<DirectConversationDto>))
)]
pub async fn list_dms(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<DirectConversationDto>>, ApiError> {
    let conversations = state
        .channels
        .list_direct_for_user(current.user().id)
        .await?;
    Ok(Json(
        conversations
            .into_iter()
            .map(DirectConversationDto::from)
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/dms",
    tag = "social",
    security(("bearer_auth" = [])),
    request_body = CreateDmRequest,
    responses((status = 200, body = DirectConversationDto))
)]
pub async fn create_dm(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<CreateDmRequest>,
) -> Result<Json<DirectConversationDto>, ApiError> {
    state.users.get_user(payload.recipient_id).await?;
    limit_social(&state, "dm-create", current.user().id, 120).await?;
    state
        .social
        .can_dm(current.user().id, payload.recipient_id)
        .await?;
    let channel = state
        .channels
        .create_direct(current.user().id, payload.recipient_id, current.user().id)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "dm.create",
            None,
            Some(("channel", channel.id)),
        )
        .await;
    Ok(Json(DirectConversationDto {
        channel_id: channel.id,
        other_user_id: payload.recipient_id,
        created_at: channel.created_at,
    }))
}
