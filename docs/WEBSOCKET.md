# Tulpar WebSocket Gateway

**Status:** Phase 0 protocol contract. Not implemented.

**Endpoint:** `wss://<instance>/gateway`

**Protocol version:** 1

The gateway pushes committed state changes to connected clients. It is not a message database and not the only source of truth. Every event can be recovered through the REST API.

---

## 1. Design principles

1. **Authenticate before subscribing.** No event is sent before a valid `IDENTIFY`.
2. **Authorize every event.** The gateway filters by the recipient's current community membership and permissions, not by client-side claims.
3. **Never trust the socket for durable writes.** Messages, edits, deletions, reads, and moderation actions go through REST. The socket carries presence and typing inputs plus server pushes.
4. **Recover through REST.** On reconnect or detected gap, the client calls `GET /sync` and per-channel message catch-up. There is no durable per-user event log in v1.
5. **Drop slow consumers, not the server.** Each connection has a bounded queue and a maximum frame size.
6. **No secrets in URLs or logs.** The ticket is sent in the first frame, not the query string.
7. **Instance isolation.** A socket belongs to exactly one instance. Switching instances opens a separate socket and uses separate credentials.

---

## 2. Connection lifecycle

```
Client                                   Gateway
  │ TCP/TLS + HTTP upgrade                 │
  │───────────────────────────────────────>│
  │                                        │ HELLO (op 10)
  │<───────────────────────────────────────│
  │ IDENTIFY {ticket, protocol_version}    │
  │───────────────────────────────────────>│ validate ticket, session, account
  │                                        │ compute visible communities/channels
  │ READY (op 3, seq 0)                    │
  │<───────────────────────────────────────│
  │ HEARTBEAT (op 1)                       │
  │───────────────────────────────────────>│
  │ HEARTBEAT_ACK (op 11)                  │
  │<───────────────────────────────────────│
  │ DISPATCH events (seq 1..n)             │
  │<───────────────────────────────────────│
```

### 2.1 Obtain a ticket

```
POST /api/v1/gateway/ticket
Authorization: Bearer <access token>
```

Response:

```json
{
  "ticket": "opaque-single-use-value",
  "expires_in": 60,
  "gateway_url": "wss://tulpar.example.com/gateway"
}
```

Ticket rules:

- 32 random bytes, stored hashed in Redis with a 60-second TTL.
- Single use. A second `IDENTIFY` with the same ticket fails.
- Bound to the user ID and session ID that requested it.
- Never placed in a URL, query string, or log.
- New connections fail closed if Redis is unavailable.

### 2.2 HELLO

The server sends this immediately after the WebSocket upgrade:

```json
{
  "op": 10,
  "d": {
    "protocol_version": 1,
    "heartbeat_interval_ms": 30000,
    "max_frame_bytes": 65536,
    "server_time": "2026-11-12T12:00:00Z"
  }
}
```

The client has 5 seconds to send `IDENTIFY`. Otherwise the server closes with `4002`.

### 2.3 IDENTIFY

```json
{
  "op": 2,
  "d": {
    "ticket": "opaque-single-use-value",
    "protocol_version": 1,
    "client": {
      "name": "Tulpar Web",
      "version": "0.1.0",
      "platform": "web"
    },
    "presence": {
      "status": "online"
    }
  }
}
```

`presence.status` is optional. If omitted, the user's `preferred_status` is used. `invisible` means other users see offline; the user's own clients still see their real state.

### 2.4 READY

```json
{
  "op": 3,
  "t": "READY",
  "s": 0,
  "id": "0192f...",
  "d": {
    "session_id": "0192f...",
    "user": { "...": "..." },
    "instance": {
      "name": "Tulpar",
      "public_url": "https://tulpar.example.com",
      "features": { "voice": true, "video": true, "embeds": false }
    },
    "heartbeat_interval_ms": 30000,
    "protocol_version": 1
  }
}
```

