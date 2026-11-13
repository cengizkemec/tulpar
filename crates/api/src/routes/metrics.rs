use axum::{extract::State, http::header, response::IntoResponse};
use sqlx::Row;

use crate::state::AppState;

/// Minimal Prometheus text exposition for internal scraping.
pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let mut lines = Vec::new();
    lines.push("# HELP tulpar_up Tulpar API process is running.".to_string());
    lines.push("# TYPE tulpar_up gauge".to_string());
    lines.push("tulpar_up 1".to_string());

    let queries = [
        (
            "tulpar_users",
            "SELECT count(*) FROM users WHERE deleted_at IS NULL",
        ),
        (
            "tulpar_communities",
            "SELECT count(*) FROM communities WHERE deleted_at IS NULL",
        ),
        (
            "tulpar_messages",
            "SELECT count(*) FROM messages WHERE deleted_at IS NULL",
        ),
        (
            "tulpar_outbox_pending",
            "SELECT count(*) FROM outbox_events WHERE processed_at IS NULL",
        ),
        (
            "tulpar_webhook_pending",
            "SELECT count(*) FROM webhook_deliveries WHERE delivered_at IS NULL",
        ),
    ];

    for (name, query) in queries {
        lines.push(format!("# TYPE {name} gauge"));
        match sqlx::query(query).fetch_one(&state.db).await {
            Ok(row) => {
                let value: i64 = row.try_get(0).unwrap_or(0);
                lines.push(format!("{name} {value}"));
            }
            Err(error) => {
                tracing::warn!(%error, metric = name, "metrics query failed");
                lines.push(format!("# {name} unavailable"));
            }
        }
    }

    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        lines.join("\n") + "\n",
    )
}
