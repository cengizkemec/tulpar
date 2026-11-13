use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use common::{normalize_text, normalize_username, validate_username};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    dto::{BotDto, CreateBotRequest},
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
};

#[derive(Debug, FromRow)]
struct BotRow {
    id: Uuid,
    username: String,
    display_name: String,
    created_at: DateTime<Utc>,
}

impl From<BotRow> for BotDto {
    fn from(value: BotRow) -> Self {
        Self {
            id: value.id,
            username: value.username,
            display_name: value.display_name,
            created_at: value.created_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/bots",
    tag = "users",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [BotDto]))
)]
pub async fn list_bots(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<BotDto>>, ApiError> {
    if current.user().is_bot {
        return Err(ApiError::forbidden(
            "bot_account",
            "Bot accounts cannot manage other bots.",
        ));
    }
    let bots = sqlx::query_as::<_, BotRow>(
        "SELECT id, username, display_name, created_at
         FROM users
         WHERE bot_owner_id = $1 AND is_bot AND deleted_at IS NULL
         ORDER BY created_at",
    )
    .bind(current.user().id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to list bots");
        ApiError::dependency_unavailable("Bot accounts are temporarily unavailable.")
    })?;
    Ok(Json(bots.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/me/bots",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = CreateBotRequest,
    responses((status = 200, body = BotDto))
)]
pub async fn create_bot(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<CreateBotRequest>,
) -> Result<Json<BotDto>, ApiError> {
    if current.user().is_bot {
        return Err(ApiError::forbidden(
            "bot_account",
            "Bot accounts cannot create other bots.",
        ));
    }
    let username = normalize_username(&payload.username);
    validate_username(&username)
        .map_err(|error| ApiError::validation(format!("Invalid bot username: {error}")))?;
    let display_name = normalize_text(&payload.display_name);
    if display_name.is_empty() || display_name.chars().count() > 64 {
        return Err(ApiError::validation(
            "Bot display names must be between 1 and 64 characters.",
        ));
    }
    let id = Uuid::now_v7();
    let email = format!("bot-{id}@bots.invalid");
    let bot = sqlx::query_as::<_, BotRow>(
        "INSERT INTO users
            (id, username, display_name, email, is_bot, bot_owner_id)
         VALUES ($1, $2, $3, $4, true, $5)
         RETURNING id, username, display_name, created_at",
    )
    .bind(id)
    .bind(&username)
    .bind(&display_name)
    .bind(&email)
    .bind(current.user().id)
    .fetch_one(&state.db)
    .await;
    let bot = match bot {
        Ok(bot) => bot,
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::conflict(
                "username_taken",
                "That bot username is already taken.",
            ));
        }
        Err(error) => {
            tracing::error!(%error, "failed to create bot");
            return Err(ApiError::dependency_unavailable(
                "Bot accounts are temporarily unavailable.",
            ));
        }
    };
    state
        .record_audit(
            Some(current.user().id),
            "bot.create",
            None,
            Some(("user", bot.id)),
        )
        .await;
    Ok(Json(bot.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/users/me/bots/{bot_id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("bot_id" = Uuid, Path, description = "Bot user identifier")),
    responses((status = 204, description = "Bot disabled"))
)]
pub async fn delete_bot(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(bot_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if current.user().is_bot {
        return Err(ApiError::forbidden(
            "bot_account",
            "Bot accounts cannot manage bots.",
        ));
    }
    let updated = sqlx::query(
        "UPDATE users
         SET deleted_at = now(), disabled_at = now(), updated_at = now()
         WHERE id = $1 AND bot_owner_id = $2 AND is_bot AND deleted_at IS NULL",
    )
    .bind(bot_id)
    .bind(current.user().id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to delete bot");
        ApiError::dependency_unavailable("Bot accounts are temporarily unavailable.")
    })?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("Bot not found."));
    }
    let _ = sqlx::query(
        "UPDATE api_keys SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(bot_id)
    .execute(&state.db)
    .await;
    state
        .record_audit(
            Some(current.user().id),
            "bot.delete",
            None,
            Some(("user", bot_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}
