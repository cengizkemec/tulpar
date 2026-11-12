-- Phase 2: communities, roles, invitations, channels, friendships, blocks, DMs.

CREATE TABLE communities (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    slug citext NOT NULL,
    description text,
    icon_file_id uuid REFERENCES files(id) ON DELETE SET NULL,
    banner_file_id uuid REFERENCES files(id) ON DELETE SET NULL,
    owner_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    visibility text NOT NULL DEFAULT 'private',
    default_notifications text NOT NULL DEFAULT 'mentions',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    CONSTRAINT ck_communities_name CHECK (char_length(name) BETWEEN 2 AND 100),
    CONSTRAINT ck_communities_slug CHECK (char_length(slug::text) BETWEEN 2 AND 100),
    CONSTRAINT ck_communities_description CHECK (description IS NULL OR char_length(description) <= 2000),
    CONSTRAINT ck_communities_visibility CHECK (visibility IN ('public', 'private')),
    CONSTRAINT ck_communities_notifications CHECK (default_notifications IN ('all', 'mentions', 'none'))
);

CREATE UNIQUE INDEX uq_communities_slug_active
    ON communities (slug) WHERE deleted_at IS NULL;
CREATE INDEX idx_communities_owner ON communities (owner_user_id);
CREATE INDEX idx_communities_discovery ON communities (visibility, created_at DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_communities_name_trgm ON communities USING gin (name gin_trgm_ops);

CREATE TABLE channels (
    id uuid PRIMARY KEY,
    scope text NOT NULL,
    community_id uuid REFERENCES communities(id) ON DELETE CASCADE,
    parent_id uuid REFERENCES channels(id) ON DELETE CASCADE,
    kind text NOT NULL,
    name text,
    topic text,
    position integer NOT NULL DEFAULT 0,
    nsfw boolean NOT NULL DEFAULT false,
    slowmode_seconds integer NOT NULL DEFAULT 0,
    user_limit integer,
    bitrate integer,
    voice_password_hash text,
    last_message_at timestamptz,
    last_message_id uuid,
    archived_at timestamptz,
    deleted_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_channels_scope CHECK (scope IN ('community', 'direct')),
    CONSTRAINT ck_channels_kind CHECK (kind IN ('category', 'text', 'announcement', 'voice', 'forum', 'direct')),
    CONSTRAINT ck_channels_name CHECK (name IS NULL OR char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT ck_channels_topic CHECK (topic IS NULL OR char_length(topic) <= 1024),
    CONSTRAINT ck_channels_slowmode CHECK (slowmode_seconds BETWEEN 0 AND 21600),
    CONSTRAINT ck_channels_user_limit CHECK (user_limit IS NULL OR user_limit BETWEEN 0 AND 500),
    CONSTRAINT ck_channels_bitrate CHECK (bitrate IS NULL OR bitrate BETWEEN 8000 AND 512000),
    CONSTRAINT ck_channels_scope_fields CHECK (
        (scope = 'community' AND community_id IS NOT NULL AND kind <> 'direct' AND name IS NOT NULL)
        OR
        (scope = 'direct' AND community_id IS NULL AND kind = 'direct' AND name IS NULL AND parent_id IS NULL)
    ),
    CONSTRAINT ck_channels_category_parent CHECK (kind <> 'category' OR parent_id IS NULL)
);

CREATE INDEX idx_channels_community_order ON channels (community_id, parent_id, position);
CREATE INDEX idx_channels_scope ON channels (scope);
CREATE INDEX idx_channels_last_message ON channels (community_id, last_message_at DESC);
CREATE INDEX idx_channels_deleted ON channels (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE UNIQUE INDEX uq_channels_name_active
    ON channels (
        community_id,
        COALESCE(parent_id, '00000000-0000-0000-0000-000000000000'::uuid),
        lower(name)
    )
    WHERE deleted_at IS NULL AND scope = 'community';

CREATE TABLE invites (
    id uuid PRIMARY KEY,
    code text NOT NULL UNIQUE,
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    channel_id uuid REFERENCES channels(id) ON DELETE SET NULL,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    max_uses integer,
    uses integer NOT NULL DEFAULT 0,
    expires_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_invites_max_uses CHECK (max_uses IS NULL OR max_uses > 0),
    CONSTRAINT ck_invites_uses CHECK (uses >= 0),
    CONSTRAINT ck_invites_use_limit CHECK (max_uses IS NULL OR uses <= max_uses),
    CONSTRAINT ck_invites_expiry CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE INDEX idx_invites_community ON invites (community_id, created_at DESC);
CREATE UNIQUE INDEX uq_invites_active_code ON invites (code) WHERE revoked_at IS NULL;
CREATE INDEX idx_invites_expiry ON invites (expires_at) WHERE revoked_at IS NULL;

CREATE TABLE community_members (
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    nickname text,
    joined_at timestamptz NOT NULL DEFAULT now(),
    communication_disabled_until timestamptz,
    joined_via_invite_id uuid REFERENCES invites(id) ON DELETE SET NULL,
    PRIMARY KEY (community_id, user_id),
    CONSTRAINT ck_community_members_nickname CHECK (nickname IS NULL OR char_length(nickname) <= 64)
);

CREATE INDEX idx_community_members_user ON community_members (user_id);
CREATE INDEX idx_community_members_community_joined ON community_members (community_id, joined_at DESC);
CREATE INDEX idx_community_members_timeout ON community_members (community_id)
    WHERE communication_disabled_until IS NOT NULL;

CREATE TABLE roles (
    id uuid PRIMARY KEY,
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    name text NOT NULL,
    color integer NOT NULL DEFAULT 0,
    position integer NOT NULL DEFAULT 0,
    permissions bigint NOT NULL DEFAULT 0,
    is_default boolean NOT NULL DEFAULT false,
    is_managed boolean NOT NULL DEFAULT false,
    mentionable boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_roles_name CHECK (char_length(name) BETWEEN 1 AND 64),
    CONSTRAINT ck_roles_color CHECK (color BETWEEN 0 AND 16777215),
    CONSTRAINT ck_roles_permissions CHECK (permissions >= 0),
    CONSTRAINT uq_roles_community_id UNIQUE (community_id, id)
);

CREATE INDEX idx_roles_community_position ON roles (community_id, position DESC);
CREATE UNIQUE INDEX uq_roles_one_default ON roles (community_id) WHERE is_default;

CREATE TABLE member_roles (
    community_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role_id uuid NOT NULL,
    assigned_at timestamptz NOT NULL DEFAULT now(),
    assigned_by uuid REFERENCES users(id) ON DELETE SET NULL,
    PRIMARY KEY (community_id, user_id, role_id),
    CONSTRAINT fk_member_roles_member FOREIGN KEY (community_id, user_id)
        REFERENCES community_members(community_id, user_id) ON DELETE CASCADE,
    CONSTRAINT fk_member_roles_role FOREIGN KEY (community_id, role_id)
        REFERENCES roles(community_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_member_roles_role ON member_roles (role_id);

CREATE TABLE bans (
    community_id uuid NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    moderator_id uuid REFERENCES users(id) ON DELETE SET NULL,
    reason text,
    expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (community_id, user_id),
    CONSTRAINT ck_bans_reason CHECK (reason IS NULL OR char_length(reason) <= 500)
);

CREATE INDEX idx_bans_community ON bans (community_id, created_at DESC);
CREATE INDEX idx_bans_user ON bans (user_id);

CREATE TABLE channel_permission_overwrites (
    id uuid PRIMARY KEY,
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    role_id uuid REFERENCES roles(id) ON DELETE CASCADE,
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    allow bigint NOT NULL DEFAULT 0,
    deny bigint NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_overwrites_target CHECK (
        (role_id IS NOT NULL AND user_id IS NULL)
        OR (role_id IS NULL AND user_id IS NOT NULL)
    ),
    CONSTRAINT ck_overwrites_allow CHECK (allow >= 0),
    CONSTRAINT ck_overwrites_deny CHECK (deny >= 0),
    CONSTRAINT ck_overwrites_disjoint CHECK ((allow & deny) = 0)
);

CREATE INDEX idx_overwrites_channel ON channel_permission_overwrites (channel_id);
CREATE UNIQUE INDEX uq_overwrites_role ON channel_permission_overwrites (channel_id, role_id)
    WHERE role_id IS NOT NULL;
CREATE UNIQUE INDEX uq_overwrites_user ON channel_permission_overwrites (channel_id, user_id)
    WHERE user_id IS NOT NULL;

CREATE TABLE direct_conversations (
    channel_id uuid PRIMARY KEY REFERENCES channels(id) ON DELETE CASCADE,
    user_a_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_direct_pair CHECK (user_a_id < user_b_id),
    CONSTRAINT uq_direct_pair UNIQUE (user_a_id, user_b_id),
    CONSTRAINT ck_direct_creator CHECK (created_by IS NULL OR created_by IN (user_a_id, user_b_id))
);

CREATE INDEX idx_direct_user_a ON direct_conversations (user_a_id);
CREATE INDEX idx_direct_user_b ON direct_conversations (user_b_id);

CREATE TABLE friendships (
    id uuid PRIMARY KEY,
    user_a_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    requested_by uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status text NOT NULL DEFAULT 'pending',
    created_at timestamptz NOT NULL DEFAULT now(),
    responded_at timestamptz,
    CONSTRAINT ck_friendships_pair CHECK (user_a_id < user_b_id),
    CONSTRAINT ck_friendships_status CHECK (status IN ('pending', 'accepted', 'declined')),
    CONSTRAINT ck_friendships_requester CHECK (requested_by IN (user_a_id, user_b_id)),
    CONSTRAINT uq_friendships_pair UNIQUE (user_a_id, user_b_id)
);

CREATE INDEX idx_friendships_a ON friendships (user_a_id, status);
CREATE INDEX idx_friendships_b ON friendships (user_b_id, status);
CREATE INDEX idx_friendships_requester ON friendships (requested_by, created_at DESC);

CREATE TABLE user_blocks (
    blocker_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason text,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT ck_user_blocks_self CHECK (blocker_id <> blocked_id),
    CONSTRAINT ck_user_blocks_reason CHECK (reason IS NULL OR char_length(reason) <= 500)
);

CREATE INDEX idx_user_blocks_blocked ON user_blocks (blocked_id);
