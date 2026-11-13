use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use common::{random_token, token_hash};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use permissions::Permissions;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use url::Url;
use uuid::Uuid;

use crate::{
    dto::{
        CreateEmbedAppRequest, EmbedAppDto, EmbedAuthorizeRequest, EmbedAuthorizeResponse,
        EmbedCommunityDto, EmbedMessageDto, EmbedTokenRequest, EmbedTokenResponse,
        UpdateEmbedAppRequest,
    },
    error::ApiError,
    extract::{enforce_rate_limit, request_ip_key, CurrentUser, RequestContext},
    state::AppState,
};

const EMBED_AUDIENCE: &str = "embed";
const EMBED_TOKEN_TTL_SECONDS: i64 = 900;
const EMBED_CODE_TTL_SECONDS: i64 = 60;
const WIDGETS: &[&str] = &[
    "community",
    "members",
    "voice_preview",
    "channel_preview",
    "join",
];

#[derive(Debug, Clone, FromRow)]
struct EmbedAppRow {
    id: Uuid,
    community_id: Uuid,
    name: String,
    public_key: String,
    allowed_origins: Vec<String>,
    widgets: Vec<String>,
    allow_authenticated: bool,
    allow_message_preview: bool,
    public_channel_id: Option<Uuid>,
    token_version: i32,
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

impl From<EmbedAppRow> for EmbedAppDto {
    fn from(value: EmbedAppRow) -> Self {
        Self {
            id: value.id,
            community_id: value.community_id,
            name: value.name,
            public_key: value.public_key,
            allowed_origins: value.allowed_origins,
            widgets: value.widgets,
            allow_authenticated: value.allow_authenticated,
            allow_message_preview: value.allow_message_preview,
            public_channel_id: value.public_channel_id,
            token_version: value.token_version,
            created_at: value.created_at,
            revoked_at: value.revoked_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct EmbedClaims {
    iss: String,
    sub: Option<Uuid>,
    aud: String,
    app: Uuid,
    community: Uuid,
    origin: String,
    widget: String,
    scope: String,
    ver: i32,
    iat: i64,
    exp: i64,
}

fn require_permission(
    access: &communities::CommunityAccess,
    permission: Permissions,
) -> Result<(), ApiError> {
    if access.permissions.contains(permission) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "permission_denied",
            "You do not have permission to perform this action.",
        ))
    }
}

fn normalize_origin(input: &str) -> Result<String, ApiError> {
    let trimmed = input.trim().trim_end_matches('/');
    let url =
        Url::parse(trimmed).map_err(|_| ApiError::validation("Enter a valid embed origin."))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || (!url.path().is_empty() && url.path() != "/")
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ApiError::validation(
            "Embed origins must be exact http(s) origins without a path, query, or credentials.",
        ));
    }
    if url.scheme() == "http"
        && !matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1")
        )
    {
        return Err(ApiError::validation(
            "Plain HTTP embed origins are only allowed for localhost development.",
        ));
    }
    let mut origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    Ok(origin)
}

