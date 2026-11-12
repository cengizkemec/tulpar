use axum::{
    extract::{Path, State},
    Json,
};
use channels::{Channel, ChannelOverwrite};
use chrono::Utc;
use communities::CommunityAccess;
use permissions::{
    Overwrite as PermissionOverwrite, OverwriteTarget, PermissionContext, Permissions, Role,
};
use uuid::Uuid;

use crate::{
    dto::{ChannelDto, ChannelOverwriteDto, ChannelPermissionsDto, OverwriteRequest},
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
};

fn permission_target(overwrite: &ChannelOverwrite) -> Option<OverwriteTarget> {
    match (overwrite.role_id, overwrite.user_id) {
        (Some(role_id), None) => Some(OverwriteTarget::Role(role_id)),
        (None, Some(user_id)) => Some(OverwriteTarget::Member(user_id)),
        _ => None,
    }
}

fn permission_overwrites(overwrites: &[ChannelOverwrite]) -> Vec<PermissionOverwrite> {
    overwrites
        .iter()
        .filter_map(|overwrite| {
            let target = permission_target(overwrite)?;
            Some(PermissionOverwrite::new(
                target,
                overwrite.allow_permissions(),
                overwrite.deny_permissions(),
            ))
        })
        .collect()
}

pub(crate) async fn effective_permissions(
    state: &AppState,
    access: &CommunityAccess,
    channel: &Channel,
) -> Result<Permissions, ApiError> {
    let category_overwrites = match channel.parent_id {
        Some(parent_id) => state.channels.list_overwrites(parent_id).await?,
        None => Vec::new(),
    };
    let channel_overwrites = state.channels.list_overwrites(channel.id).await?;
    let category = permission_overwrites(&category_overwrites);
    let direct = permission_overwrites(&channel_overwrites);
    let roles: Vec<Role> = access
        .roles
        .iter()
        .map(|role| Role::new(role.id, role.permissions, role.position, role.is_default))
        .collect();
    let timed_out = access
        .member
        .communication_disabled_until
        .is_some_and(|until| until > Utc::now());

    Ok(permissions::evaluate(PermissionContext {
        is_owner: access.is_owner,
        member_id: access.member.user_id,
        roles: &roles,
        category_overwrites: &category,
        channel_overwrites: &direct,
        timed_out,
    }))
}

pub async fn visible_channels(
    state: &AppState,
    community_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<Channel>, ApiError> {
    let access = match state.communities.access(community_id, user_id).await {
        Ok(access) => access,
        Err(_) => return Ok(Vec::new()),
    };
    let channels = state.channels.list_for_community(community_id).await?;
    let mut visible = Vec::with_capacity(channels.len());
    for channel in channels {
        let permissions = effective_permissions(state, &access, &channel).await?;
        if permissions.contains(Permissions::VIEW_CHANNEL) {
            visible.push(channel);
        }
    }
    Ok(visible)
}

pub(crate) async fn channel_access(
    state: &AppState,
    user_id: Uuid,
    channel_id: Uuid,
) -> Result<(Channel, CommunityAccess, Permissions), ApiError> {
    let channel = state.channels.get(channel_id).await?;
    let community_id = channel
        .community_id
        .ok_or_else(|| ApiError::not_found("Channel not found."))?;
    let access = state.communities.access(community_id, user_id).await?;
    let permissions = effective_permissions(state, &access, &channel).await?;
    Ok((channel, access, permissions))
}

fn require_manage_roles(access: &CommunityAccess) -> Result<(), ApiError> {
    if access.permissions.contains(Permissions::MANAGE_ROLES) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "permission_denied",
            "You do not have permission to manage channel permissions.",
        ))
    }
}

