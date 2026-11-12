-- Phase 4: durable LiveKit webhook deduplication.

CREATE TABLE voice_webhook_events (
    event_id text PRIMARY KEY,
    event_type text NOT NULL,
    room_name text,
    received_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_voice_webhook_received ON voice_webhook_events (received_at);
