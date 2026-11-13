use std::sync::Arc;

use auth::LogEmailSender;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::ServiceExt;
use tulpar_api::{router, AppState};

fn test_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.instance.environment = "test".to_string();
    config.instance.public_url = "http://localhost:8081".to_string();
    config.database.url = "postgres://unused".to_string();
    config.auth.jwt_secret = "x".repeat(32);
    config.auth.require_email_verification = false;
    config.auth.cookie_secure = false;
    config.email.driver = "log".to_string();
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

fn json_request(
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> axum::http::Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

#[sqlx::test(migrations = "../database/migrations")]
async fn webhooks_are_signed_delivered_and_scoped(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state.clone());
    let owner = register_and_login(&app, "webhookowner", "webhookowner@example.com").await;

    let created_community = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/communities",
            Some(&owner),
            json!({
                "name": "Webhook Test",
                "slug": "webhook-test",
                "visibility": "public"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created_community.status(), StatusCode::OK);
    let community_id = body_json(created_community).await["community"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let channels = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/communities/{community_id}/channels"))
                .header(header::AUTHORIZATION, format!("Bearer {owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(channels.status(), StatusCode::OK);
    let channel_id = body_json(channels).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let receiver = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 16 * 1024];
        let read = socket.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        request
    });

    let created_webhook = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/communities/{community_id}/webhooks"),
            Some(&owner),
            json!({
                "url": format!("http://127.0.0.1:{port}/hook"),
                "events": ["message.created"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created_webhook.status(), StatusCode::OK);
    let webhook_json = body_json(created_webhook).await;
    let webhook_id = webhook_json["webhook"]["id"].as_str().unwrap().to_string();
    assert!(webhook_json["secret"]
        .as_str()
        .unwrap()
        .starts_with("whsec_"));

    let listed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/communities/{community_id}/webhooks"))
                .header(header::AUTHORIZATION, format!("Bearer {owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed_json = body_json(listed).await;
    assert_eq!(listed_json.as_array().unwrap().len(), 1);
    assert!(listed_json[0].get("secret").is_none());

    let sent = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/channels/{channel_id}/messages"),
            Some(&owner),
            json!({ "content": "webhook message" }),
        ))
        .await
        .unwrap();
    assert_eq!(sent.status(), StatusCode::OK);

    let delivered = tulpar_api::workers::dispatch_webhooks_once(&state).await;
    assert_eq!(delivered, 1);
    let request = receiver.await.unwrap();
    assert!(request.contains("x-tulpar-signature: sha256="));
    assert!(request.contains("x-tulpar-event: message.created"));

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/webhooks/{webhook_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let private_url = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/communities/{community_id}/webhooks"),
            Some(&owner),
            json!({
                "url": "http://10.0.0.1/hook",
                "events": ["message.created"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(private_url.status(), StatusCode::BAD_REQUEST);
}
