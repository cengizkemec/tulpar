use std::{sync::Arc, time::Duration};

use auth::LogEmailSender;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use config::AppConfig;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};
use tower::ServiceExt;
use tulpar_api::{router, AppState};
use uuid::Uuid;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn test_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.instance.environment = "test".to_string();
    config.instance.public_url = "http://localhost:8081".to_string();
    config.database.url = "postgres://unused".to_string();
    config.auth.jwt_secret = "x".repeat(32);
    config.auth.require_email_verification = false;
    config.auth.cookie_secure = false;
    config.email.driver = "log".to_string();
    config.storage.root = std::env::temp_dir()
        .join(format!("tulpar-gateway-test-{}", Uuid::now_v7()))
        .display()
        .to_string();
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

async fn create_community(app: &axum::Router, token: &str) -> Value {
    let request = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        token,
    )
    .body(Body::from(
        json!({
            "name": "Gateway Community",
            "slug": "gateway-community",
            "visibility": "private"
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

async fn next_json(socket: &mut Socket) -> Option<Value> {
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .ok()??
        .ok()?;
    match frame {
        WsMessage::Text(text) => serde_json::from_str(&text).ok(),
        _ => None,
    }
}

async fn wait_for(socket: &mut Socket, event: &str) -> Value {
    for _ in 0..20 {
        let Some(value) = next_json(socket).await else {
            continue;
        };
        if value.get("t").and_then(Value::as_str) == Some(event) {
            return value;
        }
    }
    panic!("event {event} not received");
}

async fn wait_for_op(socket: &mut Socket, op: u64) -> Value {
    for _ in 0..20 {
        let Some(value) = next_json(socket).await else {
            continue;
        };
        if value.get("op").and_then(Value::as_u64) == Some(op) {
            return value;
        }
    }
    panic!("opcode {op} not received");
}

async fn open_socket(addr: std::net::SocketAddr, ticket: &str) -> Socket {
    let (mut socket, _) = connect_async(format!("ws://{addr}/gateway"))
        .await
        .expect("websocket connect");
    let hello = next_json(&mut socket).await.expect("hello");
    assert_eq!(hello["op"], 10);
    socket
        .send(WsMessage::Text(
            json!({
                "op": 2,
                "d": { "ticket": ticket, "protocol_version": 1 }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let ready = next_json(&mut socket).await.expect("ready");
    assert_eq!(ready["op"], 3);
    assert_eq!(ready["t"], "READY");
    socket
}

async fn ticket(app: &axum::Router, token: &str) -> String {
    let request = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/gateway/ticket"),
        token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await["ticket"]
        .as_str()
        .unwrap()
        .to_string()
}

#[sqlx::test(migrations = "../database/migrations")]
async fn gateway_presence_typing_messages_and_permissions(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state.clone());

    let owner_token = register_and_login(&app, "gatewayowner", "gatewayowner@example.com").await;
    let member_token = register_and_login(&app, "gatewaymember", "gatewaymember@example.com").await;
    let community = create_community(&app, &owner_token).await;
    let community_id = community["community"]["id"].as_str().unwrap().to_string();
    let channel_id = community["channels"].as_array().unwrap()[1]["id"]
        .as_str()
        .unwrap()
        .to_string();

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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_app = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, server_app).await.unwrap();
    });

    let owner_socket = open_socket(addr, &ticket(&app, &owner_token).await).await;
    let mut owner_socket = owner_socket;
    let _presence = wait_for(&mut owner_socket, "PRESENCE_UPDATE").await;

    owner_socket
        .send(WsMessage::Text(
            json!({ "op": 1, "d": { "seq": 0 } }).to_string().into(),
        ))
        .await
        .unwrap();
    let ack = wait_for_op(&mut owner_socket, 11).await;
    assert_eq!(ack["op"], 11);

    let mut member_socket = open_socket(addr, &ticket(&app, &member_token).await).await;
    let _member_ready_presence = wait_for(&mut member_socket, "PRESENCE_UPDATE").await;

    let send = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(
        json!({ "content": "gateway hello" }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(send).await.unwrap().status(),
        StatusCode::OK
    );
    let created = wait_for(&mut owner_socket, "MESSAGE_CREATE").await;
    assert_eq!(created["d"]["content"], "gateway hello");

    member_socket
        .send(WsMessage::Text(
            json!({ "op": 16, "d": { "channel_id": channel_id } })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let typing = wait_for(&mut owner_socket, "TYPING_START").await;
    assert_eq!(typing["d"]["user_id"], member_id);

    member_socket
        .send(WsMessage::Text(
            json!({ "op": 15, "d": { "status": "dnd" } })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let presence = wait_for(&mut owner_socket, "PRESENCE_UPDATE").await;
    assert_eq!(presence["d"]["status"], "dnd");

    // A channel overwrite takes effect on an existing socket.
    let deny = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{channel_id}/permissions/users/{member_id}"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "allow": [], "deny": ["VIEW_CHANNEL"] }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(deny).await.unwrap().status(),
        StatusCode::OK
    );

    let denied_send = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "content": "hidden" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(denied_send).await.unwrap().status(),
        StatusCode::OK
    );
    let hidden = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            let value = next_json(&mut member_socket).await?;
            if value.get("t").and_then(Value::as_str) == Some("MESSAGE_CREATE")
                && value["d"]["content"] == "hidden"
            {
                return Some(value);
            }
        }
    })
    .await;
    assert!(hidden.is_err() || hidden.unwrap().is_none());

    let sync = with_auth(
        Request::builder().method("GET").uri("/api/v1/sync"),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(sync).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let snapshot = body_json(response).await;
    assert_eq!(snapshot["communities"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["gateway"]["protocol_version"], 1);
    assert!(!snapshot["unread_counts"].is_null());

    // A ticket is single-use.
    let reused_ticket = ticket(&app, &owner_token).await;
    let _first = open_socket(addr, &reused_ticket).await;
    let (mut second, _) = connect_async(format!("ws://{addr}/gateway")).await.unwrap();
    let _ = next_json(&mut second).await;
    second
        .send(WsMessage::Text(
            json!({
                "op": 2,
                "d": { "ticket": reused_ticket, "protocol_version": 1 }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let close = tokio::time::timeout(Duration::from_secs(2), second.next()).await;
    assert!(close.is_ok());

    server.abort();
}
