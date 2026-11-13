//! Axum REST API, DTOs, middleware, and OpenAPI.

#![forbid(unsafe_code)]

pub mod dto;
pub mod error;
pub mod extract;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod webhooks;
pub mod workers;

use std::time::Duration;

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

pub use openapi::ApiDoc;
pub use state::AppState;

/// Build the complete API router.
pub fn router(state: AppState) -> Router {
    let auth = Router::new()
        .route("/register", post(routes::auth::register))
        .route("/login", post(routes::auth::login))
        .route("/refresh", post(routes::auth::refresh))
        .route("/logout", post(routes::auth::logout))
        .route("/logout-all", post(routes::auth::logout_all))
        .route(
            "/sessions",
            get(routes::auth::list_sessions).delete(routes::auth::revoke_other_sessions),
        )
        .route(
            "/sessions/{session_id}",
            delete(routes::auth::revoke_session),
        )
        .route("/verify-email", post(routes::auth::verify_email))
        .route(
            "/resend-verification",
            post(routes::auth::resend_verification),
        )
        .route("/password/forgot", post(routes::auth::forgot_password))
        .route("/password/reset", post(routes::auth::reset_password))
        .route("/password/change", post(routes::auth::change_password))
        .route("/reauth", post(routes::auth::reauth))
        .route("/device/code", post(routes::auth::device_code))
        .route("/device/approve", post(routes::auth::approve_device))
        .route("/device/deny", post(routes::auth::deny_device))
        .route("/device/token", post(routes::auth::device_token));

    let users = Router::new()
        .route("/", get(routes::users::search_users))
        .route(
            "/me",
            get(routes::users::get_me)
                .patch(routes::users::update_me)
                .delete(routes::users::delete_me),
        )
        .route(
            "/me/settings",
            get(routes::users::get_settings).patch(routes::users::update_settings),
        )
        .route("/me/export", get(routes::users::export_me))
        .route("/me/communities", get(routes::communities::list_mine))
        .route(
            "/me/notification-preferences",
            get(routes::notifications::get_preferences)
                .put(routes::notifications::update_preferences),
        )
        .route(
            "/me/api-keys",
            get(routes::api_keys::list_keys).post(routes::api_keys::create_key),
        )
        .route(
            "/me/api-keys/{key_id}",
            delete(routes::api_keys::revoke_key),
        )
        .route("/{user_id}", get(routes::users::get_user));

    let communities = Router::new()
        .route(
            "/",
            get(routes::communities::list_public).post(routes::communities::create_community),
        )
        .route(
            "/{community_id}",
            get(routes::communities::get_community)
                .patch(routes::communities::update_community)
                .delete(routes::communities::delete_community),
        )
        .route(
            "/{community_id}/join",
            post(routes::communities::join_public),
        )
        .route(
            "/{community_id}/transfer-ownership",
            post(routes::communities::transfer_ownership),
        )
        .route(
            "/{community_id}/permissions/me",
            get(routes::communities::my_permissions),
        )
        .route(
            "/{community_id}/members",
            get(routes::communities::list_members),
        )
        .route(
            "/{community_id}/members/{user_id}",
            axum::routing::patch(routes::communities::update_member)
                .delete(routes::communities::kick_member),
        )
        .route("/{community_id}/bans", get(routes::communities::list_bans))
        .route(
            "/{community_id}/bans/{user_id}",
            put(routes::communities::ban_member).delete(routes::communities::unban_member),
        )
        .route(
            "/{community_id}/audit-logs",
            get(routes::communities::list_audit_logs),
        )
        .route(
            "/{community_id}/roles",
            get(routes::communities::list_roles).post(routes::communities::create_role),
        )
        .route(
            "/{community_id}/roles/reorder",
            axum::routing::patch(routes::communities::reorder_roles),
        )
        .route(
            "/{community_id}/roles/{role_id}",
            axum::routing::patch(routes::communities::update_role)
                .delete(routes::communities::delete_role),
        )
        .route(
            "/{community_id}/members/{user_id}/roles/{role_id}",
            put(routes::communities::assign_role).delete(routes::communities::remove_role),
        )
        .route(
            "/{community_id}/channels",
            get(routes::communities::list_channels).post(routes::communities::create_channel),
        )
        .route(
            "/{community_id}/invites",
            get(routes::communities::list_invites).post(routes::communities::create_invite),
        )
        .route(
            "/{community_id}/embed-apps",
            get(routes::embed::list_apps).post(routes::embed::create_app),
        )
        .route(
            "/{community_id}/webhooks",
            get(routes::webhooks::list_webhooks).post(routes::webhooks::create_webhook),
        );

    let invites = Router::new().route(
        "/{code}",
        get(routes::communities::preview_invite)
            .post(routes::communities::join_invite)
            .delete(routes::communities::revoke_invite),
    );

    let channels = Router::new()
        .route("/{channel_id}", get(routes::channels::get_channel))
        .route(
            "/{channel_id}/permissions/me",
            get(routes::channels::my_permissions),
        )
        .route(
            "/{channel_id}/permissions",
            get(routes::channels::list_overwrites),
        )
        .route(
            "/{channel_id}/permissions/roles/{role_id}",
            put(routes::channels::set_role_overwrite)
                .delete(routes::channels::delete_role_overwrite),
        )
        .route(
            "/{channel_id}/permissions/users/{user_id}",
            put(routes::channels::set_user_overwrite)
                .delete(routes::channels::delete_user_overwrite),
        )
        .route(
            "/{channel_id}/messages",
            get(routes::messages::list_messages).post(routes::messages::send_message),
        )
        .route(
            "/{channel_id}/messages/search",
            get(routes::messages::search_messages),
        )
        .route(
            "/{channel_id}/read-state",
            get(routes::messages::get_read_state).put(routes::messages::set_read_state),
        )
        .route("/{channel_id}/pins", get(routes::messages::list_pins))
        .route(
            "/{channel_id}/pins/{message_id}",
            put(routes::messages::pin_message).delete(routes::messages::unpin_message),
        )
        .route(
            "/{channel_id}/attachments",
            post(routes::attachments::upload_attachment),
        )
        .route(
            "/{channel_id}/voice/token",
            post(routes::voice::issue_token),
        )
        .route(
            "/{channel_id}/voice/states",
            get(routes::voice::voice_states),
        )
        .route(
            "/{channel_id}/voice/password",
            put(routes::voice::set_password),
        )
        .route(
            "/{channel_id}/voice/participants/{user_id}/server-mute",
            put(routes::voice::server_mute),
        )
        .route(
            "/{channel_id}/voice/participants/{user_id}/server-deafen",
            put(routes::voice::server_deafen),
        )
        .route(
            "/{channel_id}/voice/participants/{user_id}",
            delete(routes::voice::disconnect_participant),
        )
        .route(
            "/{channel_id}/voice/participants/{user_id}/move",
            post(routes::voice::move_participant),
        );

    let messages = Router::new()
        .route(
            "/{message_id}",
            get(routes::messages::get_message)
                .patch(routes::messages::edit_message)
                .delete(routes::messages::delete_message),
        )
        .route(
            "/{message_id}/reactions",
            get(routes::messages::list_reactions),
        )
        .route(
            "/{message_id}/reactions/{emoji}",
            put(routes::messages::add_reaction).delete(routes::messages::remove_reaction),
        )
        .route(
            "/{message_id}/attachments/{attachment_id}",
            get(routes::attachments::download_attachment),
        );

    let notifications = Router::new()
        .route("/", get(routes::notifications::list_notifications))
        .route(
            "/read",
            post(routes::notifications::mark_notifications_read),
        );

    let gateway = Router::new().route("/ticket", post(routes::gateway::issue_ticket));

    let sync = Router::new()
        .route("/", get(routes::sync::sync))
        .route("/instance", get(routes::sync::sync_instance));

    let admin = Router::new()
        .route("/overview", get(routes::admin::overview))
        .route("/users", get(routes::admin::list_users))
        .route(
            "/users/{user_id}",
            axum::routing::patch(routes::admin::update_user),
        )
        .route("/audit-logs", get(routes::admin::audit_logs));

    let friends = Router::new()
        .route("/", get(routes::social::list_friends))
        .route(
            "/requests",
            get(routes::social::list_requests).post(routes::social::send_request),
        )
        .route(
            "/requests/{request_id}/accept",
            post(routes::social::accept_request),
        )
        .route(
            "/requests/{request_id}/decline",
            post(routes::social::decline_request),
        )
        .route("/{user_id}", delete(routes::social::remove_friend));

    let blocks = Router::new()
        .route("/", get(routes::social::list_blocks))
        .route(
            "/{user_id}",
            put(routes::social::block_user).delete(routes::social::unblock_user),
        );

    let dms = Router::new().route(
        "/",
        get(routes::social::list_dms).post(routes::social::create_dm),
    );

    let embed = Router::new()
        .route("/authorize", post(routes::embed::authorize))
        .route("/token", post(routes::embed::token))
        .route("/community", get(routes::embed::community));
    let embed_apps = Router::new().route(
        "/{app_id}",
        axum::routing::patch(routes::embed::update_app).delete(routes::embed::revoke_app),
    );
    let webhooks = Router::new().route("/{webhook_id}", delete(routes::webhooks::delete_webhook));

    let cors = cors_layer(&state);
    let body_limit = state
        .config
        .server
        .body_limit_bytes
        .max(state.config.storage.max_upload_bytes as usize + 64 * 1024);
    let timeout = Duration::from_secs(state.config.server.request_timeout_seconds);
    let api_key_state = state.clone();

    Router::new()
        .route("/health/live", get(routes::health::live))
        .route("/health/ready", get(routes::health::ready))
        .route("/metrics", get(routes::metrics::metrics))
        .route("/api/v1/version", get(routes::health::version))
        .route("/gateway", get(routes::gateway::gateway))
        .route("/webhooks/livekit", post(routes::voice::livekit_webhook))
        .nest("/api/v1/auth", auth)
        .nest("/api/v1/users", users)
        .nest("/api/v1/communities", communities)
        .nest("/api/v1/invites", invites)
        .nest("/api/v1/channels", channels)
        .nest("/api/v1/messages", messages)
        .nest("/api/v1/notifications", notifications)
        .nest("/api/v1/gateway", gateway)
        .nest("/api/v1/sync", sync)
        .nest("/api/v1/admin", admin)
        .nest("/api/v1/friends", friends)
        .nest("/api/v1/blocks", blocks)
        .nest("/api/v1/dms", dms)
        .nest("/api/v1/embed", embed)
        .nest("/api/v1/embed-apps", embed_apps)
        .nest("/api/v1/webhooks", webhooks)
        .layer(middleware::from_fn_with_state(
            api_key_state,
            extract::api_key_auth,
        ))
        .with_state(state)
        .merge(SwaggerUi::new("/docs").url("/api/v1/openapi.json", openapi::document()))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
        .layer(RequestBodyLimitLayer::new(body_limit))
        .layer(cors)
        .layer(middleware::from_fn(request_id))
        .layer(TraceLayer::new_for_http())
}

fn cors_layer(state: &AppState) -> CorsLayer {
    let origins: Vec<HeaderValue> = state
        .config
        .cors
        .allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();

    let methods = [
        Method::GET,
        Method::POST,
        Method::PATCH,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ];
    let headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::ORIGIN,
        HeaderName::from_static("idempotency-key"),
        HeaderName::from_static("x-tulpar-client"),
    ];
    let exposed = [
        HeaderName::from_static("x-request-id"),
        HeaderName::from_static("x-ratelimit-limit"),
        HeaderName::from_static("x-ratelimit-remaining"),
        HeaderName::from_static("retry-after"),
    ];

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(methods)
        .allow_headers(headers)
        .expose_headers(exposed)
        .allow_credentials(true)
        .max_age(Duration::from_secs(600))
}

async fn request_id(mut request: Request<Body>, next: Next) -> Response {
    let id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 128 && !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    request.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&id).unwrap_or_else(|_| HeaderValue::from_static("invalid")),
    );

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}
