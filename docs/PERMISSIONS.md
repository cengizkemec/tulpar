# Tulpar Roles, Permissions, and Authorization

**Status:** Phase 2. Permission bits, role hierarchy, overwrite ordering, administrator/owner bypass, timeout handling, and the pure evaluator are implemented and unit-tested. Channel overwrite REST endpoints and channel-scoped authorization in message/voice paths are still planned.

**Rule zero:** every authorization decision is made by the backend. The frontend may hide controls for usability, but hiding a button is never a security control.

---

## 1. Model overview

Tulpar uses:

- **Community roles** with a numeric position (higher is more senior).
- **A 64-bit permission set** stored per role (bits 0–62; bit 63 reserved).
- **Channel permission overwrites** for roles and individual members.
- **An implicit Owner** (`communities.owner_user_id`) who is above all roles.
- **A default `@everyone` role** that every member effectively has.
- **No global superuser inside a community.** Instance administrators do not automatically gain access to community content.

Permissions are combined, then channel overwrites are applied in a defined order. The evaluator is a pure module with no database access, so it can be exhaustively tested.

---

## 2. Permission bits

Bits are stored in PostgreSQL `bigint` and manipulated as a Rust `u64` masked to bits 0–62. The API exposes permission names, never raw integers.

| Bit | Name | Scope | Meaning |
|---:|---|---|---|
| 0 | `VIEW_CHANNEL` | Channel | See the channel and receive its events |
| 1 | `READ_MESSAGE_HISTORY` | Channel | Read messages sent before the member joined; search history and see old pins |
| 2 | `SEND_MESSAGES` | Channel | Send text messages |
| 3 | `EMBED_LINKS` | Channel | Generate link previews |
| 4 | `ATTACH_FILES` | Channel | Upload and attach files |
| 5 | `ADD_REACTIONS` | Channel | Add and remove reactions |
| 6 | `MENTION_EVERYONE` | Community | Use `@everyone` and `@here` |
| 7 | `MENTION_ROLES` | Community | Mention roles |
| 8 | `MANAGE_MESSAGES` | Channel | Delete others' messages, bulk delete |
| 9 | `PIN_MESSAGES` | Channel | Pin and unpin messages |
| 10 | `MANAGE_CHANNELS` | Community | Create, edit, reorder, delete channels/categories |
| 11 | `MANAGE_ROLES` | Community | Create/edit roles and channel overwrites |
| 12 | `MANAGE_COMMUNITY` | Community | Edit community name, description, icon, banner, visibility |
| 13 | `CREATE_INVITES` | Community | Create invite links |
| 14 | `MANAGE_INVITES` | Community | Revoke any invite, inspect invite usage |
| 15 | `KICK_MEMBERS` | Community | Remove members |
| 16 | `BAN_MEMBERS` | Community | Ban/unban members |
| 17 | `MODERATE_MEMBERS` | Community | Timeout members |
| 18 | `CHANGE_NICKNAME` | Community | Change own nickname |
| 19 | `MANAGE_NICKNAMES` | Community | Change others' nicknames |
| 20 | `VIEW_AUDIT_LOG` | Community | Read community audit logs |
| 21 | `VIEW_MEMBER_LIST` | Community | See the member list and member events |
| 22 | `CONNECT_VOICE` | Channel | Join a voice channel |
| 23 | `SPEAK` | Channel | Publish microphone audio |
| 24 | `STREAM_VIDEO` | Channel | Publish camera video |
| 25 | `SHARE_SCREEN` | Channel | Share a screen or window |
| 26 | `USE_VOICE_ACTIVITY` | Channel | Use voice activity detection; without it, push-to-talk only |
| 27 | `PRIORITY_SPEAKER` | Channel | Lower other speakers' volume while speaking |
| 28 | `MUTE_MEMBERS` | Channel | Server-mute participants |
| 29 | `DEAFEN_MEMBERS` | Channel | Server-deafen participants |
| 30 | `MOVE_MEMBERS` | Channel | Move or disconnect voice participants |
| 31 | `MANAGE_WEBHOOKS` | Community | Create, edit, delete webhooks and view deliveries |
| 32 | `MANAGE_INTEGRATIONS` | Community | Install/remove bots and manage integrations |
| 33 | `ADMINISTRATOR` | Community | All community permissions except owner-only actions; bypasses channel overwrites |
| 34 | `BYPASS_SLOWMODE` | Channel | Ignore channel slowmode |
| 35 | `CREATE_THREADS` | Channel | Create threads/posts in forum channels |
| 36 | `SEND_MESSAGES_IN_THREADS` | Channel | Reply inside threads/posts |
| 37 | `MANAGE_THREADS` | Channel | Rename, lock, archive, delete threads |
| 38–62 | Reserved | — | Must remain zero; clients cannot set them |
| 63 | Reserved | — | Must remain zero; PostgreSQL signed bigint compatibility |

