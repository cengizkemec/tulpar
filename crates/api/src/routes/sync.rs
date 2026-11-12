use axum::{extract::State, Json};
use chrono::Utc;

use crate::{
    dto::{
        BlockedUserDto, ChannelDto, CommunityDto, DirectConversationDto, FriendUserDto,
        FriendshipDto, ReadStateDto, RoleDto, SyncCommunityDto, SyncGatewayDto, SyncInstanceDto,
        SyncInstanceResponse, SyncResponse,
    },
    error::ApiError,
    extract::CurrentUser,
    routes::gateway::gateway_url,
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/sync",
    tag = "sync",
    security(("bearer_auth" = [])),
    responses((status = 200, body = SyncResponse))
)]
pub async fn sync(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<SyncResponse>, ApiError> {
    let user_id = current.user().id;
    let settings = state.users.get_settings(user_id).await?;
    let communities = state.communities.list_for_user(user_id).await?;
    let mut sync_communities = Vec::with_capacity(communities.len());
    let mut unread_counts = std::collections::HashMap::new();

    for community in communities {
        let roles = state.communities.list_roles(community.id).await?;
        let channels = super::channels::visible_channels(&state, community.id, user_id).await?;
        let mut read_states = Vec::with_capacity(channels.len());
        for channel in &channels {
            let read_state = state.messages.read_state(user_id, channel.id).await?;
            unread_counts.insert(channel.id, read_state.unread_count);
            read_states.push(ReadStateDto::from(read_state));
        }
        sync_communities.push(SyncCommunityDto {
            community: CommunityDto::from(community),
            roles: roles.into_iter().map(RoleDto::from).collect(),
            channels: channels.into_iter().map(ChannelDto::from).collect(),
            read_states,
        });
    }

    let dms = state.channels.list_direct_for_user(user_id).await?;
    for dm in &dms {
        let read_state = state.messages.read_state(user_id, dm.channel.id).await?;
        unread_counts.insert(dm.channel.id, read_state.unread_count);
    }

    let friends = state.social.list_friends(user_id).await?;
    let friend_requests = state.social.list_requests(user_id).await?;
    let blocks = state.social.list_blocks(user_id).await?;
    let notifications_unread = state.notifications.unread_count(user_id).await?;

    Ok(Json(SyncResponse {
        server_time: Utc::now(),
        instance: SyncInstanceDto {
            name: state.config.instance.name.clone(),
            public_url: state.config.instance.public_url.clone(),
            features: serde_json::json!({ "voice": false, "video": false, "embeds": false }),
        },
        user: current.user().into(),
        settings: settings.into(),
        communities: sync_communities,
        friends: friends.into_iter().map(FriendUserDto::from).collect(),
        friend_requests: friend_requests
            .into_iter()
            .map(FriendshipDto::from)
            .collect(),
        blocks: blocks.into_iter().map(BlockedUserDto::from).collect(),
        dms: dms.into_iter().map(DirectConversationDto::from).collect(),
        unread_counts,
        notifications_unread,
        gateway: SyncGatewayDto {
            url: gateway_url(&state.config.instance.public_url),
            heartbeat_interval_ms: gateway::HEARTBEAT_INTERVAL_MS,
            protocol_version: gateway::PROTOCOL_VERSION,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/sync/instance",
    tag = "sync",
    responses((status = 200, body = SyncInstanceResponse))
)]
pub async fn sync_instance(State(state): State<AppState>) -> Json<SyncInstanceResponse> {
    Json(SyncInstanceResponse {
        name: state.config.instance.name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_version: tulpar_core::API_VERSION.to_string(),
        public_url: state.config.instance.public_url.clone(),
        features: serde_json::json!({ "voice": false, "video": false, "embeds": false }),
        registration_enabled: state.config.auth.registration_enabled,
        require_email_verification: state.config.auth.require_email_verification,
    })
}
