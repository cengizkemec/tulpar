//! Minimal Rust client for the Tulpar HTTP API.
//!
//! The SDK intentionally exposes typed auth/embed responses and JSON values for the
//! fast-moving messaging/sync surface. Callers can layer their own DTOs on top.

#![forbid(unsafe_code)]

use reqwest::{Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("API error {status} ({code}): {message}")]
    Api {
        status: StatusCode,
        code: String,
        message: String,
    },
    #[error("authentication token is required")]
    MissingToken,
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub session_id: Uuid,
    pub user: UserSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedTokenRequest {
    pub public_key: String,
    pub origin: String,
    pub widget: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbedTokenResponse {
    pub token: String,
    pub expires_in: i64,
    pub widget: String,
    pub community_id: Uuid,
    pub authenticated: bool,
}

#[derive(Debug, Deserialize)]
struct ProblemDetails {
    code: Option<String>,
    detail: Option<String>,
    title: Option<String>,
}

#[derive(Clone)]
pub struct TulparClient {
    base_url: String,
    http: Client,
    token: Option<String>,
}

impl TulparClient {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, SdkError> {
        let trimmed = base_url.as_ref().trim().trim_end_matches('/');
        let parsed =
            Url::parse(trimmed).map_err(|_| SdkError::InvalidBaseUrl(trimmed.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(SdkError::InvalidBaseUrl(trimmed.to_string()));
        }
        Ok(Self {
            base_url: trimmed.to_string(),
            http: Client::builder()
                .user_agent("Tulpar-Rust-SDK/0.1")
                .build()
                .map_err(SdkError::Request)?,
            token: None,
        })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn set_token(&mut self, token: impl Into<String>) {
        self.token = Some(token.into());
    }

    pub fn clear_token(&mut self) {
        self.token = None;
    }

    pub async fn login(
        &self,
        login: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<LoginResponse, SdkError> {
        let response = self
            .http
            .post(format!("{}/api/v1/auth/login", self.base_url))
            .json(&serde_json::json!({
                "login": login.into(),
                "password": password.into(),
            }))
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn sync(&self) -> Result<Value, SdkError> {
        let response = self.authorized(Method::GET, "/api/v1/sync")?.send().await?;
        self.parse(response).await
    }

    pub async fn send_message(
        &self,
        channel_id: Uuid,
        content: impl Into<String>,
    ) -> Result<Value, SdkError> {
        let response = self
            .authorized(
                Method::POST,
                &format!("/api/v1/channels/{channel_id}/messages"),
            )?
            .json(&serde_json::json!({ "content": content.into() }))
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn embed_token(
        &self,
        request: &EmbedTokenRequest,
    ) -> Result<EmbedTokenResponse, SdkError> {
        let response = self
            .http
            .post(format!("{}/api/v1/embed/token", self.base_url))
            .json(request)
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn embed_community(&self, token: &str) -> Result<Value, SdkError> {
        let response = self
            .http
            .get(format!("{}/api/v1/embed/community", self.base_url))
            .bearer_auth(token)
            .send()
            .await?;
        self.parse(response).await
    }

    fn authorized(&self, method: Method, path: &str) -> Result<reqwest::RequestBuilder, SdkError> {
        let token = self.token.as_deref().ok_or(SdkError::MissingToken)?;
        Ok(self
            .http
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(token))
    }

    async fn parse<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<T, SdkError> {
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            let problem = serde_json::from_str::<ProblemDetails>(&text).ok();
            return Err(SdkError::Api {
                status,
                code: problem
                    .as_ref()
                    .and_then(|value| value.code.clone())
                    .unwrap_or_else(|| "request_failed".to_string()),
                message: problem
                    .and_then(|value| value.detail.or(value.title))
                    .unwrap_or_else(|| text.clone()),
            });
        }
        serde_json::from_str(&text).map_err(|error| SdkError::InvalidResponse(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https_base_urls() {
        assert!(TulparClient::new("https://tulpar.example.com/").is_ok());
        assert!(TulparClient::new("http://localhost:8080").is_ok());
    }

    #[test]
    fn rejects_invalid_base_urls() {
        assert!(TulparClient::new("not a url").is_err());
        assert!(TulparClient::new("ftp://tulpar.example.com").is_err());
    }

    #[test]
    fn authorized_requires_a_token() {
        let client = TulparClient::new("https://tulpar.example.com").unwrap();
        assert!(matches!(
            client.authorized(Method::GET, "/api/v1/sync"),
            Err(SdkError::MissingToken)
        ));
    }
}
