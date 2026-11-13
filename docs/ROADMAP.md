# Tulpar Roadmap

**Current phase:** Phase 5/6 — Web client, video, and screen sharing (in progress).

**Status:** Phase 0 is approved. Phase 1 is closed apart from the license decision. Phases 2–4 are complete. The React/Vite web client now covers auth, `/sync`, text messaging, gateway live updates, and voice/video/screen controls, verified by an Edge browser smoke test. Remaining work is client polish, friends/settings/admin UI, PWA, accessibility, device/quality controls, and the browser matrix.

**Rule:** do not begin a phase until the previous phase meets its exit criteria. A phase can be partially delivered, but it is not “complete” because the happy path works.

No delivery dates are promised. Scope and quality take precedence over calendar pressure.

---

## Phase 0 — Architecture and project foundation

### Deliverables

- [x] Inspect the empty workspace and confirm the environment.
- [x] Define product identity and scope.
- [x] Define the instance model and multi-instance client behavior.
- [x] Define the modular monolith, crate boundaries, and table ownership.
- [x] Define the PostgreSQL schema, constraints, indexes, retention, and migration strategy.
- [x] Define the REST API conventions and endpoint catalog.
- [x] Define the WebSocket protocol, events, and recovery.
- [x] Define authentication for web, desktop, bots, and API keys.
- [x] Define roles, permission bits, hierarchy, and channel overwrites.
- [x] Define LiveKit/Coturn voice architecture.
- [x] Define video and screen-sharing architecture and limitations.
- [x] Define the web and desktop client architecture.
- [x] Define the embed security model and SDK.
- [x] Define the security model, threat model, and known limitations.
- [x] Define deployment, ports, sizing, upgrades, and backups.
- [x] Define the testing strategy.
- [x] Record architecture decisions.
- [x] Create the repository structure.
- [x] Write contribution, security, and roadmap documents.
- [ ] Select a license.
- [x] Review and approve Phase 0 (approved).

### Exit criteria

- All documents are internally consistent.
- No application code, secrets, or unverified deployment manifests are presented as working.
- Risks, platform limitations, and open questions are explicit.
- The user approves the architecture and scope.
- A license decision is made or explicitly deferred before public distribution.

### Notes

- Phase 0 is approved; Phase 1 implementation exists.
- No production-ready release exists.
- Rust and pnpm are installed; Docker is still unavailable. Local PostgreSQL 16 and a portable Redis-compatible server are used for verification.
- The workspace is under OneDrive; see the development environment risk below.

---

## Phase 1 — Core backend

### Tasks

- [x] Install and verify Rust and pnpm (Docker/Compose still unavailable).
- [x] Create the Rust workspace and crate skeletons with dependency rules.
- [x] Implement configuration loading, validation, and secret handling.
- [x] Implement structured logging, request IDs, and tracing.
- [x] Implement PostgreSQL pool, migrations, and the initial identity/auth schema.
- [x] Implement Redis connection with explicit degraded behavior.
- [x] Build the Axum server with health endpoints.
- [x] Generate OpenAPI and serve the contract.
- [x] Implement registration, email verification, login, refresh rotation, logout, sessions, password reset/change, re-authentication, and device authorization.
- [x] Implement user profile and settings.
- [x] Bootstrap the first instance admin via CLI (verified with `--password-stdin`).
- [ ] Create and verify the development Docker Compose stack (blocked by missing Docker).
- [x] Write integration tests for auth, sessions, email verification, password reset, and refresh reuse.
- [x] Document configuration in `tulpar.toml.example` and `.env.example`.
- [x] Implement SMTP email delivery and test it against a local SMTP capture (Mailpit).
- [x] Implement account deletion and data export.
- [x] Add CI workflow with format, clippy, tests, secret scan, and audit jobs.
- [ ] Prove CI on GitHub and add a `cargo deny` policy.
- [x] Register the OpenAPI bearer security scheme (per-operation security annotations pending).

### Exit criteria

- [x] A clean checkout builds; unit and integration tests pass locally.
- [x] Migrations apply from an empty database.
- [x] Registration → verification → login → refresh → logout works end to end.
- [x] Refresh reuse revokes the session family.
- [x] No secret appears in logs or responses in the exercised paths.
- [x] Health/readiness correctly reflect PostgreSQL and Redis.
- [x] OpenAPI is generated and served; Swagger UI loads.
- [ ] Generated TypeScript types compile (web client not started).
- [ ] Development Compose starts the stack from documented commands (Docker unavailable).
- [x] SMTP delivery verified with Mailpit.
- [x] Account deletion/export verified by integration test.

