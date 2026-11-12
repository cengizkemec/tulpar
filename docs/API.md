# Tulpar REST API

**Status:** Phase 3 in progress. Phase 2 is complete. Phase 3 adds persistent messages, edits, deletion, replies, reactions, pins, read states, unread counts, channel search, slowmode, and DM messages. WebSocket delivery, presence, attachments, notifications, voice, and the developer platform are not implemented yet.

**Base path:** `/api/v1`

**Content type:** `application/json; charset=utf-8` unless noted.

**OpenAPI:** generated from Rust handlers with `utoipa`; served at `/api/v1/openapi.json` with a registered Bearer/JWT security scheme. Interactive documentation is served at `/docs` and is not part of the authenticated application surface. Per-operation security annotations are still being completed.

---

## 1. Design rules

1. **REST for commands and queries, WebSocket for push.** A WebSocket event is never the only way to learn about state.
2. **The server is authoritative.** Clients do not compute permissions, unread counts, or moderation outcomes.
3. **Every resource is scoped by the authenticated actor.** A valid UUID is not authorization.
4. **Private resources return `404`** when the actor is not allowed to know they exist. `403` is used when the actor can see the resource but lacks a specific action.
5. **All mutations are explicit and validated.** Unknown fields are rejected in request DTOs.
6. **No secret is returned twice.** API keys, bot tokens, and webhook signing secrets are shown once at creation.
7. **Additive changes stay in v1.** Removing or changing the meaning of a field requires a new major version.
8. **Stable error codes and stable event names.** Clients may display localized messages, but must branch on `code`, not on `detail`.

---

## 2. Authentication

### 2.1 Methods

| Method | Used by | Transport |
|---|---|---|
| Bearer access JWT | Web, desktop, bots, API clients | `Authorization: Bearer <token>` |
| Refresh cookie | Web | `HttpOnly`, `Secure`, `SameSite=Lax` cookie scoped to `/api/v1/auth` |
| Refresh token body | Desktop and trusted native clients | JSON body over TLS; stored in OS keychain by the desktop app |
| API key | Scripts and integrations | `Authorization: Bearer tlp_...` |
| Bot token | Bots | `Authorization: Bearer tlb_...` |

Access tokens are short-lived (15 minutes by default). Refresh tokens rotate on every use. The full model is in [AUTHENTICATION.md](AUTHENTICATION.md).

### 2.2 WebSocket authentication

The gateway does not accept a JWT in the URL. The client requests a single-use ticket:

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

The ticket is sent in the first `IDENTIFY` frame. See [WEBSOCKET.md](WEBSOCKET.md).

### 2.3 Cookies

Web refresh cookie attributes:

```
Set-Cookie: __Secure-tulpar_refresh=<opaque>; Path=/api/v1/auth; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000
```

- No refresh token is exposed to JavaScript.
- `SameSite=Lax` blocks cross-site POSTs from sending the cookie in modern browsers.
- The refresh and logout endpoints additionally validate the `Origin` header against the configured web origins.
- General API mutations use the Bearer access token, so they are not cookie-authenticated and do not need a CSRF token.
- If a future feature authenticates mutations with cookies, it must add a double-submit CSRF token before shipping.

### 2.4 Client headers

| Header | Required | Purpose |
|---|---|---|
| `Authorization` | Yes, except public/auth endpoints | Access token or API key |
| `X-Tulpar-Client` | Recommended | `web`, `desktop`, `embed`, `bot`, `api`; used for diagnostics and policy, never for authorization |
| `X-Tulpar-Client-Version` | Recommended | Client version for compatibility diagnostics |
| `X-Request-Id` | Optional | Client correlation ID; server generates one if absent |
| `Idempotency-Key` | Required for selected POSTs | Safe retries |
| `Accept-Language` | Optional | `en` or `tr`; affects human-readable error text, not codes |

---

## 3. Conventions

### 3.1 Identifiers and time

- IDs are UUID strings in canonical hyphenated form.
- Timestamps are RFC 3339 with `Z` or an explicit offset, always normalized to UTC.
- Durations are integer seconds unless the field name ends in `_ms`.
- File sizes are bytes.
- Permission sets in responses are arrays of permission names, not numbers.
- Enums are lowercase snake_case strings.

### 3.2 Pagination

List responses use cursor pagination:

```json
{
  "items": [],
  "next_cursor": "eyJpZCI6Ii4uLiJ9",
  "has_more": false
}
```

- `limit` defaults to 50 and is capped at 100.
- Cursors are opaque base64url strings. Clients must not decode or construct them.
- Message cursors resolve to `(created_at, id)`, so clock skew between API nodes cannot silently drop messages.
- `next_cursor` is `null` on the last page.
- Message pagination:
  - `before=<cursor>` returns messages older than the cursor, newest first.
  - `after=<cursor>` returns messages newer than the cursor, oldest first, for catch-up.
  - `around=<cursor>` returns a context window.
  - Deleted messages are returned as tombstones only when they are within the requested window and the client has `READ_MESSAGE_HISTORY`.
