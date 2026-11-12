use axum::{
    extract::{Path, Query, State},
    Json,
};
use users::{ProfileUpdate, SettingsUpdate};
use uuid::Uuid;

use crate::{
    dto::{
        DeleteAccountRequest, ProfileUpdateRequest, PublicUserDto, SelfUserDto,
        SettingsUpdateRequest, UserSearchQuery, UserSettingsDto,
    },
    error::ApiError,
    extract::{clear_refresh_cookie, CurrentUser},
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/users/me",
    tag = "users",
    security(("bearer_auth" = [])),
    responses((status = 200, body = SelfUserDto))
)]
pub async fn get_me(current: CurrentUser) -> Json<SelfUserDto> {
    Json(current.user().into())
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/export",
    tag = "users",
    security(("bearer_auth" = [])),
    responses((status = 200, body = crate::dto::UserExport))
)]
pub async fn export_me(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<crate::dto::UserExport>, ApiError> {
    let settings = state.users.get_settings(current.user().id).await?;
    let sessions = state
        .auth
        .list_sessions(current.user().id, current.session_id())
        .await?;
    Ok(Json(crate::dto::UserExport {
        exported_at: chrono::Utc::now(),
        user: current.user().into(),
        settings: settings.into(),
        sessions: sessions
            .into_iter()
            .map(|session| crate::dto::SessionDto {
                id: session.id,
                device_name: session.device_name,
                platform: session.platform,
                created_at: session.created_at,
                last_seen_at: session.last_seen_at,
                expires_at: session.expires_at,
                current: session.current,
            })
            .collect(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/users/me",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = DeleteAccountRequest,
    responses(
        (status = 204, description = "Account deleted"),
        (status = 400, description = "Invalid confirmation", body = crate::error::ProblemDetails),
        (status = 401, description = "Invalid password", body = crate::error::ProblemDetails)
    )
)]
pub async fn delete_me(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<DeleteAccountRequest>,
) -> Result<(axum::http::StatusCode, axum::http::HeaderMap), ApiError> {
    if payload.confirmation != "DELETE" {
        return Err(ApiError::validation(
            "Type DELETE in the confirmation field to delete the account.",
        ));
    }
    state
        .auth
        .delete_account(current.user().id, &payload.password)
        .await?;
    let mut headers = axum::http::HeaderMap::new();
    clear_refresh_cookie(&mut headers, &state.config)?;
    Ok((axum::http::StatusCode::NO_CONTENT, headers))
}

#[utoipa::path(
    patch,
    path = "/api/v1/users/me",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = ProfileUpdateRequest,
    responses(
        (status = 200, body = SelfUserDto),
        (status = 400, description = "Invalid request", body = crate::error::ProblemDetails)
    )
)]
pub async fn update_me(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<ProfileUpdateRequest>,
) -> Result<Json<SelfUserDto>, ApiError> {
    if let Some(display_name) = &payload.display_name {
        let length = display_name.trim().chars().count();
        if !(1..=64).contains(&length) {
            return Err(ApiError::validation(
                "Display name must be between 1 and 64 characters.",
            ));
        }
    }
    if let Some(bio) = &payload.bio {
        if bio.chars().count() > 500 {
            return Err(ApiError::validation("Bio must be at most 500 characters."));
        }
    }
    if let Some(locale) = &payload.locale {
        if !(2..=16).contains(&locale.chars().count()) {
            return Err(ApiError::validation("Invalid locale."));
        }
    }
    if let Some(status) = &payload.preferred_status {
        if !matches!(status.as_str(), "online" | "idle" | "dnd" | "invisible") {
            return Err(ApiError::validation("Invalid preferred status."));
        }
    }

    let user = state
        .users
        .update_profile(
            current.user().id,
            ProfileUpdate {
                display_name: payload.display_name.map(|value| value.trim().to_string()),
                bio: payload.bio,
                locale: payload.locale,
                preferred_status: payload.preferred_status,
            },
        )
        .await?;
    Ok(Json((&user).into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/settings",
    tag = "users",
    security(("bearer_auth" = [])),
    responses((status = 200, body = UserSettingsDto))
)]
pub async fn get_settings(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<UserSettingsDto>, ApiError> {
    let settings = state.users.get_settings(current.user().id).await?;
    Ok(Json(settings.into()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/users/me/settings",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = SettingsUpdateRequest,
    responses(
        (status = 200, body = UserSettingsDto),
        (status = 400, description = "Invalid request", body = crate::error::ProblemDetails)
    )
)]
pub async fn update_settings(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<SettingsUpdateRequest>,
) -> Result<Json<UserSettingsDto>, ApiError> {
    if let Some(theme) = &payload.theme {
        if !matches!(theme.as_str(), "dark" | "light" | "system") {
            return Err(ApiError::validation("Invalid theme."));
        }
    }
    if let Some(accent) = &payload.accent_color {
        if accent.len() != 7
            || !accent.starts_with('#')
            || !accent[1..]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(ApiError::validation("Accent color must be a hex color."));
        }
    }
    if let Some(value) = &payload.allow_friend_requests_from {
        if !matches!(value.as_str(), "everyone" | "mutual_communities" | "nobody") {
            return Err(ApiError::validation("Invalid friend request privacy."));
        }
    }
    if let Some(value) = &payload.allow_dms_from {
        if !matches!(
            value.as_str(),
            "everyone" | "friends" | "mutual_communities" | "nobody"
        ) {
            return Err(ApiError::validation("Invalid direct message privacy."));
        }
    }
    if let Some(value) = &payload.presence_visibility {
        if !matches!(value.as_str(), "everyone" | "friends" | "nobody") {
            return Err(ApiError::validation("Invalid presence privacy."));
        }
    }
    if let Some(value) = &payload.custom_status {
        if value.chars().count() > 128 {
            return Err(ApiError::validation(
                "Custom status must be at most 128 characters.",
            ));
        }
    }
    if let Some(value) = &payload.custom_status_emoji {
        if value.chars().count() > 64 {
            return Err(ApiError::validation(
                "Custom status emoji must be at most 64 characters.",
            ));
        }
    }

    let settings = state
        .users
        .update_settings(
            current.user().id,
            SettingsUpdate {
                theme: payload.theme,
                accent_color: payload.accent_color,
                compact_messages: payload.compact_messages,
                show_typing_indicators: payload.show_typing_indicators,
                allow_friend_requests_from: payload.allow_friend_requests_from,
                allow_dms_from: payload.allow_dms_from,
                presence_visibility: payload.presence_visibility,
                read_receipts: payload.read_receipts,
                custom_status: payload.custom_status,
                custom_status_emoji: payload.custom_status_emoji,
            },
        )
        .await?;
    Ok(Json(settings.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("query" = Option<String>, Query, description = "Username or display name search"),
        ("limit" = Option<i64>, Query, description = "Maximum results, 1-50")
    ),
    responses((status = 200, body = Vec<PublicUserDto>))
)]
pub async fn search_users(
    State(state): State<AppState>,
    _current: CurrentUser,
    Query(params): Query<UserSearchQuery>,
) -> Result<Json<Vec<PublicUserDto>>, ApiError> {
    let query = params
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| value.chars().count() >= 2);
    let Some(query) = query else {
        return Ok(Json(Vec::new()));
    };
    let users = state
        .users
        .search(query, params.limit.unwrap_or(20))
        .await?;
    Ok(Json(
        users.iter().map(PublicUserDto::from).collect::<Vec<_>>(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/{user_id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("user_id" = Uuid, Path, description = "User identifier")),
    responses(
        (status = 200, body = PublicUserDto),
        (status = 404, description = "User not found", body = crate::error::ProblemDetails)
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    _current: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<PublicUserDto>, ApiError> {
    let record = state.users.get_record(user_id).await?;
    if !record.is_active() {
        return Err(ApiError::not_found("User not found."));
    }
    Ok(Json((&record.public()).into()))
}
