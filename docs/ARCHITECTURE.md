# Tulpar Architecture

**Status:** Phase 0 approved; Phase 1 identity/auth and Phase 2 communities/permissions/social are implemented. Phase 3 adds persistent messaging APIs; the WebSocket gateway, presence, attachments, voice, and clients remain planned. This document describes the target architecture; later sections must not be treated as implemented.

**Audience:** maintainers, contributors, operators, security reviewers, and client developers.

---

## 1. Goals and non-goals

### 1.1 Goals

- A self-hosted communication platform that an operator can run without a central Tulpar service.
- One instance can host many communities and users; clients can connect to several independent instances.
- Reliable, persistent text messaging with recoverable real-time delivery.
- Real voice and video communication through a proven WebRTC SFU.
- A complete web client and a polished Windows/Linux desktop client sharing one frontend codebase.
- Granular, backend-enforced permissions with role hierarchy and channel overrides.
- A documented REST API, WebSocket protocol, webhooks, bots, and SDKs.
- A secure embeddable widget for third-party websites.
- A modular monolith that can be operated on one VPS and scaled component by component later.
- Clear failure behavior when Redis, LiveKit, email, or object storage are unavailable.
- Turkish and English localization from the start.

### 1.2 Non-goals for v1

- Mandatory federation between instances.
- A mobile native application. Responsive web/PWA is the mobile experience in v1.
- End-to-end encrypted messaging or voice.
- Custom media codecs, a custom SFU, or a custom TURN implementation.
- A plugin marketplace or arbitrary server-side third-party code.
- Recording, transcription, or streaming to external platforms.
- macOS desktop builds.
- Billing, subscriptions, or a centralized Tulpar cloud.
- Unlimited scalability claims. Targets are documented and must be measured.

These are deliberate scope boundaries, not silent removals. They are tracked in [ROADMAP.md](ROADMAP.md).

---

## 2. System context

```
┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│ Tulpar Web   │   │Tulpar Desktop│   │ Tulpar Embed │   │ External app │
│ React + PWA  │   │ Tauri 2      │   │ iframe + SDK │   │ REST / WS    │
└──────┬───────┘   └──────┬───────┘   └──────┬───────┘   └──────┬───────┘
       │                  │                  │                  │
       └──────────────────┴──────────┬───────┴──────────────────┘
                                     │ HTTPS / WSS
                              ┌──────▼───────┐
                              │    Caddy     │  TLS, static web, reverse proxy
                              └──┬────────┬──┘
                                 │        │
                    ┌────────────▼──┐  ┌──▼─────────────────┐
                    │ Tulpar Server │  │ LiveKit            │
                    │ API + Gateway │  │ SFU signaling/media│
                    │ + workers     │  └──┬─────────────────┘
                    └───┬───────┬───┘     │
                        │       │      ┌──▼──────┐
                ┌───────▼──┐ ┌──▼────┐ │ Coturn  │
                │PostgreSQL│ │ Redis │ │ TURN    │
                └──────────┘ └───────┘ └─────────┘
                        │
                ┌───────▼────────────┐
                │ Object storage     │
                │ local / S3 / MinIO │
                └────────────────────┘
```

### 2.1 Instance model

An **instance** is one independent Tulpar deployment with its own:

- PostgreSQL database.
- Users and credentials.
- Communities and permissions.
- Messages, files, and object storage.
- Redis namespace.
- LiveKit and Coturn configuration.
- Public URL, TLS certificate, and configuration.

There is no central registry, no mandatory Tulpar account, and no automatic data sharing between instances.

An instance can host multiple communities. A user can join multiple communities inside one instance. A client can connect to multiple instances, but every session, token, cache entry, and notification is keyed by the instance URL. Accounts are never merged implicitly.

Practical switching behavior:

- **Desktop** keeps a separate keychain entry and session per instance, so switching back does not require a new login. It may support multiple windows later; the v1 shell has one active instance view with isolated sessions.
- **Web** switches by navigating to the other instance's origin. Cookies are origin-scoped, so a page on instance A cannot read instance B's session. Returning to A reuses A's cookie if it is still valid. A single page does not aggregate private data from multiple origins, because that would require unsafe cross-origin credentials and would weaken isolation.
- Simultaneous unified notifications across instances are a desktop capability, not a web one. Web notifications are per active origin.

### 2.2 Future federation boundary

Federation is not implemented in v1. The design keeps the door open by:

- Using globally unique UUIDv7 identifiers for public resources.
- Keeping all authorization decisions local to the instance.
- Avoiding numeric database IDs in public contracts.
- Keeping message and membership data owned by one instance.
- Reserving `origin_instance` / shadow-user mapping tables for a future bridge service rather than adding speculative columns now.