- No offset pagination for messages, members, audit logs, or notifications.

### 3.3 Idempotency

`Idempotency-Key` is required for:

- `POST /channels/{channel_id}/messages`
- `POST /files`
- `POST /communities`
- `POST /communities/{community_id}/invites`
- `POST /bots`
- `POST /communities/{community_id}/webhooks`

Rules:

- Keys are scoped to the authenticated actor.
- The server stores a hash of the method, path, and body for 24 hours.
- A retry with the same key and same body returns the original status and body.
- A retry with the same key and a different body returns `409 idempotency_conflict`.
- The response includes `Idempotency-Replayed: true` on a replay.

Messages also accept a `nonce` UUID in the body as a second layer of duplicate protection.

### 3.4 Errors

Errors use `application/problem+json` (RFC 9457):

```json
{
  "type": "urn:tulpar:error:validation_error",
  "title": "Invalid request",
  "status": 400,
  "detail": "One or more fields are invalid.",
  "code": "validation_error",
  "errors": [
    {
      "field": "content",
      "code": "too_long",
      "message": "Content must be at most 8000 characters."
    }
  ],
  "request_id": "01J..."
}
```

Rules:

- `code` is stable and machine-readable.
- `detail` is human-readable and localized according to `Accept-Language`.
- `errors` is present only for field-level validation failures.
- Stack traces, SQL, internal hostnames, and secrets never appear in responses.
- The `X-Request-Id` response header is always present and appears in server logs. The problem body may include the same value when the handler has it; clients should use the header for correlation.

Common codes:

| HTTP | Code | Meaning |
|---|---|---|
| 400 | `validation_error` | Malformed or invalid fields |
| 400 | `invalid_cursor` | Cursor is malformed or expired |
| 401 | `authentication_required` | Missing or invalid credentials |
| 401 | `invalid_credentials` | Login failed; intentionally generic |
| 401 | `session_expired` | Session or refresh token expired |
| 403 | `email_verification_required` | Account exists but email is not verified |
| 403 | `permission_denied` | Actor lacks a required permission |
| 403 | `reauth_required` | Sensitive action needs a recent password confirmation |
| 403 | `account_disabled` | Instance-level suspension |
| 404 | `not_found` | Resource absent or not visible to the actor |
| 409 | `conflict` | State conflict |
| 409 | `idempotency_conflict` | Same key, different request |
| 409 | `already_member` | Join conflict |
| 410 | `invite_expired` | Invite no longer valid |
| 413 | `payload_too_large` | Body or upload exceeds the limit |
| 415 | `unsupported_media_type` | File type or content type rejected |
| 422 | `unprocessable_entity` | Semantically invalid but well-formed |
| 429 | `rate_limited` | Too many requests |
| 503 | `dependency_unavailable` | Database, Redis, LiveKit, storage, or email unavailable |
| 503 | `maintenance_mode` | Instance is in maintenance |

### 3.5 Rate limits

Rate limits are enforced per actor and per IP. The exact budget is configurable; defaults are documented below.

| Class | Default |
|---|---|
| Login / password reset | 5 attempts per 15 minutes per IP + account key |
| Registration | 3 per hour per IP |
| General authenticated API | 300 requests/minute per user |
| Message send | 30 messages/10 seconds per user, plus 5/5 seconds per channel |
| Typing | 1 per 5 seconds per user/channel |
| Friend requests | 20/hour per user |
| Public community joins | 10/hour per user |
| Invite creation | 10/hour per user/community |
| File uploads | 20/hour and 500 MB/day per user (configurable) |
| Search | 30/minute per user |
| Initial `/sync` | 10/minute per user |
| WebSocket tickets | 10/minute per user |
| Embed session | 60/minute per embed key + origin |
| Webhook test | 10/minute per endpoint |

Responses include:

```
X-RateLimit-Limit: 300
X-RateLimit-Remaining: 287
X-RateLimit-Reset: 1760000000
Retry-After: 12
```

Rate-limit buckets use Redis when available. When Redis is down, each node uses a bounded in-memory limiter and readiness reports degraded mode. Login limits fail closed if the limiter cannot be initialized.

### 3.6 CORS

- Default: same-origin only.
- A configured web origin may use credentialed requests for auth endpoints.
- Desktop webview origins (for example the Tauri custom-scheme origin) are added explicitly only if the desktop uses direct webview HTTP calls. Bearer tokens are still required; CORS is not an authorization boundary.
- `Access-Control-Allow-Origin` is an exact origin, never `*` when credentials are allowed.
- `Vary: Origin` is always set.
- Embed requests are validated against the embed application's allowlist, independent of general CORS.

### 3.7 Localization

The API returns stable machine-readable values. Human-readable `title`, `detail`, validation messages, and notification text are localized for `en` and `tr` based on `Accept-Language`. Unsupported languages fall back to English.

---

## 4. Endpoint catalog

Legend:

