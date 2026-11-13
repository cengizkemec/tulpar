# Changelog

All notable changes to Tulpar are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Tulpar uses semantic versioning for public releases once versioning begins.

## [Unreleased]

### Added — Phase 1 core backend foundation

- Rust workspace with 21 crates and shared dependency policy.
- Validated configuration loading from TOML and `TULPAR__*` environment variables.
- PostgreSQL connection pool, initial identity/auth migration, and migration CLI.
- Redis connection manager with degraded-mode health reporting and Redis-backed/in-memory rate limiting.
- Axum API with request IDs, tracing, CORS, body limits, timeouts, and structured problem+json errors.
- Health endpoints, version endpoint, and generated OpenAPI document with Swagger UI.
- Argon2id password hashing, Argon2 concurrency-safe async execution, and dummy-hash timing mitigation.
- Registration, email verification tokens, login, refresh rotation with reuse detection, logout, session listing/revocation, password change/reset, and re-authentication.
- Desktop device authorization flow with one-time codes.
- Self profile, public profile, and user settings endpoints.
- SMTP email delivery via `lettre`, verified end to end with Mailpit.
- Account deletion with anonymization, username reservation, session revocation, and last-admin protection.
- Account data export with profile, settings, and session metadata.
- GitHub Actions CI workflow and Dependabot configuration (not yet proven on GitHub).
- Integration tests against PostgreSQL for auth, refresh reuse, email verification, password reset, account deletion, and export.
- Local verification report in `docs/PHASE_1_REPORT.md`.

### Added — Phase 2

- Communities, members, roles, role assignments, invites, bans, and basic channel foundation.
- Channel permission overwrite endpoints and channel-scoped visibility/lookup enforcement.
- Centralized permission bitfield and evaluator with role hierarchy, overwrite ordering, administrator bypass, owner bypass, and timeout handling.
- Friendships, friend requests, blocking, and DM conversation metadata with privacy checks.
- Community/member/role/invite/channel/friend/block/DM REST endpoints and OpenAPI schemas.
- Public community discovery, user/member search, ban listing, role reordering, and ownership transfer.
- Audit records for community, moderation, role, invite, channel-overwrite, friend, block, and DM metadata actions, with a `VIEW_AUDIT_LOG`-protected audit endpoint.
- Per-user rate limits for community creation, joins, invite creation, friend requests, and DM creation.
- Phase 2 integration tests covering the main social flow, an authorization/escalation matrix, and Phase 2 database constraints.
- Verified Docker development stack with pinned PostgreSQL, Redis, LiveKit, and Coturn images.

### Added — Phase 3 messaging foundation

- `0003_messaging.sql` with messages, reactions, mentions, pins, read states, and search indexes.
- Persistent message send/list/search/edit/delete with keyset pagination and client-nonce idempotency.
- Same-channel replies, reactions, pins, read-state cursors, unread counts, and slowmode.
- Community-channel permission enforcement and participant-only DM messages.
- Moderator message-delete and pin audit records.
- Messaging integration test covering idempotency, edits, reactions, pins, read states, search, slowmode, timeouts, tombstones, and DMs.

### Added — Phase 3 completion

- WebSocket gateway with single-use tickets, HELLO/IDENTIFY/READY, heartbeats, sequence numbers, permission-scoped dispatch, presence, typing, and slow-consumer handling.
- `/sync` bootstrap snapshot and per-channel `after` catch-up.
- Local attachment storage, upload validation, attachment-only messages, and permission-checked downloads.
- User mentions with member, role, and `@everyone` authorization.
- In-app notifications, notification preferences, channel/community mute enforcement, and mark-read APIs.
- Transactional outbox events and a background dispatcher for `NOTIFICATION_CREATE` gateway delivery.
- Docker uploads volume with a privilege-dropping entrypoint.

### Added — Phase 4 voice backend

- LiveKit token minting with permission-derived publish sources and server-only admin grants.
- Coturn REST ephemeral credentials and STUN/TURN/TLS ICE server response.
- Voice token endpoint with channel visibility, `CONNECT_VOICE`, bans, password, capacity, and timeout-to-listener policy.
- Room password set/remove and voice participant projection.
- Signed LiveKit webhooks with body SHA-256 verification, durable deduplication, room state, join/leave, tracks, and stale-left protection.
- LiveKit admin integration for server mute/deafen, disconnect, move, and room deletion.
- Kick/ban, timeout, and user-level `CONNECT_VOICE` overwrite revocation paths.
- Voice unit and integration tests with a mock LiveKit admin API.
- Redis-backed voice projection with process-local fallback.
- LiveKit participant reconciliation worker and role/overwrite bulk revocation.
- Edge-based two-client voice e2e harness with fake microphones, direct and forced TURN relay modes.

### Added — Phase 5/6 web client foundation

