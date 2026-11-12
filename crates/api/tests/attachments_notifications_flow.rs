use std::sync::Arc;

use auth::LogEmailSender;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use tulpar_api::{router, AppState};
use uuid::Uuid;

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
        .join(format!("tulpar-attachments-test-{}", Uuid::now_v7()))
        .display()
        .to_string();
    config.storage.max_upload_bytes = 1024;
    config
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
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

fn multipart_body(filename: &str, content_type: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "----tulpar-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[sqlx::test(migrations = "../database/migrations")]
async fn attachments_mentions_notifications_and_outbox(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state.clone());

    let owner_token = register_and_login(&app, "attachowner", "attachowner@example.com").await;
    let member_token = register_and_login(&app, "attachmember", "attachmember@example.com").await;
    let outsider_token =
        register_and_login(&app, "attachoutsider", "attachoutsider@example.com").await;

    let create = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Attachment Community",
            "slug": "attachment-community",
            "visibility": "private"
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let community = body_json(response).await;
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

    let (content_type, body) = multipart_body("note.txt", "text/plain", b"hello attachment");
    let upload = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/attachments"))
            .header(header::CONTENT_TYPE, content_type),
        &owner_token,
    )
    .body(Body::from(body))
    .unwrap();
    let response = app.clone().oneshot(upload).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let file = body_json(response).await;
    let file_id = file["id"].as_str().unwrap().to_string();
    assert_eq!(file["size_bytes"], 16);

    let send = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "attachment_ids": [file_id] }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(send).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let message = body_json(response).await;
    let message_id = message["id"].as_str().unwrap().to_string();
    assert!(message["content"].is_null());
    assert_eq!(message["attachments"].as_array().unwrap().len(), 1);
    let attachment_id = message["attachments"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let download = with_auth(
        Request::builder().method("GET").uri(format!(
            "/api/v1/messages/{message_id}/attachments/{attachment_id}"
        )),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(download).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "text/plain"
    );
    assert_eq!(body_bytes(response).await, b"hello attachment");

    let outsider_download = with_auth(
        Request::builder().method("GET").uri(format!(
            "/api/v1/messages/{message_id}/attachments/{attachment_id}"
        )),
        &outsider_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(outsider_download)
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let (content_type, body) = multipart_body("page.html", "text/html", b"<script></script>");
    let bad_upload = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/attachments"))
            .header(header::CONTENT_TYPE, content_type),
        &owner_token,
    )
    .body(Body::from(body))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(bad_upload).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    let (content_type, body) = multipart_body("large.txt", "text/plain", &vec![b'x'; 2048]);
    let large_upload = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/attachments"))
            .header(header::CONTENT_TYPE, content_type),
        &owner_token,
    )
    .body(Body::from(body))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(large_upload).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    let mention = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "content": format!("<@{member_id}> please check") }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(mention).await.unwrap().status(),
        StatusCode::OK
    );

    let notifications = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/notifications"),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(notifications).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let notifications = body_json(response).await;
    assert_eq!(notifications.as_array().unwrap().len(), 1);
    assert_eq!(notifications[0]["kind"], "mention");
    let notification_id = notifications[0]["id"].as_str().unwrap().to_string();

    let processed = tulpar_api::workers::dispatch_outbox_once(&state)
        .await
        .expect("outbox dispatch");
    assert!(processed >= 1);
    assert_eq!(state.outbox.pending_count().await.unwrap(), 0);

    let retry_event = state
        .outbox
        .enqueue("test.retry", "test", None, None, json!({ "probe": true }))
        .await
        .unwrap();
    let retry_row: i64 = sqlx::query_scalar("SELECT id FROM outbox_events WHERE event_id = $1")
        .bind(retry_event)
        .fetch_one(&pool)
        .await
        .unwrap();
    state
        .outbox
        .mark_failed(retry_row, "temporary", 60)
        .await
        .unwrap();
    assert!(state.outbox.pending(10).await.unwrap().is_empty());
    state.outbox.mark_processed(retry_row).await.unwrap();
    assert_eq!(state.outbox.pending_count().await.unwrap(), 0);

    let preferences = with_auth(
        Request::builder()
            .method("PUT")
            .uri("/api/v1/users/me/notification-preferences")
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(
        json!({ "desktop_enabled": false, "mentions_only": true }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(preferences).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let preferences = body_json(response).await;
    assert_eq!(preferences["desktop_enabled"], false);
    assert_eq!(preferences["mentions_only"], true);

    let mark_read = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/read")
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "ids": [notification_id] }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(mark_read).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let list = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/notifications"),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    let notifications = body_json(response).await;
    assert!(!notifications[0]["read_at"].is_null());

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
    assert!(!snapshot["unread_counts"].as_object().unwrap().is_empty());
}
