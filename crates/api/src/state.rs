use std::sync::Arc;

use audit::{AuditEvent, AuditService};
use auth::{AuthError, AuthService, EmailSender};
use channels::ChannelService;
use communities::CommunityService;
use config::AppConfig;
use messaging::MessageService;
use realtime::{RateLimiter, RedisStore};
use social::SocialService;
use sqlx::PgPool;
use users::UserService;
use uuid::Uuid;

/// Shared API state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: PgPool,
    pub redis: Option<RedisStore>,
    pub rate_limiter: RateLimiter,
    pub auth: Arc<AuthService>,
    pub users: UserService,
    pub audit: AuditService,
    pub communities: CommunityService,
    pub channels: ChannelService,
    pub social: SocialService,
    pub messages: MessageService,
}

impl AppState {
    pub fn new(
        config: Arc<AppConfig>,
        db: PgPool,
        redis: Option<RedisStore>,
        email: Arc<dyn EmailSender>,
    ) -> Result<Self, AuthError> {
        let auth = Arc::new(AuthService::new(db.clone(), config.clone(), email)?);
        let rate_limiter = RateLimiter::new(redis.clone());
        Ok(Self {
            config,
            users: UserService::new(db.clone()),
            audit: AuditService::new(db.clone()),
            communities: CommunityService::new(db.clone()),
            channels: ChannelService::new(db.clone()),
            social: SocialService::new(db.clone()),
            messages: MessageService::new(db.clone()),
            auth,
            db,
            redis,
            rate_limiter,
        })
    }

    /// Best-effort audit write. A failure is logged but never turns a successful action into a failed response.
    pub async fn record_audit(
        &self,
        actor: Option<Uuid>,
        action: &str,
        community_id: Option<Uuid>,
        target: Option<(&str, Uuid)>,
    ) {
        let mut event = AuditEvent::system(action);
        if let Some(actor) = actor {
            event = event.user(actor);
        }
        if let Some(community_id) = community_id {
            event = event.community(community_id);
        }
        if let Some((target_type, target_id)) = target {
            event = event.target(target_type, target_id);
        }
        if let Err(error) = self.audit.record(event).await {
            tracing::error!(%error, action, "failed to write audit record");
        }
    }
}