### 2.1 Permission names and validation

- Role and overwrite updates accept arrays of names.
- Unknown names, reserved bits, duplicates, and allow/deny overlap are rejected with `400 validation_error`.
- A role's permissions are returned as a sorted array of names.
- `ADMINISTRATOR` is returned alongside the explicit bits stored on the role. The UI explains that it grants all current and future community permissions except owner-only actions.
- API clients must not compute effective permissions locally for security decisions. They may use the effective-permissions endpoint for UI hints.

### 2.2 Why a bitfield

- Fast evaluation and compact storage.
- Channel overwrites need allow/deny masks; two integers are simpler than thousands of rows.
- Permission checks are on the hot path for every message, event, and voice token.
- The API hides the representation, so a future storage change does not break clients.

The cost is that adding a permission requires a schema-compatible bit allocation and documentation. Bits are never reused or renumbered.

---

## 3. Default roles

Every community has an implicit Owner and a non-deletable default role. The other roles are templates created during community creation.

| Role | Stored? | Default permissions | Notes |
|---|---|---|---|
| **Owner** | Implicit (`communities.owner_user_id`) | All community permissions, including owner-only | Cannot be assigned; transfer is a dedicated operation |
| **Administrator** | Yes | `ADMINISTRATOR` | Only the owner can create/grant a role containing `ADMINISTRATOR` |
| **Moderator** | Yes | `MANAGE_MESSAGES`, `PIN_MESSAGES`, `KICK_MEMBERS`, `MODERATE_MEMBERS`, `VIEW_AUDIT_LOG`, `MUTE_MEMBERS`, `DEAFEN_MEMBERS`, `MOVE_MEMBERS`, `MANAGE_THREADS` | Ban permission is not granted by default; an admin can add it |
| **Member** | The `@everyone` role | `VIEW_CHANNEL`, `READ_MESSAGE_HISTORY`, `SEND_MESSAGES`, `EMBED_LINKS`, `ATTACH_FILES`, `ADD_REACTIONS`, `CHANGE_NICKNAME`, `VIEW_MEMBER_LIST`, `CONNECT_VOICE`, `SPEAK`, `STREAM_VIDEO`, `SHARE_SCREEN`, `USE_VOICE_ACTIVITY`, `CREATE_THREADS`, `SEND_MESSAGES_IN_THREADS` | Every member has this role. The UI may label it “Member (@everyone)” |
| **Guest** | Optional template | `VIEW_CHANNEL`, `READ_MESSAGE_HISTORY` | Created on demand for restricted access; assigned explicitly, usually with channel overwrites |

The `@everyone` role has `is_default = true`, position 0, and cannot be deleted or have its position changed. The Owner is not a role row and is always at the top of the hierarchy.

`ADMINISTRATOR` is stored as a single bit on the Administrator role. The evaluator expands it to all community permissions except owner-only actions. This keeps the role's intent visible and avoids a stale “all bits” mask when new permissions are added.

