-- Phase 3: message persistence, reactions, mentions, pins, and read states.

CREATE TABLE messages (
    id uuid PRIMARY KEY,
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id uuid REFERENCES users(id) ON DELETE SET NULL,
    kind text NOT NULL DEFAULT 'default',
    content text,
    reply_to_id uuid REFERENCES messages(id) ON DELETE SET NULL,
    client_nonce uuid,
    edited_at timestamptz,
    deleted_at timestamptz,
    deleted_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    search_vector tsvector GENERATED ALWAYS AS (
        to_tsvector('simple'::regconfig, coalesce(content, ''))
    ) STORED,
    CONSTRAINT ck_messages_kind CHECK (kind IN ('default', 'system')),
    CONSTRAINT ck_messages_content CHECK (content IS NULL OR char_length(content) <= 8000),
    CONSTRAINT ck_messages_body CHECK (deleted_at IS NOT NULL OR content IS NOT NULL)
);

CREATE INDEX idx_messages_channel_id ON messages (channel_id, id DESC);
CREATE INDEX idx_messages_channel_created ON messages (channel_id, created_at DESC, id DESC);
CREATE INDEX idx_messages_author ON messages (author_id, created_at DESC);
CREATE INDEX idx_messages_search ON messages USING gin (search_vector);
CREATE INDEX idx_messages_reply ON messages (reply_to_id) WHERE reply_to_id IS NOT NULL;
CREATE INDEX idx_messages_deleted ON messages (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE UNIQUE INDEX uq_messages_client_nonce
    ON messages (channel_id, author_id, client_nonce)
    WHERE client_nonce IS NOT NULL AND author_id IS NOT NULL;

CREATE TABLE message_reactions (
    message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, user_id, emoji),
    CONSTRAINT ck_message_reactions_emoji CHECK (char_length(emoji) BETWEEN 1 AND 64)
);

CREATE INDEX idx_message_reactions_message ON message_reactions (message_id);

CREATE TABLE message_mentions (
    id uuid PRIMARY KEY,
    message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    kind text NOT NULL,
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    role_id uuid REFERENCES roles(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_message_mentions_kind CHECK (kind IN ('user', 'role', 'everyone')),
    CONSTRAINT ck_message_mentions_target CHECK (
        (kind = 'user' AND user_id IS NOT NULL AND role_id IS NULL)
        OR (kind = 'role' AND role_id IS NOT NULL AND user_id IS NULL)
        OR (kind = 'everyone' AND user_id IS NULL AND role_id IS NULL)
    )
);

CREATE INDEX idx_message_mentions_user ON message_mentions (user_id, created_at DESC);
CREATE INDEX idx_message_mentions_role ON message_mentions (role_id);
CREATE INDEX idx_message_mentions_message ON message_mentions (message_id);

CREATE TABLE pinned_messages (
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    pinned_by uuid REFERENCES users(id) ON DELETE SET NULL,
    pinned_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, message_id)
);

CREATE INDEX idx_pinned_messages_channel ON pinned_messages (channel_id, pinned_at DESC);

CREATE TABLE read_states (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_message_id uuid,
    last_read_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, channel_id)
);

CREATE INDEX idx_read_states_user ON read_states (user_id, updated_at DESC);
