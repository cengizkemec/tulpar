# Phase 0 Report

**Status:** Complete pending review and approval.

**Scope:** Architecture and project foundation only. No application code, database migration, Compose stack, or installable release exists.

---

## 1. Workspace inspection

| Item | Result |
|---|---|
| Workspace | `C:\Users\cengi\OneDrive\Belgeler\OpenCode\tulpar` |
| Initial contents | Empty |
| Git repository | Not initialized at inspection time |
| Node.js | v26.13.0 |
| pnpm | Not found |
| Rust / Cargo | Not found |
| Docker | Not found |
| PostgreSQL client | Not found |
| Git | 2.56.0.windows.1 |

The machine has Node, but not the Rust, Docker, pnpm, or PostgreSQL tooling required for Phase 1. The workspace is under OneDrive, which is a build-performance and file-lock risk.

---

## 2. Files created

### Root

- `README.md` — product overview, status, architecture, stack, documentation index.
- `CHANGELOG.md` — Unreleased/Phase 0 entry.
- `CONTRIBUTING.md` — pointer to the full guide.
- `SECURITY.md` — vulnerability reporting policy placeholder.
- `ROADMAP.md` — pointer to the full roadmap.
- `CODE_OF_CONDUCT.md` — conduct expectations.
- `.gitignore`, `.gitattributes`, `.editorconfig`.

### Documentation

| File | Contents |
|---|---|
| `docs/ARCHITECTURE.md` | System design, instance model, crate boundaries, flows, failure modes, risks, limitations |
| `docs/DATABASE.md` | Full PostgreSQL schema, constraints, indexes, migrations, retention |
| `docs/API.md` | REST conventions, error model, endpoint catalog, webhooks |
| `docs/WEBSOCKET.md` | Gateway protocol, events, presence, recovery, close codes |
| `docs/AUTHENTICATION.md` | Passwords, JWT/session model, rotation, device flow, CSRF, keychain |
| `docs/PERMISSIONS.md` | Permission bits, roles, hierarchy, channel overwrites, escalation tests |
| `docs/VOICE.md` | LiveKit, Coturn, token rules, moderation, failure handling |
| `docs/VIDEO.md` | Camera, screen share, adaptation, platform limitations |
| `docs/DESKTOP.md` | Tauri 2 architecture, secure storage, native features, releases |
| `docs/WEB.md` | React SPA, PWA, state, i18n, accessibility, security, testing |
| `docs/EMBED_SDK.md` | Embed model, origin/token security, postMessage, SDK, demo |
| `docs/SECURITY.md` | Threat model, controls, secrets, supply chain, known limitations |
| `docs/DEPLOYMENT.md` | Topology, ports, services, sizing, installation/upgrade plan |
| `docs/BACKUP.md` | Backup/restore policy, retention, disaster recovery |
| `docs/TESTING.md` | Test pyramid, CI gates, security/load/voice/desktop testing |
| `docs/DECISIONS.md` | ADR-0001 through ADR-0022 |
| `docs/ROADMAP.md` | Phases 0–9 with exit criteria and open questions |
| `docs/CONTRIBUTING.md` | Workflow, standards, security rules, PR checklist |
| `docs/PHASE_0_REPORT.md` | This review packet |

### Repository structure

Created directories with `.gitkeep` placeholders:

- `apps/web`, `apps/desktop`, `apps/embed-demo`
- `crates/` for core, common, config, database, events, permissions, users, auth, social, communities, channels, messaging, notifications, voice, storage, audit, integrations, realtime, api, gateway, server
- `packages/` for shared-types, api-client, ui, i18n, livekit-client, embed-sdk
- `infrastructure/` for docker, postgres, redis, livekit, coturn, caddy, monitoring
- `deployments/` for docker-compose, production, development
- `tests/` for integration, e2e, load, security
- `scripts/`

