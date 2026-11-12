//! Redis-backed ephemeral state, rate limiting, and real-time coordination.

#![forbid(unsafe_code)]

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use redis::aio::ConnectionManager;
use thiserror::Error;
use tokio::sync::Mutex;

/// Errors returned by the realtime layer.
#[derive(Debug, Error)]
pub enum RealtimeError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
}

/// A lazy Redis connection that recovers when Redis comes back.
#[derive(Clone)]
pub struct RedisStore {
    client: redis::Client,
    connection: Arc<Mutex<Option<ConnectionManager>>>,
}

impl RedisStore {
    /// Create a store. A failed initial connection is tolerated and retried on demand.
    pub fn new(url: &str) -> Result<Self, RealtimeError> {
        let client = redis::Client::open(url)?;
        Ok(Self {
            client,
            connection: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn connection(&self) -> Result<ConnectionManager, RealtimeError> {
        let mut guard = self.connection.lock().await;
        if let Some(connection) = guard.as_ref() {
            return Ok(connection.clone());
        }
        let connection = self.client.get_connection_manager().await?;
        *guard = Some(connection.clone());
        Ok(connection)
    }

    pub async fn health(&self) -> Result<(), RealtimeError> {
        let mut connection = self.connection().await?;
        let _: String = redis::cmd("PING").query_async(&mut connection).await?;
        Ok(())
    }

    pub async fn set_ex(&self, key: &str, value: &str, ttl: Duration) -> Result<(), RealtimeError> {
        let mut connection = self.connection().await?;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("PX")
            .arg(ttl.as_millis() as u64)
            .query_async::<()>(&mut connection)
            .await?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, RealtimeError> {
        let mut connection = self.connection().await?;
        let value: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut connection)
            .await?;
        Ok(value)
    }

    pub async fn delete(&self, key: &str) -> Result<(), RealtimeError> {
        let mut connection = self.connection().await?;
        redis::cmd("DEL")
            .arg(key)
            .query_async::<()>(&mut connection)
            .await?;
        Ok(())
    }
}

/// Rate-limit decision returned to callers.
#[derive(Debug, Clone)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub remaining: u64,
    pub retry_after: Duration,
    pub backend: &'static str,
}

impl RateLimitDecision {
    pub fn limit_headers(&self, limit: u64) -> Vec<(&'static str, String)> {
        vec![
            ("X-RateLimit-Limit", limit.to_string()),
            ("X-RateLimit-Remaining", self.remaining.to_string()),
        ]
    }
}

struct MemoryWindow {
    count: u64,
    reset_at: Instant,
}

/// Fixed-window rate limiter with Redis and in-process fallback.
#[derive(Clone)]
pub struct RateLimiter {
    redis: Option<RedisStore>,
    memory: Arc<Mutex<HashMap<String, MemoryWindow>>>,
}

impl RateLimiter {
    pub fn new(redis: Option<RedisStore>) -> Self {
        Self {
            redis,
            memory: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn memory_only() -> Self {
        Self::new(None)
    }

    pub async fn check(&self, key: &str, limit: u64, window: Duration) -> RateLimitDecision {
        if limit == 0 {
            return RateLimitDecision {
                allowed: false,
                remaining: 0,
                retry_after: window,
                backend: "disabled",
            };
        }

        if let Some(redis) = &self.redis {
            match redis_check(redis, key, limit, window).await {
                Ok(decision) => return decision,
                Err(error) => {
                    tracing::warn!(%error, "redis rate limiter unavailable; using process-local fallback");
                }
            }
        }

        memory_check(&self.memory, key, limit, window).await
    }
}

async fn redis_check(
    redis: &RedisStore,
    key: &str,
    limit: u64,
    window: Duration,
) -> Result<RateLimitDecision, RealtimeError> {
    let redis_key = format!("tulpar:rl:{key}");
    let mut connection = redis.connection().await?;
    let count: i64 = redis::cmd("INCR")
        .arg(&redis_key)
        .query_async(&mut connection)
        .await?;

    if count == 1 {
        let _: () = redis::cmd("PEXPIRE")
            .arg(&redis_key)
            .arg(window.as_millis() as u64)
            .query_async(&mut connection)
            .await?;
    }

    let ttl: i64 = redis::cmd("PTTL")
        .arg(&redis_key)
        .query_async(&mut connection)
        .await?;
    let retry_after = if ttl > 0 {
        Duration::from_millis(ttl as u64)
    } else {
        window
    };

    let allowed = count as u64 <= limit;
    Ok(RateLimitDecision {
        allowed,
        remaining: limit.saturating_sub(count.max(0) as u64),
        retry_after,
        backend: "redis",
    })
}

async fn memory_check(
    memory: &Arc<Mutex<HashMap<String, MemoryWindow>>>,
    key: &str,
    limit: u64,
    window: Duration,
) -> RateLimitDecision {
    let now = Instant::now();
    let mut guard = memory.lock().await;

    if guard.len() > 10_000 {
        guard.retain(|_, entry| entry.reset_at > now);
    }

    let entry = guard.entry(key.to_string()).or_insert(MemoryWindow {
        count: 0,
        reset_at: now + window,
    });

    if entry.reset_at <= now {
        entry.count = 0;
        entry.reset_at = now + window;
    }

    entry.count = entry.count.saturating_add(1);
    let allowed = entry.count <= limit;
    let remaining = limit.saturating_sub(entry.count);
    let retry_after = entry.reset_at.saturating_duration_since(now);

    RateLimitDecision {
        allowed,
        remaining,
        retry_after,
        backend: "memory",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_limiter_enforces_limit() {
        let limiter = RateLimiter::memory_only();
        let window = Duration::from_secs(60);

        assert!(limiter.check("login:test", 2, window).await.allowed);
        assert!(limiter.check("login:test", 2, window).await.allowed);
        let denied = limiter.check("login:test", 2, window).await;
        assert!(!denied.allowed);
        assert_eq!(denied.remaining, 0);
    }

    #[tokio::test]
    async fn memory_limiter_isolates_keys() {
        let limiter = RateLimiter::memory_only();
        let window = Duration::from_secs(60);

        assert!(limiter.check("a", 1, window).await.allowed);
        assert!(limiter.check("b", 1, window).await.allowed);
        assert!(!limiter.check("a", 1, window).await.allowed);
    }
}
