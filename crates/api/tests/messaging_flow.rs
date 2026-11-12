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
            "name": "Messaging Community",
            "slug": "messaging-community",
            "visibility": "private"
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

#[sqlx::test(migrations = "../database/migrations")]
async fn messaging_flow(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool.clone(),
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let owner_token = register_and_login(&app, "msgowner", "msgowner@example.com").await;
    let member_token = register_and_login(&app, "msgmember", "msgmember@example.com").await;
    let outsider_token = register_and_login(&app, "msgoutsider", "msgoutsider@example.com").await;

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
    let outsider_id = {
        let request = with_auth(
            Request::builder().method("GET").uri("/api/v1/users/me"),
            &outsider_token,
        )
        .body(Body::empty())
        .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        body_json(response).await["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let nonce = Uuid::now_v7();
    let send = |content: &str, nonce: Option<Uuid>| {
        let body = json!({ "content": content, "client_nonce": nonce });
        with_auth(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/channels/{channel_id}/messages"))
                .header(header::CONTENT_TYPE, "application/json"),
            &member_token,
        )
        .body(Body::from(body.to_string()))
        .unwrap()
    };

    let response = app
        .clone()
        .oneshot(send("hello from the member", Some(nonce)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first_message = body_json(response).await;
    let first_id = first_message["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(send("hello from the member", Some(nonce)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["id"], first_id);

    let list = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{channel_id}/messages")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await.as_array().unwrap().len(), 1);

    let reply = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "content": "owner reply", "reply_to_id": first_id }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(reply).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let reply_id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let edit = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/messages/{first_id}"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "content": "edited hello" }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(edit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let edited = body_json(response).await;
    assert_eq!(edited["content"], "edited hello");
    assert!(!edited["edited_at"].is_null());

    let foreign_edit = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/messages/{reply_id}"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "content": "hijacked" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(foreign_edit).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let react = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/messages/{first_id}/reactions/heart")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(react).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let reactions = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/messages/{first_id}/reactions")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(reactions).await.unwrap();
    let reactions = body_json(response).await;
    assert_eq!(reactions.as_array().unwrap().len(), 1);
    assert_eq!(reactions[0]["emoji"], "heart");

    let unreact = with_auth(
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/messages/{first_id}/reactions/heart")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(unreact).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let pin = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/channels/{channel_id}/pins/{first_id}")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(pin).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let pins = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{channel_id}/pins")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(pins).await.unwrap();
    let pins = body_json(response).await;
    assert_eq!(pins.as_array().unwrap().len(), 1);
    assert_eq!(pins[0]["id"], first_id);

    let read_cursor = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/channels/{channel_id}/read-state"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "message_id": reply_id }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(read_cursor).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["unread_count"], 0);

    let second_send = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "content": "unique searchable needle" }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(second_send).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second_id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let catch_up = with_auth(
        Request::builder().method("GET").uri(format!(
            "/api/v1/channels/{channel_id}/messages?after={first_id}"
        )),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(catch_up).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let caught_up = body_json(response).await;
    assert_eq!(caught_up.as_array().unwrap().len(), 2);
    assert_eq!(caught_up[0]["id"], second_id);

    let unread = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{channel_id}/read-state")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(unread).await.unwrap();
    assert_eq!(body_json(response).await["unread_count"], 1);

    let search = with_auth(
        Request::builder().method("GET").uri(format!(
            "/api/v1/channels/{channel_id}/messages/search?query=needle"
        )),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(search).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let results = body_json(response).await;
    assert_eq!(results.as_array().unwrap().len(), 1);
    assert_eq!(results[0]["id"], second_id);

    let outsider_read = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{channel_id}/messages")),
        &outsider_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(outsider_read).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    // Slowmode applies to members without bypass.
    sqlx::query("UPDATE channels SET slowmode_seconds = 60 WHERE id = $1")
        .bind(channel_id.parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let response = app.clone().oneshot(send("slow down", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    // Timeout removes SEND_MESSAGES.
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
    let timed_out_send = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "content": "blocked" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(timed_out_send).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    // Moderator deletion produces a tombstone and removes the message from lists.
    let delete = with_auth(
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/messages/{first_id}")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(delete).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let tombstone = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/messages/{first_id}")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(tombstone).await.unwrap();
    let tombstone = body_json(response).await;
    assert!(tombstone["content"].is_null());
    assert!(!tombstone["deleted_at"].is_null());

    let list = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{channel_id}/messages")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    let messages = body_json(response).await;
    assert!(messages
        .as_array()
        .unwrap()
        .iter()
        .all(|message| message["id"] != first_id));

    // DM messages are participant-only.
    let friend_request = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/friends/requests")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "user_id": member_id }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(friend_request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let requests = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/friends/requests"),
        &member_token,
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
        &member_token,
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
    let dm_channel_id = body_json(response).await["channel_id"]
        .as_str()
        .unwrap()
        .to_string();

    let dm_send = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/channels/{dm_channel_id}/messages"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "content": "private hello" }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(dm_send).await.unwrap().status(),
        StatusCode::OK
    );

    let dm_read = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{dm_channel_id}/messages")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(dm_read).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await.as_array().unwrap().len(), 1);

    let outsider_dm = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/channels/{dm_channel_id}/messages")),
        &outsider_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(outsider_dm).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let _ = outsider_id;
}