### 3.1 Owner-only actions

The following are never granted by `ADMINISTRATOR`:

- Delete the community.
- Transfer community ownership.
- Grant or revoke a role containing `ADMINISTRATOR`.
- Remove the last owner (there is always exactly one owner).
- Change the community owner through a normal member update.

The owner can do everything else without an explicit role.

---

## 4. Effective permission evaluation

### 4.1 Inputs

- Community owner status.
- The member's assigned roles, including the default `@everyone` role.
- Channel permission overwrites for the category (if any) and the channel.
- The channel kind (`text`, `announcement`, `voice`, `forum`, `direct`, `category`).
- For DMs: the participant pair and block state.
- Member timeout (`communication_disabled_until`).

### 4.2 Algorithm

```
fn effective_permissions(member, channel):
    if member is community owner:
        return ALL_COMMUNITY_PERMISSIONS

    base = OR(role.permissions for all member roles, including @everyone)

    if base contains ADMINISTRATOR:
        return ALL_COMMUNITY_PERMISSIONS_EXCEPT_OWNER_ONLY

    perms = base

    if channel has a category:
        perms = apply_overwrites(perms, category, member)
    perms = apply_overwrites(perms, channel, member)

    if member is timed out:
        perms = perms - (SEND_MESSAGES, ADD_REACTIONS, SPEAK, STREAM_VIDEO,
                         SHARE_SCREEN, CREATE_THREADS, SEND_MESSAGES_IN_THREADS)

    return perms
```

`apply_overwrites` order:

1. `@everyone` overwrite for that channel/category.
2. All role overwrites for roles the member has. Combine all `allow` bits with OR and all `deny` bits with OR. Apply `(perms | allow) & ~deny`. Deny wins if two roles disagree.
3. Member-specific overwrite last: `(perms | allow) & ~deny`.

At each layer, a deny removes a permission even if another role at the same layer allows it. A later layer can re-grant it. This is intentional and matches the principle that a channel-specific member overwrite is the final word.

### 4.3 Categories

- A category is a `channels` row with `kind = 'category'`.
- Category overwrites apply to its child channels before the child's own overwrites.
- A child can re-grant a permission denied by the category.
- Moving a channel to a different category changes effective permissions immediately.
- Category depth is limited to one level; a category cannot have a parent.
- A category itself has no messages or voice. Access to the category is derived from child visibility.

### 4.4 Administrator bypass

`ADMINISTRATOR` bypasses all channel overwrites. An administrator can see every channel, read history, send messages, and join voice. This is a deliberately powerful role.

The owner can grant `ADMINISTRATOR` only because the owner already has equivalent power. A non-owner administrator cannot create another administrator. This prevents an administrator from minting a peer who could then act against the owner.

### 4.5 Timeouts

`communication_disabled_until` in the future removes:

- `SEND_MESSAGES`
- `ADD_REACTIONS`
- `SPEAK`
- `STREAM_VIDEO`
- `SHARE_SCREEN`
- `CREATE_THREADS`
- `SEND_MESSAGES_IN_THREADS`

A timed-out member can still read visible channels, receive events, and join voice as a listener if `CONNECT_VOICE` remains. The UI shows the timeout expiry. A timeout cannot be applied to the owner or to a member at or above the actor's highest role.

### 4.6 Announcement and forum channels

- **Announcement:** only members with `MANAGE_MESSAGES` can send new messages, regardless of `SEND_MESSAGES`. Others can read and react if they have the relevant permissions. This makes the channel read-only by default.
- **Forum:** creating a post/thread requires `CREATE_THREADS`; replying requires `SEND_MESSAGES_IN_THREADS`; managing threads requires `MANAGE_THREADS`. Forum implementation is later; the permission bits are allocated now to avoid renumbering.
- **Read-only text:** deny `SEND_MESSAGES` with a channel overwrite.

### 4.7 Direct messages

DM permissions are not role-based:

