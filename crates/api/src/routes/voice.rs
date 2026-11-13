use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use channels::Channel;
use chrono::Utc;
use permissions::Permissions;
use serde_json::json;
use uuid::Uuid;

use crate::{
    dto::{
        LiveKitWebhookAck, VoiceModerationRequest, VoiceMoveRequest, VoiceParticipantDto,
        VoicePasswordRequest, VoiceTokenDto, VoiceTokenRequest,
    },
    error::ApiError,
    extract::{enforce_rate_limit, CurrentUser},
    routes::messages,
    state::AppState,
};
use voice::{
    self as voice_core, LiveKitAdmin, VoiceError, VoiceParticipant, VoicePermissions, VoiceTrack,
};

fn voice_unavailable(error: VoiceError) -> ApiError {
    match error {
        VoiceError::NotFound => ApiError::not_found("Voice channel not found."),
        VoiceError::Forbidden => ApiError::forbidden(
            "permission_denied",
            "You do not have permission to perform this action.",
        ),
        VoiceError::RoomFull => ApiError::conflict("room_full", "The voice channel is full."),
        VoiceError::WebhookInvalid => {
            ApiError::forbidden("webhook_invalid", "The webhook signature is invalid.")
        }
        VoiceError::Invalid(message) => ApiError::validation(message),
        VoiceError::Unavailable | VoiceError::Backend(_) => {
            tracing::error!(%error, "voice backend error");
            ApiError::dependency_unavailable("Voice is temporarily unavailable.")
        }
    }
}

fn admin(state: &AppState) -> Result<&LiveKitAdmin, ApiError> {
    state
        .livekit_admin
        .as_deref()
        .ok_or_else(|| ApiError::dependency_unavailable("Voice is not configured."))
}

fn community_id(authorization: &messages::ChannelAuthorization) -> Option<Uuid> {
    authorization.community_id()
}

fn publish_state(
    state: &AppState,
    authorization: &messages::ChannelAuthorization,
    participant: &VoiceParticipant,
) {
    let event =
        serde_json::to_value(VoiceParticipantDto::from(participant.clone())).unwrap_or_default();
    state.gateway.publish(gateway::GatewayEvent::channel(
        "VOICE_STATE_UPDATE",
        community_id(authorization),
        participant.channel_id,
        event,
    ));
}

fn publish_ended(
    state: &AppState,
    community_id: Option<Uuid>,
    channel_id: Uuid,
    user_id: Uuid,
    reason: &str,
) {
    state.gateway.publish(gateway::GatewayEvent::channel(
        "VOICE_SESSION_ENDED",
        community_id,
        channel_id,
        json!({ "channel_id": channel_id, "user_id": user_id, "reason": reason }),
    ));
}

