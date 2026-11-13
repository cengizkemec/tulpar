-- Phase 8: outgoing webhook subscriptions and delivery queue.

CREATE TABLE webhooks (
    id uuid PRIMARY KEY,
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    url text NOT NULL,
    secret text NOT NULL,
    events text[] NOT NULL,
    active boolean NOT NULL DEFAULT true,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    last_delivery_at timestamptz,
    last_error text,
    CONSTRAINT ck_webhooks_events CHECK (array_length(events, 1) BETWEEN 1 AND 8)
);

CREATE INDEX idx_webhooks_community ON webhooks (community_id) WHERE active;

CREATE TABLE webhook_deliveries (
    id uuid PRIMARY KEY,
    webhook_id uuid NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_id uuid NOT NULL,
    event_type text NOT NULL,
    payload jsonb NOT NULL,
    attempts integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    delivered_at timestamptz,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_webhook_deliveries_pending
    ON webhook_deliveries (next_attempt_at)
    WHERE delivered_at IS NULL;
CREATE INDEX idx_webhook_deliveries_webhook
    ON webhook_deliveries (webhook_id, created_at DESC);
