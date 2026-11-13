use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use common::{random_token, token_hash};
use uuid::Uuid;

use crate::{
    dto::{ApiKeyDto, CreateApiKeyRequest, CreateApiKeyResponse},
    error::ApiError,
    extract::{enforce_rate_limit, CurrentUser},
    state::AppState,
};

const SCOPES: &[&str] = &["read", "write"];

type ApiKeyRow = (
    Uuid,
    String,
    Vec<String>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
);

fn to_dto(row: ApiKeyRow) -> ApiKeyDto {
    ApiKeyDto {
        id: row.0,
        name: row.1,
        scopes: row.2,
        created_at: row.3,
        last_used_at: row.4,
        revoked_at: row.5,
    }
}

fn normalize_scopes(scopes: &[String]) -> Result<Vec<String>, ApiError> {
    if scopes.is_empty() || scopes.len() > SCOPES.len() {
        return Err(ApiError::validation(
            "Select at least one API key scope (read or write).",
        ));
    }
    let mut normalized = Vec::new();
    for scope in scopes {
        let value = scope.trim();
        if !SCOPES.contains(&value) {
            return Err(ApiError::validation(format!(
                "Unknown API key scope: {value}"
            )));
        }
        if !normalized.iter().any(|existing| existing == value) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/api-keys",
    tag = "users",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [ApiKeyDto]))
)]
pub async fn list_keys(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<ApiKeyDto>>, ApiError> {
    if current.is_api_key() {
        return Err(ApiError::forbidden(
            "api_key_scope",
            "API keys cannot manage other API keys.",
        ));
    }
    let rows: Vec<ApiKeyRow> = sqlx::query_as(
        "SELECT id, name, scopes, created_at, last_used_at, revoked_at
         FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(current.user().id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to list API keys");
        ApiError::dependency_unavailable("API keys are temporarily unavailable.")
    })?;
    Ok(Json(rows.into_iter().map(to_dto).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/me/api-keys",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = CreateApiKeyRequest,
    responses((status = 200, body = CreateApiKeyResponse))
)]
pub async fn create_key(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    if current.is_api_key() {
        return Err(ApiError::forbidden(
            "api_key_scope",
            "API keys cannot create other API keys.",
        ));
    }
    enforce_rate_limit(
        &state,
        &format!("api_key:create:{}", current.user().id),
        10,
        Duration::from_secs(3600),
    )
    .await?;
    let name = payload.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::validation(
            "API key names must be between 1 and 100 characters.",
        ));
    }
    let scopes = normalize_scopes(&payload.scopes)?;
    let plaintext = format!("tk_{}", random_token(32));
    let hash = token_hash(&plaintext);
    let id = Uuid::now_v7();
    let row: ApiKeyRow = sqlx::query_as(
        "INSERT INTO api_keys (id, user_id, name, key_hash, scopes)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, name, scopes, created_at, last_used_at, revoked_at",
    )
    .bind(id)
    .bind(current.user().id)
    .bind(name)
    .bind(hash.to_vec())
    .bind(&scopes)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to create API key");
        ApiError::dependency_unavailable("API keys are temporarily unavailable.")
    })?;
    state
        .record_audit(
            Some(current.user().id),
            "api_key.create",
            None,
            Some(("api_key", id)),
        )
        .await;
    Ok(Json(CreateApiKeyResponse {
        api_key: plaintext,
        key: to_dto(row),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/users/me/api-keys/{key_id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("key_id" = Uuid, Path, description = "API key identifier")),
    responses((status = 204, description = "API key revoked"))
)]
pub async fn revoke_key(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(key_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if current.is_api_key() {
        return Err(ApiError::forbidden(
            "api_key_scope",
            "API keys cannot revoke API keys.",
        ));
    }
    let updated = sqlx::query(
        "UPDATE api_keys SET revoked_at = now()
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(current.user().id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to revoke API key");
        ApiError::dependency_unavailable("API keys are temporarily unavailable.")
    })?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("API key not found."));
    }
    state
        .record_audit(
            Some(current.user().id),
            "api_key.revoke",
            None,
            Some(("api_key", key_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}
