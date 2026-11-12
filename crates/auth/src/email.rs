//! Email delivery abstraction.

use async_trait::async_trait;
use lettre::{
    message::{header::ContentType, Mailbox},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("email configuration error: {0}")]
    Config(String),
    #[error("email delivery failed: {0}")]
    Delivery(String),
}

/// Transactional email sender. Implementations must never log recipient addresses.
#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_verification(&self, to: &str, link: &str) -> Result<(), EmailError>;
    async fn send_password_reset(&self, to: &str, link: &str) -> Result<(), EmailError>;
}

/// Development-only sender. Production startup rejects this driver when email is required.
#[derive(Clone, Default)]
pub struct LogEmailSender;

#[async_trait]
impl EmailSender for LogEmailSender {
    async fn send_verification(&self, _to: &str, link: &str) -> Result<(), EmailError> {
        tracing::warn!(link, "development email: verification link (not sent)");
        Ok(())
    }

    async fn send_password_reset(&self, _to: &str, link: &str) -> Result<(), EmailError> {
        tracing::warn!(link, "development email: password reset link (not sent)");
        Ok(())
    }
}

/// SMTP sender for production/self-hosted deployments.
pub struct SmtpEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailSender {
    pub fn new(smtp_url: &str, from: &str) -> Result<Self, EmailError> {
        if smtp_url.trim().is_empty() {
            return Err(EmailError::Config("smtp_url must not be empty".into()));
        }
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)
            .map_err(|error| EmailError::Config(error.to_string()))?
            .build();
        let from = from
            .parse::<Mailbox>()
            .map_err(|error| EmailError::Config(error.to_string()))?;
        Ok(Self { transport, from })
    }

    async fn send(&self, to: &str, subject: &str, body: String) -> Result<(), EmailError> {
        let to = to
            .parse::<Mailbox>()
            .map_err(|error| EmailError::Config(error.to_string()))?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .map_err(|error| EmailError::Config(error.to_string()))?;
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|error| EmailError::Delivery(error.to_string()))
    }
}

#[async_trait]
impl EmailSender for SmtpEmailSender {
    async fn send_verification(&self, to: &str, link: &str) -> Result<(), EmailError> {
        self.send(
            to,
            "Verify your Tulpar email address",
            format!(
                "Welcome to Tulpar.\n\nVerify your email address by opening this link:\n\n{link}\n\nIf you did not create this account, ignore this message.\n"
            ),
        )
        .await
    }

    async fn send_password_reset(&self, to: &str, link: &str) -> Result<(), EmailError> {
        self.send(
            to,
            "Reset your Tulpar password",
            format!(
                "A password reset was requested for your Tulpar account.\n\nOpen this link to choose a new password:\n\n{link}\n\nIf you did not request this, ignore this message.\n"
            ),
        )
        .await
    }
}