- **Public** — no authentication.
- **Auth** — any authenticated user.
- **Owner** — community owner.
- **Instance admin** — row in `instance_admins`.
- **Permission** — a community permission from [PERMISSIONS.md](PERMISSIONS.md).
- **Participant** — a participant in the DM channel.

### 4.1 Authentication

| Method | Path | Access | Notes |
|---|---|---|---|
| POST | `/auth/register` | Public | Rate limited; may require email verification |
| POST | `/auth/login` | Public | Email or username; sets refresh cookie for web |
| POST | `/auth/refresh` | Cookie or body | Rotates refresh token |
| POST | `/auth/logout` | Auth | Revokes current session, clears cookie |
| POST | `/auth/logout-all` | Auth | Revokes all sessions, including current |
| POST | `/auth/verify-email` | Public | Consumes a verification token |
| POST | `/auth/resend-verification` | Public | Generic response; rate limited |
| POST | `/auth/password/forgot` | Public | Generic response; rate limited |
| POST | `/auth/password/reset` | Public | Consumes a reset token; revokes sessions |
| POST | `/auth/password/change` | Auth | Requires current password; revokes other sessions |
| POST | `/auth/reauth` | Auth | Confirms the password and refreshes the access token's `auth_time` |
| GET | `/auth/sessions` | Auth | Lists active sessions/devices |
| DELETE | `/auth/sessions/{session_id}` | Auth | Revokes one session |
| DELETE | `/auth/sessions` | Auth | Revokes all sessions except the current one |
| POST | `/auth/device/code` | Public | Starts desktop device authorization |
| POST | `/auth/device/approve` | Auth | Approves a user code |
| POST | `/auth/device/deny` | Auth | Denies a user code |
| POST | `/auth/device/token` | Public | Polls for a token; rate limited |

#### Register

```http
POST /api/v1/auth/register
Content-Type: application/json

{
  "username": "ada",
  "display_name": "Ada",
  "email": "ada@example.com",
  "password": "correct horse battery staple",
  "locale": "en"
}
```

Registration always returns `202 Accepted` and never logs the user in automatically:

```json
{
  "verification_required": true,
  "login_required": true
}
```

- If email verification is disabled, `verification_required` is `false` and the user can log in immediately.
- If the email cannot be registered, the response is the same generic `202`. This avoids turning registration into an email-enumeration oracle.
- A username conflict may return `409 conflict` because usernames are public and searchable.
- The client must not infer success from the presence of tokens or a user object; registration never returns either.

#### Login

```http
POST /api/v1/auth/login
Content-Type: application/json

{
  "login": "ada@example.com",
  "password": "...",
  "device_name": "Firefox on Windows"
}
```

Web response:

```json
{
  "access_token": "...",
  "token_type": "Bearer",
  "expires_in": 900,
  "user": { "...": "..." },
  "session": { "id": "...", "device_name": "Firefox on Windows", "platform": "web" }
}
```

`Set-Cookie: __Secure-tulpar_refresh=...`.

Desktop clients do not use this endpoint for production login; they use the device authorization flow. A desktop client that does use it must not store the refresh token in webview storage.

#### Refresh

Web:

```http
POST /api/v1/auth/refresh
Cookie: __Secure-tulpar_refresh=...
Origin: https://tulpar.example.com
```

Desktop:

```json
{
  "refresh_token": "...",
  "client": "desktop"
}
```

Web response returns a new access token and a rotated cookie. Desktop response returns both a new access token and a new refresh token. Reuse detection revokes the session family and returns `401 session_expired`.

#### Device authorization

Start:

```http
POST /api/v1/auth/device/code

{
  "client_name": "Tulpar Desktop",
  "platform": "desktop"
}
```

```json
{
  "device_code": "opaque-secret",
  "user_code": "ABCD-EFGH",
  "verification_uri": "https://tulpar.example.com/device",
  "expires_in": 600,
  "interval": 5
}
```

The desktop opens the verification URI in the system browser. The browser approves with the authenticated user's session. The desktop polls:

```http
POST /api/v1/auth/device/token
{
  "device_code": "opaque-secret"
}
```

- `400 authorization_pending` while waiting.
- `400 slow_down` if polling too fast.
- `400 access_denied` if denied.
- `400 expired_token` if expired.
- `200` with access token, refresh token, user, and session once, then the row is consumed.

---

