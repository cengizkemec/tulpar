use std::sync::Arc;

use auth::LogEmailSender;
use axum::{
    body::{to_bytes, Body},
    extract::{Path, State},
    http::{header, Request, StatusCode},
    routing::post,
    Json, Router,
};
use config::AppConfig;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::sync::Mutex;
use tower::ServiceExt;
use tulpar_api::{router, AppState};
use uuid::Uuid;

#[derive(Clone, Default)]
struct Recorded {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    participants: Arc<Mutex<Vec<Value>>>,
}

async fn mock_livekit(
    State(recorded): State<Recorded>,
    Path(method): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    recorded.calls.lock().await.push((method.clone(), body));
    if method == "ListParticipants" {
        let participants = recorded.participants.lock().await.clone();
        return Json(json!({ "participants": participants }));
    }
    Json(json!({}))
}

fn test_config(api_url: &str) -> AppConfig {
    let mut config = AppConfig::default();
    config.instance.environment = "test".to_string();
    config.instance.public_url = "http://localhost:8081".to_string();
    config.database.url = "postgres://unused".to_string();
    config.auth.jwt_secret = "x".repeat(32);
    config.auth.require_email_verification = false;
    config.auth.cookie_secure = false;
    config.email.driver = "log".to_string();
    config.storage.root = std::env::temp_dir()
        .join(format!("tulpar-voice-test-{}", Uuid::now_v7()))
        .display()
        .to_string();
    config.voice.livekit_url = "ws://localhost:7880".to_string();
    config.voice.livekit_api_url = api_url.to_string();
    config.voice.api_key = "devkey".to_string();
    config.voice.api_secret = "voice-secret-voice-secret-voice-secret".to_string();
    config.voice.turn_secret = "turn-secret".to_string();
    config.voice.turn_url = "turn:localhost:3478".to_string();
    config.voice.stun_url = "stun:localhost:3478".to_string();
    config
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn register_and_login(app: &axum::Router, username: &str, email: &str) -> String {
    let register = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "display_name": username,
                "email": email,
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(register).await.unwrap().status(),
        StatusCode::ACCEPTED
    );
    let login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "login": email, "password": "correct horse battery staple" }).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn with_auth(request: axum::http::request::Builder, token: &str) -> axum::http::request::Builder {
    request.header(header::AUTHORIZATION, format!("Bearer {token}"))
}