fn parse_overwrite(payload: OverwriteRequest) -> Result<(Permissions, Permissions), ApiError> {
    let allow = Permissions::from_names(&payload.allow).map_err(ApiError::validation)?;
    let deny = Permissions::from_names(&payload.deny).map_err(ApiError::validation)?;
    if !(allow & deny).is_empty() {
        return Err(ApiError::validation(
            "A permission cannot be both allowed and denied.",
        ));
    }
    Ok((allow, deny))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    responses((status = 200, body = ChannelDto))
)]
pub async fn get_channel(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<ChannelDto>, ApiError> {
    let channel = state.channels.get(channel_id).await?;
    if channel.scope == "direct" {
        let other = state
            .channels
            .direct_other_user(channel_id, current.user().id)
            .await?;
        if other.is_none() {
            return Err(ApiError::not_found("Channel not found."));
        }
        return Ok(Json(channel.into()));
    }
    let (_, _, permissions) = channel_access(&state, current.user().id, channel_id).await?;
    if !permissions.contains(Permissions::VIEW_CHANNEL) {
        return Err(ApiError::not_found("Channel not found."));
    }
    Ok(Json(channel.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/permissions/me",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    responses((status = 200, body = ChannelPermissionsDto))
)]
pub async fn my_permissions(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<ChannelPermissionsDto>, ApiError> {
    let (_, _, permissions) = channel_access(&state, current.user().id, channel_id).await?;
    if !permissions.contains(Permissions::VIEW_CHANNEL) {
        return Err(ApiError::not_found("Channel not found."));
    }
    Ok(Json(ChannelPermissionsDto {
        permissions: permissions
            .names()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/permissions",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    responses((status = 200, body = Vec<ChannelOverwriteDto>))
)]
pub async fn list_overwrites(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<ChannelOverwriteDto>>, ApiError> {
    let (_, access, _) = channel_access(&state, current.user().id, channel_id).await?;
    require_manage_roles(&access)?;
    let overwrites = state.channels.list_overwrites(channel_id).await?;
    Ok(Json(
        overwrites
            .into_iter()
            .map(ChannelOverwriteDto::from)
            .collect(),
    ))
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/permissions/roles/{role_id}",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("role_id" = Uuid, Path, description = "Role identifier")
    ),
    request_body = OverwriteRequest,
    responses((status = 200, body = ChannelOverwriteDto))
)]
pub async fn set_role_overwrite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, role_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<OverwriteRequest>,
) -> Result<Json<ChannelOverwriteDto>, ApiError> {
    let (_, access, _) = channel_access(&state, current.user().id, channel_id).await?;
    require_manage_roles(&access)?;
    let role = state.communities.get_role(role_id).await?;
    let community_id = state
        .channels
        .get(channel_id)
        .await?
        .community_id
        .ok_or_else(|| ApiError::not_found("Channel not found."))?;
    if role.community_id != community_id {
        return Err(ApiError::not_found("Role not found."));
    }
    if !access.is_owner && role.position >= access.highest_role_position {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot modify an overwrite for a role at or above your highest role.",
        ));
    }
    let (allow, deny) = parse_overwrite(payload)?;
    if !access.is_owner && !access.permissions.contains(allow) {
        return Err(ApiError::forbidden(
            "permission_escalation",
            "You cannot grant permissions you do not hold.",
        ));
    }
    let overwrite = state
        .channels
        .set_overwrite(channel_id, Some(role_id), None, allow, deny)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "channel.overwrite.set",
            Some(access.community.id),
            Some(("channel", channel_id)),
        )
        .await;
    Ok(Json(overwrite.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/permissions/roles/{role_id}",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("role_id" = Uuid, Path, description = "Role identifier")
    ),
    responses((status = 204, description = "Overwrite removed"))
)]
pub async fn delete_role_overwrite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, role_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let (_, access, _) = channel_access(&state, current.user().id, channel_id).await?;
    require_manage_roles(&access)?;
    let role = state.communities.get_role(role_id).await?;
    if !access.is_owner && role.position >= access.highest_role_position {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot modify an overwrite for a role at or above your highest role.",
        ));
    }
    state
        .channels
        .delete_overwrite(channel_id, Some(role_id), None)
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "channel.overwrite.delete",
            Some(access.community.id),
            Some(("channel", channel_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/permissions/users/{user_id}",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("user_id" = Uuid, Path, description = "User identifier")
    ),
    request_body = OverwriteRequest,
    responses((status = 200, body = ChannelOverwriteDto))
)]
pub async fn set_user_overwrite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<OverwriteRequest>,
) -> Result<Json<ChannelOverwriteDto>, ApiError> {
    let (_, access, _) = channel_access(&state, current.user().id, channel_id).await?;
    require_manage_roles(&access)?;
    let community_id = state
        .channels
        .get(channel_id)
        .await?
        .community_id
        .ok_or_else(|| ApiError::not_found("Channel not found."))?;
    let target = state.communities.access(community_id, user_id).await?;
    if target.is_owner
        || (!access.is_owner && access.highest_role_position <= target.highest_role_position)
    {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot overwrite a member at or above your role position.",
        ));
    }
    let (allow, deny) = parse_overwrite(payload)?;
    if !access.is_owner && !access.permissions.contains(allow) {
        return Err(ApiError::forbidden(
            "permission_escalation",
            "You cannot grant permissions you do not hold.",
        ));
    }
    let overwrite = state
        .channels
        .set_overwrite(channel_id, None, Some(user_id), allow, deny)
        .await?;
    if deny.contains(Permissions::CONNECT_VOICE) {
        if let Ok(channel) = state.channels.get(channel_id).await {
            super::voice::revoke_channel_user(&state, &channel, user_id, "permission_revoked")
                .await;
        }
    }
    state
        .record_audit(
            Some(current.user().id),
            "channel.overwrite.set",
            Some(community_id),
            Some(("channel", channel_id)),
        )
        .await;
    Ok(Json(overwrite.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/permissions/users/{user_id}",
    tag = "channels",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Channel identifier"),
        ("user_id" = Uuid, Path, description = "User identifier")
    ),
    responses((status = 204, description = "Overwrite removed"))
)]
pub async fn delete_user_overwrite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let (_, access, _) = channel_access(&state, current.user().id, channel_id).await?;
    require_manage_roles(&access)?;
    state
        .channels
        .delete_overwrite(channel_id, None, Some(user_id))
        .await?;
    state
        .record_audit(
            Some(current.user().id),
            "channel.overwrite.delete",
            Some(access.community.id),
            Some(("channel", channel_id)),
        )
        .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
