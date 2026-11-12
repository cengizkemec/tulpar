# Phase 1 Report — Core Backend Foundation

**Status:** Implemented and locally verified; Phase 1 is not fully closed.

**Date:** 2026-11-12

Phase 1 created a working Rust backend foundation with real PostgreSQL and Redis-backed integration tests. It is not a production release. The remaining Phase 1 gaps are listed explicitly below.

---

## 1. Environment

| Item | Result |
|---|---|
| OS | Windows, workspace under OneDrive |
| Rust | 1.99.1 (stable) |
| Cargo | 1.99.1 |
| Node.js | 26.13.0 local (project target remains Node 24 LTS) |
| pnpm | 10.34.9 (installed globally) |
| Docker | Not installed |
| PostgreSQL | 16.15 portable user-local cluster on `127.0.0.1:55432` |
| Redis-compatible server | Portable Redis 5.0.14.1 on `127.0.0.1:6380` |
| Admin privileges | Not used for PostgreSQL or Redis |

The winget PostgreSQL installer exited with code 1 after creating files, but its binaries were usable. A separate user-local cluster was initialized with `initdb` and runs without a Windows service. Redis is a portable development binary, not a production recommendation.

Norton was not modified. No antivirus intervention was required for the build or tests.

---

## 2. Implemented

### Workspace

- `Cargo.toml` with 21 workspace crates.
- `rust-toolchain.toml` with stable Rust, rustfmt, and clippy.
- Centralized workspace dependency versions.
- `Cargo.lock` generated and kept for reproducible builds.

### Foundation crates

- `tulpar-core` — protocol constants and UUIDv7 IDs.
- `tulpar-common` — username/email normalization and validation, NFC text/password normalization, random tokens, SHA-256 token hashing.
- `tulpar-config` — TOML + `TULPAR__*` environment configuration, validation, secret redaction discipline, production safety checks.
- `tulpar-database` — PostgreSQL pool, migrations, health check.
- `tulpar-audit` — append-only audit records.
- `tulpar-realtime` — lazy Redis connection manager, health, basic get/set/delete, Redis-backed fixed-window rate limiting with in-memory fallback.
- `tulpar-users` — user records, profile/settings updates, credential lookup, transactional password/email helpers.
- `tulpar-auth` — Argon2id passwords, access JWTs, refresh rotation/reuse detection, sessions, device authorization, email verification, password reset/change, re-authentication, and SMTP delivery via `lettre`.
- `tulpar-api` — Axum router, DTOs, problem+json errors, Bearer extractor, cookie helpers, Origin checks, request IDs, CORS, body limits, timeouts, OpenAPI, Swagger UI.
- `tulpar-server` — `serve`, `migrate`, `admin create`, and `version` CLI commands.

The other 11 crates are documented workspace skeletons with no fake functionality. They are placeholders for their owning phases.

### Database

Initial migration `crates/database/migrations/0001_initial.sql`:

- `citext` and `pg_trgm`.
- `users`, `user_settings`, `username_reservations`, `instance_admins`.
- `files`, `file_variants`.
- `sessions`, `refresh_tokens`, `email_verification_tokens`, `password_reset_tokens`, `device_authorizations`.
- `audit_logs`, `outbox_events`, `idempotency_keys`, `instance_settings`.
- Constraints, partial indexes, trigram indexes, and session/token indexes.

The full v1 schema for communities, roles, channels, messaging, social, voice, notifications, integrations, and embeds remains planned for later phases.

### API

Implemented endpoints:

