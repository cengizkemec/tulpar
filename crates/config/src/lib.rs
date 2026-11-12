//! Configuration loading and validation.

#![forbid(unsafe_code)]

use std::path::Path;

use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors returned while loading or validating configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration could not be loaded: {0}")]
    Load(String),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

/// Complete runtime configuration.
#[derive(Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub instance: InstanceConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    pub cors: CorsConfig,
    pub log: LogConfig,
    pub email: EmailConfig,
    pub storage: StorageConfig,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct InstanceConfig {
    pub name: String,
    pub public_url: String,
    pub environment: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind: String,
    pub request_timeout_seconds: u64,
    pub body_limit_bytes: usize,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_seconds: u64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_kid: String,
    pub audience: String,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_seconds: i64,
    pub session_absolute_ttl_seconds: i64,
    pub cookie_secure: bool,
    pub cookie_domain: Option<String>,
    pub registration_enabled: bool,
    pub require_email_verification: bool,
    pub max_sessions_per_user: i64,
    pub password_min_length: usize,
    pub password_max_length: usize,
    pub email_verification_ttl_seconds: i64,
    pub password_reset_ttl_seconds: i64,
    pub reauth_window_seconds: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct LogConfig {
    pub level: String,
    pub format: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct EmailConfig {
    pub driver: String,
    pub from: String,
    pub smtp_url: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub provider: String,
    pub root: String,
    pub max_upload_bytes: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            instance: InstanceConfig {
                name: "Tulpar".to_string(),
                public_url: "http://localhost:8080".to_string(),
                environment: "development".to_string(),
            },
            server: ServerConfig {
                bind: "0.0.0.0:8080".to_string(),
                request_timeout_seconds: 30,
                body_limit_bytes: 2 * 1024 * 1024,
            },
            database: DatabaseConfig {
                url: String::new(),
                max_connections: 10,
                min_connections: 1,
                acquire_timeout_seconds: 5,
            },
            redis: RedisConfig {
                url: "redis://127.0.0.1:6379".to_string(),
            },
            auth: AuthConfig {
                jwt_secret: String::new(),
                jwt_kid: "dev".to_string(),
                audience: "tulpar-api".to_string(),
                access_token_ttl_seconds: 900,
                refresh_token_ttl_seconds: 2_592_000,
                session_absolute_ttl_seconds: 2_592_000,
                cookie_secure: true,
                cookie_domain: None,
                registration_enabled: true,
                require_email_verification: true,
                max_sessions_per_user: 20,
                password_min_length: 12,
                password_max_length: 128,
                email_verification_ttl_seconds: 86_400,
                password_reset_ttl_seconds: 3_600,
                reauth_window_seconds: 300,
            },
            cors: CorsConfig {
                allowed_origins: vec!["http://localhost:5173".to_string()],
            },
            log: LogConfig {
                level: "info".to_string(),
                format: "pretty".to_string(),
            },
            email: EmailConfig {
                driver: "log".to_string(),
                from: "Tulpar <noreply@localhost>".to_string(),
                smtp_url: String::new(),
            },
            storage: StorageConfig {
                provider: "local".to_string(),
                root: "data/uploads".to_string(),
                max_upload_bytes: 25 * 1024 * 1024,
            },
        }
    }
}

impl AppConfig {
    /// Load configuration from `tulpar.toml` and `TULPAR__*` environment variables.
    pub fn load() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let config_path =
            std::env::var("TULPAR_CONFIG").unwrap_or_else(|_| "tulpar.toml".to_string());
        let mut figment = Figment::from(Serialized::defaults(Self::default()));
        if Path::new(&config_path).exists() {
            figment = figment.merge(Toml::file(config_path));
        }
        figment = figment.merge(Env::prefixed("TULPAR__").split("__"));

