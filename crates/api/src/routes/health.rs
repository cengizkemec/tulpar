use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;

use crate::{
    dto::{HealthChecks, HealthResponse, LivenessResponse, VersionResponse},
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    responses((status = 200, body = LivenessResponse))
)]
pub async fn live() -> Json<LivenessResponse> {
    Json(LivenessResponse {
        status: "ok".to_string(),
    })
}

#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    responses(
        (status = 200, body = HealthResponse),
        (status = 503, body = HealthResponse)
    )
)]
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let database_up = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();
    let redis_status = match &state.redis {
        Some(redis) => match redis.health().await {
            Ok(()) => "up".to_string(),
            Err(_) => "down".to_string(),
        },
        None => "not_configured".to_string(),
    };

    let status = if !database_up {
        "unavailable"
    } else if redis_status == "down" {
        "degraded"
    } else {
        "ready"
    };

    let http_status = if database_up {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        http_status,
        Json(HealthResponse {
            status: status.to_string(),
            checks: HealthChecks {
                database: if database_up { "up" } else { "down" }.to_string(),
                redis: redis_status,
            },
            server_time: Utc::now(),
        }),
    )
}

#[utoipa::path(
    get,
    path = "/api/v1/version",
    tag = "health",
    responses((status = 200, body = VersionResponse))
)]
pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        name: "Tulpar".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_version: tulpar_core::API_VERSION.to_string(),
    })
}
