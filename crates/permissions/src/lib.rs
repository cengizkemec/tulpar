//! Permission bits and the centralized authorization evaluator.

#![forbid(unsafe_code)]

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

bitflags! {
    /// Community permissions. Bits 0-37 are defined; 63 is reserved.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Permissions: u64 {
        const VIEW_CHANNEL = 1 << 0;
        const READ_MESSAGE_HISTORY = 1 << 1;
        const SEND_MESSAGES = 1 << 2;
        const EMBED_LINKS = 1 << 3;
        const ATTACH_FILES = 1 << 4;
        const ADD_REACTIONS = 1 << 5;
        const MENTION_EVERYONE = 1 << 6;
        const MENTION_ROLES = 1 << 7;
        const MANAGE_MESSAGES = 1 << 8;
        const PIN_MESSAGES = 1 << 9;
        const MANAGE_CHANNELS = 1 << 10;
        const MANAGE_ROLES = 1 << 11;
        const MANAGE_COMMUNITY = 1 << 12;
        const CREATE_INVITES = 1 << 13;
        const MANAGE_INVITES = 1 << 14;
        const KICK_MEMBERS = 1 << 15;
        const BAN_MEMBERS = 1 << 16;
        const MODERATE_MEMBERS = 1 << 17;
        const CHANGE_NICKNAME = 1 << 18;
        const MANAGE_NICKNAMES = 1 << 19;
        const VIEW_AUDIT_LOG = 1 << 20;
        const VIEW_MEMBER_LIST = 1 << 21;
        const CONNECT_VOICE = 1 << 22;
        const SPEAK = 1 << 23;
        const STREAM_VIDEO = 1 << 24;
        const SHARE_SCREEN = 1 << 25;
        const USE_VOICE_ACTIVITY = 1 << 26;
        const PRIORITY_SPEAKER = 1 << 27;
        const MUTE_MEMBERS = 1 << 28;
        const DEAFEN_MEMBERS = 1 << 29;
        const MOVE_MEMBERS = 1 << 30;
        const MANAGE_WEBHOOKS = 1 << 31;
        const MANAGE_INTEGRATIONS = 1 << 32;
        const ADMINISTRATOR = 1 << 33;
        const BYPASS_SLOWMODE = 1 << 34;
        const CREATE_THREADS = 1 << 35;
        const SEND_MESSAGES_IN_THREADS = 1 << 36;
        const MANAGE_THREADS = 1 << 37;
    }
}

/// The default `@everyone` permissions.
pub const DEFAULT_MEMBER_PERMISSIONS: Permissions = Permissions::VIEW_CHANNEL
    .union(Permissions::READ_MESSAGE_HISTORY)
    .union(Permissions::SEND_MESSAGES)
    .union(Permissions::EMBED_LINKS)
    .union(Permissions::ATTACH_FILES)
    .union(Permissions::ADD_REACTIONS)
    .union(Permissions::CHANGE_NICKNAME)
    .union(Permissions::VIEW_MEMBER_LIST)
    .union(Permissions::CONNECT_VOICE)
    .union(Permissions::SPEAK)
    .union(Permissions::STREAM_VIDEO)
    .union(Permissions::SHARE_SCREEN)
    .union(Permissions::USE_VOICE_ACTIVITY)
    .union(Permissions::CREATE_THREADS)
    .union(Permissions::SEND_MESSAGES_IN_THREADS);

/// Permissions removed while a member is timed out.
pub const TIMEOUT_REMOVED: Permissions = Permissions::SEND_MESSAGES
    .union(Permissions::ADD_REACTIONS)
    .union(Permissions::SPEAK)
    .union(Permissions::STREAM_VIDEO)
    .union(Permissions::SHARE_SCREEN)
    .union(Permissions::CREATE_THREADS)
    .union(Permissions::SEND_MESSAGES_IN_THREADS);

/// One role relevant to an authorization decision.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub permissions: Permissions,
    pub position: i32,
    pub is_default: bool,
}

impl Role {
    pub fn new(id: Uuid, permissions: Permissions, position: i32, is_default: bool) -> Self {
        Self {
            id,
            permissions,
            position,
            is_default,
        }
    }
}

/// The target of a channel permission overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverwriteTarget {
    Role(Uuid),
    Member(Uuid),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Overwrite {
    pub target: OverwriteTarget,
    pub allow: Permissions,
    pub deny: Permissions,
}

impl Overwrite {
    pub fn new(target: OverwriteTarget, allow: Permissions, deny: Permissions) -> Self {
        Self {
            target,
            allow,
            deny,
        }
    }
}

/// Inputs for a single channel authorization decision.
#[derive(Debug, Clone, Copy)]
pub struct PermissionContext<'a> {
    pub is_owner: bool,
    pub member_id: Uuid,
    pub roles: &'a [Role],
    pub category_overwrites: &'a [Overwrite],
    pub channel_overwrites: &'a [Overwrite],
    pub timed_out: bool,
}