A future federation design would add explicit remote-actor and remote-community tables, signed instance-to-instance requests, and a policy layer. It would not turn the existing schema into a multi-tenant distributed system.

---

## 3. Deployment topology

### 3.1 Single-VPS topology (initial target)

| Service | Exposure | Responsibility |
|---|---|---|
| Caddy | Public 80/443 | TLS, HTTP→HTTPS, static web assets, reverse proxy `/api` and `/gateway` |
| Tulpar Server | Internal 8080 (proxied) | REST API, WebSocket gateway, background workers |
| PostgreSQL | Internal only | Source of truth |
| Redis | Internal only | Presence, tickets, rate limits, pub/sub, ephemeral state |
| LiveKit | Signaling via Caddy, media via UDP | SFU for voice/video |
| Coturn | Public 3478/5349 and relay UDP range | TURN/STUN fallback |
| Prometheus/Grafana/Loki | Optional profile, internal or admin-only | Observability |
| Object storage | Local volume or external S3/MinIO | Attachments and avatars |

PostgreSQL, Redis, and the LiveKit API port are never published publicly. LiveKit's media ports and Coturn must be reachable by clients. The full port plan is in [DEPLOYMENT.md](DEPLOYMENT.md).

### 3.2 Domain layout

Recommended:

- `tulpar.example.com` — web client, REST API, WebSocket gateway, embed pages.
- `rtc.example.com` — LiveKit signaling (proxied by Caddy).
- `turn.example.com` — Coturn TLS endpoint.
- `usercontent.example.com` — optional dedicated origin for user-uploaded files.

One origin for the app, API, and gateway keeps cookies, CORS, CSP, and WebSocket authorization simple. User content is isolated on a separate origin to reduce the impact of any stored-content XSS or misconfiguration.

### 3.3 Scaling path

The modular monolith can scale without a rewrite:

1. **Vertical first.** Increase CPU/RAM; tune PostgreSQL and LiveKit.
2. **Separate workers.** Run the same binary in API and worker roles when background delivery competes with requests.
3. **Multiple API/gateway nodes.** API nodes are stateless apart from local WebSocket connections. Redis pub/sub distributes events. A reconnect may land on another node and triggers a REST resync; no sticky sessions are required.
4. **PostgreSQL read replicas.** For read-heavy discovery and search, with explicit consistency rules.
5. **LiveKit cluster.** Multiple SFU nodes coordinated by LiveKit's Redis-backed distributed mode, with a shared TURN layer.
6. **Object storage.** Move from local disk to S3/MinIO before adding API nodes.

Microservices are not justified for the initial target. The only components that are independently deployed from day one are LiveKit and Coturn, because media infrastructure has fundamentally different scaling and network requirements.

---

## 4. Rust workspace: modular monolith

### 4.1 Crate map

| Crate | Responsibility | Owns tables / state |
|---|---|---|
| `tulpar-core` | IDs, domain enums, error primitives, pagination, clocks, shared value objects | — |
| `tulpar-common` | Validation helpers, normalization, token generation, safe formatting, time utilities | — |
| `tulpar-config` | Static configuration loading, validation, secret references, feature flags, instance settings | `instance_settings` |
| `tulpar-database` | Pool, transactions, migration runner, outbox helper, idempotency helper, health checks | `outbox_events`, `idempotency_keys`, `_sqlx_migrations` |
| `tulpar-events` | Versioned event contracts shared by REST, gateway, workers and integrations | — |
| `tulpar-permissions` | Permission bits, role/overwrite evaluation, hierarchy rules, escalation checks | — |
| `tulpar-users` | Accounts, profiles, settings, privacy preferences, instance admins | `users`, `user_settings`, `instance_admins` |
| `tulpar-auth` | Password verification, sessions, refresh rotation, device flow, verification/reset tokens | `sessions`, `refresh_tokens`, `email_verification_tokens`, `password_reset_tokens`, `device_authorizations` |
| `tulpar-social` | Friendships, blocks, DM conversation metadata | `friendships`, `user_blocks`, `direct_conversations` |
| `tulpar-communities` | Communities, members, roles, role assignments, invites, bans | `communities`, `community_members`, `roles`, `member_roles`, `invites`, `bans` |
| `tulpar-channels` | Categories, text/voice channels, DM channel rows, permission overwrites | `channels`, `channel_permission_overwrites` |
| `tulpar-messaging` | Messages, edits, deletion, reactions, replies, mentions, pins, read states, attachment links | `messages`, `message_reactions`, `message_mentions`, `attachments`, `pinned_messages`, `read_states` |
| `tulpar-notifications` | Notification records and preferences | `notifications`, `notification_preferences`, `community_notification_settings`, `channel_notification_settings` |
| `tulpar-voice` | Voice authorization, LiveKit token issuance, room state projection, moderation calls | Redis `voice:*` keys, no permanent tables |
| `tulpar-storage` | Storage abstraction, upload intents, validation, signed access | `files` |
| `tulpar-audit` | Append-only audit records and query API | `audit_logs` |
| `tulpar-integrations` | Webhooks, deliveries, API keys, bot applications/tokens, embed apps and authorization codes | `webhook_endpoints`, `webhook_deliveries`, `api_keys`, `bot_applications`, `bot_tokens`, `embed_apps`, `embed_authorization_codes` |
| `tulpar-realtime` | Redis pub/sub, presence, gateway tickets and session registry, fan-out | Redis `presence:*`, `gateway:*`, `typing:*` |
| `tulpar-api` | Axum REST router, OpenAPI, middleware, request/response DTOs, application services | — |
| `tulpar-gateway` | WebSocket protocol, connection lifecycle, authorization filters, dispatch | — |
| `tulpar-server` | Binary, dependency wiring, background workers, CLI subcommands | — |

