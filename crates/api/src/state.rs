use std::sync::Arc;

use auth::{AuthError, AuthService, EmailSender};
use channels::ChannelService;
use communities::CommunityService;
use config::AppConfig;
use realtime::{RateLimiter, RedisStore};
use social::SocialService;
use sqlx::PgPool;
use users::UserService;

/// Shared API state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: PgPool,
    pub redis: Option<RedisStore>,
    pub rate_limiter: RateLimiter,
    pub auth: Arc<AuthService>,
    pub users: UserService,
    pub communities: CommunityService,
    pub channels: ChannelService,
    pub social: SocialService,
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
            communities: CommunityService::new(db.clone()),
            channels: ChannelService::new(db.clone()),
            social: SocialService::new(db.clone()),
            auth,
            db,
            redis,
            rate_limiter,
        })
    }
}