### 4.2 Users and settings

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/users/me` | Auth | Full self profile |
| PATCH | `/users/me` | Auth | Display name, bio, avatar, locale, custom status |
| DELETE | `/users/me` | Auth | Account deletion; requires confirmation |
| GET | `/users/me/settings` | Auth | UI, privacy, notification settings |
| PATCH | `/users/me/settings` | Auth | Partial update |
| PUT | `/users/me/presence` | Auth | REST fallback for status; WS is preferred |
| GET | `/users/me/communities` | Auth | Communities the user belongs to |
| GET | `/users/me/notifications` | Auth | Paginated notifications |
| POST | `/users/me/notifications/read` | Auth | Mark IDs or all as read |
| GET | `/users/{user_id}` | Auth | Public profile, filtered by privacy/blocking |
| GET | `/users/search` | Auth | `?q=`; rate limited |
| GET | `/users/me/export` | Auth | Profile, settings, and session metadata export |

Profile visibility rules:

- A blocked user cannot view the blocker's profile beyond the minimal public card.
- `presence_visibility` controls online state visibility.
- Email is never exposed to other users.
- `last_seen_at` is coarse and only shown when privacy settings allow it.

---

### 4.3 Friends and blocks

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/friends` | Auth | Accepted friends |
| GET | `/friends/requests` | Auth | Incoming and outgoing pending requests |
| POST | `/friends/requests` | Auth | `{ "user_id": "..." }` or `{ "username": "..." }` |
| POST | `/friends/requests/{request_id}/accept` | Auth | Addressee only |
| POST | `/friends/requests/{request_id}/decline` | Auth | Addressee only |
| DELETE | `/friends/{user_id}` | Auth | Removes an accepted friend |
| GET | `/blocks` | Auth | Users blocked by the caller |
| PUT | `/blocks/{user_id}` | Auth | Idempotent block; removes friendship |
| DELETE | `/blocks/{user_id}` | Auth | Unblock |

Friend requests respect `allow_friend_requests_from`. A block prevents requests and DMs in both directions. The backend checks blocks on every relevant path; there is no client-only enforcement.

---

### 4.4 Direct messages

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/dms` | Auth | DM channels, ordered by recent activity |
| POST | `/dms` | Auth | `{ "recipient_id": "..." }`; get-or-create |
| GET | `/channels/{channel_id}` | Participant | DM channel metadata |
| GET | `/channels/{channel_id}/messages` | Participant | DM history |
| POST | `/channels/{channel_id}/messages` | Participant | Send DM |
| POST | `/channels/{channel_id}/read` | Participant | Update read cursor |
| POST | `/channels/{channel_id}/typing` | Participant | Ephemeral typing signal |

DM creation is idempotent: the canonical participant pair has one row. If either user blocks the other, creation returns `403 permission_denied` without revealing the block reason.

---

### 4.5 Communities

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/communities` | Public/Auth | Discovery; public communities only unless member |
| POST | `/communities` | Auth | Creates community, owner, default role, default channel |
| GET | `/communities/{community_id}` | Member or public | Core community data |
| POST | `/communities/{community_id}/join` | Auth | Self-join a public community; idempotent |
| PATCH | `/communities/{community_id}` | `MANAGE_COMMUNITY` | Name, description, icon, banner, visibility |
| DELETE | `/communities/{community_id}` | Owner | Soft delete with confirmation |
| POST | `/communities/{community_id}/transfer-ownership` | Owner | Requires password re-auth |
| GET | `/communities/{community_id}/members` | `VIEW_MEMBER_LIST` | Cursor paginated; searchable |
| GET | `/communities/{community_id}/members/{user_id}` | Member | Member profile card |
| PATCH | `/communities/{community_id}/members/{user_id}` | `MANAGE_NICKNAMES` / `MODERATE_MEMBERS` | Nickname, timeout |
| DELETE | `/communities/{community_id}/members/{user_id}` | `KICK_MEMBERS` | Kick |
| GET | `/communities/{community_id}/bans` | `BAN_MEMBERS` | Paginated |
| PUT | `/communities/{community_id}/bans/{user_id}` | `BAN_MEMBERS` | Create/update ban |
| DELETE | `/communities/{community_id}/bans/{user_id}` | `BAN_MEMBERS` | Unban |
| GET | `/communities/{community_id}/permissions/me` | Member | Effective permissions for UI hints |
| GET | `/communities/{community_id}/audit-logs` | `VIEW_AUDIT_LOG` | Paginated, filterable |
| GET | `/communities/{community_id}/invites` | `MANAGE_INVITES` | List active invites |
| POST | `/communities/{community_id}/invites` | `CREATE_INVITES` | Create invite |
| DELETE | `/invites/{code}` | `MANAGE_INVITES` or creator | Revoke |
| GET | `/invites/{code}` | Public | Preview; no membership required |
| POST | `/invites/{code}` | Auth | Join community |
| GET | `/communities/{community_id}/webhooks` | `MANAGE_WEBHOOKS` | List |
| POST | `/communities/{community_id}/webhooks` | `MANAGE_WEBHOOKS` | Create; secret shown once |
| PATCH | `/webhooks/{webhook_id}` | `MANAGE_WEBHOOKS` | Update URL/events/enabled |
| DELETE | `/webhooks/{webhook_id}` | `MANAGE_WEBHOOKS` | Delete |
| POST | `/webhooks/{webhook_id}/test` | `MANAGE_WEBHOOKS` | Signed test delivery |
| GET | `/webhooks/{webhook_id}/deliveries` | `MANAGE_WEBHOOKS` | Delivery log |

Community creation defaults:

- The creator becomes owner and a member.
- A non-deletable `@everyone` role is created with the default permission set.
- A `#general` text channel and a `General` category are created.
- No public discovery listing unless `visibility = public` and discovery is enabled by the instance.