- `GET /health/live`
- `GET /health/ready`
- `GET /api/v1/version`
- `GET /api/v1/openapi.json`
- `GET /docs` (Swagger UI)
- `POST /api/v1/auth/register`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/refresh`
- `POST /api/v1/auth/logout`
- `POST /api/v1/auth/logout-all`
- `GET /api/v1/auth/sessions`
- `DELETE /api/v1/auth/sessions`
- `DELETE /api/v1/auth/sessions/{session_id}`
- `POST /api/v1/auth/verify-email`
- `POST /api/v1/auth/resend-verification`
- `POST /api/v1/auth/password/forgot`
- `POST /api/v1/auth/password/reset`
- `POST /api/v1/auth/password/change`
- `POST /api/v1/auth/reauth`
- `POST /api/v1/auth/device/code`
- `POST /api/v1/auth/device/approve`
- `POST /api/v1/auth/device/deny`
- `POST /api/v1/auth/device/token`
- `GET /api/v1/users/me`
- `PATCH /api/v1/users/me`
- `DELETE /api/v1/users/me`
- `GET /api/v1/users/me/export`
- `GET /api/v1/users/me/settings`
- `PATCH /api/v1/users/me/settings`
- `GET /api/v1/users/{user_id}`

### Security behavior implemented

- Argon2id hashing with a dummy-hash path for unknown users.
- 15-minute HS256 access tokens with issuer, audience, session ID, auth version, auth time, and JTI.
- 256-bit refresh tokens, SHA-256 hashed at rest, rotated on every use.
- Refresh reuse revokes the session family.
- Browser refresh cookie with `HttpOnly`, `SameSite=Lax`, path scoping, `Secure` in production, and development-only `tulpar_refresh` naming.
- Desktop device authorization with one-time device/user codes.
- Origin validation on cookie and auth endpoints.
- Login/registration/password-reset/device-code rate limits backed by Redis with memory fallback.
- Password change bumps `auth_version`, invalidates old access tokens, revokes other sessions, and keeps the current session.
- Account deletion requires the current password plus a typed `DELETE` confirmation, anonymizes the account, reserves the username for 90 days, removes instance-admin access, and revokes all sessions/tokens. Community/DM anonymization is extended in later phases.
- Data export returns the current profile, settings, and session metadata without secrets.
- No password, refresh token, or JWT secret is logged by the implemented paths.
- Security audit records for registration, login success/failure, refresh reuse, logout, session revocation, email verification, password changes/resets, re-authentication, and device approval/consumption.

---

## 3. Verification performed

### Commands

```text
cargo fmt --all -- --check          -> pass
cargo clippy --workspace --all-targets -- -D warnings -> pass
cargo test --workspace              -> pass
```

### Test totals

| Suite | Result |
|---|---|
| API auth integration (`crates/api/tests/auth_flow.rs`) | 4 passed |
| Auth unit (password hashing) | 3 passed |
| Common unit (normalization, tokens) | 4 passed |
| Config unit (validation) | 4 passed |
| Realtime unit (memory rate limiter) | 2 passed |
| Other workspace crates | 0 tests (skeleton crates) |

**Total: 17 tests passed.** Tests use a real PostgreSQL database created by `sqlx::test` and apply the initial migration.

### Manual HTTP verification

| Flow | Result |
|---|---|
| `GET /health/live` | `200 {"status":"ok"}` |
| `GET /health/ready` | `200`, database up, Redis up |
| Register | `202`, generic accepted response |
| Login | `200`, access token + refresh cookie |
| `GET /users/me` with Bearer | `200` |
| Refresh with cookie | `200`, rotated access token |
| Refresh reuse with the same old cookie | `401` |
| Logout | `204` |
| Password change | `200`; old access token `401`, new access token `200` |
| User settings update | `200` |
| Account deletion | `204`; old access token `401`; row anonymized |
| User data export | `200` with profile, settings, session metadata |
| Admin CLI create (`--password-stdin`) | instance administrator created and verified in `instance_admins` |
| Invalid Origin on login | `403` |
| Six failed logins | five `401`, sixth `429` |
| Desktop device code → approve → poll | `200` → `204` → `200` with tokens |
| SMTP verification email via Mailpit | delivered, token extracted, `POST /verify-email` → `204` |
| SMTP password-reset email via Mailpit | delivered and captured |
| Swagger UI `/docs` | `200` |
| OpenAPI JSON | `200`, all implemented paths present |

### Reproduce locally

The local services used during verification are not part of the repository:

```text
PostgreSQL: 127.0.0.1:55432, user tulpar, database tulpar_dev
Redis:      127.0.0.1:6380
API:        127.0.0.1:8082 (8080/8081 were occupied on this machine)
```

Local `.env` and `tulpar.toml` contain the development JWT secret and local connection URLs. Both are ignored by Git. `tulpar.toml.example` and `.env.example` contain no real secrets.

```powershell
$env:DATABASE_URL = "postgres://tulpar@127.0.0.1:55432/tulpar_dev"
cargo run -p tulpar-server -- migrate
cargo run -p tulpar-server -- serve
```

The server currently applies migrations on startup as well as via the `migrate` command. Production should use only the one-shot migration job; this is a Phase 9 deployment change.

---

## 4. Known gaps and limitations

1. **Docker Compose is not created or verified.** Docker is not installed and WSL has no distribution. Local verification used native processes. The Phase 1 exit criterion for Compose is open.
2. **CI has not run on GitHub yet.** `.github/workflows/ci.yml` is written but cannot be validated locally; the first push must prove the workflow and service configuration.
3. **Generated TypeScript types do not exist yet.** The OpenAPI document is served, but the web client and `packages/shared-types` are Phase 6 work.
4. **OpenAPI Bearer scheme is registered, but per-operation security requirements are not annotated yet.** Swagger shows the authorize control; protected paths should be marked with the scheme in a follow-up.
5. **SQLx queries are runtime-checked.** Compile-time checked macros and `.sqlx` offline data are deferred to avoid requiring a database during every build.
6. **OneDrive build warnings.** Rust incremental compilation sometimes fails to finalize a session directory with access denied. Builds still pass; moving the repository or setting `CARGO_TARGET_DIR` outside OneDrive is recommended.
7. **Local Node is 26, target is Node 24 LTS.** No frontend package exists yet, so this has no runtime impact.
8. **Only the Phase 1 schema subset exists.** Permissions, communities, channels, messaging, voice, embeddings, and social tables are still planned.
9. **No MFA, bots, API keys, or webhooks yet.** Those are later phases.

---

## 5. Recommended next steps

1. Push the repository and prove the CI workflow on GitHub; add `cargo deny` and migration checks.
2. Add the OpenAPI Bearer security scheme and generated TypeScript types when the web package is created.
3. Install Docker/WSL or move to a Linux development host, then create and verify the development Compose stack.
4. Move the repository off OneDrive or set `CARGO_TARGET_DIR` outside the synced directory.
5. Begin Phase 2 only after the Phase 1 gaps above are closed or explicitly accepted.

---

## 6. Commit preparation

The Phase 1 checkpoint includes the Phase 0 documentation, Phase 1 code, migrations, tests, and examples. `.env`, `tulpar.toml`, `target/`, and local database/Redis files are ignored. Runtime secrets are not part of the repository. `Cargo.lock`, migrations, source, tests, examples, CI configuration, and documentation are included. Production-facing work remains gated on the open items above.
