use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use auth::{EmailError, EmailSender, LogEmailSender};
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
    config.auth.registration_enabled = true;
    config.email.driver = "log".to_string();
    config
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn refresh_cookie(response: &axum::response::Response) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let cookie = value.split(';').next()?.trim();
            cookie
                .starts_with("tulpar_refresh=")
                .then(|| cookie.to_string())
        })
}

#[sqlx::test(migrations = "../database/migrations")]
async fn register_login_refresh_and_reuse_detection(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool,
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let register = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": "ada",
                "display_name": "Ada",
                "email": "ada@example.com",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "login": "ada@example.com",
                "password": "correct horse battery staple",
                "device_name": "Test browser"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = refresh_cookie(&response).expect("refresh cookie");
    let login_body = body_json(response).await;
    let access_token = login_body["access_token"].as_str().expect("access token");

    let me = Request::builder()
        .method("GET")
        .uri("/api/v1/users/me")
        .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(me).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let export = Request::builder()
        .method("GET")
        .uri("/api/v1/users/me/export")
        .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(export).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["user"]["username"], "ada");

    let refresh = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &cookie)
        .body(Body::from("{}"))
        .unwrap();
    let response = app.clone().oneshot(refresh).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed = body_json(response).await;
    assert!(refreshed["access_token"].as_str().is_some());

    let reuse = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &cookie)
        .body(Body::from("{}"))
        .unwrap();
    let response = app.oneshot(reuse).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../database/migrations")]
async fn invalid_login_is_generic(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool,
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "login": "nobody@example.com",
                "password": "wrong password"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["code"], "invalid_credentials");
}

#[sqlx::test(migrations = "../database/migrations")]
async fn account_deletion_anonymizes_and_revokes_access(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let register = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": "deleteme",
                "display_name": "Delete Me",
                "email": "deleteme@example.com",
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
            json!({
                "login": "deleteme@example.com",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let access = body_json(response).await["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let delete = Request::builder()
        .method("DELETE")
        .uri("/api/v1/users/me")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .body(Body::from(
            json!({
                "password": "correct horse battery staple",
                "confirmation": "DELETE"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(delete).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let old_access = Request::builder()
        .method("GET")
        .uri("/api/v1/users/me")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(old_access).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let deleted: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT display_name, deleted_at FROM users WHERE email LIKE 'deleted+%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(deleted.0, "Deleted user");
    assert!(deleted.1.is_some());
}

#[derive(Default, Clone)]
struct CaptureEmail {
    verification: Arc<Mutex<Vec<String>>>,
    reset: Arc<Mutex<Vec<String>>>,
}

impl CaptureEmail {
    fn last_verification(&self) -> String {
        self.verification.lock().unwrap().last().cloned().unwrap()
    }

    fn last_reset(&self) -> String {
        self.reset.lock().unwrap().last().cloned().unwrap()
    }
}

#[async_trait]
impl EmailSender for CaptureEmail {
    async fn send_verification(&self, _to: &str, link: &str) -> Result<(), EmailError> {
        self.verification.lock().unwrap().push(link.to_string());
        Ok(())
    }

    async fn send_password_reset(&self, _to: &str, link: &str) -> Result<(), EmailError> {
        self.reset.lock().unwrap().push(link.to_string());
        Ok(())
    }
}

fn fragment_token(link: &str) -> String {
    link.split("#token=")
        .nth(1)
        .expect("token fragment")
        .to_string()
}

#[sqlx::test(migrations = "../database/migrations")]
async fn email_verification_and_password_reset(pool: PgPool) {
    let mut config = test_config();
    config.auth.require_email_verification = true;
    let email = Arc::new(CaptureEmail::default());
    let state = AppState::new(Arc::new(config), pool, None, email.clone()).expect("test state");
    let app = router(state);

    let register = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": "grace",
                "display_name": "Grace",
                "email": "grace@example.com",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_token = fragment_token(&email.last_verification());

    let blocked_login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "login": "grace@example.com",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(blocked_login).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let verify = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/verify-email")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "token": verify_token }).to_string()))
        .unwrap();
    let response = app.clone().oneshot(verify).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "login": "grace@example.com",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let forgot = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/password/forgot")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "login": "grace@example.com" }).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(forgot).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let reset_token = fragment_token(&email.last_reset());
    let reset = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/password/reset")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "token": reset_token,
                "new_password": "a new correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(reset).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let old_login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "login": "grace@example.com",
                "password": "correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(old_login).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let new_login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "login": "grace@example.com",
                "password": "a new correct horse battery staple"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(new_login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
