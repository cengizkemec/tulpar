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

    let create_public = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Public Discovery",
            "slug": "public-discovery",
            "visibility": "public"
        })
        .to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(create_public).await.unwrap().status(),
        StatusCode::OK
    );

    let discovery = Request::builder()
        .method("GET")
        .uri("/api/v1/communities?query=discovery")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(discovery).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let results = body_json(response).await;
    assert!(results
        .as_array()
        .unwrap()
        .iter()
        .any(|community| community["slug"] == "public-discovery"));
    assert!(results
        .as_array()
        .unwrap()
        .iter()
        .all(|community| community["slug"] != "test-community"));

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

    let member_search = with_auth(
        Request::builder().method("GET").uri(format!(
            "/api/v1/communities/{community_id}/members?query=memberuser"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(member_search).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let found = body_json(response).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["username"], "memberuser");

    let user_search = with_auth(
        Request::builder()
            .method("GET")
            .uri("/api/v1/users?query=memberuser"),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(user_search).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let users = body_json(response).await;
    assert_eq!(users.as_array().unwrap().len(), 1);
    assert_eq!(users[0]["username"], "memberuser");

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

    let reorder = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/communities/{community_id}/roles/reorder"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "roles": [{ "role_id": role_id, "position": 5 }] }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(reorder).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let roles = body_json(response).await;
    assert!(roles
        .as_array()
        .unwrap()
        .iter()
        .any(|role| role["id"] == role_id && role["position"] == 5));

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
        app.clone().oneshot(blocked_dm).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let ban = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/communities/{community_id}/bans/{member_id}"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "reason": "integration test" }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(ban).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let bans = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/bans")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(bans).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bans = body_json(response).await;
    assert_eq!(bans.as_array().unwrap().len(), 1);
    assert_eq!(bans[0]["user_id"], member_id);
    assert_eq!(bans[0]["reason"], "integration test");

    let unban = with_auth(
        Request::builder().method("DELETE").uri(format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(unban).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let audit = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/audit-logs")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(audit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let audit = body_json(response).await;
    let actions = audit
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect::<Vec<_>>();
    for expected in [
        "community.create",
        "member.join",
        "role.create",
        "role.assign",
        "role.reorder",
        "channel.overwrite.set",
        "channel.overwrite.delete",
        "member.ban",
        "member.unban",
    ] {
        assert!(
            actions.contains(&expected),
            "missing audit action: {expected}"
        );
    }

    let member_audit = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/audit-logs")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.oneshot(member_audit).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../database/migrations")]
async fn community_authorization_matrix(pool: PgPool) {
    let state = AppState::new(
        Arc::new(test_config()),
        pool,
        None,
        Arc::new(LogEmailSender),
    )
    .expect("test state");
    let app = router(state);

    let owner_token = register_and_login(&app, "matrixowner", "matrixowner@example.com").await;
    let member_token = register_and_login(&app, "matrixmember", "matrixmember@example.com").await;
    let outsider_token =
        register_and_login(&app, "matrixoutsider", "matrixoutsider@example.com").await;

    let create_community = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Matrix Community",
            "slug": "matrix-community",
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

    let outsider_view = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}")),
        &outsider_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(outsider_view).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let invite_a = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/invites"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "max_uses": 1 }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(invite_a).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let invite_a_code = body_json(response).await["code"]
        .as_str()
        .unwrap()
        .to_string();

    let join_a = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/invites/{invite_a_code}")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(join_a).await.unwrap().status(),
        StatusCode::OK
    );

    let exhausted = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/invites/{invite_a_code}")),
        &outsider_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(exhausted).await.unwrap().status(),
        StatusCode::GONE
    );

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

    // A non-owner with management permissions still cannot grant administrator.
    let moderator_role = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/roles"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Matrix Moderator",
            "position": 10,
            "permissions": ["VIEW_CHANNEL", "MANAGE_ROLES", "KICK_MEMBERS"]
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(moderator_role).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let moderator_role_id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let assign = with_auth(
        Request::builder().method("PUT").uri(format!(
            "/api/v1/communities/{community_id}/members/{member_id}/roles/{moderator_role_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(assign).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let escalation = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/roles"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(
        json!({
            "name": "Escalated",
            "position": 1,
            "permissions": ["ADMINISTRATOR"]
        })
        .to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(escalation).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let update_community = with_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/communities/{community_id}"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "name": "Hijacked" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(update_community)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let create_channel = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/channels"))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(
        json!({ "kind": "text", "name": "hijacked" }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(create_channel).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let kick_owner = with_auth(
        Request::builder().method("DELETE").uri(format!(
            "/api/v1/communities/{community_id}/members/{}",
            community["community"]["owner_user_id"].as_str().unwrap()
        )),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(kick_owner).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let overwrite_owner = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/channels/{general_channel_id}/permissions/users/{}",
                community["community"]["owner_user_id"].as_str().unwrap()
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(
        json!({ "allow": [], "deny": ["VIEW_CHANNEL"] }).to_string(),
    ))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(overwrite_owner).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    // Cross-community role IDs must not be assignable.
    let second_community = with_auth(
        Request::builder()
            .method("POST")
            .uri("/api/v1/communities")
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({
            "name": "Other Community",
            "slug": "other-community",
            "visibility": "private"
        })
        .to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(second_community).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second_community_id = body_json(response).await["community"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let foreign_role = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{second_community_id}/roles"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(
        json!({ "name": "Foreign", "permissions": ["VIEW_CHANNEL"] }).to_string(),
    ))
    .unwrap();
    let response = app.clone().oneshot(foreign_role).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let foreign_role_id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let cross_assign = with_auth(
        Request::builder().method("PUT").uri(format!(
            "/api/v1/communities/{community_id}/members/{member_id}/roles/{foreign_role_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(cross_assign).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let owner_id = community["community"]["owner_user_id"]
        .as_str()
        .unwrap()
        .to_string();

    let non_owner_transfer = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/communities/{community_id}/transfer-ownership"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "new_owner_id": member_id }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(non_owner_transfer)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let transfer = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/communities/{community_id}/transfer-ownership"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "new_owner_id": member_id }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(transfer).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let member_permissions = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/permissions/me")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(member_permissions).await.unwrap();
    assert_eq!(body_json(response).await["is_owner"], true);

    let old_owner_delete = with_auth(
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/communities/{community_id}")),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(old_owner_delete)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let transfer_back = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/communities/{community_id}/transfer-ownership"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &member_token,
    )
    .body(Body::from(json!({ "new_owner_id": owner_id }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(transfer_back).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    // Timeout removes send/voice permissions but preserves read access.
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

    let permissions = with_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/communities/{community_id}/permissions/me")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(permissions).await.unwrap();
    let names = body_json(response).await["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert!(names.contains(&"VIEW_CHANNEL".to_string()));
    assert!(!names.contains(&"SEND_MESSAGES".to_string()));

    // Ban revokes membership and blocks rejoin until unbanned.
    let invite_b = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/communities/{community_id}/invites"))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "max_uses": 2 }).to_string()))
    .unwrap();
    let response = app.clone().oneshot(invite_b).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let invite_b_code = body_json(response).await["code"]
        .as_str()
        .unwrap()
        .to_string();

    let ban = with_auth(
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/v1/communities/{community_id}/bans/{member_id}"
            ))
            .header(header::CONTENT_TYPE, "application/json"),
        &owner_token,
    )
    .body(Body::from(json!({ "reason": "matrix ban" }).to_string()))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(ban).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let blocked_rejoin = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/invites/{invite_b_code}")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(blocked_rejoin).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let unban = with_auth(
        Request::builder().method("DELETE").uri(format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        )),
        &owner_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(unban).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let rejoin = with_auth(
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/invites/{invite_b_code}")),
        &member_token,
    )
    .body(Body::empty())
    .unwrap();
    assert_eq!(
        app.clone().oneshot(rejoin).await.unwrap().status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../database/migrations")]
async fn phase2_schema_constraints(pool: PgPool) {
    let owner_token = {
        let app = router(
            AppState::new(
                Arc::new(test_config()),
                pool.clone(),
                None,
                Arc::new(LogEmailSender),
            )
            .expect("test state"),
        );
        register_and_login(&app, "constraintowner", "constraintowner@example.com").await
    };
    assert!(!owner_token.is_empty());

    let owner_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind("constraintowner@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();

    let invalid_visibility = sqlx::query(
        "INSERT INTO communities (id, name, slug, owner_user_id, visibility) VALUES (gen_random_uuid(), 'Bad', 'bad-visibility', $1, 'invalid')",
    )
    .bind(owner_id)
    .execute(&pool)
    .await;
    assert!(invalid_visibility.is_err());

    sqlx::query(
        "INSERT INTO communities (id, name, slug, owner_user_id, visibility) VALUES (gen_random_uuid(), 'Good', 'constraint-community', $1, 'private')",
    )
    .bind(owner_id)
    .execute(&pool)
    .await
    .unwrap();

    let duplicate_slug = sqlx::query(
        "INSERT INTO communities (id, name, slug, owner_user_id, visibility) VALUES (gen_random_uuid(), 'Duplicate', 'constraint-community', $1, 'private')",
    )
    .bind(owner_id)
    .execute(&pool)
    .await;
    assert!(duplicate_slug.is_err());

    let community_id: Uuid =
        sqlx::query_scalar("SELECT id FROM communities WHERE slug = 'constraint-community'")
            .fetch_one(&pool)
            .await
            .unwrap();

    let bad_direct_channel = sqlx::query(
        "INSERT INTO channels (id, scope, community_id, kind, name) VALUES (gen_random_uuid(), 'direct', $1, 'text', 'bad')",
    )
    .bind(community_id)
    .execute(&pool)
    .await;
    assert!(bad_direct_channel.is_err());

    let role_id: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (id, community_id, name, color, position, permissions) VALUES (gen_random_uuid(), $1, 'Constraint Role', 0, 1, 0) RETURNING id",
    )
    .bind(community_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let bad_color = sqlx::query(
        "INSERT INTO roles (id, community_id, name, color) VALUES (gen_random_uuid(), $1, 'Bad Color', 16777216)",
    )
    .bind(community_id)
    .execute(&pool)
    .await;
    assert!(bad_color.is_err());

    let channel_id: Uuid = sqlx::query_scalar(
        "INSERT INTO channels (id, scope, community_id, kind, name) VALUES (gen_random_uuid(), 'community', $1, 'text', 'constraint-channel') RETURNING id",
    )
    .bind(community_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let overlapping_overwrite = sqlx::query(
        "INSERT INTO channel_permission_overwrites (id, channel_id, role_id, allow, deny) VALUES (gen_random_uuid(), $1, $2, 1, 1)",
    )
    .bind(channel_id)
    .bind(role_id)
    .execute(&pool)
    .await;
    assert!(overlapping_overwrite.is_err());

    let missing_overwrite_target = sqlx::query(
        "INSERT INTO channel_permission_overwrites (id, channel_id, allow, deny) VALUES (gen_random_uuid(), $1, 0, 0)",
    )
    .bind(channel_id)
    .execute(&pool)
    .await;
    assert!(missing_overwrite_target.is_err());

    let self_direct = sqlx::query(
        "INSERT INTO direct_conversations (channel_id, user_a_id, user_b_id, created_by) VALUES (gen_random_uuid(), $1, $1, $1)",
    )
    .bind(owner_id)
    .execute(&pool)
    .await;
    assert!(self_direct.is_err());

    let self_block =
        sqlx::query("INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $1)")
            .bind(owner_id)
            .execute(&pool)
            .await;
    assert!(self_block.is_err());
}