- React 19/Vite/Tailwind web app with login/register and refresh-cookie session bootstrap.
- `/sync`-driven community, channel, and DM navigation.
- Text message history and sending with REST and live gateway invalidation.
- LiveKit voice join/leave, microphone, camera, and screen-share controls with lazy-loaded media chunk.
- Full English/Turkish UI localization with persisted language choice and `document.lang` updates.
- Friends UI: user search, requests, add/remove, block/unblock, and DM creation.
- Personal settings UI: profile, status, privacy, read receipts, and notification preferences.
- In-app notification center with unread badge and read controls.
- PWA manifest, install prompt, offline banner, and shell-only service worker; private API/gateway responses are never cached, and offline shell navigation is verified in Docker mode.
- Camera/screen video tiles with LiveKit track attachment, grid/focus/pin layouts, and fullscreen; remote audio cleanup on leave/unsubscribe.
- Microphone and camera device selection with live device switching.
- Push-to-talk hold control and LiveKit connection-quality indicator.
- Voice connection now succeeds without microphone permission; mic errors are non-fatal.
- Browser notification permission control and gateway-driven notifications.
- Instance administration API and UI: overview metrics, user search, enable/disable, grant/revoke admin, and global audit log.
- Accessibility pass: skip link, navigation landmark, labelled controls, Escape-to-close mobile navigation, and WCAG AA accent contrast; Lighthouse login shell scores 100/100/100.
- Web connects to its own origin; multi-server IP/domain entry is reserved for the desktop client.
- Community settings UI: general edits, member kick/ban, role display, invites, bans, and audit log with permission-gated actions.
- Session bootstrap avoids unauthenticated refresh requests on first visit using a non-sensitive local hint.
- Headless Edge web smoke test covering login, friends/settings/notification/community routes, language switching, skip link, responsive navigation, message send, gateway delivery, voice connection, camera publishing, screen sharing, leave cleanup, offline banner, service worker, and offline shell reload.
- Docker nginx web container at `http://localhost:15173` proxying API, gateway, health, and webhooks; trusted-proxy IP handling for rate limits and audits.

### Added — Phase 7 desktop client

- Tauri 2 shell in `apps/desktop` embedding the shared web build.
- Desktop-only server address entry, saved instances, and device authorization with browser approval.
- Web `/device` approval page.
- OS keychain storage for refresh tokens, serialized refresh, and keychain cleanup on logout/revocation.
- System tray with close-to-tray, unread tooltip, and native notifications.
- Global push-to-talk shortcut registration and `ptt` down/up events.
- Deep links (`tulpar://add-instance`), single-instance forwarding, autostart, and window-state persistence.
- Desktop capability reporting and CSP/CORS support for WebView2 origins.
- `tests/web-e2e/desktop.mjs` Windows smoke test covering device approval, keychain refresh, PTT, capabilities, and deep links.

### Added — Phase 8 embed platform

- Embed applications with exact origin allowlists, widget flags, public keys, and token-version revocation.
- Public embed tokens and one-time authenticated member authorization codes.
- `/embed/authorize` popup flow and member-view sign-in in the widget.
- Public message previews for a configured public text/announcement channel.
- `/api/v1/embed/*` token, community, and authorization endpoints.
- Hosted `/embed/widget` iframe page with postMessage resize/ready events.
- `/embed/v1/tulpar-embed.js` loader and `@tulpar/embed-sdk` TypeScript wrapper.
- Embed demo site and community-settings embed management UI.
- Embed integration tests for public/member flows, origins, widgets, code reuse, revocation, and private communities.
- Scoped API keys with hashed `tk_...` tokens, read/write scopes, revocation, rate limits, and settings UI.
- Outgoing webhooks with HMAC-SHA256 signatures, queued retries with exponential backoff, redirect blocking, and SSRF checks at create and delivery time.
- Minimal Rust SDK for login, sync, message sending, and embed token/community calls.
- Minimal Prometheus `/metrics` endpoint and optional production scrape profile.

### Added — Phase 9 production hardening

- Production Compose stack with PostgreSQL, Redis, LiveKit, Coturn, API, web, and Caddy.
- Caddy TLS, HTTP/3, HSTS, and security headers.
- Production environment template and deployment/firewall/update documentation.
- Backup and restore scripts with checksums and retention.
- Windows desktop shell check in CI.

### Planned — Phase 4 follow-ups

- Real-browser audio verification and TURN fallback with UDP blocked.
- Client device selection, mute/deafen, PTT, VAD, and per-user volume.
- Redis-backed multi-node voice projection and LiveKit reconciliation worker.
- Role-level bulk voice revocation.

### Planned — Phase 3 follow-ups

- Multi-node Redis fan-out and shared presence/ticket state.
- Gateway multi-client load testing.
- S3/MinIO storage, attachment scanning, quotas, and cleanup.
- Role/`@everyone` notification fan-out and per-community/channel setting APIs.
- Generated SDK/API client wrappers.

### Planned — Phase 1/2 hardening

- `cargo deny` policy and dependency-review workflow.
- Remove the documented audit exceptions or replace the optional RSA/SQLite-bearing dependencies.
- Move the repository outside OneDrive for direct Docker builds.
- Transactional community creation with default channels.
- Full production Compose stack and runbooks (Phase 9).

### Notes

- No production-ready release exists.
- The project license remains undecided.
- Local email delivery uses a development log driver; production configuration rejects it.