`POST /communities/{community_id}/join` succeeds only for `visibility = public`. It is idempotent, rejects banned users, respects instance bans, and writes a member-join audit event. A private community requires a valid invite code through `POST /invites/{code}`. Discovery being disabled does not change the join policy of a public community; it only hides it from listings.

Community deletion is soft for 30 days. The owner can cancel during the grace period through an admin endpoint or support procedure; normal API deletion hides the community immediately.

---

### 4.6 Channels, categories, and permission overwrites

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/communities/{community_id}/channels` | `VIEW_CHANNEL` on at least one | Returns channels the actor can see |
| POST | `/communities/{community_id}/channels` | `MANAGE_CHANNELS` | Create category/text/voice/announcement/forum |
| GET | `/channels/{channel_id}` | `VIEW_CHANNEL` | Channel details |
| PATCH | `/channels/{channel_id}` | `MANAGE_CHANNELS` or owner | Name, topic, position, slowmode, limits |
| DELETE | `/channels/{channel_id}` | `MANAGE_CHANNELS` | Soft delete; messages retained for 30 days. A category must be empty before deletion |
| PUT | `/channels/{channel_id}/permissions/roles/{role_id}` | `MANAGE_ROLES` | Set overwrite |
| DELETE | `/channels/{channel_id}/permissions/roles/{role_id}` | `MANAGE_ROLES` | Remove overwrite |
| PUT | `/channels/{channel_id}/permissions/users/{user_id}` | `MANAGE_ROLES` | Set member overwrite |
| DELETE | `/channels/{channel_id}/permissions/users/{user_id}` | `MANAGE_ROLES` | Remove overwrite |
| GET | `/channels/{channel_id}/permissions` | `MANAGE_ROLES` | List effective overwrites |
| GET | `/channels/{channel_id}/permissions/me` | `VIEW_CHANNEL` | Effective permissions for UI hints |
| PUT | `/channels/{channel_id}/voice-password` | `MANAGE_CHANNELS` | Set/rotate room password |
| DELETE | `/channels/{channel_id}/voice-password` | `MANAGE_CHANNELS` | Remove password |

Overwrite body:

```json
{
  "allow": ["SEND_MESSAGES", "ADD_REACTIONS"],
  "deny": ["ATTACH_FILES"]
}
```

Permissions are returned as names. The server rejects unknown permissions, overlapping allow/deny, and attempts to grant permissions the actor does not hold.

A category is a channel with `kind = "category"`. Moving a channel between categories and reordering is a `PATCH` with `parent_id` and `position`; the server validates cycles and performs the reorder in one transaction. Maximum category depth is one level.

---

### 4.7 Roles

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/communities/{community_id}/roles` | Member | Ordered by position |
| POST | `/communities/{community_id}/roles` | `MANAGE_ROLES` | Create below actor's highest role |
| PATCH | `/communities/{community_id}/roles/{role_id}` | `MANAGE_ROLES` | Name, color, permissions, mentionable |
| DELETE | `/communities/{community_id}/roles/{role_id}` | `MANAGE_ROLES` | Cannot delete `@everyone` |
| PUT | `/communities/{community_id}/roles/order` | `MANAGE_ROLES` | Atomic position rewrite |
| PUT | `/communities/{community_id}/members/{user_id}/roles/{role_id}` | `MANAGE_ROLES` | Assign |
| DELETE | `/communities/{community_id}/members/{user_id}/roles/{role_id}` | `MANAGE_ROLES` | Unassign |

Hierarchy and escalation rules are in [PERMISSIONS.md](PERMISSIONS.md). The server, not the UI, enforces that an actor cannot edit a role at or above their highest position, cannot assign a role they do not hold, and cannot grant permissions they do not possess.

---

### 4.8 Messages, reactions, pins, and read state

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/channels/{channel_id}/messages` | `VIEW_CHANNEL` | Cursor: `before`, `after`, `around`, `limit`; results are clamped to post-join history without `READ_MESSAGE_HISTORY` |
| POST | `/channels/{channel_id}/messages` | `SEND_MESSAGES` | Idempotency-Key required |
| GET | `/messages/{message_id}` | `VIEW_CHANNEL` | Single message; older-than-join history is hidden without `READ_MESSAGE_HISTORY` |
| PATCH | `/messages/{message_id}` | Author or `MANAGE_MESSAGES` | Edit content |
| DELETE | `/messages/{message_id}` | Author or `MANAGE_MESSAGES` | Soft delete |
| POST | `/messages/{message_id}/reactions/{emoji}` | `ADD_REACTIONS` | Idempotent add |
| DELETE | `/messages/{message_id}/reactions/{emoji}` | `ADD_REACTIONS` | Idempotent remove |
| GET | `/channels/{channel_id}/pins` | `VIEW_CHANNEL` | Pinned messages; old pins are hidden without `READ_MESSAGE_HISTORY` |
| PUT | `/messages/{message_id}/pin` | `PIN_MESSAGES` | Pin; max pinned limit |
| DELETE | `/messages/{message_id}/pin` | `PIN_MESSAGES` | Unpin |
| GET | `/channels/{channel_id}/search` | `READ_MESSAGE_HISTORY` | Full-text search within the channel |
| POST | `/channels/{channel_id}/typing` | `SEND_MESSAGES` | Ephemeral; rate limited |
| POST | `/channels/{channel_id}/read` | `VIEW_CHANNEL` | Update read cursor |
| POST | `/channels/{channel_id}/messages/bulk-delete` | `MANAGE_MESSAGES` | Up to 100 IDs; audited |

#### Send message

```http
POST /api/v1/channels/0192f.../messages
Authorization: Bearer ...
Idempotency-Key: 0192f...
Content-Type: application/json

