//! Voice authorization, LiveKit tokens/webhooks, TURN credentials, and projection.

#![forbid(unsafe_code)]

use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("invalid voice configuration: {0}")]
    Invalid(String),
    #[error("voice is unavailable")]
    Unavailable,
    #[error("forbidden")]
    Forbidden,
    #[error("room is full")]
    RoomFull,
    #[error("not found")]
    NotFound,
    #[error("webhook signature is invalid")]
    WebhookInvalid,
    #[error("voice backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoicePermissions {
    pub connect: bool,
    pub speak: bool,
    pub stream_video: bool,
    pub share_screen: bool,
}

impl VoicePermissions {
    pub fn sources(&self) -> Vec<&'static str> {
        let mut sources = Vec::new();
        if self.speak {
            sources.push("microphone");
        }
        if self.stream_video {
            sources.push("camera");
        }
        if self.share_screen {
            sources.push("screen_share");
            sources.push("screen_share_audio");
        }
        sources
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

pub fn verify_password(password: &str, stored_hash: Option<&str>) -> bool {
    let Some(stored_hash) = stored_hash else {
        return true;
    };
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn room_name(channel_id: Uuid) -> String {
    format!("tulpar_channel_{channel_id}")
}

pub fn parse_room_name(room: &str) -> Option<Uuid> {
    room.strip_prefix("tulpar_channel_")
        .and_then(|value| value.parse().ok())
}

#[allow(clippy::too_many_arguments)]
pub fn mint_token(
    api_key: &str,
    api_secret: &str,
    user_id: Uuid,
    display_name: &str,
    channel_id: Uuid,
    permissions: VoicePermissions,
    ttl_seconds: i64,
    session_id: Uuid,
) -> Result<(String, DateTime<Utc>), VoiceError> {
    if api_key.is_empty() || api_secret.is_empty() || ttl_seconds <= 0 {
        return Err(VoiceError::Unavailable);
    }
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(ttl_seconds);
    let sources = permissions.sources();
    let can_publish = !sources.is_empty();
    let video = json!({
        "room": room_name(channel_id),
        "roomJoin": true,
        "canPublish": can_publish,
        "canSubscribe": true,
        "canPublishData": false,
        "canPublishSources": sources,
        "roomAdmin": false,
        "roomCreate": false,
        "hidden": false,
    });
    let claims = json!({
        "iss": api_key,
        "sub": user_id.to_string(),
        "name": display_name,
        "nbf": now.timestamp(),
        "exp": expires_at.timestamp(),
        "video": video,
        "metadata": json!({ "userId": user_id, "sessionId": session_id }).to_string(),
    });
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(api_secret.as_bytes()),
    )
    .map_err(|error| VoiceError::Backend(error.to_string()))?;
    Ok((token, expires_at))
}

pub fn turn_credentials(
    secret: &str,
    user_id: Uuid,
    ttl_seconds: i64,
) -> Result<(String, String), VoiceError> {
    if secret.is_empty() {
        return Err(VoiceError::Unavailable);
    }
    let expires_at = Utc::now().timestamp() + ttl_seconds;
    let username = format!("{expires_at}:{user_id}");
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
        .map_err(|error| VoiceError::Invalid(error.to_string()))?;
    mac.update(username.as_bytes());
    let credential = BASE64.encode(mac.finalize().into_bytes());
    Ok((username, credential))
}

pub fn ice_servers(
    stun_url: &str,
    turn_url: &str,
    turn_tls_url: Option<&str>,
    turn_secret: &str,
    user_id: Uuid,
    ttl_seconds: i64,
) -> Result<Vec<IceServer>, VoiceError> {
    let mut servers = Vec::new();
    if !stun_url.trim().is_empty() {
        servers.push(IceServer {
            urls: vec![stun_url.to_string()],
            username: None,
            credential: None,
        });
    }
    if !turn_url.trim().is_empty() {
        let (username, credential) = turn_credentials(turn_secret, user_id, ttl_seconds)?;
        servers.push(IceServer {
            urls: vec![turn_url.to_string()],
            username: Some(username),
            credential: Some(credential),
        });
    }
    if let Some(turn_tls_url) = turn_tls_url.filter(|value| !value.trim().is_empty()) {
        let (username, credential) = turn_credentials(turn_secret, user_id, ttl_seconds)?;
        servers.push(IceServer {
            urls: vec![turn_tls_url.to_string()],
            username: Some(username),
            credential: Some(credential),
        });
    }
    Ok(servers)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookEvent {
    #[serde(default)]
    pub id: String,
    pub event: String,
    #[serde(default)]
    pub created_at: i64,
    pub room: Option<WebhookRoom>,
    pub participant: Option<WebhookParticipant>,
    pub track: Option<WebhookTrack>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRoom {
    #[serde(default)]
    pub sid: String,
    pub name: String,
    #[serde(default)]
    pub num_participants: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookParticipant {
    pub identity: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub joined_at: i64,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub tracks: Vec<WebhookTrack>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookTrack {
    pub sid: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub muted: bool,
}

pub fn verify_webhook(
    api_key: &str,
    api_secret: &str,
    authorization: &str,
    body: &[u8],
) -> Result<WebhookEvent, VoiceError> {
    if api_key.is_empty() || api_secret.is_empty() {
        return Err(VoiceError::Unavailable);
    }
    let token = authorization
        .strip_prefix("Bearer ")
        .unwrap_or(authorization)
        .trim();
    if token.is_empty() {
        return Err(VoiceError::WebhookInvalid);
    }
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[api_key]);
    let data = decode::<Value>(
        token,
        &DecodingKey::from_secret(api_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| VoiceError::WebhookInvalid)?;

    let expected = data
        .claims
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or(VoiceError::WebhookInvalid)?;
    let actual = hex_sha256(body);
    if !constant_time_eq(expected.as_bytes(), actual.as_bytes()) {
        return Err(VoiceError::WebhookInvalid);
    }

    serde_json::from_slice(body).map_err(|_| VoiceError::WebhookInvalid)
}

fn hex_sha256(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceTrack {
    pub sid: String,
    pub source: String,
    pub kind: String,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceParticipant {
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub display_name: String,
    pub joined_at: DateTime<Utc>,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub server_mute: bool,
    pub server_deaf: bool,
    pub tracks: Vec<VoiceTrack>,
}

#[derive(Clone, Default)]
pub struct VoiceProjection {
    rooms: Arc<Mutex<HashMap<Uuid, HashMap<Uuid, VoiceParticipant>>>>,
}

impl VoiceProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn join(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        display_name: String,
        joined_at: DateTime<Utc>,
        tracks: Vec<VoiceTrack>,
    ) {
        let mut rooms = self.rooms.lock().await;
        let participants = rooms.entry(channel_id).or_default();
        let entry = participants
            .entry(user_id)
            .or_insert_with(|| VoiceParticipant {
                user_id,
                channel_id,
                display_name: display_name.clone(),
                joined_at,
                self_mute: false,
                self_deaf: false,
                server_mute: false,
                server_deaf: false,
                tracks: Vec::new(),
            });
        entry.display_name = display_name;
        entry.joined_at = joined_at;
        entry.tracks = tracks;
    }

    pub async fn leave(&self, channel_id: Uuid, user_id: Uuid) -> Option<VoiceParticipant> {
        let mut rooms = self.rooms.lock().await;
        let removed = rooms
            .get_mut(&channel_id)
            .and_then(|participants| participants.remove(&user_id));
        if rooms
            .get(&channel_id)
            .is_some_and(|participants| participants.is_empty())
        {
            rooms.remove(&channel_id);
        }
        removed
    }

    pub async fn remove_user(&self, user_id: Uuid) {
        let mut rooms = self.rooms.lock().await;
        for participants in rooms.values_mut() {
            participants.remove(&user_id);
        }
        rooms.retain(|_, participants| !participants.is_empty());
    }

    pub async fn clear_channel(&self, channel_id: Uuid) -> Vec<VoiceParticipant> {
        let mut rooms = self.rooms.lock().await;
        rooms
            .remove(&channel_id)
            .map(|participants| participants.into_values().collect())
            .unwrap_or_default()
    }

    pub async fn participants(&self, channel_id: Uuid) -> Vec<VoiceParticipant> {
        let rooms = self.rooms.lock().await;
        rooms
            .get(&channel_id)
            .map(|participants| participants.values().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn participant(&self, channel_id: Uuid, user_id: Uuid) -> Option<VoiceParticipant> {
        let rooms = self.rooms.lock().await;
        rooms
            .get(&channel_id)
            .and_then(|participants| participants.get(&user_id))
            .cloned()
    }

    pub async fn count(&self, channel_id: Uuid) -> usize {
        let rooms = self.rooms.lock().await;
        rooms.get(&channel_id).map(HashMap::len).unwrap_or_default()
    }

    pub async fn contains(&self, channel_id: Uuid, user_id: Uuid) -> bool {
        let rooms = self.rooms.lock().await;
        rooms
            .get(&channel_id)
            .is_some_and(|participants| participants.contains_key(&user_id))
    }

    pub async fn set_server_mute(&self, channel_id: Uuid, user_id: Uuid, muted: bool) -> bool {
        let mut rooms = self.rooms.lock().await;
        if let Some(participant) = rooms
            .get_mut(&channel_id)
            .and_then(|participants| participants.get_mut(&user_id))
        {
            participant.server_mute = muted;
            true
        } else {
            false
        }
    }

    pub async fn set_server_deaf(&self, channel_id: Uuid, user_id: Uuid, deafened: bool) -> bool {
        let mut rooms = self.rooms.lock().await;
        if let Some(participant) = rooms
            .get_mut(&channel_id)
            .and_then(|participants| participants.get_mut(&user_id))
        {
            participant.server_deaf = deafened;
            true
        } else {
            false
        }
    }

    pub async fn update_tracks(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        tracks: Vec<VoiceTrack>,
    ) -> bool {
        let mut rooms = self.rooms.lock().await;
        if let Some(participant) = rooms
            .get_mut(&channel_id)
            .and_then(|participants| participants.get_mut(&user_id))
        {
            participant.tracks = tracks;
            true
        } else {
            false
        }
    }
}

pub struct LiveKitAdmin {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    api_secret: String,
}

impl LiveKitAdmin {
    pub fn new(base_url: &str, api_key: &str, api_secret: &str) -> Result<Self, VoiceError> {
        if base_url.trim().is_empty() || api_key.is_empty() || api_secret.is_empty() {
            return Err(VoiceError::Unavailable);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|error| VoiceError::Backend(error.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        })
    }

    fn admin_token(&self, room: &str) -> Result<String, VoiceError> {
        let now = Utc::now();
        let claims = json!({
            "iss": self.api_key,
            "sub": "tulpar-server",
            "nbf": now.timestamp(),
            "exp": (now + chrono::Duration::seconds(60)).timestamp(),
            "video": {
                "room": room,
                "roomAdmin": true,
                "roomJoin": true,
                "canPublish": false,
                "canSubscribe": false,
                "roomCreate": true,
            }
        });
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.api_secret.as_bytes()),
        )
        .map_err(|error| VoiceError::Backend(error.to_string()))
    }

    async fn call<T: Serialize + ?Sized>(
        &self,
        method: &str,
        body: &T,
        room: &str,
    ) -> Result<(), VoiceError> {
        let token = self.admin_token(room)?;
        let response = self
            .client
            .post(format!(
                "{}/twirp/livekit.RoomService/{method}",
                self.base_url
            ))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|error| VoiceError::Backend(error.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(VoiceError::Backend(format!("{status}: {body}")))
        }
    }

    pub async fn remove_participant(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), VoiceError> {
        let room = room_name(channel_id);
        self.call(
            "RemoveParticipant",
            &json!({ "room": room, "identity": user_id.to_string() }),
            &room,
        )
        .await
    }

    pub async fn mute_track(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        track_sid: &str,
        muted: bool,
    ) -> Result<(), VoiceError> {
        let room = room_name(channel_id);
        self.call(
            "MutePublishedTrack",
            &json!({
                "room": room,
                "identity": user_id.to_string(),
                "track_sid": track_sid,
                "muted": muted
            }),
            &room,
        )
        .await
    }

    pub async fn update_permissions(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        can_publish: bool,
        can_subscribe: bool,
    ) -> Result<(), VoiceError> {
        let room = room_name(channel_id);
        self.call(
            "UpdateParticipant",
            &json!({
                "room": room,
                "identity": user_id.to_string(),
                "permission": {
                    "canPublish": can_publish,
                    "canSubscribe": can_subscribe,
                    "canPublishData": false
                }
            }),
            &room,
        )
        .await
    }

    pub async fn delete_room(&self, channel_id: Uuid) -> Result<(), VoiceError> {
        let room = room_name(channel_id);
        self.call("DeleteRoom", &json!({ "room": room }), &room)
            .await
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_contains_permission_sources() {
        let user_id = Uuid::now_v7();
        let channel_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let (token, _) = mint_token(
            "key",
            "secret-secret-secret-secret-secret",
            user_id,
            "Ada",
            channel_id,
            VoicePermissions {
                connect: true,
                speak: true,
                stream_video: false,
                share_screen: true,
            },
            600,
            session_id,
        )
        .unwrap();
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&["key"]);
        let decoded = decode::<Value>(
            &token,
            &DecodingKey::from_secret(b"secret-secret-secret-secret-secret"),
            &validation,
        )
        .unwrap();
        assert_eq!(decoded.claims["sub"], user_id.to_string());
        assert_eq!(
            decoded.claims["video"]["canPublishSources"],
            json!(["microphone", "screen_share", "screen_share_audio"])
        );
        assert_eq!(decoded.claims["video"]["roomJoin"], true);
        assert_eq!(decoded.claims["video"]["roomAdmin"], false);
    }

    #[test]
    fn turn_credentials_are_time_bound_and_deterministic() {
        let user_id = Uuid::now_v7();
        let (username, credential) = turn_credentials("turn-secret", user_id, 600).unwrap();
        assert!(username.ends_with(&user_id.to_string()));
        let (_, credential_again) = turn_credentials("turn-secret", user_id, 600).unwrap();
        assert_eq!(credential, credential_again);
    }

    #[test]
    fn room_names_round_trip() {
        let channel_id = Uuid::now_v7();
        assert_eq!(parse_room_name(&room_name(channel_id)), Some(channel_id));
        assert_eq!(parse_room_name("other"), None);
    }

    #[tokio::test]
    async fn projection_tracks_join_leave_and_moderation() {
        let projection = VoiceProjection::new();
        let channel_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        projection
            .join(channel_id, user_id, "Ada".into(), Utc::now(), Vec::new())
            .await;
        assert_eq!(projection.count(channel_id).await, 1);
        assert!(projection.set_server_mute(channel_id, user_id, true).await);
        assert!(
            projection
                .participant(channel_id, user_id)
                .await
                .unwrap()
                .server_mute
        );
        assert!(projection.leave(channel_id, user_id).await.is_some());
        assert_eq!(projection.count(channel_id).await, 0);
    }
}
