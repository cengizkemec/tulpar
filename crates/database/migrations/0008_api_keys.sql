-- Phase 8: scoped API keys for bot and automation access.

CREATE TABLE api_keys (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name text NOT NULL,
    key_hash bytea NOT NULL UNIQUE,
    scopes text[] NOT NULL,
    last_used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    CONSTRAINT ck_api_keys_name CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT ck_api_keys_scopes CHECK (array_length(scopes, 1) BETWEEN 1 AND 2)
);

CREATE INDEX idx_api_keys_user ON api_keys (user_id);
CREATE INDEX idx_api_keys_active ON api_keys (user_id) WHERE revoked_at IS NULL;