`READY` is intentionally small. The client immediately calls `GET /sync` to load communities, channels, roles, read states, friends, DMs, and voice projections.

---

## 3. Envelope and sequencing

Every server frame is JSON:

```json
{
  "op": 0,
  "t": "MESSAGE_CREATE",
  "s": 42,
  "id": "0192f...",
  "d": { "...": "..." }
}
```

| Field | Meaning |
|---|---|
| `op` | Opcode |
| `t` | Event name, present on `DISPATCH` and protocol frames |
| `s` | Per-connection sequence number, starts at 0 for `READY`, increments by 1 for each `DISPATCH` |
| `id` | Stable event UUID for deduplication |
| `d` | Payload |

Rules:

- `HEARTBEAT`, `HEARTBEAT_ACK`, `IDENTIFY`, and control frames do not increment `s`.
- The client tracks the highest `s`. A jump greater than +1 means a dispatch was missed, even if the connection is still alive.
- A missed sequence triggers `RESYNC_REQUIRED` handling: the client reconnects or calls `/sync` and catches up messages.
- Sequence numbers are per connection. They are not global ordering guarantees and are not reused after reconnect.
- Events may arrive out of order across channels. Within a channel, clients order messages by `id` (UUIDv7) and reconcile against REST.

---

## 4. Opcodes

| Op | Name | Direction | Purpose |
|---|---|---|---|
| 0 | `DISPATCH` | Server → client | A committed event |
| 1 | `HEARTBEAT` | Client → server | Liveness |
| 2 | `IDENTIFY` | Client → server | Authenticate with ticket |
| 3 | `READY` | Server → client | Authentication accepted |
| 10 | `HELLO` | Server → client | Protocol parameters |
| 11 | `HEARTBEAT_ACK` | Server → client | Heartbeat acknowledgement |
| 12 | `INVALID_SESSION` | Server → client | Authentication or session invalid |
| 13 | `RESYNC_REQUIRED` | Server → client | Client must reconcile via REST |
| 14 | `RATE_LIMITED` | Server → client | Client action throttled |
| 15 | `PRESENCE_UPDATE` | Client → server | Set user presence |
| 16 | `TYPING_START` | Client → server | Ephemeral typing signal |
| 17 | `SUBSCRIBE` | Client → server | Reserved for future per-channel subscription optimization |
| 18 | `UNSUBSCRIBE` | Client → server | Reserved |

Opcodes 17 and 18 are defined for forward compatibility but are not required in v1. The server automatically subscribes the connection to everything the user is authorized to receive. A client that sends an unsupported opcode receives `4001`.

Unknown opcodes are ignored if they are in the reserved range and the connection remains healthy; malformed payloads close the connection with `4001`.

---

## 5. Heartbeat

Client:

```json
{ "op": 1, "d": { "seq": 42 } }
```

Server:

```json
{ "op": 11, "d": { "seq": 42 } }
```

Rules:

- Send every `heartbeat_interval_ms` with ±20% jitter to avoid synchronized reconnect storms.
- The server expects a heartbeat within 2.5 intervals. If none arrives, it closes with `4000` (or `1001` going away when possible).
- The heartbeat payload echoes the client's last received sequence for diagnostics. It is not used for replay.
- The server may close an idle connection if the account is disabled, the session is revoked, or maintenance begins.

---

## 6. Events

### 6.1 Message events

| Event | Payload | Recipients |
|---|---|---|
| `MESSAGE_CREATE` | Full message object, same shape as REST | Users with `VIEW_CHANNEL` |
| `MESSAGE_UPDATE` | `id`, `channel_id`, `content`, `edited_at`, `updated_at` | `VIEW_CHANNEL` plus post-join visibility or `READ_MESSAGE_HISTORY` |
| `MESSAGE_DELETE` | `id`, `channel_id`, `deleted_at`, `deleted_by` | Same as update |
| `MESSAGE_REACTION_ADD` | `message_id`, `channel_id`, `user_id`, `emoji`, `created_at` | Same as update |
| `MESSAGE_REACTION_REMOVE` | `message_id`, `channel_id`, `user_id`, `emoji` | Same as update |
| `MESSAGE_PIN_UPDATE` | `channel_id`, `message_id`, `pinned`, `pinned_by` | Same as update |
| `TYPING_START` | `channel_id`, `user_id`, `expires_at` | Users with `VIEW_CHANNEL`, excluding blocked pairs |

