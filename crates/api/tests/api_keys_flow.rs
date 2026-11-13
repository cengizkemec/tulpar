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

fn get_request(uri: &str, token: &str) -> axum::http::Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrations = "../database/migrations")]
async fn api_keys_are_scoped_and_revocable(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);
    let owner = register_and_login(&app, "apikeyowner", "apikeyowner@example.com").await;

    let created_community = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/communities",
            Some(&owner),
            json!({
                "name": "API Key Test",
                "slug": "api-key-test",
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
        .oneshot(get_request(
            &format!("/api/v1/communities/{community_id}/channels"),
            &owner,
        ))
        .await
        .unwrap();
    assert_eq!(channels.status(), StatusCode::OK);
    let channel_id = body_json(channels).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_key = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/users/me/api-keys",
            Some(&owner),
            json!({ "name": "CI bot", "scopes": ["read", "write"] }),
        ))
        .await
        .unwrap();
    assert_eq!(created_key.status(), StatusCode::OK);
    let created_json = body_json(created_key).await;
    let plaintext = created_json["api_key"].as_str().unwrap().to_string();
    let key_id = created_json["key"]["id"].as_str().unwrap().to_string();
    assert!(plaintext.starts_with("tk_"));

    let me = app
        .clone()
        .oneshot(get_request("/api/v1/users/me", &plaintext))
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);

    let key_list_with_key = app
        .clone()
        .oneshot(get_request("/api/v1/users/me/api-keys", &plaintext))
        .await
        .unwrap();
    assert_eq!(key_list_with_key.status(), StatusCode::FORBIDDEN);

    let send = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/channels/{channel_id}/messages"),
            Some(&plaintext),
            json!({ "content": "bot message" }),
        ))
        .await
        .unwrap();
    assert_eq!(send.status(), StatusCode::OK);

    let logout = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/logout",
            Some(&plaintext),
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::FORBIDDEN);

    let read_only_key = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/users/me/api-keys",
            Some(&owner),
            json!({ "name": "Read only", "scopes": ["read"] }),
        ))
        .await
        .unwrap();
    assert_eq!(read_only_key.status(), StatusCode::OK);
    let read_only = body_json(read_only_key).await["api_key"]
        .as_str()
        .unwrap()
        .to_string();
    let denied_send = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/channels/{channel_id}/messages"),
            Some(&read_only),
            json!({ "content": "read only" }),
        ))
        .await
        .unwrap();
    assert_eq!(denied_send.status(), StatusCode::FORBIDDEN);

    let revoked = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/users/me/api-keys/{key_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let after_revoke = app
        .clone()
        .oneshot(get_request("/api/v1/users/me", &plaintext))
        .await
        .unwrap();
    assert_eq!(after_revoke.status(), StatusCode::UNAUTHORIZED);
}