/// Evaluate effective permissions for one member/channel pair.
pub fn evaluate(context: PermissionContext<'_>) -> Permissions {
    if context.is_owner {
        return Permissions::all();
    }

    let mut permissions = context
        .roles
        .iter()
        .fold(Permissions::empty(), |acc, role| acc | role.permissions);

    if permissions.contains(Permissions::ADMINISTRATOR) {
        return Permissions::all();
    }

    permissions = apply_overwrites(permissions, context.category_overwrites, &context);
    permissions = apply_overwrites(permissions, context.channel_overwrites, &context);

    if context.timed_out {
        permissions.remove(TIMEOUT_REMOVED);
    }

    permissions
}

fn apply_overwrites(
    mut permissions: Permissions,
    overwrites: &[Overwrite],
    context: &PermissionContext<'_>,
) -> Permissions {
    // @everyone first.
    for overwrite in overwrites {
        if let OverwriteTarget::Role(role_id) = overwrite.target {
            if context
                .roles
                .iter()
                .any(|role| role.id == role_id && role.is_default)
            {
                permissions = (permissions | overwrite.allow) & !overwrite.deny;
            }
        }
    }

    // All member roles combined; deny wins within this layer.
    let mut role_allow = Permissions::empty();
    let mut role_deny = Permissions::empty();
    for overwrite in overwrites {
        if let OverwriteTarget::Role(role_id) = overwrite.target {
            if context
                .roles
                .iter()
                .any(|role| role.id == role_id && !role.is_default)
            {
                role_allow |= overwrite.allow;
                role_deny |= overwrite.deny;
            }
        }
    }
    permissions = (permissions | role_allow) & !role_deny;

    // Member-specific overwrite is last.
    for overwrite in overwrites {
        if overwrite.target == OverwriteTarget::Member(context.member_id) {
            permissions = (permissions | overwrite.allow) & !overwrite.deny;
        }
    }

    permissions
}