No `Cargo.toml`, `pnpm-workspace.yaml`, Compose file, or migration was created. Those are Phase 1 deliverables and must be verified with the actual toolchain, not generated as unverified placeholders.

---

## 3. Architecture decisions

| Area | Decision |
|---|---|
| Backend shape | Rust modular monolith; one API/gateway/worker binary; 21 narrow crates |
| Durable data | PostgreSQL 16+ is the source of truth |
| Ephemeral data | Redis 7+ for presence, tickets, rate limits, typing, pub/sub; not messages |
| IDs | Application-generated UUIDv7 |
| Realtime | REST commands + WebSocket push; no durable event replay; `/sync` + message cursors for recovery |
| Events | Transactional outbox + Redis pub/sub; idempotent consumers |
| Auth | Argon2id; 15-minute access JWT; rotating refresh token; browser HttpOnly cookie; desktop device flow + OS keychain |
| Permissions | 64-bit bitfield (bits 0–62), API string arrays, centralized evaluator, role hierarchy, channel overwrites |
| Voice/video | LiveKit SFU + Coturn TURN; short-lived LiveKit tokens; backend authorization; no custom media |
| Storage | Local/S3/MinIO behind an `ObjectStore` abstraction |
| Desktop | Tauri 2 wrapping the shared React SPA; Rust owns keychain/auth/native features |
| Web | React 19 + Vite + Zustand + TanStack Query + Tailwind + Radix; PWA |
| Embed | Hosted iframe + exact origin allowlist + scoped short-lived tokens; popup member auth |
| Admin | `/admin` lazy-loaded section; instance admin separate from community roles; no private content access by default |
| Deployment | Docker Compose on Ubuntu 24.04; Caddy TLS; LiveKit/Coturn independent; optional monitoring |
| Federation/E2EE | Not in v1; documented and designed not to block additive future work |

Full rationale is in [DECISIONS.md](DECISIONS.md).

---

## 4. Database summary

- ~40 tables covering identity, sessions, device auth, communities, members, roles, invites, bans, channels, overwrites, DMs, messages, reactions, mentions, attachments, files, read states, notifications, webhooks, API keys, bots, embeds, audit, outbox, and idempotency.
- UUIDv7 public IDs; `timestamptz`; `citext` usernames/emails; `pg_trgm` search.
- Soft delete for communities/channels/messages/accounts; hard delete for sessions/tokens/reactions/read states.
- Transactional outbox and idempotency keys.
- Permission bitfields with constraints; composite FKs prevent cross-community role assignment.
- Message search uses `simple` + trigram; permission-scoped.
- Expand/contract migrations after the first public release.
- Default retention documented; account deletion anonymizes/removes private data.

Details: [DATABASE.md](DATABASE.md).

---

## 5. Security summary

- Backend-only authorization; private resources return `404` where existence must not leak.
- Argon2id; refresh rotation with reuse detection; HttpOnly browser cookie; desktop device flow and keychain.
- Short-lived access tokens; revocation window documented (≤15 minutes for REST; immediate for gateway/voice).
- LiveKit/TURN/S3/SMTP/JWT secrets never reach clients.
- Upload MIME sniffing, size/quota limits, safe filenames, separate user-content origin, no SVG/HTML inline.
- Webhook HMAC signing, encrypted secrets, SSRF controls, replay protection.
- Embed origin allowlist, audience-scoped tokens, postMessage validation, CSP `frame-ancestors`.
- Rate limits, Argon2 concurrency bound, gateway backpressure, body/frame limits.
- Audit logs without message bodies or secrets; no mandatory telemetry.
- Known limitations: no E2EE, no MFA in v1, Redis degraded behavior, no recording, no native mobile app, no formal compliance certification.

Details: [SECURITY.md](SECURITY.md).

---

## 6. Deployment plan

