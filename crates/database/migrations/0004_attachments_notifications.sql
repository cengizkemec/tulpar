-- Phase 3 completion: attachments, notifications, and notification preferences.

-- Attachment-only messages are valid; the service enforces content-or-attachment.
ALTER TABLE messages DROP CONSTRAINT IF EXISTS ck_messages_body;

CREATE TABLE attachments (
    id uuid PRIMARY KEY,
    message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    file_id uuid NOT NULL REFERENCES files(id) ON DELETE RESTRICT,
    filename text NOT NULL,
    content_type text NOT NULL,
    size_bytes bigint NOT NULL,
    width integer,
    height integer,
    duration_ms integer,
    spoiler boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_attachments_filename CHECK (char_length(filename) BETWEEN 1 AND 255),
    CONSTRAINT ck_attachments_size CHECK (size_bytes > 0),
    CONSTRAINT ck_attachments_dimensions CHECK (
        (width IS NULL OR width > 0) AND (height IS NULL OR height > 0)
    ),
    CONSTRAINT uq_attachments_message_file UNIQUE (message_id, file_id)
);

CREATE INDEX idx_attachments_message ON attachments (message_id);
CREATE INDEX idx_attachments_file ON attachments (file_id);

CREATE TABLE notifications (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind text NOT NULL,
    community_id uuid REFERENCES communities(id) ON DELETE CASCADE,
    channel_id uuid REFERENCES channels(id) ON DELETE CASCADE,
    message_id uuid REFERENCES messages(id) ON DELETE CASCADE,
    actor_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    read_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_notifications_kind CHECK (
        kind IN ('mention', 'reply', 'dm', 'friend_request', 'community', 'voice', 'system', 'moderation')
    )
);

CREATE INDEX idx_notifications_user_created ON notifications (user_id, created_at DESC);
CREATE INDEX idx_notifications_user_unread ON notifications (user_id) WHERE read_at IS NULL;
CREATE INDEX idx_notifications_community ON notifications (community_id);
CREATE INDEX idx_notifications_channel ON notifications (channel_id);

CREATE TABLE notification_preferences (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    desktop_enabled boolean NOT NULL DEFAULT true,
    sound_enabled boolean NOT NULL DEFAULT true,
    mentions_only boolean NOT NULL DEFAULT false,
    suppress_everyone boolean NOT NULL DEFAULT false,
    email_enabled boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE community_notification_settings (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    level text NOT NULL DEFAULT 'mentions',
    muted_until timestamptz,
    suppress_everyone boolean NOT NULL DEFAULT false,
    PRIMARY KEY (user_id, community_id),
    CONSTRAINT ck_community_notification_level CHECK (level IN ('all', 'mentions', 'none'))
);

CREATE INDEX idx_community_notification_community ON community_notification_settings (community_id);

CREATE TABLE channel_notification_settings (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    level text NOT NULL DEFAULT 'mentions',
    muted_until timestamptz,
    PRIMARY KEY (user_id, channel_id),
    CONSTRAINT ck_channel_notification_level CHECK (level IN ('all', 'mentions', 'none'))
);

CREATE INDEX idx_channel_notification_channel ON channel_notification_settings (channel_id);
