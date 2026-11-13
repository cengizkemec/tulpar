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

#[sqlx::test(migrations = "../database/migrations")]
async fn admin_overview_users_and_audit(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let admin_token = register_and_login(&app, "adminuser", "adminuser@example.com").await;
    let _member_token = register_and_login(&app, "memberuser", "memberuser@example.com").await;

    let admin_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind("adminuser@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();
    let member_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind("memberuser@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();

    let forbidden = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/overview"),
        &admin_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(forbidden).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    sqlx::query("INSERT INTO instance_admins (user_id, granted_by) VALUES ($1, NULL)")
        .bind(admin_id)
        .execute(&pool)
        .await
        .unwrap();

    let overview = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/overview"),
        &admin_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(overview).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let overview = body_json(response).await;
    assert_eq!(overview["users"], 2);

    let users = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/users?query=memberuser"),
        &admin_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(users).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let users = body_json(response).await;
    assert_eq!(users.as_array().unwrap().len(), 1);
    assert_eq!(users[0]["id"], member_id.to_string());

    let disable = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/users/{member_id}"))
            .header(header::CONTENT_TYPE, "application/json"),
        &admin_token,
    )
    .body(Body::from(json!({ "disabled": true }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(disable).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!body_json(response).await["disabled_at"].is_null());

    let member_login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "login": "memberuser@example.com", "password": "correct horse battery staple" })
                .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(member_login).await.unwrap();
    assert!(response.status().is_client_error());

    let enable = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/users/{member_id}"))
            .header(header::CONTENT_TYPE, "application/json"),
        &admin_token,
    )
    .body(Body::from(json!({ "disabled": false }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(enable).await.unwrap().status(),
        StatusCode::OK
    );

    let self_disable = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/users/{admin_id}"))
            .header(header::CONTENT_TYPE, "application/json"),
        &admin_token,
    )
    .body(Body::from(json!({ "disabled": true }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(self_disable).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    // The member can log in again after being re-enabled.
    let member_login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "login": "memberuser@example.com", "password": "correct horse battery staple" })
                .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(member_login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let member_token = body_json(response).await["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let audit = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/audit-logs"),
        &admin_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(audit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let audit = body_json(response).await;
    assert!(audit
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| { entry["action"] == "admin.user_update" }));

    let member_admin = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/overview"),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(member_admin).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}