### 4.2 Dependency rules

```
                         ┌──────────────┐
                         │ tulpar-server│
                         └───┬──────┬───┘
                 ┌───────────▼──┐ ┌─▼────────────┐
                 │  tulpar-api  │ │tulpar-gateway│
                 └──┬───┬───┬───┘ └──┬───┬───────┘
        ┌───────────┘   │   └────────┼───┘
        ▼               ▼            ▼
 ┌────────────┐  ┌────────────┐  ┌────────────┐
 │  domain    │  │ integrations│  │  realtime  │
 │  crates    │  │  voice      │  │  events    │
 └─────┬──────┘  └─────┬──────┘  └─────┬──────┘
       │               │               │
       └───────────────┴───────┬───────┘
                               ▼
                    ┌────────────────────┐
                    │ core / common /    │
                    │ config / database  │
                    │ permissions        │
                    └────────────────────┘
```

Rules:

- Dependencies point downward. No cycles.
- `tulpar-core`, `tulpar-common`, `tulpar-permissions`, and `tulpar-events` do not depend on other Tulpar crates except `core`.
- Domain crates may depend on foundation crates and `tulpar-events`.
- `tulpar-auth` may depend on `tulpar-users`; `tulpar-social` may depend on `tulpar-users` and `tulpar-channels`; `tulpar-messaging` may depend on `tulpar-channels`, `tulpar-users`, `tulpar-social`, and `tulpar-storage`; `tulpar-voice` may depend on `tulpar-channels`, `tulpar-communities`, `tulpar-users`, and `tulpar-permissions`.
- `tulpar-notifications` may depend on `tulpar-users` and `tulpar-events`; `tulpar-integrations` may depend on `tulpar-users`, `tulpar-communities`, `tulpar-channels`, `tulpar-permissions`, and `tulpar-events`; `tulpar-storage` may depend on `tulpar-users` and `tulpar-config`.
- `tulpar-api` and `tulpar-gateway` are composition roots. They may depend on every domain crate; domains never depend on them.
- A crate owns writes to its tables. Read-only joins for projections are allowed, but a write to another domain's table must go through that domain's service. This is enforced by review and by keeping SQL in the owning crate.
- `tulpar-server` wires everything and starts background workers.

### 4.3 Why a modular monolith

| Option | Assessment |
|---|---|
| Modular monolith | Chosen. One deployable, one transaction boundary, simple local development, clear crate boundaries, no network partition between domains. |
| Microservices from day one | Rejected. Adds distributed transactions, service discovery, network failure handling, duplicate infrastructure, and operational burden before product-market fit. |
| Single giant crate | Rejected. Encourages coupling, slow incremental builds, and unclear ownership. |
| Separate media service written in Rust | Rejected. LiveKit is a mature SFU; rewriting it is high-risk and out of scope. |

LiveKit, Coturn, PostgreSQL, Redis, and object storage remain independent infrastructure. That is a deliberate exception where independent deployment is justified by scaling and network topology.

### 4.4 Runtime processes

`tulpar-server` is a single binary with subcommands:

- `serve` — API, gateway, and in-process workers. Default.
- `migrate` — apply database migrations with an advisory lock and exit.
- `admin` — bootstrap or manage instance administrators.
- `worker` — run only background jobs. Reserved for horizontal worker scaling; not required initially.
- `version` / `doctor` — diagnostics.

Background jobs in `serve`:

- Outbox dispatcher: publishes committed domain events to Redis and local subscribers.
- Webhook delivery worker: exponential backoff, signed requests, delivery log.
- Email sender: verification, password reset, security notifications.
- Cleanup worker: expired sessions/tokens, orphaned uploads, old notifications, soft-deleted rows past retention.
- Voice state reconciler: removes stale LiveKit participant projections.

Workers use PostgreSQL `FOR UPDATE SKIP LOCKED` so multiple worker processes are safe.

