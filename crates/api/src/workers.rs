//! Background workers started by the server process.

use std::collections::HashSet;

use events::EventError;
use gateway::GatewayEvent;
use notifications::Notification;

use crate::{dto::VoiceParticipantDto, state::AppState};

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

/// Rebuild voice projection from LiveKit's authoritative participant list.
pub async fn reconcile_voice_once(state: &AppState) -> usize {
    let Some(admin) = state.livekit_admin.as_deref() else {
        return 0;
    };
    let channels = match state.channels.list_voice_channels().await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "voice reconciliation channel lookup failed");
            return 0;
        }
    };
    let mut reconciled = 0;
    for channel in channels {
        let participants = match admin.list_participants(channel.id).await {
            Ok(participants) => participants,
            Err(error) => {
                tracing::warn!(%error, channel_id = %channel.id, "LiveKit participant listing failed");
                continue;
            }
        };
        let identities = participants
            .iter()
            .map(|participant| participant.identity)
            .collect::<HashSet<_>>();

        for participant in participants {
            if let Some(community_id) = channel.community_id {
                if !state
                    .communities
                    .is_member(community_id, participant.identity)
                    .await
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            let display_name = if participant.name.is_empty() {
                state
                    .users
                    .get_user(participant.identity)
                    .await
                    .map(|user| user.display_name)
                    .unwrap_or_else(|_| participant.identity.to_string())
            } else {
                participant.name
            };
            let joined_at = participant.joined_at.unwrap_or_else(chrono::Utc::now);
            state
                .voice
                .join(
                    channel.id,
                    participant.identity,
                    display_name,
                    joined_at,
                    participant.tracks,
                )
                .await;
            if let Some(projected) = state
                .voice
                .participant(channel.id, participant.identity)
                .await
            {
                state.gateway.publish(GatewayEvent::channel(
                    "VOICE_STATE_UPDATE",
                    channel.community_id,
                    channel.id,
                    serde_json::to_value(VoiceParticipantDto::from(projected)).unwrap_or_default(),
                ));
            }
        }

        for projected in state.voice.participants(channel.id).await {
            if !identities.contains(&projected.user_id) {
                state.voice.leave(channel.id, projected.user_id).await;
                state.gateway.publish(GatewayEvent::channel(
                    "VOICE_SESSION_ENDED",
                    channel.community_id,
                    channel.id,
                    serde_json::json!({
                        "channel_id": channel.id,
                        "user_id": projected.user_id,
                        "reason": "reconciled",
                    }),
                ));
            }
        }
        reconciled += 1;
    }
    reconciled
}