        let config: Self = figment
            .extract()
            .map_err(|error| ConfigError::Load(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Validate values that cannot be safely defaulted.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.instance.name.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "instance.name must not be empty".into(),
            ));
        }

        let public_url = Url::parse(&self.instance.public_url)
            .map_err(|_| ConfigError::Invalid("instance.public_url must be a valid URL".into()))?;
        if !matches!(public_url.scheme(), "http" | "https") {
            return Err(ConfigError::Invalid(
                "instance.public_url must use http or https".into(),
            ));
        }
        if self.is_production() && public_url.scheme() != "https" {
            return Err(ConfigError::Invalid(
                "instance.public_url must use https in production".into(),
            ));
        }

        if !self.database.url.starts_with("postgres://")
            && !self.database.url.starts_with("postgresql://")
        {
            return Err(ConfigError::Invalid(
                "database.url must be a PostgreSQL connection URL".into(),
            ));
        }
        if self.database.max_connections == 0
            || self.database.min_connections > self.database.max_connections
        {
            return Err(ConfigError::Invalid(
                "database connection pool settings are inconsistent".into(),
            ));
        }

        if self.auth.jwt_secret.len() < 32 {
            return Err(ConfigError::Invalid(
                "auth.jwt_secret must be at least 32 bytes".into(),
            ));
        }
        if self.auth.access_token_ttl_seconds <= 0
            || self.auth.refresh_token_ttl_seconds <= 0
            || self.auth.session_absolute_ttl_seconds <= 0
        {
            return Err(ConfigError::Invalid(
                "auth token lifetimes must be positive".into(),
            ));
        }
        if self.auth.password_min_length < 8
            || self.auth.password_max_length < self.auth.password_min_length
        {
            return Err(ConfigError::Invalid(
                "auth password length bounds are invalid".into(),
            ));
        }

        for origin in &self.cors.allowed_origins {
            let parsed = Url::parse(origin)
                .map_err(|_| ConfigError::Invalid(format!("invalid CORS origin: {origin}")))?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(ConfigError::Invalid(format!(
                    "invalid CORS origin: {origin}"
                )));
            }
        }

        if self.is_production() && self.email.driver != "smtp" {
            return Err(ConfigError::Invalid(
                "production requires email.driver=smtp".into(),
            ));
        }
        if self.email.driver == "smtp" && self.email.smtp_url.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "email.smtp_url is required when email.driver=smtp".into(),
            ));
        }
        if self.email.driver != "log" && self.email.driver != "smtp" {
            return Err(ConfigError::Invalid(format!(
                "unsupported email.driver={}",
                self.email.driver
            )));
        }

        if self.storage.provider != "local" {
            return Err(ConfigError::Invalid(
                "storage.provider must be local in this release".into(),
            ));
        }
        if self.storage.root.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "storage.root must not be empty".into(),
            ));
        }
        if self.storage.max_upload_bytes == 0 {
            return Err(ConfigError::Invalid(
                "storage.max_upload_bytes must be positive".into(),
            ));
        }

        Ok(())
    }

    pub fn is_production(&self) -> bool {
        self.instance.environment.eq_ignore_ascii_case("production")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> AppConfig {
        let mut config = AppConfig::default();
        config.instance.environment = "test".into();
        config.instance.public_url = "http://localhost:8080".into();
        config.database.url = "postgres://tulpar:tulpar@localhost/tulpar".into();
        config.auth.jwt_secret = "x".repeat(32);
        config
    }

    #[test]
    fn valid_config_passes() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn production_requires_https_and_real_email() {
        let mut config = valid_config();
        config.instance.environment = "production".into();
        assert!(config.validate().is_err());
        config.instance.public_url = "https://tulpar.example.com".into();
        assert!(config.validate().is_err());
        config.email.driver = "smtp".into();
        config.email.smtp_url = "smtp://localhost:1025".into();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_short_jwt_secret() {
        let mut config = valid_config();
        config.auth.jwt_secret = "short".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_cors_origin() {
        let mut config = valid_config();
        config.cors.allowed_origins = vec!["javascript:alert(1)".into()];
        assert!(config.validate().is_err());
    }
}