---

## Phase 2 — Communities and social features

**Status:** Complete. The backend community, permission, moderation, discovery, and social scope is implemented, tested, and CI-green. The admin UI is deferred to the web phase.

### Tasks

- [x] Communities, members, invites, bans, timeouts.
- [x] Roles, role assignments, hierarchy, channel-overwrite endpoints, and role reordering.
- [x] Permission evaluator and effective community/channel permission endpoints.
- [x] Friends, requests, blocks, privacy settings.
- [x] Direct conversations (channel + participant privacy; messages are Phase 3).
- [x] Basic user profiles and user search.
- [x] Public community discovery/search, member search, and ban listing.
- [x] Community settings updates, discovery listing, and audit-log API.
- [x] Audit records for community, moderation, role, invite, channel-overwrite, friend, block, and DM metadata actions.
- [x] Authorization matrix and adversarial escalation tests for the community/role/invite/ban/channel/social flows.
- [ ] Community/role/channel admin UI foundations (web scaffolding does not exist yet; Phase 6).

### Exit criteria

- [x] Owner/admin bypass, hierarchy, category/channel overwrite ordering, deny-wins, and timeout rules have passing tests.
- [x] Privilege escalation attempts are rejected and covered by integration tests.
- [x] Invite limits, bans, timeouts, and ownership transfer have full integration coverage.
- [x] DMs are participant-only and block-aware in the implemented conversation flow.
- [x] Private-community and cross-community IDOR probes return the documented status.
- [x] Audit records are written for moderation/hierarchy actions.
- [x] Migration constraint tests cover the Phase 2 schema.

---

## Phase 3 — Real-time messaging

**Status:** Complete for the v1 backend scope. Multi-node fan-out, S3 storage, and gateway load testing are follow-ups.

### Tasks

- [x] WebSocket gateway, tickets, heartbeat, sequencing, filtering, backpressure.
- [x] Text/announcement/forum channel foundations (community channel metadata from Phase 2).
- [x] Message persistence, edit, delete, replies, reactions, pins.
- [x] Mentions and mention authorization.
- [x] Read states and unread counts.
- [x] Typing and presence.
- [x] Notifications and preferences.
- [x] File attachments and the storage abstraction.
- [x] Message search (channel-scoped full-text).
- [x] Reconnect/resync and message catch-up.
- [x] Outbox dispatcher and event contracts.
- [x] Gateway backpressure tests (slow-consumer lag unit test).
- [ ] Gateway multi-client load test.
- [x] Web UI for messaging if Phase 6 has begun; otherwise API-level verification (API-level verified).

### Exit criteria

- [x] Messages survive API/gateway restarts (PostgreSQL-backed; Docker volume verified).
- [x] A client that misses events recovers via `/sync` and `after` queries without duplicates or lost messages.
- [x] Permission changes take effect on the socket.
- [x] Slow consumers observe a lag error and are disconnected without blocking others.
- [x] Redis loss degrades tickets/presence coordination but not message send.
- [x] Message search is permission-scoped.
- [x] File upload validation and quota tests pass.
- [x] Outbox dispatch, retry scheduling, and unique event IDs are tested.

---

## Phase 4 - Voice infrastructure

**Status:** Backend and transport verification complete. Two real Edge clients hear each other directly and through Coturn relay; client UI/device foundations move to Phase 6.

### Tasks

- [x] Deploy LiveKit and Coturn in development and staging.
- [x] LiveKit token issuance with permission-based sources.
- [x] Room password, capacity, bans, timeouts.
- [x] TURN ephemeral credentials.
- [x] LiveKit webhooks, Redis-backed projection, and reconciliation worker.
- [x] Server mute/deafen/move/disconnect.
- [x] Revocation paths for kick, ban, timeout, user/role channel overwrite, and role changes.
- [ ] Device selection, mute/deafen, PTT, VAD, per-user volume (client foundation, Phase 6).
- [x] Real audio tests on supported browsers (Edge two-client harness with fake devices).
- [x] TURN relay-only tests (`iceTransportPolicy=relay` through Coturn).

### Exit criteria

- [x] Two or more real clients can hear each other through LiveKit.
- [x] TURN fallback works through a forced relay path (UDP-blocked network/TURN-TLS remains a follow-up).
- [x] Token source grants match permissions.
- [x] Kick/ban/permission loss removes or restricts participants.
- [x] Webhook signature/replay/out-of-order tests pass.
- [x] Redis loss does not drop active media (media plane is LiveKit; Redis is not on the media path).
- [x] No LiveKit/TURN secret is exposed.

