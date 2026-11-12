-- Tulpar Phase 1 foundation schema.
-- Later phases extend this schema with append-only migrations.

CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE users (
    id uuid PRIMARY KEY,
    username citext NOT NULL UNIQUE,
    display_name text NOT NULL,
    email citext NOT NULL UNIQUE,
    email_verified_at timestamptz,
    password_hash text,
    avatar_file_id uuid,
    bio text,
    locale text NOT NULL DEFAULT 'en',
    preferred_status text NOT NULL DEFAULT 'online',
    is_bot boolean NOT NULL DEFAULT false,
    bot_owner_id uuid REFERENCES users(id) ON DELETE CASCADE,
    auth_version integer NOT NULL DEFAULT 0,
    disabled_at timestamptz,
    disabled_reason text,
    last_seen_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    CONSTRAINT ck_users_password CHECK (
        (is_bot AND password_hash IS NULL)
        OR (NOT is_bot AND (password_hash IS NOT NULL OR deleted_at IS NOT NULL))
    ),
    CONSTRAINT ck_users_bot_owner CHECK (
        (is_bot AND bot_owner_id IS NOT NULL)
        OR (NOT is_bot AND bot_owner_id IS NULL)
    ),
    CONSTRAINT ck_users_username_bounds CHECK (char_length(username::text) BETWEEN 2 AND 32),
    CONSTRAINT ck_users_username_chars CHECK (username::text ~ '^[a-z0-9._-]+$'),
    CONSTRAINT ck_users_display_name CHECK (char_length(display_name) BETWEEN 1 AND 64),
    CONSTRAINT ck_users_email CHECK (char_length(email::text) <= 254),
    CONSTRAINT ck_users_bio CHECK (bio IS NULL OR char_length(bio) <= 500),
    CONSTRAINT ck_users_locale CHECK (char_length(locale) BETWEEN 2 AND 16),
    CONSTRAINT ck_users_status CHECK (preferred_status IN ('online', 'idle', 'dnd', 'invisible'))
);