fn sign_webhook(secret: &str, body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let sha256 = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let now = chrono::Utc::now().timestamp();
    encode(
        &Header::new(Algorithm::HS256),
        &json!({
            "iss": "devkey",
            "sub": "livekit",
            "nbf": now - 1,
            "exp": now + 60,
            "sha256": sha256,
        }),
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

async fn webhook(app: &axum::Router, secret: &str, body: Value) -> axum::response::Response {
    let bytes = serde_json::to_vec(&body).unwrap();
    let token = sign_webhook(secret, &bytes);
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/livekit")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

fn decode_claims(token: &str, secret: &str) -> Value {
    let mut validation = jsonwebtoken::Validation::new(Algorithm::HS256);
    validation.set_issuer(&["devkey"]);
    jsonwebtoken::decode::<Value>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .unwrap()
    .claims
}

#[sqlx::test(migrations = "../database/migrations")]
async fn voice_tokens_passwords_webhooks_and_moderation(pool: PgPool) {
    let recorded = Recorded::default();
    let mock = Router::new()
        .route("/twirp/livekit.RoomService/{method}", post(mock_livekit))
        .with_state(recorded.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mock_server = tokio::spawn(async move {
        axum::serve(listener, mock).await.unwrap();
    });

    let config = test_config(&format!("http://{addr}"));
    let state = AppState::new(
        Arc::new(config),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state.clone());

    let owner_token = register_and_login(&app, "voiceowner", "voiceowner@example.com").await;
    let member_token = register_and_login(&app, "voicemember", "voicemember@example.com").await;
    let outsider_token =
        register_and_login(&app, "voiceoutsider", "voiceoutsider@example.com").await;

    let create = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Voice Community",
            "slug": "voice-community",
            "visibility": "private"
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let community = body_json(response).await;
    let community_id = community["community"]["id"].as_str().unwrap().to_string();

    let invite = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/invites"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "max_uses": 5 }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(invite).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let code = body_json(response).await["code"]
        .as_str()
        .unwrap()
        .to_string();
    let join = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/invites/{code}")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(join).await.unwrap().status(),
        StatusCode::OK
    );

    let member_id = {
        let request = with_auth(
            Request::builder().method("GET").uri("/api/v1/users/me"),
            &member_token,
        )
        .body(Body::empty())
        .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        body_json(response).await["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let create_voice = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/channels"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "kind": "voice",
            "name": "Voice Room",
            "user_limit": 10
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(create_voice).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let voice_channel_id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let token_request = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/token"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({}).to_string()))
    .unwrap();
    let response = app.clone().oneshot(token_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let voice = body_json(response).await;
    assert_eq!(voice["room"], format!("tulpar_channel_{voice_channel_id}"));
    assert_eq!(voice["url"], "ws://localhost:7880");
    assert_eq!(voice["ice_servers"].as_array().unwrap().len(), 2);
    assert!(
        voice["ice_servers"][1]["credential"]
            .as_str()
            .unwrap()
            .len()
            > 10
    );
    let claims = decode_claims(
        voice["token"].as_str().unwrap(),
        "voice-secret-voice-secret-voice-secret",
    );
    assert_eq!(
        claims["video"]["canPublishSources"],
        json!(["microphone", "camera", "screen_share", "screen_share_audio"])
    );

    let outsider = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/token"))
            .header(header::CONTENT_TYPE, "application/json"),
        &outsider_token,
    )
    .body(Body::from(json!({}).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(outsider).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let password = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{voice_channel_id}/voice/password"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "password": "room-secret" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(password).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let wrong_password = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/token"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "password": "wrong" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(wrong_password).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let correct_password = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/token"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "password": "room-secret" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(correct_password)
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // Timeout downgrades the client to a listener.
    let timeout = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!(
                "/api/v1/communities/{community_id}/members/{member_id}"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "timeout_seconds": 3600 }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(timeout).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let listener_token = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/token"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "password": "room-secret" }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(listener_token).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let listener = body_json(response).await;
    let claims = decode_claims(
        listener["token"].as_str().unwrap(),
        "voice-secret-voice-secret-voice-secret",
    );
    assert_eq!(claims["video"]["canPublishSources"], json!([]));

    // Capacity projection is enforced.
    let channel_id = voice_channel_id.parse::<Uuid>().unwrap();
    let other_user = Uuid::now_v7();
    state
        .voice
        .join(
            channel_id,
            other_user,
            "Other".to_string(),
            chrono::Utc::now(),
            Vec::new(),
        )
        .await;
    sqlx::query("UPDATE channels SET user_limit = 1 WHERE id = $1")
        .bind(channel_id)
        .execute(&pool)
        .await
        .unwrap();
    let full = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/token"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "password": "room-secret" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(full).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    state.voice.remove_user(other_user).await;

    // Signed join webhook projects the participant.
    let joined_at = chrono::Utc::now().timestamp();
    let joined_body = json!({
        "id": "event-join-1",
        "event": "participant_joined",
        "createdAt": joined_at,
        "room": { "sid": "room-1", "name": format!("tulpar_channel_{voice_channel_id}") },
        "participant": {
            "identity": member_id,
            "name": "voicemember",
            "joinedAt": joined_at,
            "state": "ACTIVE",
            "tracks": [{
                "sid": "TR_mic",
                "source": "microphone",
                "kind": "audio",
                "name": "mic",
                "muted": false
            }]
        }
    });
    let response = webhook(
        &app,
        "voice-secret-voice-secret-voice-secret",
        joined_body.clone(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["duplicate"], false);
    let states = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{voice_channel_id}/voice/states")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(states).await.unwrap();
    let states = body_json(response).await;
    assert_eq!(states.as_array().unwrap().len(), 1);
    assert_eq!(states[0]["user_id"], member_id);
    assert_eq!(states[0]["tracks"][0]["sid"], "TR_mic");

    // Replay is deduplicated.
    let response = webhook(
        &app,
        "voice-secret-voice-secret-voice-secret",
        joined_body.clone(),
    )
    .await;
    assert_eq!(body_json(response).await["duplicate"], true);

    // A stale participant_left cannot remove the rejoined participant.
    let stale_left = json!({
        "id": "event-left-stale",
        "event": "participant_left",
        "createdAt": joined_at - 10,
        "room": { "sid": "room-1", "name": format!("tulpar_channel_{voice_channel_id}") },
        "participant": { "identity": member_id, "name": "voicemember" }
    });
    let response = webhook(&app, "voice-secret-voice-secret-voice-secret", stale_left).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.voice.count(channel_id).await, 1);

    // Tampered bodies are rejected.
    let bytes = serde_json::to_vec(&joined_body).unwrap();
    let token = sign_webhook("voice-secret-voice-secret-voice-secret", &bytes);
    let tampered = Request::builder()
        .method("POST")
        .uri("/webhooks/livekit")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(br#"{"event":"room_finished"}"#.to_vec()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(tampered).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    // Server mute, deafen, and disconnect call LiveKit with the expected methods.
    let mute = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{voice_channel_id}/voice/participants/{member_id}/server-mute"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "enabled": true }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(mute).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let deafen = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{voice_channel_id}/voice/participants/{member_id}/server-deafen"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "enabled": true }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(deafen).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let calls = recorded.calls.lock().await.clone();
    assert!(calls
        .iter()
        .any(|(method, body)| method == "MutePublishedTrack" && body["track_sid"] == "TR_mic"));
    assert!(calls
        .iter()
        .any(|(method, body)| method == "UpdateParticipant"
            && body["permission"]["canSubscribe"] == false));

    // Reconciliation removes projection entries that LiveKit no longer reports.
    *recorded.participants.lock().await = vec![json!({
        "identity": member_id,
        "name": "voicemember",
        "joinedAt": joined_at,
        "tracks": [{
            "sid": "TR_mic",
            "source": "microphone",
            "type": "audio",
            "muted": false
        }]
    })];
    let ghost = Uuid::now_v7();
    state
        .voice
        .join(
            channel_id,
            ghost,
            "Ghost".to_string(),
            chrono::Utc::now(),
            Vec::new(),
        )
        .await;
    let reconciled = tulpar_api::workers::reconcile_voice_once(&state).await;
    assert_eq!(reconciled, 1);
    assert!(
        state
            .voice
            .contains(channel_id, member_id.parse().unwrap())
            .await
    );
    assert!(!state.voice.contains(channel_id, ghost).await);

    // Denying CONNECT_VOICE on the default role removes existing participants.
    let default_role_id = community["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let role_deny = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{voice_channel_id}/permissions/roles/{default_role_id}"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "allow": [], "deny": ["CONNECT_VOICE"] }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(role_deny).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(state.voice.count(channel_id).await, 0);

    // Rejoin through a fresh webhook for the kick revocation test.
    let rejoin_body = json!({
        "id": "event-join-2",
        "event": "participant_joined",
        "createdAt": joined_at + 1,
        "room": { "sid": "room-1", "name": format!("tulpar_channel_{voice_channel_id}") },
        "participant": {
            "identity": member_id,
            "name": "voicemember",
            "joinedAt": joined_at + 1,
            "state": "ACTIVE",
            "tracks": []
        }
    });
    let response = webhook(&app, "voice-secret-voice-secret-voice-secret", rejoin_body).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.voice.count(channel_id).await, 1);

    // Kick revokes the participant and closes the voice session.
    let kick = with_auth(
        Request::builder().method("DELETE").uri(format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(kick).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(state.voice.count(channel_id).await, 0);
    let calls = recorded.calls.lock().await.clone();
    assert!(calls
        .iter()
        .any(|(method, body)| { method == "RemoveParticipant" && body["identity"] == member_id }));

    mock_server.abort();
}
