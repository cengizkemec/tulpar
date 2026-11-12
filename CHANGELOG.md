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
- Phase 2 integration test covering community creation, invite join, role assignment, permissions, friendship, DM, and blocking.
- Verified Docker development stack with pinned PostgreSQL, Redis, LiveKit, and Coturn images.

### Planned — Phase 2 completion

- Channel permission overwrite endpoints and channel-scoped evaluation.
- Community discovery/search and full moderation listing endpoints.
- Membership and role hierarchy edge cases beyond the integration test.
- DM message storage and delivery (Phase 3).
- Generated SDK/API client wrappers.

### Planned — Phase 1/2 hardening

- First successful GitHub Actions run.
- `cargo deny` policy and dependency-review workflow.
- Remove the documented audit exception or replace the RSA-bearing JWT dependency.
- Move the repository outside OneDrive for direct Docker builds.
- Full production Compose stack and runbooks (Phase 9).

### Notes

- No production-ready release exists.
- The project license remains undecided.
- Local email delivery uses a development log driver; production configuration rejects it.
