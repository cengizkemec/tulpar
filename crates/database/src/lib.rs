//! PostgreSQL pool, migrations, and platform helpers.

#![forbid(unsafe_code)]

use std::time::Duration;

use config::DatabaseConfig;
use sqlx::{postgres::PgPoolOptions, PgPool};
use thiserror::Error;

/// Errors returned by the database layer.
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database connection failed: {0}")]
    Connect(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// PostgreSQL connection pool wrapper.
#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Connect to PostgreSQL using validated configuration.
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
            .connect(&config.url)
            .await?;

        tracing::info!(
            max_connections = config.max_connections,
            min_connections = config.min_connections,
            "connected to PostgreSQL"
        );

        Ok(Self { pool })
    }

    /// Apply all embedded migrations. This is safe to run repeatedly.
    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Verify that the database accepts a trivial query.
    pub async fn health(&self) -> Result<(), sqlx::Error> {
        let _: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn into_pool(self) -> PgPool {
        self.pool
    }
}