Message events never contain a different payload from REST. The client can apply them directly or invalidate the query and refetch.

### 6.2 Channel events

| Event | Payload | Recipients |
|---|---|---|
| `CHANNEL_CREATE` | Channel object | Community members who can see it |
| `CHANNEL_UPDATE` | Channel object or changed fields | Members who can see it |
| `CHANNEL_DELETE` | `id`, `community_id`, `deleted_at` | All community members |
| `CHANNEL_ACCESS_UPDATE` | `channel_id`, `community_id`, `visible` | Affected user |
| `CHANNEL_PERMISSIONS_UPDATE` | `channel_id`, `community_id`, `version` | Community members; client invalidates cached permissions and refetches |
| `CHANNEL_VOICE_PASSWORD_UPDATE` | `channel_id`, `has_password` | Members with `CONNECT_VOICE`; never the password |

`CHANNEL_ACCESS_UPDATE` is sent when a role, overwrite, timeout, or membership change affects whether the user can see a channel. The client removes or adds the channel without a full reload.

### 6.3 Community and member events

| Event | Payload | Recipients |
|---|---|---|
| `COMMUNITY_UPDATE` | Public community fields | Members |
| `COMMUNITY_DELETE` | `id`, `deleted_at` | Members |
| `COMMUNITY_MEMBER_ADD` | Member object | Members with `VIEW_MEMBER_LIST` |
| `COMMUNITY_MEMBER_UPDATE` | Member object | Same |
| `COMMUNITY_MEMBER_REMOVE` | `community_id`, `user_id`, `reason` | Same |
| `COMMUNITY_ROLE_CREATE` | Role object | Members |
| `COMMUNITY_ROLE_UPDATE` | Role object | Members |
| `COMMUNITY_ROLE_DELETE` | `community_id`, `role_id` | Members |
| `COMMUNITY_INVITE_CREATE` | Invite metadata, not the full code for non-managers | Members with `MANAGE_INVITES` |
| `COMMUNITY_INVITE_DELETE` | `invite_id`, `community_id` | Same |
| `COMMUNITY_BAN_ADD` | `community_id`, `user_id`, `expires_at` | Members with `BAN_MEMBERS` |
| `COMMUNITY_BAN_REMOVE` | `community_id`, `user_id` | Same |

The gateway never sends an invite code to a user who lacks `MANAGE_INVITES`, even if that user is a member.

### 6.4 Social events

| Event | Payload | Recipients |
|---|---|---|
| `FRIEND_REQUEST_CREATE` | Request object | Addressee |
| `FRIEND_REQUEST_UPDATE` | Request object | Both users |
| `FRIEND_REQUEST_DELETE` | `request_id`, `user_ids` | Both users |
| `FRIEND_ADD` | Friend user summary | Both users |
| `FRIEND_REMOVE` | `user_id` | Both users |
| `DM_CHANNEL_CREATE` | Channel and participant summaries | Both participants |
| `DM_CHANNEL_UPDATE` | Channel metadata | Both participants |
| `USER_BLOCK_ADD` | `user_id` (the blocker) | The blocked user only if a minimal “message cannot be delivered” signal is needed; otherwise target the blocker's other sessions |
| `USER_BLOCK_REMOVE` | `user_id` | Blocker's other sessions |

Blocking is applied to message delivery. A blocked user does not receive a “you were blocked” event; the backend simply rejects or drops the interaction.

### 6.5 Presence and typing

`PRESENCE_UPDATE` dispatch payload:

