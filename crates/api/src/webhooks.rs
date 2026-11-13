use std::{net::IpAddr, time::Duration};

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::{FromRow, PgPool};
use url::Url;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, FromRow)]
pub struct WebhookRow {
    pub id: Uuid,
    pub community_id: Uuid,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, FromRow)]
struct DeliveryRow {
    id: Uuid,
    webhook_id: Uuid,
    event_id: Uuid,
    event_type: String,
    payload: Value,
    attempts: i32,
}

#[derive(Clone)]
pub struct WebhookService {
    pool: PgPool,
    http: Client,
    allow_localhost: bool,
}

impl WebhookService {
    pub fn new(pool: PgPool, allow_localhost: bool) -> Self {
        Self {
            pool,
            http: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("Tulpar-Webhook/0.1")
                .build()
                .expect("webhook HTTP client"),
            allow_localhost,
        }
    }

    pub async fn list(&self, community_id: Uuid) -> Result<Vec<WebhookRow>, sqlx::Error> {
        sqlx::query_as::<_, WebhookRow>(
            "SELECT id, community_id, url, secret, events, active, created_at,
                    last_delivery_at, last_error
             FROM webhooks WHERE community_id = $1 ORDER BY created_at DESC",
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get(&self, webhook_id: Uuid) -> Result<Option<WebhookRow>, sqlx::Error> {
        sqlx::query_as::<_, WebhookRow>(
            "SELECT id, community_id, url, secret, events, active, created_at,
                    last_delivery_at, last_error
             FROM webhooks WHERE id = $1",
        )
        .bind(webhook_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create(
        &self,
        community_id: Uuid,
        url: &str,
        secret: &str,
        events: &[String],
        created_by: Uuid,
    ) -> Result<WebhookRow, sqlx::Error> {
        sqlx::query_as::<_, WebhookRow>(
            "INSERT INTO webhooks (id, community_id, url, secret, events, created_by)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, community_id, url, secret, events, active, created_at,
                       last_delivery_at, last_error",
        )
        .bind(Uuid::now_v7())
        .bind(community_id)
        .bind(url)
        .bind(secret)
        .bind(events)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn delete(&self, community_id: Uuid, webhook_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM webhooks WHERE id = $1 AND community_id = $2")
            .bind(webhook_id)
            .bind(community_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn enqueue_message(
        &self,
        community_id: Uuid,
        payload: Value,
    ) -> Result<usize, sqlx::Error> {
        let hooks: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM webhooks
             WHERE community_id = $1 AND active AND 'message.created' = ANY(events)",
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        let mut queued = 0;
        for (webhook_id,) in hooks {
            sqlx::query(
                "INSERT INTO webhook_deliveries
                    (id, webhook_id, event_id, event_type, payload)
                 VALUES ($1, $2, $3, 'message.created', $4)",
            )
            .bind(Uuid::now_v7())
            .bind(webhook_id)
            .bind(Uuid::now_v7())
            .bind(&payload)
            .execute(&self.pool)
            .await?;
            queued += 1;
        }
        Ok(queued)
    }

    pub async fn dispatch_once(&self, limit: i64) -> usize {
        let deliveries = match sqlx::query_as::<_, DeliveryRow>(
            "SELECT d.id, d.webhook_id, d.event_id, d.event_type, d.payload, d.attempts
             FROM webhook_deliveries d
             JOIN webhooks w ON w.id = d.webhook_id
             WHERE d.delivered_at IS NULL AND d.next_attempt_at <= now() AND w.active
             ORDER BY d.next_attempt_at
             LIMIT $1",
        )
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await
        {
            Ok(deliveries) => deliveries,
            Err(error) => {
                tracing::error!(%error, "webhook delivery lookup failed");
                return 0;
            }
        };

        let mut delivered = 0;
        for delivery in deliveries {
            let webhook = match sqlx::query_as::<_, WebhookRow>(
                "SELECT id, community_id, url, secret, events, active, created_at,
                        last_delivery_at, last_error
                 FROM webhooks WHERE id = $1",
            )
            .bind(delivery.webhook_id)
            .fetch_optional(&self.pool)
            .await
            {
                Ok(Some(webhook)) => webhook,
                Ok(None) => {
                    let _ = self.mark_delivered(delivery.id).await;
                    continue;
                }
                Err(error) => {
                    tracing::error!(%error, "webhook lookup failed");
                    continue;
                }
            };

            let body = json!({
                "id": delivery.event_id,
                "type": delivery.event_type,
                "created_at": Utc::now(),
                "data": delivery.payload,
            })
            .to_string();
            if let Err(error) = self.validate_url(&webhook.url).await {
                self.mark_failed(delivery.id, webhook.id, delivery.attempts, &error)
                    .await;
                continue;
            }
            let timestamp = Utc::now().timestamp();
            let signature = sign(&webhook.secret, timestamp, &body);
            let response = self
                .http
                .post(&webhook.url)
                .header("content-type", "application/json")
                .header("x-tulpar-event", &delivery.event_type)
                .header("x-tulpar-delivery", delivery.event_id.to_string())
                .header("x-tulpar-timestamp", timestamp.to_string())
                .header("x-tulpar-signature", format!("sha256={signature}"))
                .body(body)
                .send()
                .await;

            match response {
                Ok(response) if response.status().is_success() => {
                    if self.mark_delivered(delivery.id).await.is_ok() {
                        let _ = sqlx::query(
                            "UPDATE webhooks SET last_delivery_at = now(), last_error = NULL
                             WHERE id = $1",
                        )
                        .bind(webhook.id)
                        .execute(&self.pool)
                        .await;
                        delivered += 1;
                    }
                }
                Ok(response) => {
                    self.mark_failed(
                        delivery.id,
                        webhook.id,
                        delivery.attempts,
                        &format!("HTTP {}", response.status()),
                    )
                    .await;
                }
                Err(error) => {
                    self.mark_failed(
                        delivery.id,
                        webhook.id,
                        delivery.attempts,
                        &error.to_string(),
                    )
                    .await;
                }
            }
        }
        delivered
    }

    async fn mark_delivered(&self, delivery_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE webhook_deliveries SET delivered_at = now() WHERE id = $1")
            .bind(delivery_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_failed(&self, delivery_id: Uuid, webhook_id: Uuid, attempts: i32, error: &str) {
        let backoff = (30_i64 * 2_i64.pow(attempts.clamp(0, 7) as u32)).min(3600);
        let _ = sqlx::query(
            "UPDATE webhook_deliveries
             SET attempts = attempts + 1, last_error = $2,
                 next_attempt_at = now() + make_interval(secs => $3)
             WHERE id = $1",
        )
        .bind(delivery_id)
        .bind(error)
        .bind(backoff)
        .execute(&self.pool)
        .await;
        let _ = sqlx::query("UPDATE webhooks SET last_error = $2 WHERE id = $1")
            .bind(webhook_id)
            .bind(error)
            .execute(&self.pool)
            .await;
    }

    pub async fn validate_url(&self, input: &str) -> Result<String, String> {
        let url = Url::parse(input.trim()).map_err(|_| "Enter a valid webhook URL.".to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("Webhook URLs must use http or https.".to_string());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("Webhook URLs cannot contain credentials.".to_string());
        }
        let host = url
            .host_str()
            .ok_or_else(|| "The webhook URL must include a host.".to_string())?;
        let localhost = matches!(host, "localhost" | "127.0.0.1" | "::1");
        if url.scheme() == "http" && (!localhost || !self.allow_localhost) {
            return Err(
                "Plain HTTP webhook URLs are only allowed for localhost development.".to_string(),
            );
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            check_ip(ip, self.allow_localhost)?;
        } else if !localhost {
            let port = url.port_or_known_default().unwrap_or(443);
            let addresses = tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| "The webhook host could not be resolved.".to_string())?;
            for address in addresses {
                check_ip(address.ip(), false)?;
            }
        }
        Ok(url.to_string())
    }
}

fn check_ip(ip: IpAddr, allow_loopback: bool) -> Result<(), String> {
    if ip.is_loopback() {
        return if allow_loopback {
            Ok(())
        } else {
            Err("Loopback webhook addresses are not allowed.".to_string())
        };
    }
    let special = match ip {
        IpAddr::V4(value) => {
            value.is_private()
                || value.is_link_local()
                || value.is_broadcast()
                || value.is_unspecified()
                || value.is_multicast()
        }
        IpAddr::V6(value) => {
            value.is_unique_local()
                || value.is_unicast_link_local()
                || value.is_unspecified()
                || value.is_multicast()
        }
    };
    if special {
        return Err("Private or special-purpose webhook addresses are not allowed.".to_string());
    }
    Ok(())
}

fn sign(secret: &str, timestamp: i64, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key");
    mac.update(format!("{timestamp}.{body}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