- Both participants can read, send, react, attach files, edit their own messages, and delete their own messages.
- A block prevents new messages and typing in both directions. The backend checks both `blocker_id → blocked_id` and `blocked_id → blocker_id`.
- Existing DM history remains readable by each participant after a block unless they delete the conversation. A future “delete conversation for me” feature is additive.
- DMs cannot be made public, discovered, or accessed by community roles or instance admins.

### 4.8 Public embed access

Embed public widgets are **not** community membership and do not receive a role. They use a separate, read-only public view policy configured on the embed app:

- Community card and member count may be public.
- Channel names and voice presence may be shown only if the embed app enables that widget.
- Message previews are opt-in per app/channel and are read-only.
- Private channels, DMs, audit logs, member emails/IPs, and LiveKit tokens are never exposed.
- The embed API re-checks the app's enabled widgets and the token's origin/scope on every request.

This keeps public discovery from silently granting a normal `@everyone` role or access to every channel.

### 4.9 Instance administrators and content privacy

Instance administrators can:

- Manage users, sessions, instance settings, registration, storage, and maintenance.
- See aggregate counts, system health, and audit metadata.
- Disable an account or force a logout.

Instance administrators cannot automatically:

- Read private community messages.
- Join a private community without an invitation or membership.
- Read another user's DMs.
- Retrieve voice recordings (none exist in v1).

If an operator requires content access for legal or abuse handling, it must be an explicit, documented, audited process with a separate policy. The default product must not normalize silent content surveillance.

---

## 5. Role hierarchy and anti-escalation rules

### 5.1 Position

- Higher `position` = more senior.
- Owner is above every role.
- The default `@everyone` role is position 0.
- Ties are ordered by role UUID for deterministic behavior, but the UI should avoid ties.
- Reordering is a single transaction that rewrites positions; the server rejects an order that would place a role above the actor's highest role unless the actor is the owner.

### 5.2 Actor and target highest role

```
actor_highest = max(position of actor roles, including @everyone)
target_highest = max(position of target roles, including @everyone)
```

For actions on another member (kick, ban, timeout, nickname, role assignment):

- Actor must be the owner, or `actor_highest > target_highest`.
- The owner cannot be kicked, banned, timed out, or have roles removed by anyone, including an administrator.
- A member cannot perform these actions on themselves where it would be destructive (kick/ban/timeout).
- A bot is subject to the same hierarchy as a user.

### 5.3 Role editing

- `@everyone` cannot be deleted, renamed to reserved names, or moved.
- A non-owner cannot create or edit a role at or above their highest role.
- A non-owner cannot grant a permission they do not hold.
- Only the owner can create, edit, assign, or revoke a role containing `ADMINISTRATOR`.
- A role cannot be assigned to a non-member.
- A role from another community is rejected by the composite foreign key and by the service.
- Deleting a role removes its assignments and channel overwrites; the operation is audited.

### 5.4 Channel overwrites

- Setting an overwrite requires `MANAGE_ROLES` and the hierarchy/anti-escalation checks. `MANAGE_CHANNELS` is not sufficient on its own.
- A non-owner cannot grant a permission they do not hold through an overwrite.
- A member overwrite cannot grant a permission the member could not receive from their roles? **It can** re-grant a denied permission, but only if the actor holds that permission and the target is not above the actor. This is intentional: member-specific overrides are a legitimate moderation tool.
- Overwrites cannot target a role from another community or a non-member user.
- An overwrite with both allow and deny for the same permission is rejected; use one side.
- Removing an overwrite recalculates effective permissions and emits `CHANNEL_PERMISSIONS_UPDATE`.
- The server invalidates the connection's visibility set and sends `CHANNEL_ACCESS_UPDATE` when visibility changes.

### 5.5 Privilege escalation examples