- Ubuntu 24.04 + Docker Compose.
- Caddy for TLS, static web, `/api`, `/gateway`, `/embed`, and LiveKit signaling.
- PostgreSQL and Redis internal only.
- LiveKit media UDP 50000–50999; Coturn 3478/5349 and relay 51000–51999; 7881 TCP fallback.
- Local storage initially; MinIO/S3 for scale.
- Optional Prometheus/Grafana/Loki.
- Nightly PostgreSQL dump + off-host encrypted backups; optional WAL/PITR; quarterly restore drills.
- Upgrade via one-shot migrations, backup, health checks, and a documented rollback.
- No Compose or installer is claimed to work until Phase 9 tests it on a clean host.

Details: [DEPLOYMENT.md](DEPLOYMENT.md), [BACKUP.md](BACKUP.md).

---

## 7. Identified risks

| Risk | Severity | Mitigation |
|---|---|---|
| Scope is very large | High | Strict phases, exit criteria, no phase skipping |
| Permission complexity | High | Central evaluator, property tests, authorization matrix |
| LiveKit/Coturn networking | High | Early Phase 4 spike, real TURN tests, diagnostics |
| Tauri WebView differences | Medium | Capability detection, browser fallback, CI/manual matrix |
| Screen capture on Wayland/Linux | Medium | Documented limitation; no fake support |
| No E2EE | Medium | Explicit disclosure; future design decision |
| No MFA in v1 | Medium | Strong passwords, admin network restrictions, MFA later |
| OneDrive workspace | Medium | Move repo or exclude build outputs |
| Missing toolchain | Medium | Install and verify in Phase 1 |
| Email deliverability | Medium | SPF/DKIM/DMARC, configurable SMTP, resend |
| Supply chain | Medium | Lockfiles, cargo-deny, pnpm audit, pinned images, signed releases |
| Single-VPS contention | Medium | Sizing guidance, separate media host, load tests |
| No external security audit yet | Medium | Schedule before v1 |
| Legal/license decisions | Medium | Resolve before public distribution |

---

## 8. Unresolved questions

1. **License:** AGPL-3.0 or Apache-2.0 (or another). Required before public distribution.
2. **Public domain/branding:** instance domain, email domain, embed demo domain.
3. **SMTP provider / deliverability ownership.**
4. **Object storage default:** local vs MinIO vs external S3.
5. **LiveKit/Coturn hosting:** co-located vs separate media host.
6. **Windows code-signing certificate** and desktop update hosting.
7. **Crash reporting backend:** opt-in, self-hosted, or none.
8. **MFA priority:** post-v1 or earlier.
9. **E2EE product direction.**
10. **Federation appetite.**
11. **Legal retention/erasure policy.**
12. **External security review timing/budget.**
13. **Admin UI split:** integrated `/admin` now; separate bundle later?
14. **Native mobile roadmap.**

---

## 9. Validation performed

- Workspace inspected before creation; it was empty.
- Repository structure created without overwriting user files.
- No secrets, credentials, or private keys included.
- No use of the previous project name.
- Local Markdown links and anchors checked; no broken local links.
- Cross-document checks for cookie names, permission names, port ranges, and endpoint paths.
- No build manifests or Compose files were created because the toolchain is not installed; Phase 1 starts with verification.
- No feature is claimed to work.

---

## 10. Recommended next steps

1. Review and approve the architecture, scope, and open decisions.
2. Decide the license direction.
3. Move the repository to a local, non-OneDrive path (or exclude `target/`, `node_modules/`, `dist/`).
4. Install Rust stable, Node 24 LTS, pnpm 10, Docker/Compose, and PostgreSQL client tools.
5. Begin Phase 1 with:
   - Rust workspace and crate skeletons;
   - configuration loading;
   - PostgreSQL pool and first migrations;
   - Redis connection and degraded behavior;
   - Axum health endpoints and logging;
   - registration/login/session flows;
   - integration tests and a development Compose stack.

Do not start Phase 1 until Phase 0 is approved.