CREATE INDEX idx_users_created_at ON users (created_at);
CREATE INDEX idx_users_bot ON users (is_bot) WHERE is_bot = true;
CREATE INDEX idx_users_username_trgm ON users USING gin ((username::text) gin_trgm_ops);
CREATE INDEX idx_users_display_name_trgm ON users USING gin (display_name gin_trgm_ops);
CREATE INDEX idx_users_deleted ON users (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE TABLE files (
    id uuid PRIMARY KEY,
    owner_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    storage_provider text NOT NULL DEFAULT 'local',
    object_key text NOT NULL UNIQUE,
    original_filename text NOT NULL,
    content_type text NOT NULL,
    size_bytes bigint NOT NULL,
    sha256 bytea,
    purpose text NOT NULL DEFAULT 'attachment',
    status text NOT NULL DEFAULT 'pending',
    width integer,
    height integer,
    duration_ms integer,
    upload_expires_at timestamptz,
    completed_at timestamptz,
    deleted_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_files_provider CHECK (storage_provider IN ('local', 's3', 'minio')),
    CONSTRAINT ck_files_purpose CHECK (purpose IN ('attachment', 'avatar', 'community_icon', 'community_banner', 'embed_asset')),
    CONSTRAINT ck_files_status CHECK (status IN ('pending', 'ready', 'quarantined', 'deleted')),
    CONSTRAINT ck_files_size CHECK (size_bytes > 0),
    CONSTRAINT ck_files_dimensions CHECK (
        (width IS NULL OR width > 0) AND (height IS NULL OR height > 0)
    )
);

CREATE INDEX idx_files_owner_created ON files (owner_user_id, created_at DESC);
CREATE INDEX idx_files_status_created ON files (status, created_at);
CREATE INDEX idx_files_sha256 ON files (sha256) WHERE sha256 IS NOT NULL;
CREATE INDEX idx_files_deleted ON files (deleted_at) WHERE deleted_at IS NOT NULL;

ALTER TABLE users
    ADD CONSTRAINT fk_users_avatar_file
    FOREIGN KEY (avatar_file_id) REFERENCES files(id) ON DELETE SET NULL;

CREATE TABLE file_variants (
    id uuid PRIMARY KEY,
    file_id uuid NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    kind text NOT NULL,
    object_key text NOT NULL UNIQUE,
    content_type text NOT NULL,
    size_bytes bigint NOT NULL,
    width integer,
    height integer,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_file_variants_kind CHECK (kind IN ('thumbnail', 'preview')),
    CONSTRAINT ck_file_variants_size CHECK (size_bytes > 0),
    CONSTRAINT uq_file_variants_kind UNIQUE (file_id, kind)
);

CREATE INDEX idx_file_variants_file ON file_variants (file_id);

CREATE TABLE username_reservations (
    username citext PRIMARY KEY,
    reserved_until timestamptz NOT NULL,
    reason text NOT NULL DEFAULT 'account_deleted'
);

CREATE INDEX idx_username_reservations_until ON username_reservations (reserved_until);

CREATE TABLE user_settings (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    theme text NOT NULL DEFAULT 'dark' CHECK (theme IN ('dark', 'light', 'system')),
    accent_color text NOT NULL DEFAULT '#7C5CFF',
    compact_messages boolean NOT NULL DEFAULT false,
    show_typing_indicators boolean NOT NULL DEFAULT true,
    allow_friend_requests_from text NOT NULL DEFAULT 'everyone',
    allow_dms_from text NOT NULL DEFAULT 'friends',
    presence_visibility text NOT NULL DEFAULT 'everyone',
    read_receipts boolean NOT NULL DEFAULT true,
    custom_status text,
    custom_status_emoji text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_user_settings_accent CHECK (accent_color ~ '^#[0-9A-Fa-f]{6}$'),
    CONSTRAINT ck_user_settings_friend_requests CHECK (
        allow_friend_requests_from IN ('everyone', 'mutual_communities', 'nobody')
    ),
    CONSTRAINT ck_user_settings_dms CHECK (
        allow_dms_from IN ('everyone', 'friends', 'mutual_communities', 'nobody')
    ),
    CONSTRAINT ck_user_settings_presence CHECK (
        presence_visibility IN ('everyone', 'friends', 'nobody')
    ),
    CONSTRAINT ck_user_settings_custom_status CHECK (
        custom_status IS NULL OR char_length(custom_status) <= 128
    )
);

CREATE TABLE instance_admins (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    granted_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_family_id uuid NOT NULL,
    device_name text,
    platform text NOT NULL,
    ip inet,
    user_agent text,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    last_verified_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    revoked_reason text,
    CONSTRAINT ck_sessions_platform CHECK (
        platform IN ('web', 'desktop', 'embedded', 'api', 'bot')
    )
);

CREATE INDEX idx_sessions_user_created ON sessions (user_id, created_at DESC);
CREATE INDEX idx_sessions_active ON sessions (user_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_sessions_expires ON sessions (expires_at) WHERE revoked_at IS NULL;
CREATE INDEX idx_sessions_family ON sessions (token_family_id);

CREATE TABLE refresh_tokens (
    id uuid PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    issued_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    revoked_at timestamptz,
    replaced_by_id uuid REFERENCES refresh_tokens(id) ON DELETE SET NULL
);

CREATE INDEX idx_refresh_tokens_session ON refresh_tokens (session_id, issued_at DESC);
CREATE INDEX idx_refresh_tokens_expires ON refresh_tokens (expires_at) WHERE revoked_at IS NULL;

CREATE TABLE email_verification_tokens (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_email_verification_user ON email_verification_tokens (user_id, expires_at);

CREATE TABLE password_reset_tokens (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    requested_ip inet,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_password_reset_user ON password_reset_tokens (user_id, expires_at);

CREATE TABLE device_authorizations (
    id uuid PRIMARY KEY,
    device_code_hash bytea NOT NULL UNIQUE,
    user_code citext NOT NULL UNIQUE,
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    client_name text NOT NULL,
    requested_ip inet,
    platform text NOT NULL DEFAULT 'desktop',
    status text NOT NULL DEFAULT 'pending',
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    approved_at timestamptz,
    last_polled_at timestamptz,
    poll_interval_seconds smallint NOT NULL DEFAULT 5,
    CONSTRAINT ck_device_authorizations_status CHECK (
        status IN ('pending', 'approved', 'denied', 'expired', 'consumed')
    ),
    CONSTRAINT ck_device_authorizations_platform CHECK (platform IN ('desktop', 'web')),
    CONSTRAINT ck_device_authorizations_interval CHECK (poll_interval_seconds > 0)
);

CREATE INDEX idx_device_authorizations_status ON device_authorizations (status, expires_at);
CREATE INDEX idx_device_authorizations_user ON device_authorizations (user_id);

CREATE TABLE audit_logs (
    id uuid PRIMARY KEY,
    actor_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    actor_type text NOT NULL,
    action text NOT NULL,
    community_id uuid,
    target_type text,
    target_id uuid,
    ip inet,
    user_agent text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_audit_logs_actor_type CHECK (actor_type IN ('user', 'bot', 'system', 'admin'))
);

CREATE INDEX idx_audit_logs_actor ON audit_logs (actor_user_id, created_at DESC);
CREATE INDEX idx_audit_logs_action ON audit_logs (action, created_at DESC);
CREATE INDEX idx_audit_logs_created ON audit_logs (created_at);

CREATE TABLE outbox_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id uuid NOT NULL UNIQUE,
    topic text NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id uuid,
    community_id uuid,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    available_at timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0,
    last_error text,
    processed_at timestamptz
);

CREATE INDEX idx_outbox_pending ON outbox_events (available_at) WHERE processed_at IS NULL;
CREATE INDEX idx_outbox_community ON outbox_events (community_id, created_at DESC);

CREATE TABLE idempotency_keys (
    actor_id uuid NOT NULL,
    actor_type text NOT NULL,
    key text NOT NULL,
    endpoint text NOT NULL,
    request_hash bytea NOT NULL,
    response_status smallint,
    response_body jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (actor_id, key),
    CONSTRAINT ck_idempotency_actor_type CHECK (actor_type IN ('user', 'bot', 'api_key'))
);

CREATE INDEX idx_idempotency_expires ON idempotency_keys (expires_at);

CREATE TABLE instance_settings (
    key text PRIMARY KEY,
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by uuid REFERENCES users(id) ON DELETE SET NULL
);