---

## Phase 5 - Video and screen sharing

**Status:** In progress. Camera and screen-share publishing controls exist in the web client and LiveKit grants are permission-scoped. Device selection, quality controls, layouts, capability matrix, and cross-browser media tests remain open.

### Tasks

- [x] Camera publishing and mute controls (web foundation).
- [ ] Device selection.
- [x] Screen/window sharing and optional audio (browser picker).
- [ ] Resolution/fps/bitrate controls.
- [ ] Simulcast/adaptation and network indicators (LiveKit defaults only).
- [x] Video tiles, grid/focus layouts, and fullscreen; pin layout pending.
- [ ] Capability detection and fallback UI.
- [ ] Test matrix across browsers and desktop WebViews (Edge verified).
- [ ] Document platform limitations with measured results.

### Exit criteria

- [x] Camera and screen share work on Edge.
- [x] Token source grants are permission-scoped and verified in the voice integration test.
- [x] Screen-share stop/leave stops the stream (browser leave flow tested).
- [ ] Capability detection never shows a known-broken control.
- [ ] Real media tests pass on the documented matrix (Edge only so far).
- [ ] Desktop limitations are recorded, not hidden.

---

## Phase 6 - Tulpar Web

**Status:** In progress. The authenticated SPA foundation, text/DM navigation, real-time gateway updates, and voice/video/screen controls are implemented and browser-tested. Friends/settings/admin UI, PWA, responsive polish, and accessibility passes remain open.

### Tasks

- [x] Authenticated SPA shell and core routes (login/register/app/channel/DM).
- [x] Communities, channels, messaging, DMs, friends, and personal settings UI.
- [x] Voice/video UI integration (join/leave, mic, camera, screen share).
- [x] In-app notification center and preferences.
- [x] Browser notifications with permission control; click routing/desktop toasts pending.
- [x] Instance admin UI (overview/users/audit); community settings UI pending.
- [ ] Responsive layouts for desktop/tablet/mobile.
- [x] PWA manifest and shell-only service worker (offline install verification pending).
- [x] `en`/`tr` localization foundation.
- [ ] Accessibility pass.
- [x] Performance budgets and code splitting (LiveKit lazy chunk).
- [x] Playwright core flows (login, message, voice join).

### Exit criteria

- A user can complete the core journey in a browser: register, join, chat, voice, DM, settings.
- PWA installs and serves an offline shell without caching private data.
- Keyboard-only navigation and screen-reader checks pass for main flows.
- No hardcoded user-facing strings.
- Lighthouse/route budgets meet the documented targets or deviations are documented.
- Browser matrix results are recorded.

---

## Phase 7 — Tulpar Desktop

### Tasks

- Tauri 2 shell and shared SPA integration.
- Device authorization, keychain, single-flight refresh.
- System tray, native notifications, window state, single instance.
- Global PTT with capability detection and fallback.
- Deep links.
- Autostart.
- Auto-update with signed manifests.
- Diagnostics and opt-in crash reporting.
- Windows and Linux builds.
- Code signing and release pipeline.
- Manual platform matrix.

### Exit criteria

- Windows 10/11 and Ubuntu 24.04 installs work.
- Refresh tokens are only in the OS keychain.
- Notifications, tray, deep links, PTT, and updates work or degrade with clear messaging.
- An unsigned update is rejected.
- No secret appears in webview storage or logs.
- Platform limitations are documented from real tests.
- Upgrade from the previous release preserves preferences and sessions.

---

## Phase 8 — Embed and developer platform

### Tasks

- Embed apps, origins, scoped tokens, iframe widgets.
- Public and authenticated widget flows.
- postMessage protocol and SDK.
- Embed demo site.
- REST API stabilization and versioning.
- WebSocket documentation and event schemas.
- Webhooks with signatures/retries/SSRF protection.
- API keys, bot accounts, bot tokens, scopes.
- TypeScript SDK and Rust SDK.
- Developer documentation and examples.
- The API security matrix and embed tests.

### Exit criteria

- A third-party demo site mounts public and authenticated widgets.
- Origin, audience, postMessage, and clickjacking tests pass.
- Webhook signatures/retries/SSRF tests pass.
- API keys and bot tokens are hashed, scoped, revocable, and rate limited.
- SDKs compile and pass contract tests.
- API/WS docs match the implementation.

---

## Phase 9 — Production hardening

### Tasks