#[utoipa::path(
    post,
    path = "/api/v1/channels/{channel_id}/voice/token",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Voice channel identifier")),
    request_body = VoiceTokenRequest,
    responses((status = 200, body = VoiceTokenDto))
)]
pub async fn issue_token(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    Json(payload): Json<VoiceTokenRequest>,
) -> Result<Json<VoiceTokenDto>, ApiError> {
    let authorization = messages::authorize_channel(&state, current.user().id, channel_id).await?;
    if authorization.channel().kind != "voice" {
        return Err(ApiError::not_found("Voice channel not found."));
    }
    authorization.require(Permissions::CONNECT_VOICE)?;
    if let Some(community_id) = authorization.community_id() {
        if state
            .communities
            .is_banned(community_id, current.user().id)
            .await?
        {
            return Err(ApiError::forbidden(
                "banned",
                "You are banned from this community.",
            ));
        }
    }

    let password_hash = state.channels.voice_password_hash(channel_id).await?;
    if password_hash.is_some() {
        enforce_rate_limit(
            &state,
            &format!("voice:password:{}:{channel_id}", current.user().id),
            5,
            std::time::Duration::from_secs(60),
        )
        .await?;
        let password = payload.password.as_deref().unwrap_or_default();
        if !voice_core::verify_password(password, password_hash.as_deref()) {
            return Err(ApiError::forbidden(
                "permission_denied",
                "The room password is incorrect.",
            ));
        }
    }

    if !state.voice.contains(channel_id, current.user().id).await {
        let count = state.voice.count(channel_id).await as i32;
        let capacity = authorization
            .channel()
            .user_limit
            .filter(|limit| *limit > 0)
            .unwrap_or(state.config.voice.max_participants)
            .min(state.config.voice.max_participants);
        if count >= capacity {
            return Err(ApiError::conflict(
                "room_full",
                "The voice channel is full.",
            ));
        }
    }

    let permissions = VoicePermissions {
        connect: true,
        speak: authorization.can(Permissions::SPEAK),
        stream_video: authorization.can(Permissions::STREAM_VIDEO),
        share_screen: authorization.can(Permissions::SHARE_SCREEN),
    };
    let (token, expires_at) = voice_core::mint_token(
        &state.config.voice.api_key,
        &state.config.voice.api_secret,
        current.user().id,
        &current.user().display_name,
        channel_id,
        permissions,
        state.config.voice.token_ttl_seconds,
        current.session_id(),
    )
    .map_err(voice_unavailable)?;
    let ice_servers = voice_core::ice_servers(
        &state.config.voice.stun_url,
        &state.config.voice.turn_url,
        state.config.voice.turn_tls_url.as_deref(),
        &state.config.voice.turn_secret,
        current.user().id,
        state.config.voice.turn_ttl_seconds,
    )
    .map_err(voice_unavailable)?;

    state
        .record_audit(
            Some(current.user().id),
            "voice.token_issue",
            authorization.community_id(),
            Some(("channel", channel_id)),
        )
        .await;

    Ok(Json(VoiceTokenDto {
        url: state.config.voice.livekit_url.clone(),
        room: voice_core::room_name(channel_id),
        token,
        expires_at,
        ice_servers: ice_servers.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/voice/states",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Voice channel identifier")),
    responses((status = 200, body = Vec<VoiceParticipantDto>))
)]
pub async fn voice_states(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<VoiceParticipantDto>>, ApiError> {
    let authorization = messages::authorize_channel(&state, current.user().id, channel_id).await?;
    if authorization.channel().kind != "voice" {
        return Err(ApiError::not_found("Voice channel not found."));
    }
    let participants = state.voice.participants(channel_id).await;
    Ok(Json(
        participants
            .into_iter()
            .map(VoiceParticipantDto::from)
            .collect(),
    ))
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/voice/password",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Voice channel identifier")),
    request_body = VoicePasswordRequest,
    responses((status = 204, description = "Voice password updated"))
)]
pub async fn set_password(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    Json(payload): Json<VoicePasswordRequest>,
) -> Result<StatusCode, ApiError> {
    let authorization = messages::authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::MANAGE_CHANNELS)?;
    if authorization.channel().kind != "voice" {
        return Err(ApiError::not_found("Voice channel not found."));
    }
    let password = payload
        .password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(password) = password {
        if password.chars().count() > 128 {
            return Err(ApiError::validation(
                "Voice passwords must be at most 128 characters.",
            ));
        }
    }
    let hash =
        match password {
            Some(password) => Some(state.passwords.hash(password.to_string()).await.map_err(
                |error| crate::error::log_and_internal("password hashing failed", error),
            )?),
            None => None,
        };
    state.channels.set_voice_password(channel_id, hash).await?;
    state
        .record_audit(
            Some(current.user().id),
            "voice.password_update",
            authorization.community_id(),
            Some(("channel", channel_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_target_below(
    state: &AppState,
    actor: &messages::ChannelAuthorization,
    target_user_id: Uuid,
) -> Result<(), ApiError> {
    let Some(community_id) = actor.community_id() else {
        return Ok(());
    };
    let Some(actor_access) = actor.community() else {
        return Ok(());
    };
    let target = state
        .communities
        .access(community_id, target_user_id)
        .await?;
    if target.is_owner
        || (!actor_access.is_owner
            && actor_access.highest_role_position <= target.highest_role_position)
    {
        return Err(ApiError::forbidden(
            "hierarchy",
            "You cannot moderate a member at or above your role position.",
        ));
    }
    Ok(())
}

async fn moderate_target(
    state: &AppState,
    actor: Uuid,
    channel_id: Uuid,
    target_user_id: Uuid,
    permission: Permissions,
) -> Result<(messages::ChannelAuthorization, &LiveKitAdmin), ApiError> {
    let authorization = messages::authorize_channel(state, actor, channel_id).await?;
    authorization.require(permission)?;
    if authorization.channel().kind != "voice" {
        return Err(ApiError::not_found("Voice channel not found."));
    }
    require_target_below(state, &authorization, target_user_id).await?;
    let admin = admin(state)?;
    Ok((authorization, admin))
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/voice/participants/{user_id}/server-mute",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Voice channel identifier"),
        ("user_id" = Uuid, Path, description = "Participant identifier")
    ),
    request_body = VoiceModerationRequest,
    responses((status = 204, description = "Server mute updated"))
)]
pub async fn server_mute(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<VoiceModerationRequest>,
) -> Result<StatusCode, ApiError> {
    let (authorization, admin) = moderate_target(
        &state,
        current.user().id,
        channel_id,
        user_id,
        Permissions::MUTE_MEMBERS,
    )
    .await?;
    let participant = state.voice.participant(channel_id, user_id).await;
    if let Some(track) = participant.as_ref().and_then(|participant| {
        participant
            .tracks
            .iter()
            .find(|track| track.source == "microphone" || track.kind == "audio")
    }) {
        admin
            .mute_track(channel_id, user_id, &track.sid, payload.enabled)
            .await
            .map_err(voice_unavailable)?;
    } else {
        admin
            .update_permissions(channel_id, user_id, !payload.enabled, true)
            .await
            .map_err(voice_unavailable)?;
    }
    state
        .voice
        .set_server_mute(channel_id, user_id, payload.enabled)
        .await;
    if let Some(participant) = state.voice.participant(channel_id, user_id).await {
        publish_state(&state, &authorization, &participant);
    }
    state
        .record_audit(
            Some(current.user().id),
            if payload.enabled {
                "voice.server_mute"
            } else {
                "voice.server_unmute"
            },
            authorization.community_id(),
            Some(("user", user_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/voice/participants/{user_id}/server-deafen",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Voice channel identifier"),
        ("user_id" = Uuid, Path, description = "Participant identifier")
    ),
    request_body = VoiceModerationRequest,
    responses((status = 204, description = "Server deafen updated"))
)]
pub async fn server_deafen(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<VoiceModerationRequest>,
) -> Result<StatusCode, ApiError> {
    let (authorization, admin) = moderate_target(
        &state,
        current.user().id,
        channel_id,
        user_id,
        Permissions::DEAFEN_MEMBERS,
    )
    .await?;
    admin
        .update_permissions(channel_id, user_id, true, !payload.enabled)
        .await
        .map_err(voice_unavailable)?;
    state
        .voice
        .set_server_deaf(channel_id, user_id, payload.enabled)
        .await;
    if let Some(participant) = state.voice.participant(channel_id, user_id).await {
        publish_state(&state, &authorization, &participant);
    }
    state
        .record_audit(
            Some(current.user().id),
            if payload.enabled {
                "voice.server_deafen"
            } else {
                "voice.server_undeafen"
            },
            authorization.community_id(),
            Some(("user", user_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/voice/participants/{user_id}",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Voice channel identifier"),
        ("user_id" = Uuid, Path, description = "Participant identifier")
    ),
    responses((status = 204, description = "Participant disconnected"))
)]
pub async fn disconnect_participant(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let (authorization, admin) = moderate_target(
        &state,
        current.user().id,
        channel_id,
        user_id,
        Permissions::MOVE_MEMBERS,
    )
    .await?;
    admin
        .remove_participant(channel_id, user_id)
        .await
        .map_err(voice_unavailable)?;
    state.voice.leave(channel_id, user_id).await;
    publish_ended(
        &state,
        authorization.community_id(),
        channel_id,
        user_id,
        "disconnected",
    );
    state
        .record_audit(
            Some(current.user().id),
            "voice.disconnect",
            authorization.community_id(),
            Some(("user", user_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/channels/{channel_id}/voice/participants/{user_id}/move",
    tag = "voice",
    security(("bearer_auth" = [])),
    params(
        ("channel_id" = Uuid, Path, description = "Current voice channel"),
        ("user_id" = Uuid, Path, description = "Participant identifier")
    ),
    request_body = VoiceMoveRequest,
    responses((status = 204, description = "Participant moved"))
)]
pub async fn move_participant(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((channel_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<VoiceMoveRequest>,
) -> Result<StatusCode, ApiError> {
    let (authorization, admin) = moderate_target(
        &state,
        current.user().id,
        channel_id,
        user_id,
        Permissions::MOVE_MEMBERS,
    )
    .await?;
    let target = messages::authorize_channel(&state, user_id, payload.channel_id).await?;
    if target.channel().kind != "voice" {
        return Err(ApiError::validation(
            "The destination must be a voice channel.",
        ));
    }
    if target.community_id() != authorization.community_id() {
        return Err(ApiError::validation(
            "The destination must be in the same community.",
        ));
    }
    admin
        .remove_participant(channel_id, user_id)
        .await
        .map_err(voice_unavailable)?;
    state.voice.leave(channel_id, user_id).await;
    publish_ended(
        &state,
        authorization.community_id(),
        channel_id,
        user_id,
        "moved",
    );
    state
        .record_audit(
            Some(current.user().id),
            "voice.move",
            authorization.community_id(),
            Some(("user", user_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/webhooks/livekit",
    tag = "voice",
    request_body(content = String, content_type = "application/webhook+json"),
    responses((status = 200, body = LiveKitWebhookAck))
)]
pub async fn livekit_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<LiveKitWebhookAck>, ApiError> {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let event = voice_core::verify_webhook(
        &state.config.voice.api_key,
        &state.config.voice.api_secret,
        authorization,
        &body,
    )
    .map_err(voice_unavailable)?;

    let event_id = if event.id.is_empty() {
        format!(
            "{}:{}:{}",
            event.event,
            event
                .room
                .as_ref()
                .map(|room| room.name.as_str())
                .unwrap_or(""),
            event.created_at
        )
    } else {
        event.id.clone()
    };
    let inserted = sqlx::query(
        "INSERT INTO voice_webhook_events (event_id, event_type, room_name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(&event_id)
    .bind(&event.event)
    .bind(event.room.as_ref().map(|room| room.name.clone()))
    .execute(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "voice webhook dedupe insert failed");
        ApiError::dependency_unavailable("The database is temporarily unavailable.")
    })?
    .rows_affected()
        > 0;
    if !inserted {
        return Ok(Json(LiveKitWebhookAck {
            received: true,
            duplicate: true,
        }));
    }

    let Some(room) = event.room.as_ref() else {
        return Ok(Json(LiveKitWebhookAck {
            received: true,
            duplicate: false,
        }));
    };
    let Some(channel_id) = voice_core::parse_room_name(&room.name) else {
        return Ok(Json(LiveKitWebhookAck {
            received: true,
            duplicate: false,
        }));
    };
    let channel = match state.channels.get(channel_id).await {
        Ok(channel) if channel.kind == "voice" => channel,
        _ => {
            return Ok(Json(LiveKitWebhookAck {
                received: true,
                duplicate: false,
            }))
        }
    };

    match event.event.as_str() {
        "room_finished" => {
            for participant in state.voice.clear_channel(channel_id).await {
                publish_ended(
                    &state,
                    channel.community_id,
                    channel_id,
                    participant.user_id,
                    "room_finished",
                );
            }
        }
        "participant_joined" => {
            if let Some(participant) = event.participant.as_ref() {
                if let Ok(user_id) = participant.identity.parse::<Uuid>() {
                    if let Some(community_id) = channel.community_id {
                        if !state
                            .communities
                            .is_member(community_id, user_id)
                            .await
                            .unwrap_or(false)
                        {
                            return Ok(Json(LiveKitWebhookAck {
                                received: true,
                                duplicate: false,
                            }));
                        }
                    }
                    let display_name = if participant.name.is_empty() {
                        state
                            .users
                            .get_user(user_id)
                            .await
                            .map(|user| user.display_name)
                            .unwrap_or_else(|_| user_id.to_string())
                    } else {
                        participant.name.clone()
                    };
                    let joined_at = if participant.joined_at > 0 {
                        chrono::DateTime::from_timestamp(participant.joined_at, 0)
                            .unwrap_or_else(Utc::now)
                    } else if event.created_at > 0 {
                        chrono::DateTime::from_timestamp(event.created_at, 0)
                            .unwrap_or_else(Utc::now)
                    } else {
                        Utc::now()
                    };
                    let tracks = participant
                        .tracks
                        .iter()
                        .map(|track| VoiceTrack {
                            sid: track.sid.clone(),
                            source: track.source.clone(),
                            kind: track.kind.clone(),
                            muted: track.muted,
                        })
                        .collect();
                    state
                        .voice
                        .join(channel_id, user_id, display_name, joined_at, tracks)
                        .await;
                    if let Some(participant) = state.voice.participant(channel_id, user_id).await {
                        state.gateway.publish(gateway::GatewayEvent::channel(
                            "VOICE_STATE_UPDATE",
                            channel.community_id,
                            channel_id,
                            serde_json::to_value(VoiceParticipantDto::from(participant))
                                .unwrap_or_default(),
                        ));
                    }
                }
            }
        }
        "participant_left" | "participant_connection_aborted" => {
            if let Some(participant) = event.participant.as_ref() {
                if let Ok(user_id) = participant.identity.parse::<Uuid>() {
                    if let Some(existing) = state.voice.participant(channel_id, user_id).await {
                        let stale = event.created_at > 0
                            && existing.joined_at.timestamp() > event.created_at;
                        if !stale {
                            state.voice.leave(channel_id, user_id).await;
                            publish_ended(
                                &state,
                                channel.community_id,
                                channel_id,
                                user_id,
                                "left",
                            );
                        }
                    }
                }
            }
        }
        "track_published" | "track_unpublished" => {
            if let (Some(participant), Some(track)) =
                (event.participant.as_ref(), event.track.as_ref())
            {
                if let Ok(user_id) = participant.identity.parse::<Uuid>() {
                    if let Some(mut existing) = state.voice.participant(channel_id, user_id).await {
                        if event.event == "track_published" {
                            existing.tracks.retain(|item| item.sid != track.sid);
                            existing.tracks.push(VoiceTrack {
                                sid: track.sid.clone(),
                                source: track.source.clone(),
                                kind: track.kind.clone(),
                                muted: track.muted,
                            });
                        } else {
                            existing.tracks.retain(|item| item.sid != track.sid);
                        }
                        state
                            .voice
                            .update_tracks(channel_id, user_id, existing.tracks.clone())
                            .await;
                        state.gateway.publish(gateway::GatewayEvent::channel(
                            "VOICE_STATE_UPDATE",
                            channel.community_id,
                            channel_id,
                            serde_json::to_value(VoiceParticipantDto::from(existing))
                                .unwrap_or_default(),
                        ));
                    }
                }
            }
        }
        _ => {}
    }

    Ok(Json(LiveKitWebhookAck {
        received: true,
        duplicate: false,
    }))
}

/// Re-check every voice participant against current role/overwrite permissions.
pub(crate) async fn reconcile_community_voice(state: &AppState, community_id: Uuid) {
    let Ok(channels) = state.channels.list_for_community(community_id).await else {
        return;
    };
    for channel in channels
        .into_iter()
        .filter(|channel| channel.kind == "voice")
    {
        for participant in state.voice.participants(channel.id).await {
            let allowed = messages::authorize_channel(state, participant.user_id, channel.id)
                .await
                .map(|authorization| authorization.can(Permissions::CONNECT_VOICE))
                .unwrap_or(false);
            if !allowed {
                revoke_channel_user(state, &channel, participant.user_id, "permission_revoked")
                    .await;
            }
        }
    }
}

/// Best-effort removal of a user from every voice channel in a community.
pub(crate) async fn revoke_community_user(state: &AppState, community_id: Uuid, user_id: Uuid) {
    let channels = match state.channels.list_for_community(community_id).await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "voice revocation channel lookup failed");
            return;
        }
    };
    for channel in channels
        .into_iter()
        .filter(|channel| channel.kind == "voice")
    {
        revoke_channel_user(state, &channel, user_id, "membership_revoked").await;
    }
}

/// Best-effort update of a user's publish permission across a community's voice rooms.
pub(crate) async fn set_community_speaking(
    state: &AppState,
    community_id: Uuid,
    user_id: Uuid,
    can_publish: bool,
) {
    let Ok(channels) = state.channels.list_for_community(community_id).await else {
        return;
    };
    let Some(admin) = state.livekit_admin.as_deref() else {
        return;
    };
    for channel in channels
        .into_iter()
        .filter(|channel| channel.kind == "voice")
    {
        if !state.voice.contains(channel.id, user_id).await {
            continue;
        }
        if let Err(error) = admin
            .update_permissions(channel.id, user_id, can_publish, true)
            .await
        {
            tracing::warn!(%error, channel_id = %channel.id, "LiveKit publish permission update failed");
        }
        if let Some(participant) = state.voice.participant(channel.id, user_id).await {
            state.gateway.publish(gateway::GatewayEvent::channel(
                "VOICE_STATE_UPDATE",
                channel.community_id,
                channel.id,
                serde_json::to_value(VoiceParticipantDto::from(participant)).unwrap_or_default(),
            ));
        }
    }
}

/// Best-effort removal of a user from one voice channel.
pub(crate) async fn revoke_channel_user(
    state: &AppState,
    channel: &Channel,
    user_id: Uuid,
    reason: &str,
) {
    let was_present = state.voice.contains(channel.id, user_id).await;
    if !was_present {
        return;
    }
    if let Some(admin) = state.livekit_admin.as_deref() {
        if let Err(error) = admin.remove_participant(channel.id, user_id).await {
            tracing::warn!(%error, channel_id = %channel.id, "LiveKit participant removal failed");
        }
    }
    state.voice.leave(channel.id, user_id).await;
    publish_ended(state, channel.community_id, channel.id, user_id, reason);
}