---

## 5. Frontend architecture

### 5.1 Shared client

The web client is the canonical SPA. The desktop client wraps the same build in Tauri 2 and adds native capabilities through a narrow platform adapter.

```
apps/web (React SPA, PWA)
   │
   ├── packages/ui              design system and accessible primitives
   ├── packages/api-client      typed REST client, auth, retries
   ├── packages/shared-types    generated OpenAPI types + WS event types
   ├── packages/i18n            en/tr resources and formatting
   └── packages/livekit-client  voice/video wrapper and device management

apps/desktop (Tauri 2 shell)
   └── loads the web build + exposes native commands through a platform adapter
```

The desktop app does not duplicate screens. Platform-specific behavior is selected at runtime:

- Web: cookies for refresh, browser notifications, focus-scoped push-to-talk.
- Desktop: device authorization flow, OS keychain, tray, native notifications, global push-to-talk where supported, deep links, updater.

### 5.2 State model

- **Server state:** TanStack Query. Cache keys are instance-scoped: `[instanceUrl, resource, id]`.
- **Client/session state:** Zustand stores for auth/session, UI layout, voice room, and draft state.
- **Realtime:** a single gateway client per instance, dispatching validated events into the query cache and stores.
- **Persistence:** localStorage for non-sensitive UI preferences only. No refresh tokens, message history, or private API responses in localStorage. IndexedDB is reserved for opt-in drafts and the PWA shell cache.
- **Instance isolation:** every cache, store, socket, and notification is keyed by instance URL. Switching instances never reuses another instance's tokens.

### 5.3 Why React

React has the broadest component ecosystem, mature accessibility primitives through Radix, strong TanStack Query/Zustand support, and a large hiring pool. Svelte would produce smaller bundles, but the ecosystem and shared-component benefits win for this project. Performance is addressed with route-level code splitting, virtualization for long lists, and strict render discipline rather than a framework change.

---

## 6. Data architecture

### 6.1 PostgreSQL is the source of truth

All durable state lives in PostgreSQL:

- Users, credentials, sessions, communities, roles, channels, messages, files, invitations, audit records, webhooks.
- Message edits and deletions are committed before a client acknowledges them.
- WebSocket events are notifications of committed state, never the only copy.

### 6.2 Redis is ephemeral

Redis holds only state that can be rebuilt or lost with bounded impact:

- Presence and heartbeat TTLs.
- WebSocket tickets and gateway session metadata.
- Rate-limit counters.
- Typing indicators.
- LiveKit participant projections.
- Pub/sub fan-out between API/gateway nodes.

The message database must remain fully functional if Redis disappears. See §9.

### 6.3 Outbox

A transactional outbox prevents the classic failure where a database commit succeeds but the real-time event or webhook is never sent.

1. A domain service writes its state change and an `outbox_events` row in one transaction.
2. The request returns after commit.
3. The outbox dispatcher claims pending rows with `FOR UPDATE SKIP LOCKED`.
4. It publishes to Redis/local subscribers and/or delivers webhooks.
5. It marks the row processed or schedules a retry with backoff.

Consumers must be idempotent: events carry a stable `event_id`, and clients deduplicate message IDs.

### 6.4 Idempotency

Mutating endpoints that create resources accept `Idempotency-Key`. The server stores a hash of the request and the response for a bounded window (24 hours by default). Identical retries return the original response; a different body with the same key returns `409`.

Message creation additionally supports a client-generated nonce to prevent duplicate sends after a network timeout.

### 6.5 Pagination and search

- Cursor pagination everywhere; no offset pagination for messages or members.
- Message cursors are opaque base64url values derived from `(created_at, id)`. A message ID passed as a cursor is resolved to its tuple for stable ordering across API nodes.
- Default page size 50, maximum 100.
- Message search uses PostgreSQL full-text search plus trigram matching for partial terms, scoped to channels the actor can read. The initial search configuration uses the `simple` dictionary to avoid incorrect stemming for Turkish, with `pg_trgm` for substring matching.

### 6.6 Retention

