use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    Json,
};
use permissions::Permissions;
use storage::sanitize_filename;
use uuid::Uuid;

use crate::{
    dto::UploadedFileDto, error::ApiError, extract::CurrentUser, routes::messages, state::AppState,
};

const MAX_ATTACHMENT_FIELD_BYTES: usize = 32 * 1024 * 1024;

#[utoipa::path(
    post,
    path = "/api/v1/channels/{channel_id}/attachments",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(("channel_id" = Uuid, Path, description = "Channel identifier")),
    request_body(content = String, content_type = "multipart/form-data", description = "File upload"),
    responses((status = 200, body = UploadedFileDto))
)]
pub async fn upload_attachment(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(channel_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<UploadedFileDto>, ApiError> {
    let authorization = messages::authorize_channel(&state, current.user().id, channel_id).await?;
    authorization.require(Permissions::ATTACH_FILES)?;
    authorization.require(Permissions::SEND_MESSAGES)?;

    let mut uploaded = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::validation(format!("invalid multipart upload: {error}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let filename = sanitize_filename(field.file_name().unwrap_or("upload.bin"));
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .trim()
            .to_string();
        let mut bytes = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|error| ApiError::validation(format!("invalid upload body: {error}")))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_ATTACHMENT_FIELD_BYTES {
                return Err(ApiError::validation("upload is too large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        state
            .storage
            .validate_upload(&filename, &content_type, bytes.len() as u64)?;
        uploaded = Some((filename, content_type, bytes));
        break;
    }

    let Some((filename, content_type, bytes)) = uploaded else {
        return Err(ApiError::validation("a file field is required"));
    };

    let file_id = tulpar_core::new_id();
    let stored = state
        .storage
        .store(current.user().id, file_id, &bytes)
        .await
        .map_err(|error| {
            tracing::error!(%error, "attachment write failed");
            ApiError::dependency_unavailable("Storage is temporarily unavailable.")
        })?;

    let result = sqlx::query(
        r#"
        INSERT INTO files (
            id, owner_user_id, storage_provider, object_key, original_filename,
            content_type, size_bytes, sha256, purpose, status, completed_at
        )
        VALUES ($1, $2, 'local', $3, $4, $5, $6, $7, 'attachment', 'ready', now())
        "#,
    )
    .bind(file_id)
    .bind(current.user().id)
    .bind(&stored.object_key)
    .bind(&filename)
    .bind(&content_type)
    .bind(stored.size_bytes as i64)
    .bind(&stored.sha256)
    .execute(&state.db)
    .await;

    if let Err(error) = result {
        let _ = state.storage.delete(&stored.object_key).await;
        tracing::error!(%error, "attachment metadata insert failed");
        return Err(ApiError::dependency_unavailable(
            "The database is temporarily unavailable.",
        ));
    }

    Ok(Json(UploadedFileDto {
        id: file_id,
        filename,
        content_type,
        size_bytes: stored.size_bytes as i64,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/messages/{message_id}/attachments/{attachment_id}",
    tag = "messages",
    security(("bearer_auth" = [])),
    params(
        ("message_id" = Uuid, Path, description = "Message identifier"),
        ("attachment_id" = Uuid, Path, description = "Attachment identifier")
    ),
    responses((status = 200, description = "Attachment bytes"))
)]
pub async fn download_attachment(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((message_id, attachment_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, HeaderMap, Body), ApiError> {
    let (message, authorization) =
        messages::authorize_message(&state, current.user().id, message_id).await?;
    authorization.require(Permissions::READ_MESSAGE_HISTORY)?;
    if message.deleted_at.is_some() {
        return Err(ApiError::not_found("Attachment not found."));
    }
    let download = state
        .messages
        .attachment_download(message_id, attachment_id)
        .await?;
    let bytes = state
        .storage
        .read(&download.object_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "attachment read failed");
            ApiError::not_found("Attachment not found.")
        })?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&download.attachment.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&download.attachment.size_bytes.to_string())
            .map_err(|_| ApiError::internal("Could not encode the response."))?,
    );
    let safe_name = sanitize_filename(&download.attachment.filename).replace('"', "'");
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{safe_name}\""))
            .map_err(|_| ApiError::internal("Could not encode the response."))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    Ok((StatusCode::OK, headers, Body::from(bytes)))
}