- Production Docker Compose and installation scripts for Ubuntu 24.04.
- TLS/Caddy hardening, security headers, firewall, secrets.
- Backup/restore scripts and a real restore drill.
- Upgrade rehearsal from the previous release.
- Monitoring dashboards/alerts and log aggregation.
- Load tests and performance fixes.
- Security review and dependency/container scans.
- External security review if feasible.
- Documentation: installation, upgrade, troubleshooting, runbooks.
- Release workflow, checksums, image signing, desktop release signing.
- Disaster recovery and incident response runbooks.

### Exit criteria

- A clean Ubuntu 24.04 host can be installed from documented scripts.
- Backup/restore is verified on a clean host.
- Upgrade and rollback are rehearsed.
- Load/security targets are measured and documented.
- No known critical/high security issue remains unresolved.
- The release is reproducible/checksummed and the desktop build is signed.
- The operator can diagnose common failures from docs and metrics.

---

## Cross-phase work

These continue throughout:

- Security review of every change.
- `en`/`tr` localization.
- Documentation updates.
- Dependency updates and audits.
- Test coverage for new behavior.
- Accessibility.
- Performance measurement.
- Migration hygiene.
- No use of the previous project name.

---

## Open questions

| Question | Why it matters | Suggested owner/decision |
|---|---|---|
| License | Legal distribution and contribution model | Project owner; AGPL-3.0 vs Apache-2.0 recommended |
| Public domain and branding | TLS, emails, docs, embed examples | Project owner |
| SMTP provider | Verification/reset deliverability | Operator/project owner |
| Object storage default | Local vs MinIO vs S3 | Phase 1/9 decision |
| LiveKit hosting model | Co-located vs separate media host | Phase 4 measurement |
| Code signing certificate | Windows trust and updates | Phase 7 |
| Update hosting | Where desktop updates live | Phase 7 |
| Crash reporting | Privacy vs diagnostics | Phase 7 |
| MFA | Account security for admins | Post-v1 or Phase 2 |
| E2EE | Product privacy model | Future design |
| Federation | Product scope | Post-v1 |
| Native mobile app | Mobile experience | Post-v1 |
| Legal retention/erasure | Compliance | Before public release |
| External security audit | Assurance | Before v1 |
| SDK packaging | TypeScript SDK wraps `api-client`; Rust SDK location (crate vs separate repo) | Phase 8 |
| Support model | Community vs commercial | Post-v1 |

---

## Post-v1 candidates

- TOTP/WebAuthn MFA.
- OIDC/SSO for organizations.
- Group DMs.
- Forum/thread polish and stage channels.
- DM voice/video calls.
- Recording/transcription with consent.
- E2EE design for private conversations.
- Optional federation.
- Native mobile apps.
- Custom emojis/stickers.
- Plugin/bot marketplace.
- Multi-region voice.
- Read replicas and horizontal scaling guides.
- Managed hosting or commercial support.

These are candidates, not commitments. Each requires an ADR and a security review.

---

## Development environment risks

1. The workspace is under OneDrive; Docker cannot read the build context directly. Use a local path for Docker or CI.
2. Rust, pnpm, Node, and Docker Desktop are installed; PostgreSQL and Redis run in the Docker development stack.
3. Windows builds for Linux/Tauri require CI or WSL; desktop WebView differences need real testing.
4. Voice/video work requires a network environment where UDP and TURN can be tested.
5. Code signing and update infrastructure require credentials and a release process.

Phase 1 began with toolchain verification. Rust, pnpm, Node, and Docker Desktop are installed; the development stack was built and verified from a local non-OneDrive build context.

---

## Phase 1 completion gate

Phase 0 is approved. Phase 1 is implemented and its remaining technical checks are closed or explicitly tracked:

- [x] SMTP delivery is implemented and tested against a local SMTP capture.
- [x] Account deletion and data export follow the documented policy.
- [x] CI workflow covers format, clippy, tests, secret scanning, dependency audit, and TypeScript contracts.
- [x] The CI workflow has passed on GitHub (commits `ab0b199` and `c24433a`).
- [x] Generated TypeScript types are produced from the OpenAPI document and typecheck.
- [x] The development Compose stack is created and verified from a local build context.
- [x] The OpenAPI Bearer security scheme is registered.
- [x] Protected OpenAPI operations are annotated with the security requirement.
- [x] The admin CLI create flow is exercised end to end.
- [ ] The license decision is recorded before any public distribution.

The implementation and verification evidence is in [PHASE_1_REPORT.md](PHASE_1_REPORT.md) and [PHASE_2_REPORT.md](PHASE_2_REPORT.md).