```json
{
  "user_id": "0192f...",
  "status": "dnd",
  "custom_status": { "text": "Deploying", "emoji": "🚀" },
  "last_seen_at": null,
  "updated_at": "2026-11-12T12:00:00Z"
}
```

- `status` is `online`, `idle`, `dnd`, `offline`, or `invisible` only for the user's own sessions. Other users never see `invisible`.
- Presence is filtered by `presence_visibility`, friendship, shared communities, and blocks.
- The server aggregates sessions: `dnd` > `online` > `idle`; `invisible` is used only when all visible sessions are invisible. The exact precedence is implemented and tested centrally.
- Presence is best-effort. Clients show “unknown” if the gateway is unavailable, not a fabricated online state.

Typing payload:

```json
{
  "channel_id": "0192f...",
  "user_id": "0192f...",
  "expires_at": "2026-11-12T12:00:08Z"
}
```

Typing is not persisted, not replayed, and not delivered to blocked users. One signal per user/channel per 5 seconds; the client displays it for 8 seconds.

### 6.6 Voice events

| Event | Payload | Recipients |
|---|---|---|
| `VOICE_STATE_UPDATE` | `channel_id`, `user_id`, `joined`, `self_mute`, `self_deaf`, `server_mute`, `server_deaf`, `tracks` | Users with `CONNECT_VOICE` on the channel |
| `VOICE_SESSION_ENDED` | `channel_id`, `user_id`, `reason` | Same |
| `VOICE_PARTICIPANT_MOVED` | `from_channel_id`, `to_channel_id`, `user_id` | Both channels |

Speaking indicators are derived locally from LiveKit active-speaker events, not from the Tulpar gateway. This avoids high-frequency WebSocket traffic and keeps the audio state close to the media plane.

### 6.7 Notification and session events

| Event | Payload | Recipients |
|---|---|---|
| `NOTIFICATION_CREATE` | Notification object | Target user |
| `READ_STATE_UPDATE` | `channel_id`, `last_read_message_id`, `updated_at` | The user's other sessions |
| `SESSION_REVOKED` | `session_id`, `reason` | The affected session, then close |
| `MAINTENANCE_START` | `message`, `starts_at` | All connections |
| `MAINTENANCE_END` | `message` | All connections |
| `INSTANCE_SETTINGS_UPDATE` | Changed public settings | All connections |
| `RESYNC_REQUIRED` | `reason` | Affected connection |

When a session is revoked, the server sends `SESSION_REVOKED`, closes with `4005`, and the client clears the instance's auth state and cached private data.

---

## 7. Client-to-server actions

### 7.1 Presence

```json
{
  "op": 15,
  "d": { "status": "idle", "custom_status": { "text": null, "emoji": null } }
}
```

Allowed statuses: `online`, `idle`, `dnd`, `invisible`. `offline` cannot be set directly; disconnect or close the socket.

Rate limit: 5 changes per minute per user. Excess changes receive `RATE_LIMITED` and are ignored, not buffered.

### 7.2 Typing

```json
{ "op": 16, "d": { "channel_id": "0192f..." } }
```

Server checks `SEND_MESSAGES` (or DM participation), block state, and the 5-second rate limit. Typing is silently dropped if not allowed; no error is returned to avoid leaking channel existence.

### 7.3 Reserved subscription ops

```json
{ "op": 17, "d": { "channel_ids": ["..."] } }
{ "op": 18, "d": { "channel_ids": ["..."] } }
```

Reserved for a future optimization where clients only receive events for the channels currently rendered. v1 ignores them and keeps automatic authorization-based subscriptions. They are documented so older clients do not break when the optimization ships.

---

## 8. Reconnection and recovery

### 8.1 Why there is no event replay

A durable per-user event log would duplicate the database, grow quickly, and create a second source of truth that can drift. Tulpar instead treats REST as authoritative:

- Messages are stored with monotonic UUIDv7 IDs.
- `GET /sync` returns a consistent snapshot of structural state.
- `GET /channels/{id}/messages?after=<last_known_id>` fills message gaps.
- Permission, role, channel, and membership changes are re-read from the snapshot.

