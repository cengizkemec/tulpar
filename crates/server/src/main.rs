use std::{net::SocketAddr, sync::Arc};

use anyhow::{bail, Context, Result};
use api::{router, AppState};
use auth::{EmailSender, LogEmailSender, PasswordService, SmtpEmailSender};
use clap::{Parser, Subcommand};
use common::{normalize_email, normalize_username, validate_email, validate_username};
use config::AppConfig;
use database::Database;
use realtime::RedisStore;
use tracing_subscriber::EnvFilter;
use users::{NewUser, UserService};

#[derive(Debug, Parser)]
#[command(name = "tulpar-server", version, about = "Tulpar self-hosted server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the API, gateway, and background workers.
    Serve,
    /// Apply database migrations and exit.
    Migrate,
    /// Manage instance administrators.
    Admin {
        #[command(subcommand)]
        action: AdminCommand,
    },
    /// Print version information.
    Version,
}

#[derive(Debug, Subcommand)]
enum AdminCommand {
    /// Create the first instance administrator.
    Create {
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long)]
        password_stdin: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve => serve().await,
        Command::Migrate => migrate().await,
        Command::Admin { action } => admin(action).await,
        Command::Version => {
            println!("tulpar-server {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

async fn serve() -> Result<()> {
    let config = Arc::new(AppConfig::load().context("configuration error")?);
    init_logging(&config);
    tracing::info!(
        instance = %config.instance.name,
        environment = %config.instance.environment,
        "starting Tulpar server"
    );

    let database = Database::connect(&config.database)
        .await
        .context("database connection failed")?;
    database
        .migrate()
        .await
        .context("database migrations failed")?;
    let pool = database.into_pool();

    let redis = match RedisStore::new(&config.redis.url) {
        Ok(store) => {
            match store.health().await {
                Ok(()) => tracing::info!("connected to Redis"),
                Err(error) => {
                    tracing::warn!(%error, "Redis is unavailable; starting in degraded mode")
                }
            }
            Some(store)
        }
        Err(error) => {
            tracing::warn!(%error, "invalid Redis configuration; starting without Redis");
            None
        }
    };

    let email: Arc<dyn EmailSender> = match config.email.driver.as_str() {
        "log" => Arc::new(LogEmailSender),
        "smtp" => Arc::new(
            SmtpEmailSender::new(&config.email.smtp_url, &config.email.from)
                .context("SMTP email configuration failed")?,
        ),
        other => bail!("unsupported email.driver={other}"),
    };

    let state = AppState::new(config.clone(), pool, redis, email)
        .context("API state initialization failed")?;
    let worker_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match api::workers::dispatch_outbox_once(&worker_state).await {
                Ok(0) => {}
                Ok(processed) => tracing::debug!(processed, "outbox batch dispatched"),
                Err(error) => tracing::error!(%error, "outbox dispatch failed"),
            }
        }
    });
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&config.server.bind)
        .await
        .with_context(|| format!("could not bind {}", config.server.bind))?;
    let address = listener.local_addr()?;
    tracing::info!(%address, "Tulpar API listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")?;

    Ok(())
}

async fn migrate() -> Result<()> {
    let config = AppConfig::load().context("configuration error")?;
    let database = Database::connect(&config.database)
        .await
        .context("database connection failed")?;
    database
        .migrate()
        .await
        .context("database migrations failed")?;
    println!("migrations applied");
    Ok(())
}

async fn admin(action: AdminCommand) -> Result<()> {
    let config = AppConfig::load().context("configuration error")?;
    init_logging(&config);
    let database = Database::connect(&config.database)
        .await
        .context("database connection failed")?;
    database
        .migrate()
        .await
        .context("database migrations failed")?;
    let pool = database.into_pool();

    match action {
        AdminCommand::Create {
            username,
            email,
            display_name,
            password_stdin,
        } => {
            let username = username
                .unwrap_or_else(|| rpassword::prompt_password("Username: ").unwrap_or_default());
            let email =
                email.unwrap_or_else(|| rpassword::prompt_password("Email: ").unwrap_or_default());
            let display_name = display_name.unwrap_or_else(|| username.clone());
            let username = normalize_username(&username);
            let email = normalize_email(&email);
            validate_username(&username).map_err(|error| anyhow::anyhow!(error.to_string()))?;
            if !validate_email(&email) {
                bail!("invalid email address");
            }

            let password = if password_stdin {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .context("could not read password from stdin")?;
                line.trim_end_matches(['\r', '\n']).to_string()
            } else {
                rpassword::prompt_password("Password: ").context("could not read password")?
            };
            if password.chars().count() < config.auth.password_min_length {
                bail!(
                    "password must be at least {} characters",
                    config.auth.password_min_length
                );
            }
            if !password_stdin {
                let confirmation = rpassword::prompt_password("Confirm password: ")
                    .context("could not read password confirmation")?;
                if password != confirmation {
                    bail!("passwords do not match");
                }
            }

            let passwords = PasswordService::new().context("password service failed")?;
            let password_hash = passwords
                .hash(common::normalize_password(&password))
                .await
                .context("password hashing failed")?;
            let users = UserService::new(pool.clone());
            let user = users
                .create_user(NewUser {
                    username,
                    display_name,
                    email,
                    password_hash,
                    locale: "en".to_string(),
                    email_verified: true,
                })
                .await
                .context("could not create administrator")?;

            sqlx::query(
                "INSERT INTO instance_admins (user_id, granted_by) VALUES ($1, NULL) ON CONFLICT (user_id) DO NOTHING",
            )
            .bind(user.id)
            .execute(&pool)
            .await
            .context("could not grant instance administrator")?;

            println!("instance administrator created: {}", user.id);
        }
    }

    Ok(())
}

fn init_logging(config: &AppConfig) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log.level.clone()));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if config.log.format == "json" {
        let _ = builder.json().try_init();
    } else {
        let _ = builder.pretty().try_init();
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