impl Permissions {
    /// Names in bit order, suitable for API responses.
    pub fn names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        for (name, _) in self.iter_names() {
            names.push(name);
        }
        names
    }

    pub fn from_names(names: &[String]) -> Result<Self, String> {
        let mut permissions = Permissions::empty();
        for name in names {
            let permission = match name.as_str() {
                "VIEW_CHANNEL" => Permissions::VIEW_CHANNEL,
                "READ_MESSAGE_HISTORY" => Permissions::READ_MESSAGE_HISTORY,
                "SEND_MESSAGES" => Permissions::SEND_MESSAGES,
                "EMBED_LINKS" => Permissions::EMBED_LINKS,
                "ATTACH_FILES" => Permissions::ATTACH_FILES,
                "ADD_REACTIONS" => Permissions::ADD_REACTIONS,
                "MENTION_EVERYONE" => Permissions::MENTION_EVERYONE,
                "MENTION_ROLES" => Permissions::MENTION_ROLES,
                "MANAGE_MESSAGES" => Permissions::MANAGE_MESSAGES,
                "PIN_MESSAGES" => Permissions::PIN_MESSAGES,
                "MANAGE_CHANNELS" => Permissions::MANAGE_CHANNELS,
                "MANAGE_ROLES" => Permissions::MANAGE_ROLES,
                "MANAGE_COMMUNITY" => Permissions::MANAGE_COMMUNITY,
                "CREATE_INVITES" => Permissions::CREATE_INVITES,
                "MANAGE_INVITES" => Permissions::MANAGE_INVITES,
                "KICK_MEMBERS" => Permissions::KICK_MEMBERS,
                "BAN_MEMBERS" => Permissions::BAN_MEMBERS,
                "MODERATE_MEMBERS" => Permissions::MODERATE_MEMBERS,
                "CHANGE_NICKNAME" => Permissions::CHANGE_NICKNAME,
                "MANAGE_NICKNAMES" => Permissions::MANAGE_NICKNAMES,
                "VIEW_AUDIT_LOG" => Permissions::VIEW_AUDIT_LOG,
                "VIEW_MEMBER_LIST" => Permissions::VIEW_MEMBER_LIST,
                "CONNECT_VOICE" => Permissions::CONNECT_VOICE,
                "SPEAK" => Permissions::SPEAK,
                "STREAM_VIDEO" => Permissions::STREAM_VIDEO,
                "SHARE_SCREEN" => Permissions::SHARE_SCREEN,
                "USE_VOICE_ACTIVITY" => Permissions::USE_VOICE_ACTIVITY,
                "PRIORITY_SPEAKER" => Permissions::PRIORITY_SPEAKER,
                "MUTE_MEMBERS" => Permissions::MUTE_MEMBERS,
                "DEAFEN_MEMBERS" => Permissions::DEAFEN_MEMBERS,
                "MOVE_MEMBERS" => Permissions::MOVE_MEMBERS,
                "MANAGE_WEBHOOKS" => Permissions::MANAGE_WEBHOOKS,
                "MANAGE_INTEGRATIONS" => Permissions::MANAGE_INTEGRATIONS,
                "ADMINISTRATOR" => Permissions::ADMINISTRATOR,
                "BYPASS_SLOWMODE" => Permissions::BYPASS_SLOWMODE,
                "CREATE_THREADS" => Permissions::CREATE_THREADS,
                "SEND_MESSAGES_IN_THREADS" => Permissions::SEND_MESSAGES_IN_THREADS,
                "MANAGE_THREADS" => Permissions::MANAGE_THREADS,
                _ => return Err(format!("unknown permission: {name}")),
            };
            permissions |= permission;
        }
        Ok(permissions)
    }

    pub fn highest_role_position(roles: &[Role]) -> i32 {
        roles.iter().map(|role| role.position).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::now_v7()
    }

    #[test]
    fn default_member_can_view_and_send() {
        let role = Role::new(id(), DEFAULT_MEMBER_PERMISSIONS, 0, true);
        let perms = evaluate(PermissionContext {
            is_owner: false,
            member_id: id(),
            roles: &[role],
            category_overwrites: &[],
            channel_overwrites: &[],
            timed_out: false,
        });
        assert!(perms.contains(Permissions::VIEW_CHANNEL));
        assert!(perms.contains(Permissions::SEND_MESSAGES));
        assert!(!perms.contains(Permissions::MANAGE_COMMUNITY));
    }

    #[test]
    fn administrator_bypasses_overwrites() {
        let admin = Role::new(id(), Permissions::ADMINISTRATOR, 10, false);
        let channel = Overwrite::new(
            OverwriteTarget::Member(id()),
            Permissions::empty(),
            Permissions::VIEW_CHANNEL,
        );
        let perms = evaluate(PermissionContext {
            is_owner: false,
            member_id: id(),
            roles: &[admin],
            category_overwrites: &[],
            channel_overwrites: &[channel],
            timed_out: false,
        });
        assert!(perms.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn deny_wins_at_the_same_layer_and_member_override_is_last() {
        let member_id = id();
        let role_a = Role::new(id(), DEFAULT_MEMBER_PERMISSIONS, 1, false);
        let role_b = Role::new(id(), DEFAULT_MEMBER_PERMISSIONS, 2, false);
        let overwrite_a = Overwrite::new(
            OverwriteTarget::Role(role_a.id),
            Permissions::empty(),
            Permissions::SEND_MESSAGES,
        );
        let overwrite_b = Overwrite::new(
            OverwriteTarget::Role(role_b.id),
            Permissions::SEND_MESSAGES,
            Permissions::empty(),
        );
        let member = Overwrite::new(
            OverwriteTarget::Member(member_id),
            Permissions::SEND_MESSAGES,
            Permissions::empty(),
        );
        let perms = evaluate(PermissionContext {
            is_owner: false,
            member_id,
            roles: &[role_a, role_b],
            category_overwrites: &[],
            channel_overwrites: &[overwrite_a, overwrite_b, member],
            timed_out: false,
        });
        assert!(perms.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn channel_overwrite_has_priority_over_category_overwrite() {
        let role = Role::new(id(), DEFAULT_MEMBER_PERMISSIONS, 1, true);
        let category = Overwrite::new(
            OverwriteTarget::Role(role.id),
            Permissions::empty(),
            Permissions::SEND_MESSAGES,
        );
        let channel = Overwrite::new(
            OverwriteTarget::Role(role.id),
            Permissions::SEND_MESSAGES,
            Permissions::empty(),
        );
        let perms = evaluate(PermissionContext {
            is_owner: false,
            member_id: id(),
            roles: &[role],
            category_overwrites: &[category],
            channel_overwrites: &[channel],
            timed_out: false,
        });
        assert!(perms.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn timeout_removes_send_and_voice_publish() {
        let role = Role::new(id(), DEFAULT_MEMBER_PERMISSIONS, 0, true);
        let perms = evaluate(PermissionContext {
            is_owner: false,
            member_id: id(),
            roles: &[role],
            category_overwrites: &[],
            channel_overwrites: &[],
            timed_out: true,
        });
        assert!(!perms.contains(Permissions::SEND_MESSAGES));
        assert!(!perms.contains(Permissions::SPEAK));
        assert!(perms.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn owner_has_every_permission() {
        let perms = evaluate(PermissionContext {
            is_owner: true,
            member_id: id(),
            roles: &[],
            category_overwrites: &[],
            channel_overwrites: &[],
            timed_out: true,
        });
        assert_eq!(perms, Permissions::all());
    }

    #[test]
    fn names_round_trip() {
        let names = vec!["VIEW_CHANNEL".to_string(), "SEND_MESSAGES".to_string()];
        let permissions = Permissions::from_names(&names).unwrap();
        assert!(permissions.contains(Permissions::VIEW_CHANNEL));
        assert!(permissions.contains(Permissions::SEND_MESSAGES));
        assert!(Permissions::from_names(&["NOT_REAL".to_string()]).is_err());
    }
}
