use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use common::random_token;
use permissions::Permissions;
use uuid::Uuid;

use crate::{
    dto::{CreateWebhookRequest, CreateWebhookResponse, WebhookDto},
    error::ApiError,
    extract::CurrentUser,
    state::AppState,
    webhooks::WebhookRow,
};

const EVENTS: &[&str] = &["message.created"];

fn to_dto(webhook: WebhookRow) -> WebhookDto {
    WebhookDto {
        id: webhook.id,
        url: webhook.url,
        events: webhook.events,
        active: webhook.active,
        created_at: webhook.created_at,
        last_delivery_at: webhook.last_delivery_at,
        last_error: webhook.last_error,
    }
}

fn normalize_events(events: &[String]) -> Result<Vec<String>, ApiError> {
    if events.is_empty() || events.len() > EVENTS.len() {
        return Err(ApiError::validation(
            "Select at least one webhook event (message.created).",
        ));
    }
    let mut normalized = Vec::new();
    for event in events {
        let value = event.trim();
        if !EVENTS.contains(&value) {
            return Err(ApiError::validation(format!(
                "Unknown webhook event: {value}"
            )));
        }
        if !normalized.iter().any(|existing| existing == value) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/webhooks",
    tag = "webhooks",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    responses((status = 200, body = [WebhookDto]))
)]
pub async fn list_webhooks(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
) -> Result<Json<Vec<WebhookDto>>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    if !access.permissions.contains(Permissions::MANAGE_COMMUNITY) {
        return Err(ApiError::forbidden(
            "permission_denied",
            "You do not have permission to manage webhooks.",
        ));
    }
    let webhooks = state.webhooks.list(community_id).await.map_err(|error| {
        tracing::error!(%error, "failed to list webhooks");
        ApiError::dependency_unavailable("Webhooks are temporarily unavailable.")
    })?;
    Ok(Json(webhooks.into_iter().map(to_dto).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/webhooks",
    tag = "webhooks",
    security(("bearer_auth" = [])),
    params(("community_id" = Uuid, Path, description = "Community identifier")),
    request_body = CreateWebhookRequest,
    responses((status = 200, body = CreateWebhookResponse))
)]
pub async fn create_webhook(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(community_id): Path<Uuid>,
    Json(payload): Json<CreateWebhookRequest>,
) -> Result<Json<CreateWebhookResponse>, ApiError> {
    let access = state
        .communities
        .access(community_id, current.user().id)
        .await?;
    if !access.permissions.contains(Permissions::MANAGE_COMMUNITY) {
        return Err(ApiError::forbidden(
            "permission_denied",
            "You do not have permission to manage webhooks.",
        ));
    }
    let events = normalize_events(&payload.events)?;
    let url = state
        .webhooks
        .validate_url(&payload.url)
        .await
        .map_err(ApiError::validation)?;
    let secret = format!("whsec_{}", random_token(32));
    let webhook = state
        .webhooks
        .create(community_id, &url, &secret, &events, current.user().id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to create webhook");
            ApiError::dependency_unavailable("Webhooks are temporarily unavailable.")
        })?;
    state
        .record_audit(
            Some(current.user().id),
            "webhook.create",
            Some(community_id),
            Some(("webhook", webhook.id)),
        )
        .await;
    Ok(Json(CreateWebhookResponse {
        webhook: to_dto(webhook),
        secret,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/webhooks/{webhook_id}",
    tag = "webhooks",
    security(("bearer_auth" = [])),
    params(("webhook_id" = Uuid, Path, description = "Webhook identifier")),
    responses((status = 204, description = "Webhook deleted"))
)]
pub async fn delete_webhook(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(webhook_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let webhook = state
        .webhooks
        .get(webhook_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to load webhook");
            ApiError::dependency_unavailable("Webhooks are temporarily unavailable.")
        })?
        .ok_or_else(|| ApiError::not_found("Webhook not found."))?;
    let access = state
        .communities
        .access(webhook.community_id, current.user().id)
        .await?;
    if !access.permissions.contains(Permissions::MANAGE_COMMUNITY) {
        return Err(ApiError::forbidden(
            "permission_denied",
            "You do not have permission to manage webhooks.",
        ));
    }
    let deleted = state
        .webhooks
        .delete(webhook.community_id, webhook_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to delete webhook");
            ApiError::dependency_unavailable("Webhooks are temporarily unavailable.")
        })?;
    if !deleted {
        return Err(ApiError::not_found("Webhook not found."));
    }
    state
        .record_audit(
            Some(current.user().id),
            "webhook.delete",
            Some(webhook.community_id),
            Some(("webhook", webhook_id)),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}