| Attempt | Result |
|---|---|
| Moderator with `MANAGE_ROLES` grants themselves `BAN_MEMBERS` | Rejected: cannot grant a permission they do not hold |
| Administrator creates a second Administrator role | Rejected: only the owner can grant `ADMINISTRATOR` |
| Moderator edits a role positioned above their highest role | Rejected |
| Admin bans the owner | Rejected |
| Member sets a channel overwrite allowing `MANAGE_CHANNELS` | Rejected: lacks `MANAGE_ROLES` |
| Bot owner grants the bot `ADMINISTRATOR` | Rejected unless the bot owner is the community owner; bots can never own a community |
| User with `MANAGE_CHANNELS` deletes a category containing channels | Rejected until the children are moved or deleted; category deletion never silently destroys child channels |
| User with `VIEW_CHANNEL` but no `READ_MESSAGE_HISTORY` opens old messages | Rejected by the history endpoint; only messages after joining are visible |
| Instance admin tries to read a private channel | Rejected unless they are a member with permission |

---

## 6. Permission checks by feature

| Feature | Required permissions |
|---|---|
| View a channel | `VIEW_CHANNEL` |
| Read pre-join messages, search history, or old pins | `READ_MESSAGE_HISTORY`; without it, the API clamps results to messages after joining |
| Send a message | `SEND_MESSAGES`, not timed out, slowmode satisfied |
| Attach a file | `SEND_MESSAGES` + `ATTACH_FILES` |
| Embed a link preview | `EMBED_LINKS` |
| React | `ADD_REACTIONS` |
| Mention `@everyone`/`@here` | `MENTION_EVERYONE` |
| Mention a role | `MENTION_ROLES` |
| Edit/delete own message | Author |
| Delete another message | `MANAGE_MESSAGES` |
| Pin | `PIN_MESSAGES` |
| Create/edit/delete channel | `MANAGE_CHANNELS`; deletion may also require owner confirmation for categories |
| Create/edit role | `MANAGE_ROLES` + hierarchy rules |
| Change community settings | `MANAGE_COMMUNITY` |
| Create invite | `CREATE_INVITES` |
| Revoke any invite | `MANAGE_INVITES` or invite creator |
| Kick | `KICK_MEMBERS` + hierarchy |
| Ban | `BAN_MEMBERS` + hierarchy |
| Timeout | `MODERATE_MEMBERS` + hierarchy |
| View audit log | `VIEW_AUDIT_LOG` |
| View member list | `VIEW_MEMBER_LIST` |
| Join voice | `CONNECT_VOICE` + password/capacity checks |
| Speak | `SPEAK` + not server-muted/deafened |
| Camera | `STREAM_VIDEO` |
| Screen share | `SHARE_SCREEN` |
| Voice activity detection | `USE_VOICE_ACTIVITY`; otherwise push-to-talk |
| Move/disconnect voice | `MOVE_MEMBERS` |
| Server mute/deafen | `MUTE_MEMBERS` / `DEAFEN_MEMBERS` |
| Manage webhooks | `MANAGE_WEBHOOKS` |
| Install/remove bots | `MANAGE_INTEGRATIONS` |
| Delete community | Owner only |
| Transfer ownership | Owner only + re-authentication |

### 6.1 Slowmode

- Slowmode is per channel, in seconds, maximum 6 hours.
- The server calculates the next allowed send time from the user's most recent message in that channel and the channel's `slowmode_seconds`.
- `BYPASS_SLOWMODE` ignores it.
- Moderators deleting messages do not reset the timer.
- Slowmode is not a substitute for rate limiting; both apply.

### 6.2 Voice authorization

Voice tokens are issued only after:

- `CONNECT_VOICE` on the channel.
- Community membership and not banned.
- Not timed out for voice-related actions (timeout blocks `SPEAK`, not joining as a listener).
- Room capacity check.
- Room password verification if set.
- LiveKit configuration present.

The LiveKit token grants only the sources allowed by `SPEAK`, `STREAM_VIDEO`, and `SHARE_SCREEN`. It is short-lived (10 minutes) and can be revoked by removing the participant from LiveKit.

