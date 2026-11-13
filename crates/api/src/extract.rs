use std::{net::IpAddr, time::Duration};

use auth::{AuthError, AuthenticatedUser};
use axum::{
    body::Bytes,
    extract::{FromRequest, FromRequestParts, Request},
    http::{
        header::{AUTHORIZATION, COOKIE, ORIGIN, SET_COOKIE, USER_AGENT},
        request::Parts,
        HeaderMap, HeaderValue,
    },
};
use config::AppConfig;
use serde::de::DeserializeOwned;
use utoipa::ToSchema;

use crate::{error::ApiError, state::AppState};

/// Authenticated user extracted from a Bearer access token.
pub struct CurrentUser(pub AuthenticatedUser);

impl CurrentUser {
    pub fn user(&self) -> &users::User {
        &self.0.user
    }

    pub fn session_id(&self) -> uuid::Uuid {
        self.0.session_id
    }

    pub fn auth_time(&self) -> chrono::DateTime<chrono::Utc> {
        self.0.auth_time
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                ApiError::unauthorized("authentication_required", "A Bearer token is required.")
            })?;

        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError::unauthorized("authentication_required", "A Bearer token is required.")
        })?;

        let authenticated = state
            .auth
            .authenticate_access_token(token)
            .await
            .map_err(|error| match error {
                AuthError::InvalidToken | AuthError::InvalidCredentials => ApiError::unauthorized(
                    "invalid_token",
                    "The access token is invalid or expired.",
                ),
                other => other.into(),
            })?;

        Ok(Self(authenticated))
    }
}

/// Optional JSON body. Missing or empty bodies are accepted; invalid JSON is a validation error.
#[derive(ToSchema)]
pub struct OptionalJson<T>(pub Option<T>);

impl<T, S> FromRequest<S> for OptionalJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|_| ApiError::validation("The request body could not be read."))?;
        if bytes.is_empty() {
            return Ok(Self(None));
        }
        serde_json::from_slice::<T>(&bytes)
            .map(|value| Self(Some(value)))
            .map_err(|_| ApiError::validation("Invalid JSON body."))
    }
}

#[derive(Debug, Clone)]
pub struct RequestMeta {
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

/// Request metadata plus a headers snapshot for cookie and Origin checks.
pub struct RequestContext {
    pub headers: HeaderMap,
    pub meta: RequestMeta,
}

impl FromRequestParts<AppState> for RequestContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            headers: parts.headers.clone(),
            meta: request_meta(parts, state.config.server.trust_proxy),
        })
    }
}

pub fn request_meta(parts: &Parts, trust_proxy: bool) -> RequestMeta {
    let forwarded = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse().ok());
    let real_ip = parts
        .headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok());
    let peer = parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|connect| connect.0.ip());

    let ip = if trust_proxy {
        forwarded.or(real_ip).or(peer)
    } else {
        peer.or(forwarded).or(real_ip)
    };

    let user_agent = parts
        .headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(512).collect());

    RequestMeta { ip, user_agent }
}

/// Validate the Origin header for cookie-authenticated endpoints.
pub fn ensure_origin(headers: &HeaderMap, config: &AppConfig) -> Result<(), ApiError> {
    let Some(origin) = headers.get(ORIGIN) else {
        return Ok(());
    };
    let origin = origin.to_str().map_err(|_| {
        ApiError::forbidden("origin_not_allowed", "The request origin is not allowed.")
    })?;
    if origin == "null" {
        return Err(ApiError::forbidden(
            "origin_not_allowed",
            "The request origin is not allowed.",
        ));
    }

    let normalized = origin.trim_end_matches('/');
    let public_origin = config.instance.public_url.trim_end_matches('/');
    let allowed = normalized == public_origin
        || config
            .cors
            .allowed_origins
            .iter()
            .any(|value| value.trim_end_matches('/') == normalized);

    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "origin_not_allowed",
            "The request origin is not allowed.",
        ))
    }
}

pub fn refresh_cookie(headers: &HeaderMap, config: &AppConfig) -> Option<String> {
    let name = refresh_cookie_name(config);
    let header = headers.get(COOKIE)?.to_str().ok()?;
    header.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

pub fn set_refresh_cookie(
    headers: &mut HeaderMap,
    config: &AppConfig,
    token: &str,
) -> Result<(), ApiError> {
    let value = refresh_cookie_value(config, token, config.auth.refresh_token_ttl_seconds);
    let header = HeaderValue::from_str(&value)
        .map_err(|_| ApiError::internal("Could not encode the refresh cookie."))?;
    headers.append(SET_COOKIE, header);
    Ok(())
}

pub fn clear_refresh_cookie(headers: &mut HeaderMap, config: &AppConfig) -> Result<(), ApiError> {
    let value = refresh_cookie_value(config, "", 0);
    let header = HeaderValue::from_str(&value)
        .map_err(|_| ApiError::internal("Could not encode the refresh cookie."))?;
    headers.append(SET_COOKIE, header);
    Ok(())
}

fn refresh_cookie_name(config: &AppConfig) -> &'static str {
    if config.auth.cookie_secure {
        "__Secure-tulpar_refresh"
    } else {
        "tulpar_refresh"
    }
}

fn refresh_cookie_value(config: &AppConfig, token: &str, max_age: i64) -> String {
    let name = refresh_cookie_name(config);
    let mut value =
        format!("{name}={token}; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    if config.auth.cookie_secure {
        value.push_str("; Secure");
    }
    if let Some(domain) = &config.auth.cookie_domain {
        value.push_str("; Domain=");
        value.push_str(domain);
    }
    value
}

pub async fn enforce_rate_limit(
    state: &AppState,
    key: &str,
    limit: u64,
    window: Duration,
) -> Result<(), ApiError> {
    let decision = state.rate_limiter.check(key, limit, window).await;
    if decision.allowed {
        Ok(())
    } else {
        Err(ApiError::rate_limited(decision.retry_after))
    }
}

pub fn request_ip_key(meta: &RequestMeta) -> String {
    meta.ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
