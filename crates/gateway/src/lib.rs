//! WebSocket gateway coordination, one-time tickets, and presence aggregation.

#![forbid(unsafe_code)]

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use common::{random_token, token_hash};
use realtime::RedisStore;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
pub const HEARTBEAT_INTERVAL_MS: u64 = 30_000;
pub const TICKET_TTL_SECONDS: u64 = 60;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("redis error: {0}")]
    Redis(#[from] realtime::RealtimeError),
    #[error("invalid presence status: {0}")]
    InvalidStatus(String),
}

#[derive(Debug, Clone)]
pub struct GatewayEvent {
    pub event: String,
    pub community_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub target_user_id: Option<Uuid>,
    pub payload: Value,
}

impl GatewayEvent {
    pub fn channel(
        event: impl Into<String>,
        community_id: Option<Uuid>,
        channel_id: Uuid,
        payload: Value,
    ) -> Self {
        Self {
            event: event.into(),
            community_id,
            channel_id: Some(channel_id),
            target_user_id: None,
            payload,
        }
    }

    pub fn user(event: impl Into<String>, target_user_id: Uuid, payload: Value) -> Self {
        Self {
            event: event.into(),
            community_id: None,
            channel_id: None,
            target_user_id: Some(target_user_id),
            payload,
        }
    }

    pub fn global(event: impl Into<String>, payload: Value) -> Self {
        Self {
            event: event.into(),
            community_id: None,
            channel_id: None,
            target_user_id: None,
            payload,
        }
    }
}

#[derive(Debug, Clone)]
struct PresenceSession {
    status: String,
}

#[derive(Debug, Clone)]
struct UserPresence {
    sessions: HashMap<Uuid, PresenceSession>,
    updated_at: DateTime<Utc>,
}

impl UserPresence {
    fn aggregate_status(&self) -> String {
        let statuses = self
            .sessions
            .values()
            .map(|session| session.status.as_str());
        let mut visible_online = false;
        let mut visible_idle = false;
        let mut visible_dnd = false;
        let mut invisible = false;
        for status in statuses {
            match status {
                "dnd" => visible_dnd = true,
                "online" => visible_online = true,
                "idle" => visible_idle = true,
                "invisible" => invisible = true,
                _ => {}
            }
        }
        if visible_dnd {
            "dnd".to_string()
        } else if visible_online {
            "online".to_string()
        } else if visible_idle {
            "idle".to_string()
        } else if invisible {
            "invisible".to_string()
        } else {
            "offline".to_string()
        }
    }
}

#[derive(Clone)]
pub struct GatewayHub {
    sender: broadcast::Sender<GatewayEvent>,
    presence: Arc<Mutex<HashMap<Uuid, UserPresence>>>,
}

impl Default for GatewayHub {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl GatewayHub {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(16));
        Self {
            sender,
            presence: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: GatewayEvent) {
        let _ = self.sender.send(event);
    }

    pub async fn connect(&self, user_id: Uuid, preferred_status: &str) -> Uuid {
        let connection_id = Uuid::now_v7();
        let status = normalize_status(preferred_status).unwrap_or("online");
        let mut presence = self.presence.lock().await;
        let entry = presence.entry(user_id).or_insert_with(|| UserPresence {
            sessions: HashMap::new(),
            updated_at: Utc::now(),
        });
        entry.sessions.insert(
            connection_id,
            PresenceSession {
                status: status.to_string(),
            },
        );
        entry.updated_at = Utc::now();
        connection_id
    }

    pub async fn update_status(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        status: &str,
    ) -> Result<(), GatewayError> {
        let normalized = normalize_status(status)
            .ok_or_else(|| GatewayError::InvalidStatus(status.to_string()))?;
        let mut presence = self.presence.lock().await;
        if let Some(entry) = presence.get_mut(&user_id) {
            if let Some(session) = entry.sessions.get_mut(&connection_id) {
                session.status = normalized.to_string();
                entry.updated_at = Utc::now();
            }
        }
        Ok(())
    }

    pub async fn disconnect(&self, user_id: Uuid, connection_id: Uuid) -> String {
        let mut presence = self.presence.lock().await;
        let Some(entry) = presence.get_mut(&user_id) else {
            return "offline".to_string();
        };
        entry.sessions.remove(&connection_id);
        entry.updated_at = Utc::now();
        if entry.sessions.is_empty() {
            presence.remove(&user_id);
            "offline".to_string()
        } else {
            entry.aggregate_status()
        }
    }

