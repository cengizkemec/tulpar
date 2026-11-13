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

fn with_auth(request: axum::http::request::Builder, token: &str) -> axum::http::request::Builder {
    request.header(header::AUTHORIZATION, format!("Bearer {token}"))
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
async fn embed_public_and_member_flow(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let owner = register_and_login(&app, "embedowner", "embedowner@example.com").await;
    let member = register_and_login(&app, "embedmember", "embedmember@example.com").await;

    let created = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/communities",
            Some(&owner),
            json!({
                "name": "Embed Test",
                "slug": "embed-test",
                "visibility": "public"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let community_id = body_json(created).await["community"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let channels = app
        .clone()
        .oneshot(
            with_auth(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/communities/{community_id}/channels")),
                &owner,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(channels.status(), StatusCode::OK);
    let channels_json = body_json(channels).await;
    let channel_id = channels_json
        .as_array()
        .unwrap()
        .iter()
        .find(|channel| channel["kind"] == "text")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let message = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/channels/{channel_id}/messages"),
            Some(&owner),
            json!({ "content": "embed preview" }),
        ))
        .await
        .unwrap();
    assert_eq!(message.status(), StatusCode::OK);

    let joined = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/communities/{community_id}/join"),
            Some(&member),
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(joined.status(), StatusCode::OK);

    let created_app = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/communities/{community_id}/embed-apps"),
            Some(&owner),
            json!({
                "name": "Demo embed",
                "allowed_origins": ["https://example.com"],
                "widgets": ["community", "join", "channel_preview"],
                "allow_authenticated": true,
                "allow_message_preview": true,
                "public_channel_id": channel_id
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created_app.status(), StatusCode::OK);
    let app_json = body_json(created_app).await;
    let public_key = app_json["public_key"].as_str().unwrap().to_string();
    let app_id = app_json["id"].as_str().unwrap().to_string();

    let public_token_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "community"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(public_token_response.status(), StatusCode::OK);
    let public_token = body_json(public_token_response).await["token"]
        .as_str()
        .unwrap()
        .to_string();

    let community_response = app
        .clone()
        .oneshot(
            with_auth(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/embed/community"),
                &public_token,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(community_response.status(), StatusCode::OK);
    let community_json = body_json(community_response).await;
    assert_eq!(community_json["name"], "Embed Test");
    assert_eq!(community_json["member_count"], 2);
    assert_eq!(community_json["authenticated"], false);
    assert_eq!(community_json["messages"][0]["content"], "embed preview");

    let denied_origin = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://evil.example",
                "widget": "community"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(denied_origin.status(), StatusCode::FORBIDDEN);

    let disabled_widget = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "members"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(disabled_widget.status(), StatusCode::FORBIDDEN);

    let authorized = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/authorize",
            Some(&member),
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "community"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    let code = body_json(authorized).await["code"]
        .as_str()
        .unwrap()
        .to_string();

    let member_token_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "community",
                "code": code
            }),
        ))
        .await
        .unwrap();
    assert_eq!(member_token_response.status(), StatusCode::OK);
    let member_token_json = body_json(member_token_response).await;
    assert_eq!(member_token_json["authenticated"], true);

    let reused_code = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "community",
                "code": code
            }),
        ))
        .await
        .unwrap();
    assert_eq!(reused_code.status(), StatusCode::UNAUTHORIZED);

    let revoked = app
        .clone()
        .oneshot(
            with_auth(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/embed-apps/{app_id}")),
                &owner,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let after_revoke = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "community"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(after_revoke.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../database/migrations")]
async fn private_community_public_embed_is_rejected(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);
    let owner = register_and_login(&app, "privateowner", "privateowner@example.com").await;

    let created = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/communities",
            Some(&owner),
            json!({
                "name": "Private Embed",
                "slug": "private-embed",
                "visibility": "private"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let community_id = body_json(created).await["community"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_app = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/communities/{community_id}/embed-apps"),
            Some(&owner),
            json!({
                "name": "Private embed",
                "allowed_origins": ["https://example.com"],
                "widgets": ["community"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created_app.status(), StatusCode::OK);
    let public_key = body_json(created_app).await["public_key"]
        .as_str()
        .unwrap()
        .to_string();

    let token_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/embed/token",
            None,
            json!({
                "public_key": public_key,
                "origin": "https://example.com",
                "widget": "community"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(token_response.status(), StatusCode::FORBIDDEN);
}