{
  "content": "Hello **Tulpar**",
  "nonce": "0192f...",
  "reply_to_id": null,
  "attachment_ids": [],
  "mention_user_ids": [],
  "mention_role_ids": [],
  "mention_everyone": false
}
```

Response `201 Created`:

```json
{
  "id": "0192f...",
  "channel_id": "0192f...",
  "author": { "id": "...", "username": "ada", "display_name": "Ada", "avatar_url": "..." },
  "kind": "default",
  "content": "Hello **Tulpar**",
  "reply_to": null,
  "attachments": [],
  "reactions": [],
  "mentions": [],
  "pinned": false,
  "edited_at": null,
  "deleted_at": null,
  "created_at": "2026-11-12T12:00:00Z",
  "updated_at": "2026-11-12T12:00:00Z"
}
```

Validation:

- Content is normalized (Unicode NFC, trim trailing whitespace), maximum 8000 characters.
- Content or at least one attachment is required.
- Attachment IDs must belong to the actor, be `ready`, and not already be attached to another message. A message may have at most 10 attachments; the configured total upload size limit applies.
- Reply target must be in the same channel and visible.
- Mentions are parsed and authorized; unauthorized `@everyone` or role mentions are rejected or stripped according to policy. Rejection is preferred so the sender knows.
- Slowmode is enforced server-side, except for actors with `BYPASS_SLOWMODE`.
- Message content is stored as plain text with a limited Markdown subset. HTML is never accepted or rendered.

#### Message pagination

```
GET /channels/{id}/messages?before=<cursor>&limit=50
```

Response:

```json
{
  "items": [ ... ],
  "has_more": true,
  "next_cursor": "..."
}
```

`after` is the catch-up path after reconnect. Clients request at most 100 messages per page and continue until `has_more` is false or they reach the last known message.

#### Read state

```json
POST /channels/{id}/read
{
  "last_read_message_id": "0192f..."
}
```

The server verifies the message belongs to the channel and advances the cursor monotonically. A client cannot move the cursor backward through this endpoint; explicit “mark unread” is a separate client-local action until a dedicated endpoint exists.

---

### 4.9 Files and attachments

| Method | Path | Access | Notes |
|---|---|---|---|
| POST | `/files` | Auth | Create upload intent; Idempotency-Key required |
| PUT | `/files/{file_id}/content` | Owner | Local-storage upload target; not used for S3 |
| POST | `/files/{file_id}/complete` | Owner | Finalize and validate |
| GET | `/files/{file_id}` | Auth/visibility | Metadata |
| GET | `/files/{file_id}/download` | Derived from message/avatar | `302` to signed URL or streamed local file |
| GET | `/files/{file_id}/thumbnail` | Derived | Generated image thumbnail |
| DELETE | `/files/{file_id}` | Owner, unused | Soft delete |

Upload intent request:

```json
{
  "filename": "screenshot.png",
  "content_type": "image/png",
  "size_bytes": 482311,
  "purpose": "attachment"
}
```

Response:

```json
{
  "file": {
    "id": "0192f...",
    "status": "pending",
    "upload_expires_at": "2026-11-12T12:15:00Z"
  },
  "upload": {
    "method": "PUT",
    "url": "https://usercontent.example.com/...signed...",
    "headers": { "Content-Type": "image/png" }
  }
}
```

For local storage, `upload.url` points to the instance's `/files/{id}/content` endpoint with a short-lived HMAC token, not to a filesystem path. For S3, it is a presigned PUT.

The server does not trust the declared MIME type. It sniffs the file after upload, validates the extension/MIME pair, re-encodes images where appropriate, and rejects SVG and executable content by default. Attachment URLs are authorized through the message/channel, never through an unguessable object key alone.

---

### 4.10 Voice

| Method | Path | Access | Notes |
|---|---|---|---|
| POST | `/channels/{channel_id}/voice/token` | `CONNECT_VOICE` | Mints a short-lived LiveKit token |
| GET | `/channels/{channel_id}/voice/participants` | `CONNECT_VOICE` | Current projection |
| POST | `/channels/{channel_id}/voice/move` | `MOVE_MEMBERS` | Move a participant to another voice channel |
| POST | `/channels/{channel_id}/voice/disconnect/{user_id}` | `MOVE_MEMBERS` | Remove from voice |
| PATCH | `/channels/{channel_id}/voice/participants/{user_id}` | `MUTE_MEMBERS` / `DEAFEN_MEMBERS` | Server-side mute/deafen request |

Token request:

```json
{
  "password": "optional-room-password"
}
```

Response:

```json
{
  "url": "wss://rtc.example.com",
  "room": "channel_0192f...",
  "token": "eyJ...",
  "expires_at": "2026-11-12T12:10:00Z",
  "ice_servers": [
    {
      "urls": ["turns:turn.example.com:5349"],
      "username": "1760000000:user",
      "credential": "ephemeral-turn-credential"
    }
  ]
}
```

Token rules and failure behavior are in [VOICE.md](VOICE.md). A `403` does not reveal whether a room password was wrong versus whether the channel exists.

#### LiveKit webhook

```
POST /webhooks/livekit
Authorization: <LiveKit signed webhook JWT>
```

This endpoint is outside `/api/v1`, verifies the LiveKit signature, rejects replayed events, and only updates the voice projection. It never trusts participant-supplied metadata for authorization.

---

### 4.11 Synchronization

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/sync` | Auth | Bootstrap snapshot for startup and reconnect |
| GET | `/sync/instance` | Public | Instance name, version, features, registration policy |

