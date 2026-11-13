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
