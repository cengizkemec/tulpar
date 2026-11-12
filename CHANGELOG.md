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

### Planned — Phase 3

- Text messages, attachments, edits, deletes, reactions, pins, and threads.
- WebSocket gateway, presence, typing, and recovery.
- DM message storage and read states.
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
