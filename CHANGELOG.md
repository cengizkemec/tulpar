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

### Planned — Phase 1 completion

- Docker Compose development stack (Docker is not installed on the current machine).
- Full OpenAPI security scheme documentation.
- Admin CLI end-to-end test.
- First successful GitHub Actions run.

### Notes

- No production-ready release exists.
- The project license remains undecided.
- Local email delivery uses a development log driver; production configuration rejects it.
