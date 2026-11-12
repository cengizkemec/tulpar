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
            json!({
                "login": email,
                "password": "correct horse battery staple"
            })
            .to_string(),
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
async fn community_invite_role_friend_and_dm_flow(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool,
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let owner_token = register_and_login(&app, "owneruser", "owner@example.com").await;
    let member_token = register_and_login(&app, "memberuser", "member@example.com").await;

    let create_community = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Test Community",
            "slug": "test-community",
            "visibility": "private"
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(create_community).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let community = body_json(response).await;
    let community_id = community["community"]["id"].as_str().unwrap().to_string();
    let general_channel_id = community["channels"].as_array().unwrap()[1]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(community["channels"].as_array().unwrap().len(), 2);
    assert!(!community["roles"].as_array().unwrap().is_empty());

    let hidden = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(hidden).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let create_invite = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/invites"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "max_uses": 5 }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(create_invite).await.unwrap();
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
    let response = app.clone().oneshot(join).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["already_member"], false);

    let members = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/members")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(members).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await.as_array().unwrap().len(), 2);

    let member_id = {
        let me = with_auth(
            Request::builder().method("GET").uri("/api/v1/users/me"),
            &member_token,
        )
        .body(Body::empty())
        .unwrap();
        let response = app.clone().oneshot(me).await.unwrap();
        body_json(response).await["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let deny_view = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{general_channel_id}/permissions/users/{member_id}"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "allow": [], "deny": ["VIEW_CHANNEL"] }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(deny_view).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let member_channels = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/channels")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(member_channels).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_json(response)
        .await
        .as_array()
        .unwrap()
        .iter()
        .all(|channel| channel["id"] != general_channel_id));

    let hidden_channel = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{general_channel_id}")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(hidden_channel).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let remove_overwrite = with_auth(
        Request::builder().method("DELETE").uri(format!(
            "/api/v1/channels/{general_channel_id}/permissions/users/{member_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(remove_overwrite)
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );

    let create_role = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/roles"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Helper",
            "permissions": ["VIEW_CHANNEL", "SEND_MESSAGES"]
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(create_role).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let role_id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let assign = with_auth(
        Request::builder().method("PUT").uri(format!(
            "/api/v1/communities/{community_id}/members/{member_id}/roles/{role_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(assign).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let permissions = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/permissions/me")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(permissions).await.unwrap();
    let permission_names = body_json(response).await["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert!(permission_names.contains(&"VIEW_CHANNEL".to_string()));
    assert!(permission_names.contains(&"SEND_MESSAGES".to_string()));

    let friend_request = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/friends/requests")
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(
        json!({ "user_id": community["community"]["owner_user_id"] }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(friend_request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let requests = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/friends/requests"),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(requests).await.unwrap();
    let requests = body_json(response).await;
    let request_id = requests.as_array().unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let accept = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/friends/requests/{request_id}/accept")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(accept).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let dm = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/dms")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "recipient_id": member_id }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(dm).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let block = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/blocks/{member_id}")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(block).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let blocked_dm = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/dms")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "recipient_id": member_id }).to_string()))
    .unwrap();
    assert_eq!(
        app.oneshot(blocked_dm).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}
