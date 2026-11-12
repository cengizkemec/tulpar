use std::time::Duration;

use axum::{
    extract::{
        ws::{CloseFrame, Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    Json,
};
use futures_util::{SinkExt, StreamExt};
use gateway::{GatewayEvent, HEARTBEAT_INTERVAL_MS, PROTOCOL_VERSION};
use permissions::Permissions;
use serde_json::{json, Value};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    dto::GatewayTicketDto,
    error::ApiError,
    extract::{enforce_rate_limit, CurrentUser},
    state::AppState,
};

pub(crate) fn gateway_url(public_url: &str) -> String {
    let url = public_url.trim_end_matches('/');
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}/gateway")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}/gateway")
    } else {
        format!("{url}/gateway")
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/gateway/ticket",
    tag = "gateway",
    security(("bearer_auth" = [])),
    responses((status = 200, body = GatewayTicketDto))
)]
pub async fn issue_ticket(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<GatewayTicketDto>, ApiError> {
    enforce_rate_limit(
        &state,
        &format!("gateway:ticket:{}", current.user().id),
        10,
        Duration::from_secs(60),
    )
    .await?;
    let (ticket, expires_in) = state
        .tickets
        .issue(current.user().id, current.session_id())
        .await
        .map_err(|error| {
            tracing::error!(%error, "gateway ticket issuance failed");
            ApiError::dependency_unavailable("The gateway is temporarily unavailable.")
        })?;
    Ok(Json(GatewayTicketDto {
        ticket,
        expires_in,
        gateway_url: gateway_url(&state.config.instance.public_url),
    }))
}

#[utoipa::path(
    get,
    path = "/gateway",
    tag = "gateway",
    responses((status = 101, description = "WebSocket upgrade"))
)]
pub async fn gateway(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> axum::response::Response {
    ws.max_message_size(64 * 1024)
        .on_upgrade(move |socket| handle_socket(state, socket))
}

async fn send_json(
    sender: &mut futures_util::stream::SplitSink<WebSocket, WsMessage>,
    value: &Value,
) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    tokio::time::timeout(
        Duration::from_secs(5),
        sender.send(WsMessage::Text(text.into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

async fn close(
    sender: &mut futures_util::stream::SplitSink<WebSocket, WsMessage>,
    code: u16,
    reason: &str,
) {
    let _ = sender
        .send(WsMessage::Close(Some(CloseFrame {
            code,
            reason: reason.to_string().into(),
        })))
        .await;
}

async fn handle_socket(state: AppState, socket: WebSocket) {
    let (mut sender, mut ws_receiver) = socket.split();

    let hello = json!({
        "op": 10,
        "d": {
            "protocol_version": PROTOCOL_VERSION,
            "heartbeat_interval_ms": HEARTBEAT_INTERVAL_MS,
            "max_frame_bytes": 65536,
            "server_time": chrono::Utc::now(),
        }
    });
    if send_json(&mut sender, &hello).await.is_err() {
        return;
    }

    let identify = tokio::time::timeout(Duration::from_secs(5), ws_receiver.next()).await;
    let frame = match identify {
        Ok(Some(Ok(WsMessage::Text(text)))) => serde_json::from_str::<Value>(&text).ok(),
        _ => None,
    };
    let Some(frame) = frame else {
        close(&mut sender, 4002, "identify timeout").await;
        return;
    };
    if frame.get("op").and_then(Value::as_u64) != Some(2) {
        close(&mut sender, 4001, "identify required").await;
        return;
    }
    let data = frame.get("d").cloned().unwrap_or(Value::Null);
    let ticket = data
        .get("ticket")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let protocol = data
        .get("protocol_version")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if protocol != PROTOCOL_VERSION as u64 {
        close(&mut sender, 4001, "unsupported protocol version").await;
        return;
    }
    let Some(record) = state.tickets.consume(ticket).await else {
        close(&mut sender, 4004, "ticket expired or used").await;
        return;
    };
    let user = match state.users.get_user(record.user_id).await {
        Ok(user) if user.deleted_at.is_none() && user.disabled_at.is_none() => user,
        _ => {
            close(&mut sender, 4003, "account unavailable").await;
            return;
        }
    };

    let mut events = state.gateway.subscribe();
    let connection_id = state.gateway.connect(user.id, &user.preferred_status).await;

    let ready = json!({
        "op": 3,
        "t": "READY",
        "s": 0,
        "id": Uuid::now_v7(),
        "d": {
            "session_id": record.session_id,
            "user": crate::dto::SelfUserDto::from(&user),
            "instance": {
                "name": state.config.instance.name,
                "public_url": state.config.instance.public_url,
                "features": { "voice": false, "video": false, "embeds": false }
            },
            "heartbeat_interval_ms": HEARTBEAT_INTERVAL_MS,
            "protocol_version": PROTOCOL_VERSION
        }
    });
    if send_json(&mut sender, &ready).await.is_err() {
        state.gateway.disconnect(user.id, connection_id).await;
        return;
    }

    let presence = status_event(user.id, &state.gateway.status(user.id).await);
    state
        .gateway
        .publish(GatewayEvent::global("PRESENCE_UPDATE", presence));

    let mut sequence: u64 = 0;
    let mut last_heartbeat = Instant::now();
    let mut heartbeat_check = tokio::time::interval(Duration::from_secs(10));
    heartbeat_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = heartbeat_check.tick() => {
                if last_heartbeat.elapsed() > Duration::from_millis(HEARTBEAT_INTERVAL_MS * 2 + 15_000) {
                    close(&mut sender, 4000, "heartbeat timeout").await;
                    break;
                }
            }
            incoming = ws_receiver.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                            close(&mut sender, 4001, "invalid payload").await;
                            break;
                        };
                        match frame.get("op").and_then(Value::as_u64) {
                            Some(1) => {
                                last_heartbeat = Instant::now();
                                let seq = frame.get("d").and_then(|d| d.get("seq")).cloned().unwrap_or(Value::Null);
                                let ack = json!({ "op": 11, "d": { "seq": seq } });
                                if send_json(&mut sender, &ack).await.is_err() {
                                    break;
                                }
                            }
                            Some(15) => {
                                let status = frame.get("d").and_then(|d| d.get("status")).and_then(Value::as_str).unwrap_or_default();
                                let decision = state.rate_limiter.check(
                                    &format!("gateway:presence:{}", user.id),
                                    5,
                                    Duration::from_secs(60),
                                ).await;
                                if !decision.allowed {
                                    let _ = send_json(&mut sender, &json!({ "op": 14, "d": { "retry_after_ms": decision.retry_after.as_millis() } })).await;
                                    continue;
                                }
                                if state.gateway.update_status(user.id, connection_id, status).await.is_err() {
                                    continue;
                                }
                                let payload = status_event(user.id, &state.gateway.status(user.id).await);
                                state.gateway.publish(GatewayEvent::global("PRESENCE_UPDATE", payload));
                            }
                            Some(16) => {
                                let Some(channel_id) = frame.get("d").and_then(|d| d.get("channel_id")).and_then(Value::as_str).and_then(|value| value.parse::<Uuid>().ok()) else {
                                    continue;
                                };
                                let authorization = match super::messages::authorize_channel(&state, user.id, channel_id).await {
                                    Ok(authorization) => authorization,
                                    Err(_) => continue,
                                };
                                if authorization.require(Permissions::SEND_MESSAGES).is_err() {
                                    continue;
                                }
                                let decision = state.rate_limiter.check(
                                    &format!("gateway:typing:{}:{channel_id}", user.id),
                                    1,
                                    Duration::from_secs(5),
                                ).await;
                                if !decision.allowed {
                                    continue;
                                }
                                let payload = json!({
                                    "channel_id": channel_id,
                                    "user_id": user.id,
                                    "expires_at": chrono::Utc::now() + chrono::Duration::seconds(8),
                                });
                                state.gateway.publish(GatewayEvent::channel(
                                    "TYPING_START",
                                    None,
                                    channel_id,
                                    payload,
                                ));
                            }
                            Some(17 | 18) => {}
                            Some(_) => {
                                close(&mut sender, 4001, "unsupported opcode").await;
                                break;
                            }
                            None => {
                                close(&mut sender, 4001, "invalid payload").await;
                                break;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Ok(WsMessage::Ping(payload))) => {
                        if sender.send(WsMessage::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {}
                    Some(Ok(WsMessage::Binary(_))) => {}
                    Some(Err(_)) => break,
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if !can_receive(&state, user.id, &event).await {
                            continue;
                        }
                        sequence += 1;
                        let envelope = json!({
                            "op": 0,
                            "t": event.event,
                            "s": sequence,
                            "id": Uuid::now_v7(),
                            "d": event.payload,
                        });
                        if send_json(&mut sender, &envelope).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = send_json(&mut sender, &json!({ "op": 13, "d": { "reason": "slow_consumer" } })).await;
                        close(&mut sender, 4008, "slow consumer").await;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    let status = state.gateway.disconnect(user.id, connection_id).await;
    state.gateway.publish(GatewayEvent::global(
        "PRESENCE_UPDATE",
        status_event(user.id, &status),
    ));
}

fn status_event(user_id: Uuid, status: &str) -> Value {
    json!({
        "user_id": user_id,
        "status": status,
        "last_seen_at": null,
        "updated_at": chrono::Utc::now(),
    })
}

async fn can_receive(state: &AppState, viewer_id: Uuid, event: &GatewayEvent) -> bool {
    if let Some(target) = event.target_user_id {
        if target == viewer_id {
            return true;
        }
    }
    if event.event == "PRESENCE_UPDATE" {
        let Some(subject_id) = event
            .payload
            .get("user_id")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<Uuid>().ok())
        else {
            return false;
        };
        if subject_id != viewer_id
            && event.payload.get("status").and_then(Value::as_str) == Some("invisible")
        {
            return false;
        }
        return can_see_presence(state, viewer_id, subject_id).await;
    }
    if let Some(channel_id) = event.channel_id {
        let allowed = super::messages::authorize_channel(state, viewer_id, channel_id)
            .await
            .is_ok();
        if !allowed {
            return false;
        }
        if event.event == "TYPING_START" {
            if let Some(subject_id) = event
                .payload
                .get("user_id")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<Uuid>().ok())
            {
                return !state
                    .social
                    .is_blocked(viewer_id, subject_id)
                    .await
                    .unwrap_or(true);
            }
        }
        return true;
    }
    if let Some(community_id) = event.community_id {
        return state
            .communities
            .is_member(community_id, viewer_id)
            .await
            .unwrap_or(false);
    }
    false
}

async fn can_see_presence(state: &AppState, viewer_id: Uuid, subject_id: Uuid) -> bool {
    if viewer_id == subject_id {
        return true;
    }
    if state
        .social
        .is_blocked(viewer_id, subject_id)
        .await
        .unwrap_or(true)
    {
        return false;
    }
    let Ok(settings) = state.users.get_settings(subject_id).await else {
        return false;
    };
    match settings.presence_visibility.as_str() {
        "everyone" => true,
        "friends" => state
            .social
            .are_friends(viewer_id, subject_id)
            .await
            .unwrap_or(false),
        _ => false,
    }
}