fn normalize_origins(origins: &[String]) -> Result<Vec<String>, ApiError> {
    if origins.is_empty() || origins.len() > 20 {
        return Err(ApiError::validation(
            "An embed application needs between 1 and 20 allowed origins.",
        ));
    }
    let mut normalized = Vec::new();
    for origin in origins {
        let value = normalize_origin(origin)?;
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    Ok(normalized)
}

fn normalize_widgets(widgets: &[String]) -> Result<Vec<String>, ApiError> {
    if widgets.is_empty() || widgets.len() > WIDGETS.len() {
        return Err(ApiError::validation(
            "Select between 1 and 5 embed widgets.",
        ));
    }
    let mut normalized = Vec::new();
    for widget in widgets {
        let value = widget.trim();
        if !WIDGETS.contains(&value) {
            return Err(ApiError::validation(format!(
                "Unknown embed widget: {value}"
            )));
        }
        if !normalized.iter().any(|existing| existing == value) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

async fn load_app_by_key(state: &AppState, public_key: &str) -> Result<EmbedAppRow, ApiError> {
    sqlx::query_as::<_, EmbedAppRow>(
        "SELECT id, community_id, name, public_key, allowed_origins, widgets,
                allow_authenticated, allow_message_preview, public_channel_id, token_version, created_at, revoked_at
         FROM embed_apps WHERE public_key = $1",
    )
    .bind(public_key)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to load embed app");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?
    .ok_or_else(|| ApiError::not_found("Embed application not found."))
}

async fn load_app_by_id(state: &AppState, app_id: Uuid) -> Result<EmbedAppRow, ApiError> {
    sqlx::query_as::<_, EmbedAppRow>(
        "SELECT id, community_id, name, public_key, allowed_origins, widgets,
                allow_authenticated, allow_message_preview, public_channel_id, token_version, created_at, revoked_at
         FROM embed_apps WHERE id = $1",
    )
    .bind(app_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to load embed app");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?
    .ok_or_else(|| ApiError::not_found("Embed application not found."))
}

fn ensure_active(app: &EmbedAppRow) -> Result<(), ApiError> {
    if app.revoked_at.is_some() {
        return Err(ApiError::forbidden(
            "embed_revoked",
            "This embed application has been revoked.",
        ));
    }
    Ok(())
}

fn ensure_widget(app: &EmbedAppRow, widget: &str) -> Result<(), ApiError> {
    if !app.widgets.iter().any(|value| value == widget) {
        return Err(ApiError::forbidden(
            "embed_widget_disabled",
            "This widget is not enabled for the embed application.",
        ));
    }
    Ok(())
}

fn ensure_origin(app: &EmbedAppRow, origin: &str) -> Result<(), ApiError> {
    if !app.allowed_origins.iter().any(|value| value == origin) {
        return Err(ApiError::forbidden(
            "embed_origin_denied",
            "This origin is not allowed for the embed application.",
        ));
    }
    Ok(())
}

async fn validate_public_channel(
    state: &AppState,
    community_id: Uuid,
    channel_id: Uuid,
) -> Result<(), ApiError> {
    let row: Option<(String, Option<Uuid>, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT kind, community_id, deleted_at FROM channels WHERE id = $1")
            .bind(channel_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| {
                tracing::error!(%error, "failed to validate embed public channel");
                ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
            })?;
    match row {
        Some((kind, Some(channel_community), None))
            if channel_community == community_id
                && matches!(kind.as_str(), "text" | "announcement") =>
        {
            Ok(())
        }
        _ => Err(ApiError::validation(
            "The public preview channel must be an active text or announcement channel in this community.",
        )),
    }
}

fn sign_embed_token(
    state: &AppState,
    app: &EmbedAppRow,
    user_id: Option<Uuid>,
    origin: &str,
    widget: &str,
    scope: &str,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = EmbedClaims {
        iss: state
            .config
            .instance
            .public_url
            .trim_end_matches('/')
            .to_string(),
        sub: user_id,
        aud: EMBED_AUDIENCE.to_string(),
        app: app.id,
        community: app.community_id,
        origin: origin.to_string(),
        widget: widget.to_string(),
        scope: scope.to_string(),
        ver: app.token_version,
        iat: now.timestamp(),
        exp: (now + Duration::seconds(EMBED_TOKEN_TTL_SECONDS)).timestamp(),
    };
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(state.config.auth.jwt_kid.clone());
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
    )
    .map_err(|error| {
        tracing::error!(%error, "failed to sign embed token");
        ApiError::internal("The embed token could not be issued.")
    })
}

fn decode_embed_token(state: &AppState, token: &str) -> Result<EmbedClaims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[state.config.instance.public_url.trim_end_matches('/')]);
    validation.set_audience(&[EMBED_AUDIENCE]);
    validation.leeway = 60;
    decode::<EmbedClaims>(
        token,
        &DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| {
        ApiError::unauthorized(
            "invalid_embed_token",
            "The embed token is invalid or expired.",
        )
    })
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/embed-apps",
    tag = "embed",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    request_body = CreateEmbedAppRequest,
    responses((status = 200, body = EmbedAppDto))
)]
pub async fn create_app(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
    Json(payload): Json<CreateEmbedAppRequest>,
) -> Result<Json<EmbedAppDto>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_COMMUNITY)?;
    let name = payload.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::validation(
            "Embed application names must be between 1 and 100 characters.",
        ));
    }
    let origins = normalize_origins(&payload.allowed_origins)?;
    let widgets = normalize_widgets(&payload.widgets)?;
    if payload.allow_message_preview && payload.public_channel_id.is_none() {
        return Err(ApiError::validation(
            "Select a public preview channel before enabling message previews.",
        ));
    }
    if let Some(channel_id) = payload.public_channel_id {
        validate_public_channel(&state, community_id, channel_id).await?;
    }
    let app = sqlx::query_as::<_, EmbedAppRow>(
        "INSERT INTO embed_apps
            (id, community_id, name, public_key, allowed_origins, widgets,
             allow_authenticated, allow_message_preview, public_channel_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id, community_id, name, public_key, allowed_origins, widgets,
                   allow_authenticated, allow_message_preview, public_channel_id, token_version, created_at, revoked_at",
    )
    .bind(Uuid::now_v7())
    .bind(community_id)
    .bind(name)
    .bind(format!("emb_{}", random_token(18)))
    .bind(&origins)
    .bind(&widgets)
    .bind(payload.allow_authenticated)
    .bind(payload.allow_message_preview)
    .bind(payload.public_channel_id)
    .bind(current.user().id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to create embed app");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?;
    state
        .record_audit(
            Some(current.user().id),
            "embed_app.create",
            Some(community_id),
            Some(("embed_app", app.id)),
        )
        .await;
    Ok(Json(app.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/embed-apps",
    tag = "embed",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = [EmbedAppDto]))
)]
pub async fn list_apps(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<Vec<EmbedAppDto>>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_COMMUNITY)?;
    let apps = sqlx::query_as::<_, EmbedAppRow>(
        "SELECT id, community_id, name, public_key, allowed_origins, widgets,
                allow_authenticated, allow_message_preview, public_channel_id, token_version, created_at, revoked_at
         FROM embed_apps WHERE community_id = $1 ORDER BY created_at DESC",
    )
    .bind(community_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to list embed apps");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?;
    Ok(Json(apps.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/embed-apps/{app_id}",
    tag = "embed",
    security(("bearer_auth" = [])),
    params(("app_id" = Uuid, Path, description = "Embed application identifier")),
    request_body = UpdateEmbedAppRequest,
    responses((status = 200, body = EmbedAppDto))
)]
pub async fn update_app(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(app_id): Path<Uuid>,
    Json(payload): Json<UpdateEmbedAppRequest>,
) -> Result<Json<EmbedAppDto>, ApiError> {
    let existing = load_app_by_id(&state, app_id).await?;
    let access = state
        .communities
        .access(existing.community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_COMMUNITY)?;

    let name = payload
        .name
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| existing.name.clone());
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::validation(
            "Embed application names must be between 1 and 100 characters.",
        ));
    }
    let invalidate =
        payload.allowed_origins.is_some() || payload.widgets.is_some() || payload.revoked.is_some();
    let origins = match payload.allowed_origins {
        Some(values) => normalize_origins(&values)?,
        None => existing.allowed_origins.clone(),
    };
    let widgets = match payload.widgets {
        Some(values) => normalize_widgets(&values)?,
        None => existing.widgets.clone(),
    };
    let allow_message_preview = payload
        .allow_message_preview
        .unwrap_or(existing.allow_message_preview);
    let public_channel_id = payload.public_channel_id.or(existing.public_channel_id);
    if allow_message_preview && public_channel_id.is_none() {
        return Err(ApiError::validation(
            "Select a public preview channel before enabling message previews.",
        ));
    }
    if let Some(channel_id) = public_channel_id {
        validate_public_channel(&state, existing.community_id, channel_id).await?;
    }
    let revoked_at = match payload.revoked {
        Some(true) => Some(Utc::now()),
        Some(false) => None,
        None => existing.revoked_at,
    };
    let token_version = if invalidate {
        existing.token_version + 1
    } else {
        existing.token_version
    };
    let app = sqlx::query_as::<_, EmbedAppRow>(
        "UPDATE embed_apps
         SET name = $2, allowed_origins = $3, widgets = $4, allow_authenticated = $5,
             allow_message_preview = $6, public_channel_id = $7, revoked_at = $8,
             token_version = $9, updated_at = now()
         WHERE id = $1
         RETURNING id, community_id, name, public_key, allowed_origins, widgets,
                   allow_authenticated, allow_message_preview, public_channel_id, token_version, created_at, revoked_at",
    )
    .bind(app_id)
    .bind(name)
    .bind(&origins)
    .bind(&widgets)
    .bind(payload.allow_authenticated.unwrap_or(existing.allow_authenticated))
    .bind(allow_message_preview)
    .bind(public_channel_id)
    .bind(revoked_at)
    .bind(token_version)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to update embed app");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?;
    state
        .record_audit(
            Some(current.user().id),
            "embed_app.update",
            Some(app.community_id),
            Some(("embed_app", app.id)),
        )
        .await;
    Ok(Json(app.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/embed-apps/{app_id}",
    tag = "embed",
    security(("bearer_auth" = [])),
    params(("app_id" = Uuid, Path, description = "Embed application identifier")),
    responses((status = 204, description = "Embed application revoked"))
)]
pub async fn revoke_app(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(app_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let existing = load_app_by_id(&state, app_id).await?;
    let access = state
        .communities
        .access(existing.community_id, current.user().id)
        .await?;
    require_permission(&access, Permissions::MANAGE_COMMUNITY)?;
    sqlx::query(
        "UPDATE embed_apps SET revoked_at = now(), token_version = token_version + 1, updated_at = now()
         WHERE id = $1",
    )
    .bind(app_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to revoke embed app");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?;
    state
        .record_audit(
            Some(current.user().id),
            "embed_app.revoke",
            Some(existing.community_id),
            Some(("embed_app", app_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/embed/authorize",
    tag = "embed",
    security(("bearer_auth" = [])),
    request_body = EmbedAuthorizeRequest,
    responses((status = 200, body = EmbedAuthorizeResponse))
)]
pub async fn authorize(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(payload): Json<EmbedAuthorizeRequest>,
) -> Result<Json<EmbedAuthorizeResponse>, ApiError> {
    enforce_rate_limit(
        &state,
        &format!("embed:authorize:{}", current.user().id),
        60,
        std::time::Duration::from_secs(3600),
    )
    .await?;
    let origin = normalize_origin(&payload.origin)?;
    let app = load_app_by_key(&state, payload.public_key.trim()).await?;
    ensure_active(&app)?;
    ensure_origin(&app, &origin)?;
    ensure_widget(&app, payload.widget.trim())?;
    if !app.allow_authenticated {
        return Err(ApiError::forbidden(
            "embed_auth_disabled",
            "Authenticated widgets are disabled for this embed application.",
        ));
    }
    state
        .communities
        .access(app.community_id, current.user().id)
        .await
        .map_err(|_| {
            ApiError::forbidden(
                "not_a_member",
                "You must be a member of this community to authorize the embed.",
            )
        })?;
    let code = random_token(32);
    let hash = token_hash(&code);
    sqlx::query(
        "INSERT INTO embed_authorization_codes
            (id, embed_app_id, user_id, code_hash, origin, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(app.id)
    .bind(current.user().id)
    .bind(hash.to_vec())
    .bind(&origin)
    .bind(Utc::now() + Duration::seconds(EMBED_CODE_TTL_SECONDS))
    .execute(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to create embed authorization code");
        ApiError::dependency_unavailable("Embed authorization is temporarily unavailable.")
    })?;
    Ok(Json(EmbedAuthorizeResponse {
        code,
        expires_in: EMBED_CODE_TTL_SECONDS,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/embed/token",
    tag = "embed",
    request_body = EmbedTokenRequest,
    responses((status = 200, body = EmbedTokenResponse))
)]
pub async fn token(
    State(state): State<AppState>,
    ctx: RequestContext,
    Json(payload): Json<EmbedTokenRequest>,
) -> Result<Json<EmbedTokenResponse>, ApiError> {
    enforce_rate_limit(
        &state,
        &format!("embed:token:{}", request_ip_key(&ctx.meta)),
        120,
        std::time::Duration::from_secs(3600),
    )
    .await?;
    let origin = normalize_origin(&payload.origin)?;
    let widget = payload.widget.trim().to_string();
    let app = load_app_by_key(&state, payload.public_key.trim()).await?;
    ensure_active(&app)?;
    ensure_origin(&app, &origin)?;
    ensure_widget(&app, &widget)?;

    let code = payload
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let user_id = if let Some(code) = code {
        if !app.allow_authenticated {
            return Err(ApiError::forbidden(
                "embed_auth_disabled",
                "Authenticated widgets are disabled for this embed application.",
            ));
        }
        let hash = token_hash(code);
        let row: Option<(Uuid,)> = sqlx::query_as(
            "UPDATE embed_authorization_codes
             SET consumed_at = now()
             WHERE code_hash = $1 AND embed_app_id = $2 AND origin = $3
               AND consumed_at IS NULL AND expires_at > now()
             RETURNING user_id",
        )
        .bind(hash.to_vec())
        .bind(app.id)
        .bind(&origin)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to consume embed authorization code");
            ApiError::dependency_unavailable("Embed authorization is temporarily unavailable.")
        })?;
        let (user_id,) = row.ok_or_else(|| {
            ApiError::unauthorized(
                "invalid_embed_code",
                "The embed authorization code is invalid or expired.",
            )
        })?;
        state
            .communities
            .access(app.community_id, user_id)
            .await
            .map_err(|_| {
                ApiError::forbidden(
                    "not_a_member",
                    "You are no longer a member of this community.",
                )
            })?;
        Some(user_id)
    } else {
        let visibility: Option<(String,)> = sqlx::query_as(
            "SELECT visibility FROM communities WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(app.community_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to load embed community");
            ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
        })?;
        if visibility.map(|value| value.0).as_deref() != Some("public") {
            return Err(ApiError::forbidden(
                "embed_private_community",
                "Public widgets are only available for public communities.",
            ));
        }
        None
    };

    let scope = if user_id.is_some() {
        "member"
    } else {
        "public"
    };
    let token = sign_embed_token(&state, &app, user_id, &origin, &widget, scope)?;
    Ok(Json(EmbedTokenResponse {
        token,
        expires_in: EMBED_TOKEN_TTL_SECONDS,
        widget,
        community_id: app.community_id,
        authenticated: user_id.is_some(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/embed/community",
    tag = "embed",
    security(("bearer_auth" = [])),
    responses((status = 200, body = EmbedCommunityDto))
)]
pub async fn community(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EmbedCommunityDto>, ApiError> {
    let token = bearer_token(&headers).ok_or_else(|| {
        ApiError::unauthorized("missing_embed_token", "An embed token is required.")
    })?;
    let claims = decode_embed_token(&state, token)?;
    let app = load_app_by_id(&state, claims.app).await?;
    ensure_active(&app)?;
    if app.token_version != claims.ver {
        return Err(ApiError::unauthorized(
            "embed_token_revoked",
            "The embed token has been revoked.",
        ));
    }
    ensure_origin(&app, &claims.origin)?;
    ensure_widget(&app, &claims.widget)?;
    if claims.community != app.community_id {
        return Err(ApiError::forbidden(
            "embed_scope_mismatch",
            "The embed token is not valid for this community.",
        ));
    }
    if let Some(user_id) = claims.sub {
        state
            .communities
            .access(claims.community, user_id)
            .await
            .map_err(|_| {
                ApiError::forbidden(
                    "not_a_member",
                    "You are no longer a member of this community.",
                )
            })?;
    }

    let row: Option<(Uuid, String, Option<String>, String, i64, i64)> = sqlx::query_as(
        "SELECT c.id, c.name, c.description, c.visibility,
                (SELECT count(*) FROM community_members m WHERE m.community_id = c.id),
                (SELECT count(*) FROM channels ch
                 WHERE ch.community_id = c.id AND ch.kind = 'voice' AND ch.deleted_at IS NULL)
         FROM communities c
         WHERE c.id = $1 AND c.deleted_at IS NULL",
    )
    .bind(claims.community)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to load embed community");
        ApiError::dependency_unavailable("Embed applications are temporarily unavailable.")
    })?;
    let (id, name, description, visibility, member_count, voice_channel_count) =
        row.ok_or_else(|| ApiError::not_found("Community not found."))?;
    if claims.sub.is_none() && visibility != "public" {
        return Err(ApiError::forbidden(
            "embed_private_community",
            "Public widgets are only available for public communities.",
        ));
    }
    let messages = if app.allow_message_preview {
        match app.public_channel_id {
            Some(channel_id) => sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
                "SELECT m.id, COALESCE(u.display_name, 'system'), m.content, m.created_at
                 FROM messages m
                 LEFT JOIN users u ON u.id = m.author_id
                 WHERE m.channel_id = $1 AND m.deleted_at IS NULL AND m.content IS NOT NULL
                 ORDER BY m.created_at DESC, m.id DESC
                 LIMIT 10",
            )
            .bind(channel_id)
            .fetch_all(&state.db)
            .await
            .map_err(|error| {
                tracing::error!(%error, "failed to load embed message previews");
                ApiError::dependency_unavailable("Embed previews are temporarily unavailable.")
            })?
            .into_iter()
            .map(|(id, author, content, created_at)| EmbedMessageDto {
                id,
                author,
                content,
                created_at,
            })
            .collect(),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    Ok(Json(EmbedCommunityDto {
        id,
        name,
        description,
        member_count,
        voice_channel_count,
        public_url: state
            .config
            .instance
            .public_url
            .trim_end_matches('/')
            .to_string(),
        authenticated: claims.sub.is_some(),
        messages,
    }))
}