`GET /sync` returns:

```json
{
  "server_time": "2026-11-12T12:00:00Z",
  "instance": {
    "name": "Tulpar",
    "public_url": "https://tulpar.example.com",
    "features": { "voice": true, "video": true, "embeds": false }
  },
  "user": { "...": "..." },
  "settings": { "...": "..." },
  "communities": [
    {
      "id": "...",
      "name": "...",
      "icon_url": "...",
      "member_count": 42,
      "roles": [ ... ],
      "channels": [ ... ],
      "read_states": [ ... ]
    }
  ],
  "friends": [ ... ],
  "friend_requests": [ ... ],
  "blocks": [ ... ],
  "dms": [ ... ],
  "unread_counts": { "channel_id": 3 },
  "voice_states": [ ... ],
  "gateway": {
    "url": "wss://tulpar.example.com/gateway",
    "heartbeat_interval_ms": 30000
  }
}
```

Each channel summary includes at least `id`, `community_id`, `parent_id`, `kind`, `name`, `position`, `last_message_id`, and `last_message_at`. The client compares `last_message_id` with its local newest message and fetches `after` for gaps. Read states and unread counts are included so muted channels and background communities are also caught up.

The snapshot is optimized for the initial target scale. If a user belongs to hundreds of communities, the client uses `?communities=summary` and lazy-loads channel/member data per community. The response shape supports that without a breaking change.

`/sync` does not return message bodies. Clients use per-channel `after` queries for message catch-up.

---

### 4.12 Instance administration

All routes require `instance_admin`. Destructive actions require recent re-authentication and are audited.

| Method | Path | Notes |
|---|---|---|
| GET | `/admin/overview` | Users, communities, messages, sessions, storage, voice counts |
| GET | `/admin/users` | Search, pagination |
| GET | `/admin/users/{user_id}` | Account details, sessions, moderation history |
| PATCH | `/admin/users/{user_id}` | Enable/disable, grant/revoke instance admin |
| DELETE | `/admin/users/{user_id}` | Force account deletion with audit |
| GET | `/admin/sessions` | Active sessions across the instance |
| DELETE | `/admin/sessions/{session_id}` | Revoke a session |
| GET | `/admin/settings` | Dynamic settings |
| PATCH | `/admin/settings` | Registration, discovery, maintenance, upload limits |
| GET | `/admin/audit-logs` | Instance-level audit |
| POST | `/admin/maintenance` | Enter/leave maintenance mode |
| GET | `/admin/health` | Detailed dependency health |
| GET | `/admin/outbox` | Pending/failed outbox events |
| POST | `/admin/outbox/{event_id}/retry` | Retry an event |
| GET | `/admin/storage` | Provider, usage, quota, orphan count |
| GET | `/admin/voice` | LiveKit health and participant summary |

Environment secrets (database URL, SMTP password, LiveKit secret, S3 secret, JWT key) are never readable through the admin API. The admin UI shows configured/not configured and allows testing connections, not exporting secrets.

---

### 4.13 Developer platform

| Method | Path | Access | Notes |
|---|---|---|---|
| GET | `/users/me/api-keys` | Auth | List keys; never returns the secret |
| POST | `/users/me/api-keys` | Auth | Create key; secret shown once |
| DELETE | `/users/me/api-keys/{key_id}` | Auth | Revoke |
| GET | `/bots` | Auth | Bots owned by the caller |
| POST | `/bots` | Auth | Create bot application and bot user |
| GET | `/bots/{bot_id}` | Owner | Bot details |
| PATCH | `/bots/{bot_id}` | Owner | Name, description, avatar |
| DELETE | `/bots/{bot_id}` | Owner | Delete bot and revoke tokens |
| POST | `/bots/{bot_id}/token/rotate` | Owner | Rotate token; old token revoked |
| GET | `/api/v1/openapi.json` | Public | OpenAPI 3.1 document |
| GET | `/api/v1/version` | Public | API version and build metadata |

