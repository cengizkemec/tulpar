//! Axum REST API, DTOs, middleware, and OpenAPI.

#![forbid(unsafe_code)]

pub mod dto;
pub mod error;
pub mod extract;
pub mod openapi;
pub mod routes;
pub mod state;

use std::time::Duration;

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post},
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
        .route("/{user_id}", get(routes::users::get_user));

    let cors = cors_layer(&state);
    let body_limit = state.config.server.body_limit_bytes;
    let timeout = Duration::from_secs(state.config.server.request_timeout_seconds);

    Router::new()
        .route("/health/live", get(routes::health::live))
        .route("/health/ready", get(routes::health::ready))
        .route("/api/v1/version", get(routes::health::version))
        .nest("/api/v1/auth", auth)
        .nest("/api/v1/users", users)
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
