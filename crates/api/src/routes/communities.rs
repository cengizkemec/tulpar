use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{Duration, Utc};
use communities::{Community, CommunityAccess, CreateCommunity, CreateInvite};
use permissions::Permissions;
use uuid::Uuid;

use crate::{
    dto::{
        BanMemberRequest, ChannelDto, CommunityDetailsDto, CommunityDto, CommunityPermissionsDto,
        CreateChannelRequest, CreateCommunityRequest, CreateInviteRequest, CreateRoleRequest,
        InviteDto, InvitePreviewDto, JoinResponse, MemberDto, RoleDto, UpdateCommunityRequest,
        UpdateMemberRequest, UpdateRoleRequest,
    },
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
};

fn require_permission(access: &CommunityAccess, permission: Permissions) -> Result<(), ApiError> {
    if access.permissions.contains(permission) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "permission_denied",
            "You do not have permission to perform this action.",
        ))
    }
}

fn require_owner(access: &CommunityAccess) -> Result<(), ApiError> {
    if access.is_owner {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "owner_required",
            "Only the community owner can perform this action.",
        ))
    }
}

fn require_target_below(actor: &CommunityAccess, target: &CommunityAccess) -> Result<(), ApiError> {
    if target.is_owner
        || (!actor.is_owner && actor.highest_role_position <= target.highest_role_position)
    {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot act on a member at or above your role position.",
        ));
    }
    Ok(())
}

