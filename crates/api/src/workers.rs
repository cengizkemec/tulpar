//! Background workers started by the server process.

use events::EventError;
use gateway::GatewayEvent;
use notifications::Notification;

use crate::state::AppState;

/// Process one outbox batch. Unknown topics are marked processed after a warning.
pub async fn dispatch_outbox_once(state: &AppState) -> Result<usize, EventError> {
    let events = state.outbox.pending(50).await?;
    let mut processed = 0;
    for event in events {
        match event.topic.as_str() {
            "notification.created" => {
                match serde_json::from_value::<Notification>(event.payload.clone()) {
                    Ok(notification) => {
                        let payload = serde_json::to_value(&notification).unwrap_or_default();
                        state.gateway.publish(GatewayEvent::user(
                            "NOTIFICATION_CREATE",
                            notification.user_id,
                            payload,
                        ));
                    }
                    Err(error) => {
                        state
                            .outbox
                            .mark_failed(event.id, &error.to_string(), 30)
                            .await?;
                        continue;
                    }
                }
            }
            other => {
                tracing::warn!(topic = other, event_id = %event.event_id, "no outbox handler");
            }
        }
        state.outbox.mark_processed(event.id).await?;
        processed += 1;
    }
    Ok(processed)
}
