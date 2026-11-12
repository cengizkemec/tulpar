use axum::{
    extract::{Query, State},
    Json,
};

use crate::{
    dto::{
        MarkNotificationsReadRequest, MarkNotificationsReadResponse, NotificationDto,
        NotificationListQuery, NotificationPreferencesDto, UpdateNotificationPreferencesRequest,
    },
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/notifications",
    tag = "notifications",
    security(("bearer_auth" = [])),
    params(
        ("before" = Option<uuid::Uuid>, Query, description = "Return notifications older than this ID"),
        ("limit" = Option<i64>, Query, description = "Maximum notifications, 1-100")
    ),
    responses((status = 200, body = Vec<NotificationDto>))
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(params): Query<NotificationListQuery>,
) -> Result<Json<Vec<NotificationDto>>, ApiError> {
    let notifications = state
        .notifications
        .list(current.user().id, params.before, params.limit.unwrap_or(50))
        .await?;
    Ok(Json(
        notifications
            .into_iter()
            .map(NotificationDto::from)
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/notifications/read",
    tag = "notifications",
    security(("bearer_auth" = [])),
    request_body = MarkNotificationsReadRequest,
    responses((status = 200, body = MarkNotificationsReadResponse))
)]
pub async fn mark_notifications_read(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<MarkNotificationsReadRequest>,
) -> Result<Json<MarkNotificationsReadResponse>, ApiError> {
    if payload.all != Some(true) && payload.ids.is_none() {
        return Err(ApiError::validation(
            "Provide notification ids or set all=true.",
        ));
    }
    let ids = if payload.all == Some(true) {
        None
    } else {
        payload.ids.as_deref()
    };
    let updated = state
        .notifications
        .mark_read(current.user().id, ids)
        .await?;
    Ok(Json(MarkNotificationsReadResponse { updated }))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/notification-preferences",
    tag = "notifications",
    security(("bearer_auth" = [])),
    responses((status = 200, body = NotificationPreferencesDto))
)]
pub async fn get_preferences(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<NotificationPreferencesDto>, ApiError> {
    let preferences = state.notifications.preferences(current.user().id).await?;
    Ok(Json(preferences.into()))
}

#[utoipa::path(
    put,
    path = "/api/v1/users/me/notification-preferences",
    tag = "notifications",
    security(("bearer_auth" = [])),
    request_body = UpdateNotificationPreferencesRequest,
    responses((status = 200, body = NotificationPreferencesDto))
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<UpdateNotificationPreferencesRequest>,
) -> Result<Json<NotificationPreferencesDto>, ApiError> {
    let preferences = state
        .notifications
        .update_preferences(
            current.user().id,
            payload.desktop_enabled,
            payload.sound_enabled,
            payload.mentions_only,
            payload.suppress_everyone,
            payload.email_enabled,
        )
        .await?;
    Ok(Json(preferences.into()))
}
