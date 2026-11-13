-- Phase 8: embed applications and one-time member authorization codes.

CREATE TABLE embed_apps (
    id uuid PRIMARY KEY,
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    name text NOT NULL,
    public_key text NOT NULL UNIQUE,
    allowed_origins text[] NOT NULL,
    widgets text[] NOT NULL,
    allow_authenticated boolean NOT NULL DEFAULT false,
    allow_message_preview boolean NOT NULL DEFAULT false,
    token_version integer NOT NULL DEFAULT 0,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    CONSTRAINT ck_embed_apps_name CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT ck_embed_apps_origins CHECK (array_length(allowed_origins, 1) BETWEEN 1 AND 20),
    CONSTRAINT ck_embed_apps_widgets CHECK (array_length(widgets, 1) BETWEEN 1 AND 8)
);

CREATE INDEX idx_embed_apps_community ON embed_apps (community_id);
CREATE INDEX idx_embed_apps_active ON embed_apps (community_id) WHERE revoked_at IS NULL;

CREATE TABLE embed_authorization_codes (
    id uuid PRIMARY KEY,
    embed_app_id uuid NOT NULL REFERENCES embed_apps(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash bytea NOT NULL UNIQUE,
    origin text NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_embed_codes_expiry ON embed_authorization_codes (expires_at);
CREATE INDEX idx_embed_codes_app ON embed_authorization_codes (embed_app_id, created_at DESC);