Retention is configurable by the operator. Deleting a community or account is a staged process: soft delete, grace period, then a purge job that removes dependent rows and files. Default retention periods are listed in [DATABASE.md](DATABASE.md#132-default-retention).

---

## 7. Key request flows

### 7.1 Web login and refresh

```
Browser                Tulpar API                 PostgreSQL / Redis
   │ POST /auth/login        │                          │
   │────────────────────────>│ verify Argon2id          │
   │                         │ create session + refresh │
   │                         │─────────────────────────>│
   │ 200 access token        │                          │
   │ Set-Cookie refresh      │                          │
   │<────────────────────────│                          │
   │                         │                          │
   │ POST /auth/refresh      │                          │
   │ (cookie, Origin check)  │                          │
   │────────────────────────>│ validate + rotate        │
   │ new access + cookie     │─────────────────────────>│
   │<────────────────────────│                          │
```

The browser keeps the access token in memory. The refresh token is an `HttpOnly`, `Secure`, `SameSite=Lax` cookie scoped to the auth path. Refresh reuse revokes the session family.

### 7.2 Desktop device authorization

```
Desktop app              Browser                  Tulpar API
   │ POST /auth/device/code  │                         │
   │──────────────────────────────────────────────────>│
   │ user_code + verification URI                      │
   │<──────────────────────────────────────────────────│
   │ open browser ──────────>│ login + approve         │
   │                         │────────────────────────>│
   │ poll /auth/device/token │                         │
   │──────────────────────────────────────────────────>│
   │ access + refresh token  │                         │
   │<──────────────────────────────────────────────────│
   │ store refresh in OS keychain, access in memory    │
```

The desktop client never stores a refresh token in webview storage. The Tauri Rust layer owns the keychain entry. See [DESKTOP.md](DESKTOP.md).

### 7.3 Send a message

```
Client → POST /channels/{id}/messages
  1. authenticate (Bearer access token or bot/API key)
  2. load channel and community membership
  3. evaluate permissions (VIEW_CHANNEL, SEND_MESSAGES, slowmode, attachments)
  4. validate content, mentions, reply target, attachment ownership
  5. BEGIN
       insert message
       insert mentions
       insert attachment links
       update channel last_message_at
       insert outbox event MESSAGE_CREATE
     COMMIT
  6. return 201 with the persisted message
  7. outbox dispatcher publishes MESSAGE_CREATE
  8. gateway fans out to authorized connections
```

A client that misses the WebSocket event recovers the message through `GET /channels/{id}/messages?after=...`. The REST response is authoritative.

### 7.4 Join a voice channel

```
Client → POST /channels/{id}/voice/token {password?}
  1. authenticate
  2. check channel is voice, community membership, CONNECT_VOICE
  3. check ban/timeout/room capacity
  4. verify room password if set (Argon2id, rate limited)
  5. mint a short-lived LiveKit JWT (10 minutes)
  6. return {url, token, ice_servers, expires_at}
Client → LiveKit SFU (WebRTC)
LiveKit → POST /webhooks/livekit (signed)
  7. update Redis voice projection and broadcast VOICE_STATE_UPDATE
```

The LiveKit API secret never leaves the server. Tulpar never issues a token for a room the user cannot join.

### 7.5 Gateway connect and reconnect

```
1. POST /gateway/ticket (Bearer access token) → single-use ticket (60s)
2. WSS /gateway, first frame IDENTIFY {ticket}
3. READY {session_id, heartbeat_interval, seq: 0}
4. Client calls GET /sync and reconciles local state
5. DISPATCH events increment seq; client detects gaps
6. On reconnect, client obtains a new ticket, calls /sync, then fetches
   messages after its last known message ID per active channel
```

There is no durable per-user event log. Recovery uses authoritative REST snapshots and per-channel message cursors. This is intentionally simpler and more reliable than replaying an event stream that can diverge from the database. Details are in [WEBSOCKET.md](WEBSOCKET.md).

---

## 8. Real-time architecture

- One WebSocket gateway per API node, mounted on the same Axum server at `/gateway`.
- Redis pub/sub distributes events between nodes; local broadcast handles single-node deployments.
- Each connection has a bounded outbound queue. A slow consumer is disconnected instead of blocking the server.
- The gateway filters every event through the recipient's current permissions. A user who loses access to a channel stops receiving that channel's events.
- Presence uses Redis keys with TTL and heartbeat. Connections are counted per user so multiple devices work correctly.
- Typing indicators are ephemeral, rate limited, and never persisted.
- Gateway sessions have a maximum lifetime and are closed when the session is revoked or the instance enters maintenance mode.

The protocol, event names, payloads, close codes, and recovery rules are specified in [WEBSOCKET.md](WEBSOCKET.md).

---

## 9. Failure modes and degraded behavior

| Failure | User impact | System behavior |
|---|---|---|
| PostgreSQL unavailable | No new reads/writes; API returns 503 | Gateway stops accepting new sessions; existing sockets receive `MAINTENANCE`; no silent data loss |
| Redis unavailable | Presence, typing, cross-node fan-out, tickets, and strict rate limits degrade | Single-node local fan-out continues; presence is local/unknown; rate limits fall back to per-process memory; tickets fail closed (gateway unavailable) until Redis returns; REST messaging continues |
| LiveKit unavailable | Voice/video unavailable; text unaffected | Token endpoint returns 503 with a clear error; no fake voice fallback |
| Coturn unavailable | Direct WebRTC may still work; restrictive networks fail | Client surfaces network error and suggests retry; LiveKit continues for direct connections |
| Object storage unavailable | Uploads/downloads fail; text unaffected | Clear 503/507; no partial attachments marked ready |
| SMTP unavailable | Verification/reset emails delayed | Jobs retry with backoff; registration state is preserved; operator alert |
| Outbox dispatcher down | Events delayed, not lost | Rows remain pending; clients can still fetch state via REST |
| API node crashes | WebSocket connections drop | Reconnect to another node and resync; no sticky sessions required |
| LiveKit webhook missed | Voice projection stale | Reconciler queries LiveKit rooms/participants and repairs state |

### 9.1 Redis optionality rules

- Redis is not on the message write path. A message commit does not require Redis.
- Redis is allowed on the gateway ticket path; if it is down, new WebSocket connections fail closed with a retryable error.
- Redis is allowed on the rate-limit path; if it is down, each node uses a bounded in-memory limiter and reports degraded mode. This is weaker but still bounded.
- Presence is explicitly best-effort. The UI must show “unknown” or “offline” rather than inventing a state.
- Multi-node fan-out requires Redis. A multi-node deployment without Redis is unsupported; production readiness checks fail.

---

## 10. Security architecture

### 10.1 Trust boundaries

| Boundary | Trust |
|---|---|
| Browser / desktop webview | Untrusted; may run modified code |
| Embed iframe / third-party site | Untrusted |
| Public internet | Untrusted |
| Caddy | Trusted TLS terminator, minimal configuration |
| Tulpar API/gateway | Trusted application boundary |
| PostgreSQL / Redis | Trusted internal services, never public |
| LiveKit / Coturn | Trusted infrastructure with scoped secrets |
| Object storage | Trusted storage; user content is untrusted |
| Instance administrator | Powerful but audited; cannot read passwords or refresh tokens |

### 10.2 Non-negotiable rules

1. Every authorization decision is made on the backend.
2. Every resource lookup is scoped by the authenticated actor's membership and permissions.
3. Private resources return `404` rather than leaking their existence when the actor is not allowed to know.
4. No secret is shipped to a client: LiveKit API secret, database URL, Redis URL, SMTP password, S3 secret, webhook signing secret, JWT signing key.
5. User-uploaded content is served from a separate origin with `Content-Disposition: attachment` for non-previewable types, `X-Content-Type-Options: nosniff`, and a restrictive CSP.
6. No custom cryptography. Standard algorithms and libraries only.
7. No plaintext passwords, tokens, or message content in logs.
8. Admin actions and moderation actions are audited.

Full details are in [SECURITY.md](SECURITY.md).

---

## 11. Performance and scalability targets

These are **targets to validate**, not marketing claims. Baseline reference hardware: 4 vCPU, 8 GB RAM, SSD, 1 Gbps network, PostgreSQL and Redis on the same host for development/small deployments.

| Metric | Initial target |
|---|---|
| Concurrent WebSocket connections per API node | 500 |
| Concurrent active text users per instance | 200 |
| Message writes sustained | 100 messages/second across channels |
| REST p95 latency (common authenticated reads) | < 200 ms |
| Message commit p95 | < 150 ms |
| WebSocket fan-out p95 (same region) | < 500 ms after commit |
| Concurrent voice participants per LiveKit node | 100 (validate with real media) |
| Concurrent video publishers per LiveKit node | 20 at 720p/30fps (bandwidth-dependent) |
| Database connections per API node | 20–30, pooled |
| Cold web app load | < 3 s on a mid-range connection |

Rules:

- Never broadcast to all users. Fan-out is scoped to communities, channels, or users.
- Permission filters run before delivery, not in the client.
- Message lists are cursor-paginated and virtualized in the UI.
- The gateway has per-connection backpressure and a maximum frame size.
- Load tests are part of the release gate once messaging exists.

---

## 12. Observability

- **Logs:** structured JSON via `tracing`, with request ID, trace ID, user ID (not email), route, status, latency. Secrets and message bodies excluded. Human-readable logs in development.
- **Metrics:** Prometheus endpoint on an internal listener. HTTP latency/error rates, DB pool saturation, Redis health, outbox lag, webhook failures, gateway connections, message throughput, voice token issuance, LiveKit participant counts.
- **Traces:** OpenTelemetry OTLP export optional, disabled by default. Sampling configurable; no message content in span attributes.
- **Health:** `/health/live` (process), `/health/ready` (database + Redis + migrations + storage + LiveKit configuration). Detailed dependency health is admin-only.
- **Dashboards/alerts:** Grafana for API latency, error budget, DB, Redis, gateway, outbox, webhook, LiveKit; Loki or compatible log store optional.
- **Audit logs:** separate append-only records with retention, visible to instance and community administrators according to policy.

No mandatory telemetry is sent to the Tulpar project.

---

## 13. Technology decisions and alternatives

| Decision | Chosen | Alternatives considered | Rationale |
|---|---|---|---|
| Backend language | Rust | Go, TypeScript, C# | Memory safety, performance, strong async ecosystem, single binary |
| HTTP framework | Axum | Actix Web, Poem | Tower ecosystem, composable middleware, active maintenance |
| Database access | SQLx | Diesel, SeaORM | Async, explicit SQL, compile-time checked queries, migrations |
| Database | PostgreSQL 16+ | MySQL, SQLite | Constraints, JSONB, full-text, trigram, mature operations |
| Ephemeral state | Redis 7+ | In-memory only, NATS | Presence, rate limits, pub/sub, simple operations |
| Realtime | Native WebSocket protocol | Socket.IO, SSE only | Control over authorization, resume, and backpressure; works with native clients |
| Media | LiveKit | Janus, mediasoup, custom SFU | Mature SFU, SDKs, scalable, standard WebRTC |
| TURN | Coturn | LiveKit built-in TURN only | Explicit, widely deployed TURN; independent from SFU |
| Desktop | Tauri 2 | Electron, Flutter | Small binaries, native integration, Rust core, shared web UI |
| Web | React 19 + Vite | SvelteKit, Angular | Ecosystem, shared UI, hiring, accessibility tooling |
| Styling | Tailwind CSS + Radix primitives | CSS-in-JS, MUI | Consistent design system, accessible primitives, small runtime |
| Server state | TanStack Query | Redux Toolkit Query, SWR | Cache control, retries, mutations, devtools |
| Client state | Zustand | Redux, Jotai | Small, explicit stores for session/UI/voice |
| Reverse proxy | Caddy | Traefik, Nginx | Automatic TLS, simple config, HTTP/3 support |
| IDs | UUIDv7 | BIGSERIAL, ULID | Globally unique, time-ordered, no coordination, future federation-friendly |
| Auth tokens | Short-lived JWT + rotating refresh | Server sessions only, PASETO | Standard, efficient verification, revocable refresh sessions |
| Permissions | 64-bit bitfield + API string arrays | Row-per-permission | Fast evaluation, compact storage, channel overwrites |
| Events | Outbox + Redis pub/sub + REST recovery | Kafka, durable per-user event log | Right-sized for initial scale; no premature event-sourcing complexity |

Exact dependency versions are pinned in Phase 1. Major versions are recorded in [DECISIONS.md](DECISIONS.md).

---

## 14. Risks and platform limitations

### 14.1 Project risks

| Risk | Impact | Mitigation |
|---|---|---|
| Scope is very large | Incomplete or unstable product | Strict phases, exit criteria, no phase skipping |
| Permission model complexity | Security bugs, privilege escalation | Central evaluator, property-based tests, adversarial integration tests |
| LiveKit/Coturn networking | Voice fails on restrictive networks | Early Phase 4 spike, real TURN testing, clear diagnostics |
| Tauri WebView differences | Desktop features vary or fail on Linux | Capability detection, fallbacks, CI builds, documented limitations |
| Single-VPS resource contention | Poor voice/text performance under load | Separate LiveKit when possible, load tests, documented sizing |
| Screenshot/screen capture on Linux Wayland | Feature unavailable in some environments | PipeWire portal detection; disable with clear explanation; do not fake support |
| Search quality in Turkish | Poor results | `simple` dictionary + trigram; evaluate language-specific configs later |
| Email deliverability | Verification/reset emails go to spam | SPF/DKIM/DMARC documentation, configurable SMTP, resend flow |
| Supply-chain compromise | Critical | Lockfiles, cargo-deny, pnpm audit, pinned images, signed releases |
| User content XSS | Account compromise | Separate user-content origin, MIME validation, download headers, strict CSP |
| No E2EE | Users may assume privacy | Explicit documentation and UI disclosure; E2EE is a future design decision |
| OneDrive workspace | Slow builds, file locks | Move repo off synced folders or exclude build outputs |

### 14.2 Platform-specific limitations (summary)

- **Browsers:** microphone/camera/screen capture require a secure context and user gesture. Background tabs throttle timers and may suspend audio. iOS Safari does not support the same screen capture or background behavior as desktop browsers. PWA installability and push differ by browser.
- **Desktop, Windows:** WebView2 runtime required; native notifications respect Focus Assist; global push-to-talk works through Tauri global shortcuts.
- **Desktop, Linux:** WebKitGTK varies by distribution; Wayland restricts global shortcuts and screen capture; tray support depends on the desktop environment and AppIndicator extension; Secret Service may be unavailable, in which case Tulpar does not persist the refresh token insecurely.
- **Voice:** UDP is required for good quality. TURN over TCP/TLS is the fallback for restrictive networks. Corporate firewalls can still block media. No client can guarantee connectivity.
- **Embed:** third-party cookie blocking, ad blockers, iframe height constraints, and host CSP can prevent or degrade embedded features. The SDK must detect and explain failures.
- **Multi-instance:** browsers isolate cookies per origin, which is exactly what Tulpar wants, but a single web app cannot read another instance's cookies. Each instance is authenticated separately.

Full details are in [WEB.md](WEB.md), [DESKTOP.md](DESKTOP.md), [VOICE.md](VOICE.md), [VIDEO.md](VIDEO.md), and [EMBED_SDK.md](EMBED_SDK.md).

---

## 15. Recommended architecture improvements

These improvements are incorporated into the design rather than deferred:

1. **Single-origin app/API/gateway.** Simplifies cookies, CSRF, CSP, and CORS. A separate user-content origin isolates uploaded files.
2. **`GET /sync` bootstrap endpoint.** Gives clients a consistent snapshot for startup and reconnect instead of relying on event replay.
3. **Transactional outbox.** Prevents lost webhooks/events without introducing Kafka.
4. **Shared event contracts crate.** Prevents REST, gateway, workers, and SDKs from drifting.
5. **Integrations crate.** Webhooks, API keys, and bots have their own lifecycle and background delivery; mixing them into `api` would blur ownership.
6. **No durable per-user WebSocket event log.** Authoritative REST recovery is simpler and avoids a second source of truth.
7. **Desktop device authorization flow.** Keeps refresh tokens out of webview storage and avoids embedding passwords in native login.
8. **Permission evaluation as a pure, heavily tested module.** Security-critical logic should not be spread across handlers.
9. **PostgreSQL 16 baseline.** Stable, available on Ubuntu 24.04, well supported by managed providers.
10. **Node 24 LTS + pnpm 10.** Avoids depending on a non-LTS local Node version.
11. **Admin UI integrated into the web app at `/admin` initially.** A separate admin bundle can be added later if the attack surface or deployment model requires it.
12. **Contract tests for OpenAPI and WebSocket types.** Generated TypeScript types must match the Rust contracts in CI.

Items deliberately not added: a message queue, a service mesh, Kubernetes manifests, GraphQL, a custom ORM, a custom crypto layer, and a plugin runtime. None are justified for the initial target.

---

## 16. Phase 0 consistency checklist

- [x] Repository structure created for all planned products.
- [x] Instance model and multi-instance client behavior defined.
- [x] Crate boundaries and dependency direction defined.
- [x] PostgreSQL schema and ownership defined.
- [x] REST API conventions and endpoint catalog defined.
- [x] WebSocket protocol, events, and recovery defined.
- [x] Authentication flows defined for web and desktop.
- [x] Permission bits, hierarchy, and overwrite evaluation defined.
- [x] LiveKit/Coturn integration and token rules defined.
- [x] Desktop and web architecture defined.
- [x] Embed security model defined.
- [x] Security model, failure modes, and limitations documented.
- [x] Deployment topology, port plan, backup, and upgrade strategy documented.
- [x] Testing strategy and phase exit criteria documented.
- [x] No application code, secrets, or unverified deployment manifests created.
- [x] No use of the previous project name.
- [ ] License selected (open decision).
- [ ] Phase 0 review and approval (waiting).

---

## 17. Development environment risks

The current workspace is on Windows under a OneDrive-synced directory. This creates three practical risks:

1. **File locking and sync conflicts** during Rust and Node builds.
2. **Slow builds** because `target/` and `node_modules/` contain hundreds of thousands of files.
3. **Antivirus and cloud sync** can cause intermittent compiler and test failures.

Recommended: move the repository to a local path such as `C:\dev\tulpar`, or configure OneDrive to exclude the repository and build-output directories. This is a developer-environment concern, not a product architecture change.

The current machine also lacks Rust, Docker, pnpm, and PostgreSQL tooling. Phase 1 must begin with a verified toolchain setup before writing backend code.

---

## 18. Next step

Phase 1 is in progress. Implemented and locally verified:

- Rust workspace and crate skeletons.
- Configuration loading and validation.
- Axum server with health endpoints and structured logging.
- PostgreSQL pool, migrations, and the initial identity/auth schema.
- Redis connection, health, and rate limiting with graceful degradation.
- OpenAPI generation and Swagger UI.
- Registration, login, refresh rotation, logout, sessions, email verification, password reset/change, re-authentication, and device authorization.
- User profile/settings endpoints and PostgreSQL integration tests.

Phase 1 is closed apart from the license decision; Phase 2 now implements communities, roles, invites, channels, friends, blocks, and DM metadata. See [PHASE_1_REPORT.md](PHASE_1_REPORT.md), [PHASE_2_REPORT.md](PHASE_2_REPORT.md), and [ROADMAP.md](ROADMAP.md).