Bot permissions are ordinary community permissions. A bot is a user with `is_bot = true`, a bot token, and membership in a community. Bots cannot be instance administrators.

---

### 4.14 Embed endpoints

Embed endpoints are intentionally separate from the general API. They accept a scoped embed credential and return only public or explicitly authorized data.

| Method | Path | Access | Notes |
|---|---|---|---|
| POST | `/api/v1/embed/session` | Embed key + allowed Origin | Creates a short-lived public embed session |
| POST | `/api/v1/embed/authorize` | Auth + embed key | Creates a one-time authorization code for a member widget |
| POST | `/api/v1/embed/token` | Embed key + code + Origin | Exchanges the code for a scoped embed token |
| GET | `/api/v1/embed/community/{community_id}` | Embed token | Public community summary |
| GET | `/api/v1/embed/community/{community_id}/members` | Embed token | Online/member preview, privacy-filtered |
| GET | `/api/v1/embed/channels/{channel_id}/preview` | Embed token | Latest public messages, if enabled |
| GET | `/api/v1/embed/voice/{channel_id}` | Embed token | Voice channel preview, no media token |

Security details are in [EMBED_SDK.md](EMBED_SDK.md). Embed tokens are never general API tokens.

---

## 5. Webhooks

### 5.1 Event envelope

```json
{
  "id": "0192f...",
  "type": "message.created",
  "version": 1,
  "created_at": "2026-11-12T12:00:00Z",
  "instance": {
    "name": "Tulpar",
    "url": "https://tulpar.example.com"
  },
  "community_id": "0192f...",
  "data": { "...": "..." }
}
```

### 5.2 Event types

- `member.joined`
- `member.left`
- `member.updated`
- `member.banned`
- `member.unbanned`
- `message.created`
- `message.updated`
- `message.deleted`
- `channel.created`
- `channel.updated`
- `channel.deleted`
- `community.updated`
- `role.created`
- `role.updated`
- `role.deleted`
- `moderation.timeout`
- `voice.joined`
- `voice.left`

Message webhook payloads contain message content only when the endpoint's community has enabled content delivery. The default is metadata only (IDs, author, channel, timestamp) to reduce accidental data exposure. Operators can opt in.

### 5.3 Signature

Headers:

```
Tulpar-Event: message.created
Tulpar-Event-Id: 0192f...
Tulpar-Timestamp: 1760000000
Tulpar-Signature: v1=hex(hmac_sha256(secret, timestamp + "." + raw_body))
User-Agent: Tulpar-Webhooks/1.0
```

Receivers must:

1. Reject timestamps older than 5 minutes.
2. Compute the HMAC over the raw request body.
3. Compare in constant time.
4. Deduplicate by `Tulpar-Event-Id`.

Retry schedule: immediate, then 1m, 5m, 30m, 2h, 12h, 24h (7 attempts). After repeated failures, the endpoint is marked unhealthy and the owner is notified; it is not deleted automatically.

### 5.4 SSRF controls

- HTTPS only.
- Resolve DNS and reject loopback, link-local, private, multicast, and metadata-service ranges.
- Re-resolve on every delivery to prevent DNS rebinding.
- No redirects.
- Connect and read timeouts.
- Response body read capped at 512 bytes for the delivery log.
- Webhook creation requires `MANAGE_WEBHOOKS` and is rate limited.
- Egress allowlisting is optional for operators who want a stricter policy.

---

## 6. API versioning and compatibility

- Current version: `v1`.
- Additive fields and new endpoints are allowed in v1.
- Removing a field, changing its type, or changing authorization semantics requires `v2`.
- Deprecated endpoints return `Deprecation` and `Sunset` headers and are documented for at least one minor release.
- The OpenAPI document is versioned with the server. Generated TypeScript and Rust SDKs are tested against it.
- WebSocket events carry their own `version`; breaking event changes use a new event name or a new payload version.

---

## 7. Contract tests

Phase 1 and later must keep these checks in CI:

- OpenAPI document validates against OpenAPI 3.1.
- Every route has authentication and error documentation.
- Generated TypeScript types compile.
- Rust DTOs serialize/deserialize round-trip.
- WebSocket event schemas match `packages/shared-types`.
- Permission names in API payloads match the bit definitions.
- No endpoint returns a field marked secret in its schema.

---

## 8. Open API questions

| Question | Current position | Needed by |
|---|---|---|
| Data export format | JSON archive; endpoint reserved, not implemented | Phase 3 |
| Mark-unread semantics | Client-local initially | Phase 3 |
| Group DMs | Not in v1 | Post-v1 |
| Guest access in embeds | Public read-only widgets only; authenticated member widgets use scoped tokens | Phase 8 |
| Bot installation flow | Direct membership provisioning by an admin; OAuth-style install later | Phase 8 |
| API key scopes | Explicit allowlist per endpoint family | Phase 8 |
| Webhook content opt-in | Metadata by default | Phase 8 |