    pub async fn status(&self, user_id: Uuid) -> String {
        let presence = self.presence.lock().await;
        presence
            .get(&user_id)
            .map(UserPresence::aggregate_status)
            .unwrap_or_else(|| "offline".to_string())
    }
}

fn normalize_status(status: &str) -> Option<&'static str> {
    match status {
        "online" => Some("online"),
        "idle" => Some("idle"),
        "dnd" => Some("dnd"),
        "invisible" => Some("invisible"),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct TicketRecord {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

struct MemoryTicket {
    record: TicketRecord,
    expires_at: Instant,
}

#[derive(Clone)]
pub struct GatewayTickets {
    redis: Option<RedisStore>,
    memory: Arc<Mutex<HashMap<String, MemoryTicket>>>,
}

impl GatewayTickets {
    pub fn new(redis: Option<RedisStore>) -> Self {
        Self {
            redis,
            memory: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn issue(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> Result<(String, u64), GatewayError> {
        let ticket = random_token(32);
        let key = ticket_key(&ticket);
        let value = format!("{user_id}:{session_id}");
        if let Some(redis) = &self.redis {
            redis
                .set_ex(&key, &value, Duration::from_secs(TICKET_TTL_SECONDS))
                .await
                .map_err(GatewayError::from)?;
        } else {
            let mut memory = self.memory.lock().await;
            prune_memory_tickets(&mut memory);
            memory.insert(
                key,
                MemoryTicket {
                    record: TicketRecord {
                        user_id,
                        session_id,
                    },
                    expires_at: Instant::now() + Duration::from_secs(TICKET_TTL_SECONDS),
                },
            );
        }
        Ok((ticket, TICKET_TTL_SECONDS))
    }

    pub async fn consume(&self, ticket: &str) -> Option<TicketRecord> {
        let key = ticket_key(ticket);
        if let Some(redis) = &self.redis {
            match redis.get_del(&key).await {
                Ok(Some(value)) => return parse_ticket_record(&value),
                Ok(None) => return None,
                Err(error) => {
                    tracing::warn!(%error, "gateway ticket store unavailable");
                    return None;
                }
            }
        }
        let mut memory = self.memory.lock().await;
        prune_memory_tickets(&mut memory);
        memory.remove(&key).map(|ticket| ticket.record)
    }
}

fn ticket_key(ticket: &str) -> String {
    format!("tulpar:gateway:ticket:{}", hex_hash(ticket))
}

fn hex_hash(value: &str) -> String {
    let hash = token_hash(value);
    let mut output = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn parse_ticket_record(value: &str) -> Option<TicketRecord> {
    let (user_id, session_id) = value.split_once(':')?;
    Some(TicketRecord {
        user_id: user_id.parse().ok()?,
        session_id: session_id.parse().ok()?,
    })
}

fn prune_memory_tickets(memory: &mut HashMap<String, MemoryTicket>) {
    let now = Instant::now();
    memory.retain(|_, ticket| ticket.expires_at > now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tickets_are_single_use() {
        let tickets = GatewayTickets::new(None);
        let user_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let (ticket, _) = tickets.issue(user_id, session_id).await.unwrap();
        let record = tickets.consume(&ticket).await.unwrap();
        assert_eq!(record.user_id, user_id);
        assert_eq!(record.session_id, session_id);
        assert!(tickets.consume(&ticket).await.is_none());
    }

    #[tokio::test]
    async fn presence_aggregates_sessions() {
        let hub = GatewayHub::default();
        let user_id = Uuid::now_v7();
        let first = hub.connect(user_id, "online").await;
        assert_eq!(hub.status(user_id).await, "online");
        let second = hub.connect(user_id, "dnd").await;
        assert_eq!(hub.status(user_id).await, "dnd");
        assert_eq!(hub.disconnect(user_id, second).await, "online");
        assert_eq!(hub.disconnect(user_id, first).await, "offline");
        assert_eq!(hub.status(user_id).await, "offline");
    }

    #[tokio::test]
    async fn slow_consumers_observe_a_lag_error() {
        let hub = GatewayHub::new(16);
        let mut receiver = hub.subscribe();
        for sequence in 0..20 {
            hub.publish(GatewayEvent::global("TEST", serde_json::json!(sequence)));
        }
        let mut lagged = false;
        for _ in 0..20 {
            match receiver.recv().await {
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    lagged = true;
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Ok(_) => {}
            }
        }
        assert!(lagged, "slow receivers must observe Lagged");
    }

    #[test]
    fn invalid_presence_is_rejected() {
        assert!(normalize_status("offline").is_none());
        assert!(normalize_status("invisible").is_some());
    }
}
