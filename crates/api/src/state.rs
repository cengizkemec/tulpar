use std::sync::Arc;

use audit::{AuditEvent, AuditService};
use auth::{AuthError, AuthService, EmailSender, PasswordService};
use channels::ChannelService;
use communities::CommunityService;
use config::AppConfig;
use events::OutboxService;
use gateway::{GatewayHub, GatewayTickets};
use messaging::MessageService;
use notifications::NotificationService;
use realtime::{RateLimiter, RedisStore};
use social::SocialService;
use sqlx::PgPool;
use storage::StorageService;
use users::UserService;
use uuid::Uuid;
use voice::{LiveKitAdmin, VoiceProjection};

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
    pub notifications: NotificationService,
    pub outbox: OutboxService,
    pub gateway: GatewayHub,
    pub tickets: GatewayTickets,
    pub storage: StorageService,
    pub passwords: PasswordService,
    pub voice: VoiceProjection,
    pub livekit_admin: Option<Arc<LiveKitAdmin>>,
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
        let tickets = GatewayTickets::new(redis.clone());
        let storage = StorageService::new(&config.storage.root, config.storage.max_upload_bytes);
        let passwords = PasswordService::new()?;
        let livekit_admin = LiveKitAdmin::new(
            &config.voice.livekit_api_url,
            &config.voice.api_key,
            &config.voice.api_secret,
        )
        .ok()
        .map(Arc::new);
        Ok(Self {
            config,
            users: UserService::new(db.clone()),
            audit: AuditService::new(db.clone()),
            communities: CommunityService::new(db.clone()),
            channels: ChannelService::new(db.clone()),
            social: SocialService::new(db.clone()),
            messages: MessageService::new(db.clone()),
            notifications: NotificationService::new(db.clone()),
            outbox: OutboxService::new(db.clone()),
            gateway: GatewayHub::default(),
            tickets,
            storage,
            passwords,
            voice: VoiceProjection::new(),
            livekit_admin,
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
