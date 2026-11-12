use std::time::Duration;

use auth::{DevicePoll, LoginInput, RegisterInput};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;

use crate::{
    dto::{
        AccessTokenResponse, ChangePasswordRequest, DeviceApproveRequest, DeviceCodeRequest,
        DeviceCodeResponseDto, DeviceTokenRequest, DeviceTokenResponse, EmailRequest,
        ForgotPasswordRequest, LoginRequest, LoginResponse, LogoutRequest, ReauthRequest,
        RefreshRequest, RefreshResponse, RegisterRequest, RegistrationResponse,
        ResetPasswordRequest, SessionDto, VerifyEmailRequest,
    },
    error::ApiError,
    extract::{
        clear_refresh_cookie, enforce_rate_limit, ensure_origin, refresh_cookie, request_ip_key,
        set_refresh_cookie, CurrentUser, OptionalJson, RequestContext,
    },
    state::AppState,
};

#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 202, description = "Registration accepted", body = RegistrationResponse),
        (status = 400, description = "Invalid request", body = crate::error::ProblemDetails),
        (status = 429, description = "Rate limited", body = crate::error::ProblemDetails)
    )
)]
pub async fn register(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegistrationResponse>), ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    enforce_rate_limit(
        &state,
        &format!("auth:register:{}", request_ip_key(&ctx.meta)),
        3,
        Duration::from_secs(3600),
    )
    .await?;

    let result = state
        .auth
        .register(RegisterInput {
            username: payload.username,
            display_name: payload.display_name,
            email: payload.email,
            password: payload.password,
            locale: payload.locale.unwrap_or_else(|| "en".to_string()),
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RegistrationResponse {
            accepted: result.accepted,
            verification_required: result.verification_required,
            login_required: true,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = crate::error::ProblemDetails),
        (status = 403, description = "Email verification required", body = crate::error::ProblemDetails),
        (status = 429, description = "Rate limited", body = crate::error::ProblemDetails)
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    enforce_rate_limit(
        &state,
        &format!("auth:login:{}", request_ip_key(&ctx.meta)),
        5,
        Duration::from_secs(900),
    )
    .await?;

    let issued = state
        .auth
        .login(LoginInput {
            login: payload.login,
            password: payload.password,
            device_name: payload.device_name,
            platform: "web".to_string(),
            ip: ctx.meta.ip,
            user_agent: ctx.meta.user_agent.clone(),
        })
        .await?;

    let mut headers = HeaderMap::new();
    set_refresh_cookie(&mut headers, &state.config, &issued.tokens.refresh_token)?;

    Ok((
        headers,
        Json(LoginResponse {
            access_token: issued.tokens.access_token,
            token_type: "Bearer".to_string(),
            expires_in: issued.tokens.expires_in,
            session_id: issued.tokens.session_id,
            user: (&issued.user).into(),
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    responses(
        (status = 200, description = "Token refreshed", body = RefreshResponse),
        (status = 401, description = "Invalid refresh token", body = crate::error::ProblemDetails)
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    ctx: RequestContext,
    body: OptionalJson<RefreshRequest>,
) -> Result<(HeaderMap, Json<RefreshResponse>), ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;

    let body = body.0;
    let body_token = body.as_ref().and_then(|value| value.refresh_token.clone());
    let is_native = body_token.is_some();
    let token = body_token
        .or_else(|| refresh_cookie(&ctx.headers, &state.config))
        .ok_or_else(|| ApiError::unauthorized("invalid_token", "A refresh token is required."))?;

    let tokens = state.auth.refresh(&token).await?;
    let mut headers = HeaderMap::new();
    let refresh_token = if is_native {
        Some(tokens.refresh_token)
    } else {
        set_refresh_cookie(&mut headers, &state.config, &tokens.refresh_token)?;
        None
    };

    Ok((
        headers,
        Json(RefreshResponse {
            access_token: tokens.access_token,
            token_type: "Bearer".to_string(),
            expires_in: tokens.expires_in,
            refresh_token,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    responses((status = 204, description = "Logged out"))
)]
pub async fn logout(
    State(state): State<AppState>,
    ctx: RequestContext,
    body: OptionalJson<LogoutRequest>,
) -> Result<(StatusCode, HeaderMap), ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    let body = body.0;
    let token = body
        .and_then(|value| value.refresh_token)
        .or_else(|| refresh_cookie(&ctx.headers, &state.config));

    if let Some(token) = token {
        state.auth.logout(&token).await?;
    }

    let mut headers = HeaderMap::new();
    clear_refresh_cookie(&mut headers, &state.config)?;
    Ok((StatusCode::NO_CONTENT, headers))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout-all",
    tag = "auth",
    security(("bearer_auth" = [])),
    responses((status = 204, description = "All sessions revoked"))
)]
pub async fn logout_all(
    State(state): State<AppState>,
    ctx: RequestContext,
    current: CurrentUser,
) -> Result<(StatusCode, HeaderMap), ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    state
        .auth
        .logout_session(current.user().id, current.session_id())
        .await?;
    let mut headers = HeaderMap::new();
    clear_refresh_cookie(&mut headers, &state.config)?;
    Ok((StatusCode::NO_CONTENT, headers))
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/sessions",
    tag = "auth",
    security(("bearer_auth" = [])),
    responses((status = 200, body = Vec<SessionDto>))
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<SessionDto>>, ApiError> {
    let sessions = state
        .auth
        .list_sessions(current.user().id, current.session_id())
        .await?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|session| SessionDto {
                id: session.id,
                device_name: session.device_name,
                platform: session.platform,
                created_at: session.created_at,
                last_seen_at: session.last_seen_at,
                expires_at: session.expires_at,
                current: session.current,
            })
            .collect(),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/auth/sessions/{session_id}",
    tag = "auth",
    security(("bearer_auth" = [])),
    params(("session_id" = Uuid, Path, description = "Session identifier")),
    responses(
        (status = 204, description = "Session revoked"),
        (status = 404, description = "Session not found", body = crate::error::ProblemDetails)
    )
)]
pub async fn revoke_session(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .logout_session(current.user().id, session_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/auth/sessions",
    tag = "auth",
    security(("bearer_auth" = [])),
    responses((status = 204, description = "Other sessions revoked"))
)]
pub async fn revoke_other_sessions(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .revoke_other_sessions(current.user().id, current.session_id())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/verify-email",
    tag = "auth",
    request_body = VerifyEmailRequest,
    responses(
        (status = 204, description = "Email verified"),
        (status = 400, description = "Invalid token", body = crate::error::ProblemDetails)
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    enforce_rate_limit(
        &state,
        &format!("auth:verify:{}", request_ip_key(&ctx.meta)),
        10,
        Duration::from_secs(3600),
    )
    .await?;
    state.auth.verify_email(&payload.token).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/resend-verification",
    tag = "auth",
    request_body = EmailRequest,
    responses((status = 202, description = "Request accepted"))
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<EmailRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    enforce_rate_limit(
        &state,
        &format!("auth:resend:{}", request_ip_key(&ctx.meta)),
        3,
        Duration::from_secs(3600),
    )
    .await?;
    state.auth.resend_verification(&payload.email).await?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/password/forgot",
    tag = "auth",
    request_body = ForgotPasswordRequest,
    responses((status = 202, description = "Request accepted"))
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    enforce_rate_limit(
        &state,
        &format!("auth:forgot:{}", request_ip_key(&ctx.meta)),
        3,
        Duration::from_secs(3600),
    )
    .await?;
    state.auth.request_password_reset(&payload.login).await?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/password/reset",
    tag = "auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 204, description = "Password reset"),
        (status = 400, description = "Invalid token", body = crate::error::ProblemDetails)
    )
)]
pub async fn reset_password(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<(StatusCode, HeaderMap), ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    enforce_rate_limit(
        &state,
        &format!("auth:reset:{}", request_ip_key(&ctx.meta)),
        5,
        Duration::from_secs(3600),
    )
    .await?;
    state
        .auth
        .reset_password(&payload.token, &payload.new_password)
        .await?;
    let mut headers = HeaderMap::new();
    clear_refresh_cookie(&mut headers, &state.config)?;
    Ok((StatusCode::NO_CONTENT, headers))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/password/change",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed", body = AccessTokenResponse),
        (status = 401, description = "Invalid password", body = crate::error::ProblemDetails)
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<AccessTokenResponse>, ApiError> {
    let token = state
        .auth
        .change_password(
            current.user().id,
            current.session_id(),
            &payload.current_password,
            &payload.new_password,
        )
        .await?;
    Ok(Json(AccessTokenResponse {
        access_token: token.access_token,
        token_type: "Bearer".to_string(),
        expires_in: token.expires_in,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/reauth",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = ReauthRequest,
    responses(
        (status = 200, description = "Re-authenticated", body = AccessTokenResponse),
        (status = 401, description = "Invalid password", body = crate::error::ProblemDetails)
    )
)]
pub async fn reauth(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<ReauthRequest>,
) -> Result<Json<AccessTokenResponse>, ApiError> {
    let token = state
        .auth
        .reauth(current.user().id, current.session_id(), &payload.password)
        .await?;
    Ok(Json(AccessTokenResponse {
        access_token: token.access_token,
        token_type: "Bearer".to_string(),
        expires_in: token.expires_in,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/device/code",
    tag = "auth",
    request_body = DeviceCodeRequest,
    responses((status = 200, body = DeviceCodeResponseDto))
)]
pub async fn device_code(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<DeviceCodeRequest>,
) -> Result<Json<DeviceCodeResponseDto>, ApiError> {
    enforce_rate_limit(
        &state,
        &format!("auth:device:code:{}", request_ip_key(&ctx.meta)),
        10,
        Duration::from_secs(3600),
    )
    .await?;
    let response = state
        .auth
        .start_device_authorization(&payload.client_name, &payload.platform, ctx.meta.ip)
        .await?;
    Ok(Json(DeviceCodeResponseDto {
        device_code: response.device_code,
        user_code: response.user_code,
        verification_uri: response.verification_uri,
        expires_in: response.expires_in,
        interval: response.interval,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/device/approve",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = DeviceApproveRequest,
    responses((status = 204, description = "Device approved"))
)]
pub async fn approve_device(
    State(state): State<AppState>,
    ctx: RequestContext,
    current: CurrentUser,
    Json(payload): Json<DeviceApproveRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    state
        .auth
        .approve_device(current.user().id, &payload.user_code)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/device/deny",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = DeviceApproveRequest,
    responses((status = 204, description = "Device denied"))
)]
pub async fn deny_device(
    State(state): State<AppState>,
    ctx: RequestContext,
    current: CurrentUser,
    Json(payload): Json<DeviceApproveRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_origin(&ctx.headers, &state.config)?;
    state
        .auth
        .deny_device(current.user().id, &payload.user_code)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/device/token",
    tag = "auth",
    request_body = DeviceTokenRequest,
    responses(
        (status = 200, body = DeviceTokenResponse),
        (status = 400, description = "Authorization pending or failed", body = crate::error::ProblemDetails)
    )
)]
pub async fn device_token(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<DeviceTokenRequest>,
) -> Result<Json<DeviceTokenResponse>, ApiError> {
    enforce_rate_limit(
        &state,
        &format!("auth:device:poll:{}", request_ip_key(&ctx.meta)),
        120,
        Duration::from_secs(3600),
    )
    .await?;

    match state.auth.poll_device(&payload.device_code).await? {
        DevicePoll::Pending => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "authorization_pending",
            "Authorization pending",
            "The device authorization has not been approved yet.",
        )),
        DevicePoll::Denied => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "access_denied",
            "Access denied",
            "The device authorization was denied.",
        )),
        DevicePoll::Expired => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "expired_token",
            "Expired token",
            "The device authorization expired.",
        )),
        DevicePoll::Approved(issued) => Ok(Json(DeviceTokenResponse {
            access_token: issued.tokens.access_token,
            token_type: "Bearer".to_string(),
            expires_in: issued.tokens.expires_in,
            refresh_token: issued.tokens.refresh_token,
            session_id: issued.tokens.session_id,
            user: (&issued.user).into(),
        })),
    }
}