async fn details(
    state: &AppState,
    community: Community,
    viewer_id: Uuid,
) -> Result<CommunityDetailsDto, ApiError> {
    let channels = super::channels::visible_channels(state, community.id, viewer_id).await?;
    let roles = state.communities.list_roles(community.id).await?;
    let member_count = state.communities.member_count(community.id).await?;
    Ok(CommunityDetailsDto {
        community: community.into(),
        channels: channels.into_iter().map(ChannelDto::from).collect(),
        roles: roles.into_iter().map(RoleDto::from).collect(),
        member_count,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/communities",
    tag = "communities",
    security(("bearer_auth" = [])),
    request_body = CreateCommunityRequest,
    responses((status = 200, body = CommunityDetailsDto))
)]
pub async fn create_community(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<CreateCommunityRequest>,
) -> Result<Json<CommunityDetailsDto>, ApiError> {
    let community = state
        .communities
        .create(
            current.user().id,
            CreateCommunity {
                name: payload.name,
                slug: payload.slug,
                description: payload.description,
                visibility: payload.visibility,
            },
        )
        .await?;

    if let Err(error) = state.channels.create_default_channels(community.id).await {
        let _ = state.communities.soft_delete(community.id).await;
        return Err(error.into());
    }

    Ok(Json(details(&state, community, current.user().id).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/communities",
    tag = "communities",
    security(("bearer_auth" = [])),
    responses((status = 200, body = Vec<CommunityDto>))
)]
pub async fn list_mine(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<CommunityDto>>, ApiError> {
    let communities = state.communities.list_for_user(current.user().id).await?;
    Ok(Json(
        communities.into_iter().map(CommunityDto::from).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}",
    tag = "communities",
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses(
        (status = 200, body = CommunityDetailsDto),
        (status = 404, description = "Community not found or private", body = crate::error::ProblemDetails)
    )
)]
pub async fn get_community(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<CommunityDetailsDto>, ApiError> {
    let community = state.communities.get(community_id).await?;
    if community.visibility != "public"
        && !state
            .communities
            .is_member(community_id, current.user().id)
            .await?
    {
        return Err(ApiError::not_found("Community not found."));
    }
    Ok(Json(details(&state, community, current.user().id).await?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/communities/{community_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    request_body = UpdateCommunityRequest,
    responses((status = 200, body = CommunityDto))
)]
pub async fn update_community(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
    Json(payload): Json<UpdateCommunityRequest>,
) -> Result<Json<CommunityDto>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_COMMUNITY)?;
    let community = state
        .communities
        .update(
            community_id,
            payload.name,
            payload.description,
            payload.visibility,
        )
        .await?;
    Ok(Json(community.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 204, description = "Community soft-deleted"))
)]
pub async fn delete_community(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_owner(&access)?;
    state.communities.soft_delete(community_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/join",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = JoinResponse))
)]
pub async fn join_public(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<JoinResponse>, ApiError> {
    let result = state
        .communities
        .join_public(community_id, current.user().id)
        .await?;
    Ok(Json(JoinResponse {
        community_id: result.community_id,
        already_member: result.already_member,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/permissions/me",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = CommunityPermissionsDto))
)]
pub async fn my_permissions(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<CommunityPermissionsDto>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    Ok(Json(access.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/members",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = Vec<MemberDto>))
)]
pub async fn list_members(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<Vec<MemberDto>>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::VIEW_MEMBER_LIST)?;
    let members = state.communities.list_members(community_id).await?;
    Ok(Json(members.into_iter().map(MemberDto::from).collect()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/communities/{community_id}/members/{user_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("user_id" = Uuid, Path, description = "Member identifier")
    ),
    request_body = UpdateMemberRequest,
    responses((status = 204, description = "Member updated"))
)]
pub async fn update_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateMemberRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    let target = state.communities.access(community_id, user_id).await?;

    let is_self = user_id == current.user().id;
    if is_self {
        if payload.timeout_seconds.is_some() {
            return Err(ApiError::forbidden(
                "permission_denied",
                "You cannot timeout yourself.",
            ));
        }
        require_permission(&actor, Permissions::CHANGE_NICKNAME)?;
    } else {
        require_target_below(&actor, &target)?;
        if payload.nickname.is_some() {
            require_permission(&actor, Permissions::MANAGE_NICKNAMES)?;
        }
        if payload.timeout_seconds.is_some() {
            require_permission(&actor, Permissions::MODERATE_MEMBERS)?;
        }
    }

    let timeout = match payload.timeout_seconds {
        None => target.member.communication_disabled_until,
        Some(0) => None,
        Some(seconds) if seconds > 0 && seconds <= 28 * 24 * 60 * 60 => {
            Some(Utc::now() + Duration::seconds(seconds))
        }
        Some(_) => {
            return Err(ApiError::validation(
                "Timeout must be between 1 second and 28 days.",
            ))
        }
    };

    if let Some(nickname) = &payload.nickname {
        if nickname.chars().count() > 64 {
            return Err(ApiError::validation(
                "Nickname must be at most 64 characters.",
            ));
        }
    }

    state
        .communities
        .update_member(community_id, user_id, payload.nickname, timeout)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}/members/{user_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("user_id" = Uuid, Path, description = "Member identifier")
    ),
    responses((status = 204, description = "Member kicked"))
)]
pub async fn kick_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::KICK_MEMBERS)?;
    let target = state.communities.access(community_id, user_id).await?;
    require_target_below(&actor, &target)?;
    state.communities.kick(community_id, user_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/v1/communities/{community_id}/bans/{user_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("user_id" = Uuid, Path, description = "User identifier")
    ),
    request_body = BanMemberRequest,
    responses((status = 204, description = "Member banned"))
)]
pub async fn ban_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<BanMemberRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::BAN_MEMBERS)?;
    if let Ok(target) = state.communities.access(community_id, user_id).await {
        require_target_below(&actor, &target)?;
    }
    let expires_at = match payload.expires_in_seconds {
        None => None,
        Some(seconds) if (1..=365 * 24 * 60 * 60).contains(&seconds) => {
            Some(Utc::now() + Duration::seconds(seconds))
        }
        Some(_) => {
            return Err(ApiError::validation(
                "Ban expiry must be between 1 second and 365 days.",
            ))
        }
    };
    state
        .communities
        .ban(
            community_id,
            user_id,
            current.user().id,
            payload.reason,
            expires_at,
        )
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}/bans/{user_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("user_id" = Uuid, Path, description = "User identifier")
    ),
    responses((status = 204, description = "Member unbanned"))
)]
pub async fn unban_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::BAN_MEMBERS)?;
    state.communities.unban(community_id, user_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/roles",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = Vec<RoleDto>))
)]
pub async fn list_roles(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<Vec<RoleDto>>, ApiError> {
    state
        .communities
        .access(community_id, current.user().id)
        .await?;
    let roles = state.communities.list_roles(community_id).await?;
    Ok(Json(roles.into_iter().map(RoleDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/roles",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    request_body = CreateRoleRequest,
    responses((status = 200, body = RoleDto))
)]
pub async fn create_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
    Json(payload): Json<CreateRoleRequest>,
) -> Result<Json<RoleDto>, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::MANAGE_ROLES)?;
    let permissions =
        Permissions::from_names(&payload.permissions).map_err(ApiError::validation)?;
    if !actor.is_owner {
        if permissions.contains(Permissions::ADMINISTRATOR) {
            return Err(ApiError::forbidden(
                "administrator_grant",
                "Only the owner can grant administrator.",
            ));
        }
        if !actor.permissions.contains(permissions) {
            return Err(ApiError::forbidden(
                "permission_escalation",
                "You cannot grant permissions you do not hold.",
            ));
        }
    }
    let position = payload.position.unwrap_or(1);
    if !actor.is_owner && position >= actor.highest_role_position {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot create a role at or above your highest role.",
        ));
    }
    let role = state
        .communities
        .create_role(
            community_id,
            &payload.name,
            payload.color.unwrap_or(0),
            position,
            permissions,
            payload.mentionable,
        )
        .await?;
    Ok(Json(role.into()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/communities/{community_id}/roles/{role_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("role_id" = Uuid, Path, description = "Role identifier")
    ),
    request_body = UpdateRoleRequest,
    responses((status = 200, body = RoleDto))
)]
pub async fn update_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, role_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateRoleRequest>,
) -> Result<Json<RoleDto>, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::MANAGE_ROLES)?;
    let role = state.communities.get_role(role_id).await?;
    if role.community_id != community_id || role.is_default {
        return Err(ApiError::forbidden(
            "protected_role",
            "The default role cannot be edited.",
        ));
    }
    if !actor.is_owner && role.position >= actor.highest_role_position {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot edit a role at or above your highest role.",
        ));
    }
    let permissions = match payload.permissions {
        Some(names) => Some(Permissions::from_names(&names).map_err(ApiError::validation)?),
        None => None,
    };
    if let Some(permissions) = permissions {
        if !actor.is_owner {
            if permissions.contains(Permissions::ADMINISTRATOR) {
                return Err(ApiError::forbidden(
                    "administrator_grant",
                    "Only the owner can grant administrator.",
                ));
            }
            if !actor.permissions.contains(permissions) {
                return Err(ApiError::forbidden(
                    "permission_escalation",
                    "You cannot grant permissions you do not hold.",
                ));
            }
        }
    }
    if let Some(position) = payload.position {
        if !actor.is_owner && position >= actor.highest_role_position {
            return Err(ApiError::forbidden(
                "hierarchy",
                "You cannot move a role to or above your highest role.",
            ));
        }
    }
    let role = state
        .communities
        .update_role(
            role_id,
            payload.name,
            payload.color,
            payload.position,
            permissions,
            payload.mentionable,
        )
        .await?;
    Ok(Json(role.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}/roles/{role_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("role_id" = Uuid, Path, description = "Role identifier")
    ),
    responses((status = 204, description = "Role deleted"))
)]
pub async fn delete_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, role_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::MANAGE_ROLES)?;
    let role = state.communities.get_role(role_id).await?;
    if role.community_id != community_id || role.is_default {
        return Err(ApiError::forbidden(
            "protected_role",
            "The default role cannot be deleted.",
        ));
    }
    if !actor.is_owner && role.position >= actor.highest_role_position {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot delete a role at or above your highest role.",
        ));
    }
    state.communities.delete_role(role_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/v1/communities/{community_id}/members/{user_id}/roles/{role_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("user_id" = Uuid, Path, description = "Member identifier"),
        ("role_id" = Uuid, Path, description = "Role identifier")
    ),
    responses((status = 204, description = "Role assigned"))
)]
pub async fn assign_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, user_id, role_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::MANAGE_ROLES)?;
    let target = state.communities.access(community_id, user_id).await?;
    require_target_below(&actor, &target)?;
    let role = state.communities.get_role(role_id).await?;
    if role.community_id != community_id {
        return Err(ApiError::not_found("Role not found."));
    }
    if !actor.is_owner {
        if role.permissions.contains(Permissions::ADMINISTRATOR) {
            return Err(ApiError::forbidden(
                "administrator_grant",
                "Only the owner can assign administrator.",
            ));
        }
        if role.position >= actor.highest_role_position {
            return Err(ApiError::forbidden(
                "hierarchy",
                "You cannot assign a role at or above your highest role.",
            ));
        }
    }
    state
        .communities
        .assign_role(community_id, user_id, role_id, current.user().id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}/members/{user_id}/roles/{role_id}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(
        ("community_id" = Uuid, Path, description = "Community identifier"),
        ("user_id" = Uuid, Path, description = "Member identifier"),
        ("role_id" = Uuid, Path, description = "Role identifier")
    ),
    responses((status = 204, description = "Role removed"))
)]
pub async fn remove_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((community_id, user_id, role_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let actor = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&actor, Permissions::MANAGE_ROLES)?;
    let target = state.communities.access(community_id, user_id).await?;
    require_target_below(&actor, &target)?;
    let role = state.communities.get_role(role_id).await?;
    if role.community_id != community_id {
        return Err(ApiError::not_found("Role not found."));
    }
    if !actor.is_owner && role.position >= actor.highest_role_position {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot remove a role at or above your highest role.",
        ));
    }
    state
        .communities
        .remove_role(community_id, user_id, role_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/channels",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = Vec<ChannelDto>))
)]
pub async fn list_channels(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<Vec<ChannelDto>>, ApiError> {
    state
        .communities
        .access(community_id, current.user().id)
        .await?;
    let channels =
        super::channels::visible_channels(&state, community_id, current.user().id).await?;
    Ok(Json(channels.into_iter().map(ChannelDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/channels",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    request_body = CreateChannelRequest,
    responses((status = 200, body = ChannelDto))
)]
pub async fn create_channel(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
    Json(payload): Json<CreateChannelRequest>,
) -> Result<Json<ChannelDto>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_CHANNELS)?;
    if let Some(parent_id) = payload.parent_id {
        let parent = state.channels.get(parent_id).await?;
        if parent.community_id != Some(community_id) || parent.kind != "category" {
            return Err(ApiError::validation(
                "The parent channel must be a category in this community.",
            ));
        }
    }
    let channel = state
        .channels
        .create_community_channel(
            community_id,
            &payload.kind,
            &payload.name,
            payload.parent_id,
            payload.position.unwrap_or(0),
            payload.topic,
        )
        .await?;
    Ok(Json(channel.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/invites",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = Vec<InviteDto>))
)]
pub async fn list_invites(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<Vec<InviteDto>>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_INVITES)?;
    let invites = state.communities.list_invites(community_id).await?;
    Ok(Json(invites.into_iter().map(InviteDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/invites",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    request_body = CreateInviteRequest,
    responses((status = 200, body = InviteDto))
)]
pub async fn create_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
    Json(payload): Json<CreateInviteRequest>,
) -> Result<Json<InviteDto>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::CREATE_INVITES)?;
    if let Some(channel_id) = payload.channel_id {
        let channel = state.channels.get(channel_id).await?;
        if channel.community_id != Some(community_id) {
            return Err(ApiError::validation(
                "The invite channel must belong to this community.",
            ));
        }
    }
    if payload.max_uses.is_some_and(|uses| uses <= 0) {
        return Err(ApiError::validation("max_uses must be positive."));
    }
    if payload
        .expires_in_hours
        .is_some_and(|hours| !(1..=720).contains(&hours))
    {
        return Err(ApiError::validation(
            "expires_in_hours must be between 1 and 720.",
        ));
    }
    let invite = state
        .communities
        .create_invite(CreateInvite {
            community_id,
            channel_id: payload.channel_id,
            created_by: current.user().id,
            max_uses: payload.max_uses,
            expires_in_hours: payload.expires_in_hours,
        })
        .await?;
    Ok(Json(invite.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/invites/{code}",
    tag = "communities",
    params(("code" = String, Path, description = "Invite code")),
    responses((status = 200, body = InvitePreviewDto), (status = 410, description = "Invite invalid"))
)]
pub async fn preview_invite(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<InvitePreviewDto>, ApiError> {
    let (invite, community) = state.communities.preview_invite(&code).await?;
    Ok(Json(InvitePreviewDto {
        invite: invite.into(),
        community: community.into(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/invites/{code}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("code" = String, Path, description = "Invite code")),
    responses((status = 200, body = JoinResponse), (status = 410, description = "Invite invalid"))
)]
pub async fn join_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(code): Path<String>,
) -> Result<Json<JoinResponse>, ApiError> {
    let result = state
        .communities
        .join_invite(&code, current.user().id)
        .await?;
    Ok(Json(JoinResponse {
        community_id: result.community_id,
        already_member: result.already_member,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/invites/{code}",
    tag = "communities",
    security(("bearer_auth" = [])),
    params(("code" = String, Path, description = "Invite code")),
    responses((status = 204, description = "Invite revoked"))
)]
pub async fn revoke_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(code): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let (invite, _) = state.communities.preview_invite(&code).await?;
    let access = state
        .communities
        .access(invite.community_id, current.user().id)
        .await?;
    if !access.permissions.contains(Permissions::MANAGE_INVITES)
        && invite.created_by != Some(current.user().id)
    {
        return Err(ApiError::forbidden(
            "permission_denied",
            "You cannot revoke this invite.",
        ));
    }
    state.communities.revoke_invite(&code).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
