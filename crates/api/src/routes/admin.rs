use axum::{
    extract::{Path, Query, State},
    Json,
};
use uuid::Uuid;

use crate::{
    dto::{
        AdminAuditQuery, AdminOverviewDto, AdminUpdateUserRequest, AdminUserDto, AdminUsersQuery,
        AuditLogDto,
    },
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
};

async fn require_admin(state: &AppState, user_id: Uuid) -> Result<(), ApiError> {
    if state.users.is_instance_admin(user_id).await? {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "instance_admin_required",
            "Instance administrator access is required.",
        ))
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/overview",
    tag = "admin",
    security(("bearer_auth" = [])),
    responses((status = 200, body = AdminOverviewDto))
)]
pub async fn overview(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<AdminOverviewDto>, ApiError> {
    require_admin(&state, current.user().id).await?;
    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM users WHERE deleted_at IS NULL),
            (SELECT COUNT(*) FROM communities WHERE deleted_at IS NULL),
            (SELECT COUNT(*) FROM messages WHERE deleted_at IS NULL),
            (SELECT COUNT(*) FROM sessions WHERE revoked_at IS NULL AND expires_at > now()),
            (SELECT COUNT(*) FROM files WHERE deleted_at IS NULL AND status = 'ready'),
            (SELECT COUNT(*) FROM channels WHERE kind = 'voice' AND deleted_at IS NULL)
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "admin overview query failed");
        ApiError::dependency_unavailable("The database is temporarily unavailable.")
    })?;
    Ok(Json(AdminOverviewDto {
        users: counts.0,
        communities: counts.1,
        messages: counts.2,
        active_sessions: counts.3,
        files: counts.4,
        voice_channels: counts.5,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "admin",
    security(("bearer_auth" = [])),
    params(
        ("query" = Option<String>, Query, description = "Username, display name, or email search"),
        ("limit" = Option<i64>, Query, description = "Maximum results, 1-100"),
        ("offset" = Option<i64>, Query, description = "Pagination offset")
    ),
    responses((status = 200, body = Vec<AdminUserDto>))
)]
pub async fn list_users(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(params): Query<AdminUsersQuery>,
) -> Result<Json<Vec<AdminUserDto>>, ApiError> {
    require_admin(&state, current.user().id).await?;
    let users = state
        .users
        .admin_list(
            params.query.as_deref(),
            params.limit.unwrap_or(50),
            params.offset.unwrap_or(0),
        )
        .await?;
    Ok(Json(users.into_iter().map(AdminUserDto::from).collect()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{user_id}",
    tag = "admin",
    security(("bearer_auth" = [])),
    params(("user_id" = Uuid, Path, description = "User identifier")),
    request_body = AdminUpdateUserRequest,
    responses((status = 200, body = AdminUserDto))
)]
pub async fn update_user(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<AdminUpdateUserRequest>,
) -> Result<Json<AdminUserDto>, ApiError> {
    require_admin(&state, current.user().id).await?;
    if user_id == current.user().id {
        if payload.disabled == Some(true) {
            return Err(ApiError::validation("You cannot disable your own account."));
        }
        if payload.instance_admin == Some(false) {
            return Err(ApiError::validation(
                "You cannot remove your own instance administrator access.",
            ));
        }
    }
    let record = state.users.get_record(user_id).await?;
    if let Some(disabled) = payload.disabled {
        state.users.set_disabled(user_id, disabled).await?;
    }
    if let Some(admin) = payload.instance_admin {
        state
            .users
            .set_instance_admin(user_id, admin, current.user().id)
            .await?;
    }
    state
        .record_audit(
            Some(current.user().id),
            "admin.user_update",
            None,
            Some(("user", user_id)),
        )
        .await;
    let users = state
        .users
        .admin_list(Some(&record.username), 100, 0)
        .await?;
    let user = users
        .into_iter()
        .find(|user| user.id == user_id)
        .ok_or_else(|| ApiError::not_found("User not found."))?;
    Ok(Json(user.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/audit-logs",
    tag = "admin",
    security(("bearer_auth" = [])),
    params(
        ("limit" = Option<i64>, Query, description = "Maximum records, 1-200"),
        ("offset" = Option<i64>, Query, description = "Pagination offset")
    ),
    responses((status = 200, body = Vec<AuditLogDto>))
)]
pub async fn audit_logs(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(params): Query<AdminAuditQuery>,
) -> Result<Json<Vec<AuditLogDto>>, ApiError> {
    require_admin(&state, current.user().id).await?;
    let logs = state
        .audit
        .list_global(params.limit.unwrap_or(50), params.offset.unwrap_or(0))
        .await
        .map_err(|error| {
            tracing::error!(%error, "admin audit query failed");
            ApiError::dependency_unavailable("Audit logs are temporarily unavailable.")
        })?;
    Ok(Json(logs.into_iter().map(AuditLogDto::from).collect()))
}