This is simpler, verifiable, and correct even after long disconnects or deployments.

### 8.2 Client algorithm

```
on close:
  if code is 4005 (session revoked) or 4003 (auth failed):
      clear credentials for this instance; stop
  else:
      schedule reconnect with exponential backoff + jitter
      (1s, 2s, 4s, 8s, 16s, 30s, 30s...)

on connect and READY:
  snapshot = GET /sync
  replace structural state
  preserve optimistic messages by nonce
  for each channel with a local newest message:
      while true:
          page = GET /channels/{id}/messages?after=<local_newest_id>&limit=100
          merge and sort by id
          if not page.has_more: break
  if a sequence gap was detected or /sync failed:
      show resync state and retry
  resume normal dispatch

on DISPATCH with s != last_s + 1:
  mark gap; call /sync; catch up messages; continue
```

Rules:

- Deduplicate messages by message ID and by `(channel_id, author_id, nonce)` for optimistic sends.
- Apply deletions and edits idempotently; a stale event must not resurrect deleted content.
- Do not clear the outbox of unsent local messages on reconnect; resend with the same `Idempotency-Key` and `nonce`.
- If the user is no longer a member of a community, remove its data immediately after `/sync`.
- If `/sync` returns a different session ID, treat it as a new session and discard stale dispatch state.

### 8.3 Server-side resync triggers

The server sends `RESYNC_REQUIRED` and may close with `4011` when:

- The connection's permission version changes in a way the incremental event cannot express safely.
- The user's membership is removed.
- The instance has applied a schema or protocol change requiring a fresh snapshot.
- A server-side sequence counter overflow occurs (not expected, but handled).

The client must not attempt to infer state from a partial event stream after a resync trigger.

---

## 9. Authorization and filtering

The gateway builds a per-connection visibility set at `READY` and updates it on:

- Membership add/remove.
- Role assignment/removal.
- Role permission change.
- Channel overwrite change.
- Channel creation/deletion.
- Timeout/ban/unban.
- Block/unblock (for typing, presence, DMs, mentions).
- Friendship changes (presence visibility).

Event delivery checks:

| Event family | Check |
|---|---|
| Channel/message | `VIEW_CHANNEL`; edits/deletes/reactions/pins of pre-join messages also require `READ_MESSAGE_HISTORY` |
| Voice | `CONNECT_VOICE` and channel visibility |
| Member list | `VIEW_MEMBER_LIST` |
| Moderation/audit | `VIEW_AUDIT_LOG`, `BAN_MEMBERS`, etc. |
| Invites | `MANAGE_INVITES` |
| DMs | Participant and block state |
| Presence | Privacy setting, friend/shared community, block state |
| Notifications | Target user only |
| Session events | Target session only |

No event payload includes data the recipient could not fetch through REST. The gateway is a push optimization, not an authorization bypass.

---

## 10. Backpressure, limits, and abuse resistance

| Limit | Default |
|---|---|
| Maximum inbound frame | 64 KiB |
| Maximum outbound frame | 64 KiB |
| Outbound queue per connection | 512 events |
| Inbound frames per minute | 120, excluding heartbeats |
| Typing signals | 1 per 5 seconds per channel |
| Presence changes | 5 per minute |
| Connections per user | 5 |
| Connections per IP | 20 |
| Ticket TTL | 60 seconds |
| Maximum session lifetime | 24 hours |
| Heartbeat interval | 30 seconds |
| Authentication timeout | 5 seconds |

- If the outbound queue is full, the server closes with `4008 slow_consumer`. It never blocks the event dispatcher.
- Compression (`permessage-deflate`) is disabled by default to avoid CPU and compression-side-channel risks. It can be enabled by configuration after load testing.
- Payloads are parsed with a maximum depth and size. Unknown fields in client frames are rejected.
- IP-based connection limits are advisory; shared NATs are common. User/session limits are stricter.
- All client actions are rate limited before touching the database.

