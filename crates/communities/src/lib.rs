//! Communities, members, roles, invitations, and moderation.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use common::{normalize_text, random_token};
use permissions::{Permissions, Role as PermissionRole, DEFAULT_MEMBER_PERMISSIONS};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

/// Errors returned by the community layer.
#[derive(Debug, Error)]
pub enum CommunityError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("community not found")]
    NotFound,
    #[error("member not found")]
    MemberNotFound,
    #[error("role not found")]
    RoleNotFound,
    #[error("invite not found or no longer valid")]
    InviteInvalid,
    #[error("already a member")]
    AlreadyMember,
    #[error("banned from this community")]
    Banned,
    #[error("a role with that name already exists")]
    RoleConflict,
    #[error("invalid community data: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Community {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub icon_file_id: Option<Uuid>,
    pub banner_file_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub visibility: String,
    pub default_notifications: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CommunityMember {
    pub community_id: Uuid,
    pub user_id: Uuid,
    pub nickname: Option<String>,
    pub joined_at: DateTime<Utc>,
    pub communication_disabled_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Role {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub color: i32,
    pub position: i32,
    pub permissions: Permissions,
    pub is_default: bool,
    pub is_managed: bool,
    pub mentionable: bool,
}

#[derive(Debug, Clone, FromRow)]
struct RoleRow {
    id: Uuid,
    community_id: Uuid,
    name: String,
    color: i32,
    position: i32,
    permissions: i64,
    is_default: bool,
    is_managed: bool,
    mentionable: bool,
}

impl From<RoleRow> for Role {
    fn from(value: RoleRow) -> Self {
        Self {
            id: value.id,
            community_id: value.community_id,
            name: value.name,
            color: value.color,
            position: value.position,
            permissions: Permissions::from_bits_truncate(value.permissions as u64),
            is_default: value.is_default,
            is_managed: value.is_managed,
            mentionable: value.mentionable,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct MemberView {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_file_id: Option<Uuid>,
    pub nickname: Option<String>,
    pub joined_at: DateTime<Utc>,
    pub communication_disabled_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CommunityAccess {
    pub community: Community,
    pub member: CommunityMember,
    pub roles: Vec<Role>,
    pub permissions: Permissions,
    pub highest_role_position: i32,
    pub is_owner: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Invite {
    pub id: Uuid,
    pub code: String,
    pub community_id: Uuid,
    pub channel_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub max_uses: Option<i32>,
    pub uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateCommunity {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub visibility: String,
}

#[derive(Debug, Clone)]
pub struct CreateInvite {
    pub community_id: Uuid,
    pub channel_id: Option<Uuid>,
    pub created_by: Uuid,
    pub max_uses: Option<i32>,
    pub expires_in_hours: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct JoinResult {
    pub community_id: Uuid,
    pub already_member: bool,
}

#[derive(Clone)]
pub struct CommunityService {
    pool: PgPool,
}

impl CommunityService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        owner_id: Uuid,
        input: CreateCommunity,
    ) -> Result<Community, CommunityError> {
        let name = normalize_text(&input.name);
        let slug = input.slug.trim().to_lowercase();
        if !(2..=100).contains(&name.chars().count()) {
            return Err(CommunityError::Invalid(
                "community name must be between 2 and 100 characters".into(),
            ));
        }
        if !(2..=100).contains(&slug.chars().count())
            || !slug.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '-' | '_')
            })
        {
            return Err(CommunityError::Invalid("invalid community slug".into()));
        }
        if !matches!(input.visibility.as_str(), "public" | "private") {
            return Err(CommunityError::Invalid(
                "invalid community visibility".into(),
            ));
        }

        let id = tulpar_core::new_id();
        let mut tx = self.pool.begin().await?;
        let community = sqlx::query_as::<_, Community>(
            r#"
            INSERT INTO communities (id, name, slug, description, owner_user_id, visibility)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, name, slug::text AS slug, description, icon_file_id, banner_file_id,
                      owner_user_id, visibility, default_notifications, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(&name)
        .bind(&slug)
        .bind(input.description)
        .bind(owner_id)
        .bind(input.visibility)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_community_error)?;

        sqlx::query("INSERT INTO community_members (community_id, user_id) VALUES ($1, $2)")
            .bind(id)
            .bind(owner_id)
            .execute(&mut *tx)
            .await?;

        // @everyone, Moderator, and Administrator templates.
        let default_permissions = DEFAULT_MEMBER_PERMISSIONS.bits() as i64;
        sqlx::query(
            "INSERT INTO roles (id, community_id, name, position, permissions, is_default) VALUES ($1, $2, '@everyone', 0, $3, true)",
        )
        .bind(tulpar_core::new_id())
        .bind(id)
        .bind(default_permissions)
        .execute(&mut *tx)
        .await?;

        let moderator_permissions = (Permissions::MANAGE_MESSAGES
            | Permissions::PIN_MESSAGES
            | Permissions::KICK_MEMBERS
            | Permissions::MODERATE_MEMBERS
            | Permissions::VIEW_AUDIT_LOG
            | Permissions::MUTE_MEMBERS
            | Permissions::DEAFEN_MEMBERS
            | Permissions::MOVE_MEMBERS
            | Permissions::MANAGE_THREADS)
            .bits() as i64;
        sqlx::query(
            "INSERT INTO roles (id, community_id, name, position, permissions) VALUES ($1, $2, 'Moderator', 50, $3)",
        )
        .bind(tulpar_core::new_id())
        .bind(id)
        .bind(moderator_permissions)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO roles (id, community_id, name, position, permissions) VALUES ($1, $2, 'Administrator', 100, $3)",
        )
        .bind(tulpar_core::new_id())
        .bind(id)
        .bind(Permissions::ADMINISTRATOR.bits() as i64)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(community)
    }

    pub async fn get(&self, community_id: Uuid) -> Result<Community, CommunityError> {
        sqlx::query_as::<_, Community>(
            r#"
            SELECT id, name, slug::text AS slug, description, icon_file_id, banner_file_id,
                   owner_user_id, visibility, default_notifications, created_at, updated_at
            FROM communities
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(community_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CommunityError::NotFound)
    }

    pub async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<Community>, CommunityError> {
        let communities = sqlx::query_as::<_, Community>(
            r#"
            SELECT c.id, c.name, c.slug::text AS slug, c.description, c.icon_file_id,
                   c.banner_file_id, c.owner_user_id, c.visibility, c.default_notifications,
                   c.created_at, c.updated_at
            FROM communities c
            JOIN community_members m ON m.community_id = c.id
            WHERE m.user_id = $1 AND c.deleted_at IS NULL
            ORDER BY c.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(communities)
    }

    pub async fn update(
        &self,
        community_id: Uuid,
        name: Option<String>,
        description: Option<String>,
        visibility: Option<String>,
    ) -> Result<Community, CommunityError> {
        if let Some(visibility) = &visibility {
            if !matches!(visibility.as_str(), "public" | "private") {
                return Err(CommunityError::Invalid("invalid visibility".into()));
            }
        }
        let community = sqlx::query_as::<_, Community>(
            r#"
            UPDATE communities
            SET name = COALESCE($2, name),
                description = CASE WHEN $3::text IS NULL THEN description WHEN $3 = '' THEN NULL ELSE $3 END,
                visibility = COALESCE($4, visibility),
                updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING id, name, slug::text AS slug, description, icon_file_id, banner_file_id,
                      owner_user_id, visibility, default_notifications, created_at, updated_at
            "#,
        )
        .bind(community_id)
        .bind(name.map(|value| normalize_text(&value)))
        .bind(description)
        .bind(visibility)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CommunityError::NotFound)?;
        Ok(community)
    }

    pub async fn soft_delete(&self, community_id: Uuid) -> Result<(), CommunityError> {
        let result = sqlx::query(
            "UPDATE communities SET deleted_at = now(), updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(community_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(CommunityError::NotFound);
        }
        Ok(())
    }

    pub async fn member(
        &self,
        community_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CommunityMember>, CommunityError> {
        let member = sqlx::query_as::<_, CommunityMember>(
            r#"
            SELECT community_id, user_id, nickname, joined_at, communication_disabled_until
            FROM community_members
            WHERE community_id = $1 AND user_id = $2
            "#,
        )
        .bind(community_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(member)
    }

    pub async fn access(
        &self,
        community_id: Uuid,
        user_id: Uuid,
    ) -> Result<CommunityAccess, CommunityError> {
        let community = self.get(community_id).await?;
        let member = self
            .member(community_id, user_id)
            .await?
            .ok_or(CommunityError::MemberNotFound)?;
        let roles = self.member_roles(community_id, user_id).await?;
        let is_owner = community.owner_user_id == user_id;
        let timed_out = member
            .communication_disabled_until
            .is_some_and(|until| until > Utc::now());
        let permission_roles: Vec<PermissionRole> = roles
            .iter()
            .map(|role| {
                PermissionRole::new(role.id, role.permissions, role.position, role.is_default)
            })
            .collect();
        let permissions = permissions::evaluate(permissions::PermissionContext {
            is_owner,
            member_id: user_id,
            roles: &permission_roles,
            category_overwrites: &[],
            channel_overwrites: &[],
            timed_out,
        });
        let highest_role_position = Permissions::highest_role_position(&permission_roles);
        Ok(CommunityAccess {
            community,
            member,
            roles,
            permissions,
            highest_role_position,
            is_owner,
        })
    }

    pub async fn member_roles(
        &self,
        community_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<Role>, CommunityError> {
        let rows = sqlx::query_as::<_, RoleRow>(
            r#"
            SELECT r.id, r.community_id, r.name, r.color, r.position, r.permissions,
                   r.is_default, r.is_managed, r.mentionable
            FROM roles r
            LEFT JOIN member_roles mr
              ON mr.role_id = r.id AND mr.community_id = r.community_id AND mr.user_id = $2
            WHERE r.community_id = $1 AND (r.is_default OR mr.user_id IS NOT NULL)
            ORDER BY r.position DESC, r.id
            "#,
        )
        .bind(community_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Role::from).collect())
    }

    pub async fn list_roles(&self, community_id: Uuid) -> Result<Vec<Role>, CommunityError> {
        let rows = sqlx::query_as::<_, RoleRow>(
            r#"
            SELECT id, community_id, name, color, position, permissions,
                   is_default, is_managed, mentionable
            FROM roles
            WHERE community_id = $1
            ORDER BY position DESC, id
            "#,
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Role::from).collect())
    }

    pub async fn get_role(&self, role_id: Uuid) -> Result<Role, CommunityError> {
        let row = sqlx::query_as::<_, RoleRow>(
            "SELECT id, community_id, name, color, position, permissions, is_default, is_managed, mentionable FROM roles WHERE id = $1",
        )
        .bind(role_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CommunityError::RoleNotFound)?;
        Ok(row.into())
    }

    pub async fn create_role(
        &self,
        community_id: Uuid,
        name: &str,
        color: i32,
        position: i32,
        permissions: Permissions,
        mentionable: bool,
    ) -> Result<Role, CommunityError> {
        let name = normalize_text(name);
        if !(1..=64).contains(&name.chars().count()) {
            return Err(CommunityError::Invalid("invalid role name".into()));
        }
        if position < 0 {
            return Err(CommunityError::Invalid("invalid role position".into()));
        }
        let row = sqlx::query_as::<_, RoleRow>(
            r#"
            INSERT INTO roles (id, community_id, name, color, position, permissions, mentionable)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, community_id, name, color, position, permissions, is_default, is_managed, mentionable
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(community_id)
        .bind(name)
        .bind(color)
        .bind(position)
        .bind(permissions.bits() as i64)
        .bind(mentionable)
        .fetch_one(&self.pool)
        .await
        .map_err(map_role_error)?;
        Ok(row.into())
    }

    pub async fn update_role(
        &self,
        role_id: Uuid,
        name: Option<String>,
        color: Option<i32>,
        position: Option<i32>,
        permissions: Option<Permissions>,
        mentionable: Option<bool>,
    ) -> Result<Role, CommunityError> {
        let row = sqlx::query_as::<_, RoleRow>(
            r#"
            UPDATE roles
            SET name = COALESCE($2, name),
                color = COALESCE($3, color),
                position = COALESCE($4, position),
                permissions = COALESCE($5, permissions),
                mentionable = COALESCE($6, mentionable),
                updated_at = now()
            WHERE id = $1 AND is_default = false
            RETURNING id, community_id, name, color, position, permissions, is_default, is_managed, mentionable
            "#,
        )
        .bind(role_id)
        .bind(name.map(|value| normalize_text(&value)))
        .bind(color)
        .bind(position)
        .bind(permissions.map(|value| value.bits() as i64))
        .bind(mentionable)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CommunityError::RoleNotFound)?;
        Ok(row.into())
    }

    pub async fn delete_role(&self, role_id: Uuid) -> Result<(), CommunityError> {
        let result = sqlx::query("DELETE FROM roles WHERE id = $1 AND is_default = false")
            .bind(role_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(CommunityError::RoleNotFound);
        }
        Ok(())
    }

    pub async fn assign_role(
        &self,
        community_id: Uuid,
        user_id: Uuid,
        role_id: Uuid,
        assigned_by: Uuid,
    ) -> Result<(), CommunityError> {
        sqlx::query(
            "INSERT INTO member_roles (community_id, user_id, role_id, assigned_by) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(community_id)
        .bind(user_id)
        .bind(role_id)
        .bind(assigned_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_role(
        &self,
        community_id: Uuid,
        user_id: Uuid,
        role_id: Uuid,
    ) -> Result<(), CommunityError> {
        sqlx::query(
            "DELETE FROM member_roles WHERE community_id = $1 AND user_id = $2 AND role_id = $3",
        )
        .bind(community_id)
        .bind(user_id)
        .bind(role_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_members(
        &self,
        community_id: Uuid,
    ) -> Result<Vec<MemberView>, CommunityError> {
        let members = sqlx::query_as::<_, MemberView>(
            r#"
            SELECT m.user_id, u.username::text AS username, u.display_name, u.avatar_file_id,
                   m.nickname, m.joined_at, m.communication_disabled_until
            FROM community_members m
            JOIN users u ON u.id = m.user_id
            WHERE m.community_id = $1 AND u.deleted_at IS NULL
            ORDER BY m.joined_at ASC
            "#,
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(members)
    }

    pub async fn add_member(
        &self,
        community_id: Uuid,
        user_id: Uuid,
        invite_id: Option<Uuid>,
    ) -> Result<bool, CommunityError> {
        if self.is_banned(community_id, user_id).await? {
            return Err(CommunityError::Banned);
        }
        let result = sqlx::query(
            "INSERT INTO community_members (community_id, user_id, joined_via_invite_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(community_id)
        .bind(user_id)
        .bind(invite_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn join_public(
        &self,
        community_id: Uuid,
        user_id: Uuid,
    ) -> Result<JoinResult, CommunityError> {
        let community = self.get(community_id).await?;
        if community.visibility != "public" {
            return Err(CommunityError::NotFound);
        }
        let inserted = self.add_member(community_id, user_id, None).await?;
        Ok(JoinResult {
            community_id,
            already_member: !inserted,
        })
    }

    pub async fn create_invite(&self, input: CreateInvite) -> Result<Invite, CommunityError> {
        let code = random_token(9);
        let expires_at = input
            .expires_in_hours
            .map(|hours| Utc::now() + chrono::Duration::hours(hours));
        let invite = sqlx::query_as::<_, Invite>(
            r#"
            INSERT INTO invites (id, code, community_id, channel_id, created_by, max_uses, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, code, community_id, channel_id, created_by, max_uses, uses, expires_at, created_at
            "#,
        )
        .bind(tulpar_core::new_id())
        .bind(code)
        .bind(input.community_id)
        .bind(input.channel_id)
        .bind(input.created_by)
        .bind(input.max_uses)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(invite)
    }

    pub async fn preview_invite(&self, code: &str) -> Result<(Invite, Community), CommunityError> {
        let invite = sqlx::query_as::<_, Invite>(
            r#"
            SELECT id, code, community_id, channel_id, created_by, max_uses, uses, expires_at, created_at
            FROM invites
            WHERE code = $1 AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > now())
              AND (max_uses IS NULL OR uses < max_uses)
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CommunityError::InviteInvalid)?;
        let community = self.get(invite.community_id).await?;
        Ok((invite, community))
    }

    pub async fn join_invite(
        &self,
        code: &str,
        user_id: Uuid,
    ) -> Result<JoinResult, CommunityError> {
        let mut tx = self.pool.begin().await?;
        let invite = sqlx::query_as::<_, Invite>(
            r#"
            SELECT id, code, community_id, channel_id, created_by, max_uses, uses, expires_at, created_at
            FROM invites
            WHERE code = $1 AND revoked_at IS NULL
            FOR UPDATE
            "#,
        )
        .bind(code)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(CommunityError::InviteInvalid)?;

        if invite.expires_at.is_some_and(|value| value <= Utc::now())
            || invite.max_uses.is_some_and(|max| invite.uses >= max)
        {
            return Err(CommunityError::InviteInvalid);
        }
        if self.is_banned(invite.community_id, user_id).await? {
            return Err(CommunityError::Banned);
        }

        let inserted = sqlx::query(
            "INSERT INTO community_members (community_id, user_id, joined_via_invite_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(invite.community_id)
        .bind(user_id)
        .bind(invite.id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;

        if inserted {
            sqlx::query("UPDATE invites SET uses = uses + 1 WHERE id = $1")
                .bind(invite.id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(JoinResult {
            community_id: invite.community_id,
            already_member: !inserted,
        })
    }

    pub async fn list_invites(&self, community_id: Uuid) -> Result<Vec<Invite>, CommunityError> {
        let invites = sqlx::query_as::<_, Invite>(
            r#"
            SELECT id, code, community_id, channel_id, created_by, max_uses, uses, expires_at, created_at
            FROM invites
            WHERE community_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(invites)
    }

    pub async fn revoke_invite(&self, code: &str) -> Result<(), CommunityError> {
        let result = sqlx::query(
            "UPDATE invites SET revoked_at = now() WHERE code = $1 AND revoked_at IS NULL",
        )
        .bind(code)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(CommunityError::InviteInvalid);
        }
        Ok(())
    }

    pub async fn kick(&self, community_id: Uuid, user_id: Uuid) -> Result<(), CommunityError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND user_id = $2")
            .bind(community_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM member_roles WHERE community_id = $1 AND user_id = $2")
            .bind(community_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn ban(
        &self,
        community_id: Uuid,
        user_id: Uuid,
        moderator_id: Uuid,
        reason: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), CommunityError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO bans (community_id, user_id, moderator_id, reason, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (community_id, user_id) DO UPDATE
            SET moderator_id = EXCLUDED.moderator_id, reason = EXCLUDED.reason, expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(community_id)
        .bind(user_id)
        .bind(moderator_id)
        .bind(reason)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND user_id = $2")
            .bind(community_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM member_roles WHERE community_id = $1 AND user_id = $2")
            .bind(community_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn unban(&self, community_id: Uuid, user_id: Uuid) -> Result<(), CommunityError> {
        sqlx::query("DELETE FROM bans WHERE community_id = $1 AND user_id = $2")
            .bind(community_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_banned(
        &self,
        community_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, CommunityError> {
        let banned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM bans WHERE community_id = $1 AND user_id = $2 AND (expires_at IS NULL OR expires_at > now()))",
        )
        .bind(community_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(banned)
    }

    pub async fn update_member(
        &self,
        community_id: Uuid,
        user_id: Uuid,
        nickname: Option<String>,
        communication_disabled_until: Option<DateTime<Utc>>,
    ) -> Result<(), CommunityError> {
        let result = sqlx::query(
            r#"
            UPDATE community_members
            SET nickname = CASE WHEN $3::text IS NULL THEN nickname WHEN $3 = '' THEN NULL ELSE $3 END,
                communication_disabled_until = $4
            WHERE community_id = $1 AND user_id = $2
            "#,
        )
        .bind(community_id)
        .bind(user_id)
        .bind(nickname)
        .bind(communication_disabled_until)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(CommunityError::MemberNotFound);
        }
        Ok(())
    }

    pub async fn member_count(&self, community_id: Uuid) -> Result<i64, CommunityError> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM community_members WHERE community_id = $1")
                .bind(community_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn is_member(
        &self,
        community_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, CommunityError> {
        Ok(self.member(community_id, user_id).await?.is_some())
    }

    pub async fn owned_community_count(&self, user_id: Uuid) -> Result<i64, CommunityError> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM communities WHERE owner_user_id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn transfer_ownership(
        &self,
        community_id: Uuid,
        new_owner_id: Uuid,
    ) -> Result<(), CommunityError> {
        if !self.is_member(community_id, new_owner_id).await? {
            return Err(CommunityError::MemberNotFound);
        }
        let result = sqlx::query(
            "UPDATE communities SET owner_user_id = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(community_id)
        .bind(new_owner_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(CommunityError::NotFound);
        }
        Ok(())
    }
}

fn map_community_error(error: sqlx::Error) -> CommunityError {
    if let sqlx::Error::Database(ref database_error) = error {
        if database_error.is_unique_violation() {
            return CommunityError::Invalid("community slug is already in use".into());
        }
    }
    CommunityError::Database(error)
}

fn map_role_error(error: sqlx::Error) -> CommunityError {
    if let sqlx::Error::Database(ref database_error) = error {
        if database_error.is_unique_violation() {
            return CommunityError::RoleConflict;
        }
    }
    CommunityError::Database(error)
}