---

## 7. Effective permissions API

For UI hints only:

```
GET /communities/{community_id}/permissions/me
GET /channels/{channel_id}/permissions/me
```

Response:

```json
{
  "permissions": ["VIEW_CHANNEL", "SEND_MESSAGES", "ADD_REACTIONS"],
  "highest_role_position": 50,
  "is_owner": false,
  "timed_out_until": null
}
```

The frontend must still handle a `403` from any action. Effective-permission responses are cached briefly and invalidated by `CHANNEL_PERMISSIONS_UPDATE`.

---

## 8. Data model notes

- `roles.permissions` is `bigint`, non-negative, bits 0–62.
- `channel_permission_overwrites.allow` and `.deny` are `bigint`, non-negative, disjoint.
- `role_id` and `user_id` are mutually exclusive in an overwrite row.
- Composite foreign keys ensure roles belong to the same community as the member assignment.
- `community_members.communication_disabled_until` holds timeouts.
- `communities.owner_user_id` is the single source of ownership truth.
- Permission changes and hierarchy-sensitive actions are written to `audit_logs`.
- There is no materialized “effective permissions” column. Caching, if added, must be versioned and invalidated on role, overwrite, membership, timeout, and channel-parent changes.

---

## 9. Caching

Initial implementation computes permissions from indexed queries per request. If profiling requires it:

- Cache key: `perm:{user_id}:{channel_id}:{version}`.
- Version key: `tulpar:permver:{community_id}`, incremented on any role, overwrite, membership, timeout, or channel-parent change.
- Redis cache misses recompute from PostgreSQL.
- If Redis is unavailable, recompute without cache.
- Never cache across users or channels without the full key.
- The gateway's per-connection visibility set is an in-memory cache with the same invalidation events.

Caching is an optimization, never a correctness dependency.

---

## 10. Test requirements

### 10.1 Unit and property-based tests

- Owner always has all community permissions.
- Administrator has all permissions except owner-only actions, regardless of overwrites.
- `@everyone` is always included.
- Role allow/deny combination is order-independent; deny wins at the same layer.
- Member overwrite is applied last.
- Channel overwrite overrides category overwrite.
- Category overwrite applies to children.
- Timeout removes exactly the documented permissions.
- No combination of roles/overwrites can produce an owner-only action for a non-owner.
- No non-owner can grant `ADMINISTRATOR`.
- A role from another community is rejected.
- Unknown/reserved bits are rejected.

### 10.2 Integration tests

- Every endpoint that takes a community/channel/message ID is tested with:
  - a member without the permission,
  - a member with the permission,
  - a member of another community,
  - a banned member,
  - a timed-out member,
  - the owner,
  - an administrator,
  - an instance admin who is not a member.
- Channel overwrite changes take effect on the next request and gateway event.
- Kicking/banning removes gateway access and voice access.
- Bot permissions follow the same rules.
- Audit records are written for every hierarchy-sensitive action.

### 10.3 Fuzzing

- Permission-name parsing.
- Overwrite payloads with overlapping allow/deny, unknown bits, huge arrays, and invalid UTF-8.
- Role position and reorder payloads.
- Gateway event filters with stale visibility sets.

---

## 11. Open permission decisions

| Decision | Current position | Needed by |
|---|---|---|
| Guest role details | Optional template; channel overwrites define actual access | Phase 2 |
| Default ban permission for Moderator | Not granted; admin can add | Phase 2 |
| Forum/thread rollout | Bits allocated; UI/API later | Phase 3 |
| Instance admin content access | No automatic access; separate audited policy if ever required | Phase 2 |
| Permission cache | Not in v1; add after profiling | Phase 3 |
| Role position uniqueness | Ties allowed, deterministic UUID fallback | Phase 2 |
| Custom emoji permissions | Not in v1 | Post-v1 |
| Soundboard/stage permissions | Not in v1 | Post-v1 |