---

## 11. Close codes

| Code | Meaning | Client action |
|---|---|---|
| 1000 | Normal closure | Do not reconnect automatically |
| 1001 | Going away / server shutdown | Reconnect with backoff |
| 4000 | Unknown error | Reconnect with backoff |
| 4001 | Invalid payload or opcode | Log, do not tight-loop; fix client |
| 4002 | Authentication timeout | Reconnect with a new ticket |
| 4003 | Authentication failed | Clear credentials; require login |
| 4004 | Ticket expired or already used | Request a new ticket |
| 4005 | Session revoked | Clear credentials for this instance |
| 4006 | Rate limited | Back off according to `Retry-After` |
| 4007 | Payload too large | Reduce payload; do not retry unchanged |
| 4008 | Slow consumer | Reconnect and resync |
| 4009 | Maintenance mode | Reconnect after the announced time |
| 4010 | Unsupported protocol version | Update the client |
| 4011 | Resync required | Reconnect, call `/sync`, catch up |
| 4012 | Instance mismatch | Clear this instance's state |

A close frame may include a JSON `reason` with `{ "code": "...", "detail": "...", "retry_after_ms": ... }`. Clients branch on the numeric close code and stable reason code, never on localized text.

---

## 12. Scaling and Redis behavior

### 12.1 Single node

- Local broadcast to connection registries.
- Redis is used for tickets, presence, typing, and rate limits, but a single node can continue local fan-out if Redis fails.
- Outbox dispatcher publishes to the local bus and, when available, Redis.

### 12.2 Multiple nodes

- Each API node runs a gateway.
- Redis pub/sub channels:
  - `tulpar:evt:user:{user_id}` — DMs, friend events, notifications, session events.
  - `tulpar:evt:community:{community_id}` — messages, channels, roles, members, voice, moderation.
  - `tulpar:evt:instance` — maintenance and public settings.
- Nodes subscribe with reference counting based on local connections. A node with no members of a community does not subscribe to it.
- Every published event carries `event_id`; nodes deduplicate if an event is delivered through both the local bus and Redis.
- A node filters community events against its local recipients' permissions before writing to sockets.
- Redis pub/sub is fire-and-forget. If a node misses an event, clients recover through `/sync` and message cursors.
- If Redis is unavailable on a multi-node deployment, the instance is degraded: new gateway tickets fail, and cross-node events are not delivered. The API health endpoint reports degraded; production alerting fires.

### 12.3 Sharding

No sharding in v1. If the number of connections exceeds a single node's capacity, add API nodes horizontally. If a single community's event volume becomes dominant, introduce per-channel Redis channels and explicit subscriptions. The protocol reserves `SUBSCRIBE`/`UNSUBSCRIBE` for that change.

---

## 13. Protocol versioning

- `HELLO` and `IDENTIFY` carry `protocol_version`.
- Additive event fields are allowed without a version bump.
- A breaking payload change uses a new event name or increments the protocol version.
- The server supports the current protocol version and the previous one during a deprecation window.
- Clients that send an unsupported version receive `4010` with the minimum supported version in the close reason.
- Event schemas are generated into `packages/shared-types` and checked in CI.

---

## 14. Security checklist

- [x] Ticket is single-use, short-lived, hashed at rest, and never in a URL.
- [x] Connection authenticates before receiving any event.
- [x] Every event is filtered by current permissions.
- [x] No client-supplied permission or role claims are trusted.
- [x] Origin allowlist for browser connections.
- [x] Payload size, depth, and rate limits.
- [x] Backpressure and slow-consumer disconnect.
- [x] Session revocation closes the socket.
- [x] No secrets, message bodies, or tokens in logs.
- [x] Reconnect uses authoritative REST recovery.
- [x] Instance isolation: one socket, one instance, one credential set.
- [ ] Origin behavior for Tauri custom schemes must be verified in Phase 7.
- [ ] Protocol fuzzing and load tests must pass in Phase 3.
