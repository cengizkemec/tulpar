# Tulpar Testing Strategy

**Status:** Phase 4 voice backend implemented. Unit tests cover config, common helpers, password hashing, rate limiting, permissions, gateway, storage, LiveKit tokens, TURN credentials, and voice projection. Integration tests cover auth, communities, moderation, messaging, WebSocket gateway, attachments, notifications, outbox, `/sync`, and voice tokens/passwords/webhooks/moderation. GitHub Actions runs Rust checks/tests and TypeScript contract checks. `tests/voice-e2e` is a manual Edge-based two-client audio suite that also verifies forced TURN relay; multi-node load, frontend, and desktop suites remain open.

**Rule:** a feature is not complete until it is tested at the level where it can fail. Mocked voice, mocked permissions, and happy-path-only API tests do not count as completion.

---

## 1. Test pyramid

```
                 ┌──────────────────────┐
                 │ E2E / manual matrix  │  Playwright, desktop, real media
                 ├──────────────────────┤
                 │ Integration          │  API + PostgreSQL + Redis + LiveKit
                 ├──────────────────────┤
                 │ Unit / property      │  pure logic, permissions, tokens
                 └──────────────────────┘
```

- **Unit tests** are fast and exhaustive for pure logic.
- **Integration tests** use real PostgreSQL and Redis, not mocks.
- **Contract tests** keep OpenAPI and WebSocket types in sync.
- **End-to-end tests** exercise real browser flows.
- **Load/security tests** validate bounds and adversarial behavior.
- **Manual matrix** covers OS/browser/device combinations that CI cannot represent reliably.

No phase is complete based only on unit tests.

---

## 2. Tooling

| Layer | Tool |
|---|---|
| Rust unit/integration | `cargo test`, `#[sqlx::test]`, `testcontainers` or CI service containers |
| Rust property tests | `proptest` |
| Rust HTTP/API | `axum-test` or `reqwest` against a test server |
| Rust WebSocket | `tokio-tungstenite` test client |
| Rust database | SQLx migrations against PostgreSQL 16 |
| Rust lint/security | `cargo fmt --check`, `clippy -D warnings`, `cargo deny`, `cargo audit` |
| Frontend unit | Vitest + React Testing Library |
| Frontend API mocking | Mock Service Worker |
| Frontend E2E | Playwright |
| Accessibility | axe-core, keyboard/screen-reader manual checks |
| Performance (web) | Lighthouse CI, Playwright traces |
| Load | k6 (HTTP/WS), custom Rust or TypeScript WS clients, LiveKit test bots |
| Desktop | Tauri build/test on Windows and Ubuntu CI; manual smoke matrix |
| Containers | Docker Compose integration environment |

Dependencies are added only when they serve a concrete test need.

---

## 3. Environments

### 3.1 Local

- PostgreSQL 16 and Redis 7 via Docker Compose.
- LiveKit and Coturn for voice tests.
- A test instance with seeded users/communities.
- `TULPAR_ENV=test` with deterministic features and no external email.
- Email uses a local capture (Mailpit/MailHog) or a no-op sender; tests must not send real email.

### 3.2 CI

- PostgreSQL and Redis service containers.
- Migrations applied from scratch on every run.
- A second job applies migrations against the previous release schema.
- LiveKit/Coturn job for voice integration (may be nightly due to resource cost).
- Frontend build and Playwright against the built app.
- Windows and Linux desktop build jobs.
- No production secrets. A throwaway test signing key is generated per job.

### 3.3 Staging

- Production-like Compose stack with real TLS and real TURN.
- Synthetic users and communities.
- Used for upgrade rehearsals, restore drills, and load tests.
- Never populated with real user data.

---

## 4. Backend tests

### 4.1 Unit

- Configuration parsing and validation.
- ID generation/parsing.
- Error mapping and problem+json shape.
- Password hashing/verification (with reduced-cost test parameters where appropriate, never in production).
- Token parsing, claims, expiry, audience/issuer, algorithm confusion.
- Permission bit parsing and serialization.
- Permission evaluation (extensive property tests).
- Pagination cursor encode/decode and tamper rejection.
- Input validation and normalization.
- File type/extension/MIME policy.
- Webhook signature generation/verification.
- Embed token claims and origin rules.
- Retention/job scheduling logic.

### 4.2 Integration

Use real PostgreSQL and Redis. Each test gets an isolated schema/database and a unique Redis namespace.

- Migrations from empty and from the previous release.
- Repositories: constraints, cascades, partial indexes, transaction rollback.
- Auth: register, verify, login, refresh rotation, reuse, logout, reset, device flow.
- Users: profile, privacy, blocks, search.
- Communities: create, join, invite limits, kick, ban, timeout, transfer, delete.
- Channels: create/reorder/delete, overwrite evaluation, category inheritance.
- Messaging: send/edit/delete, mentions, replies, reactions, pins, read states, pagination, idempotency.
- Files: upload intent/finalize/quota/MIME rejection/orphan cleanup.
- Notifications: preferences, mute, mention-only, dedupe.
- Webhooks: signature, retry, SSRF rejection, disable threshold.
- Admin: authorization, last-admin guard, audit.
- Outbox: atomic commit, retry, idempotent processing.
- WebSocket: ticket, identify, heartbeat, dispatch filters, revocation, slow consumer.
- Voice: token claims, permission combos, password, capacity, webhook projection.

### 4.3 Authorization matrix

Every endpoint that accepts a resource ID is tested with:

- Anonymous.
- Authenticated non-member.
- Member without the required permission.
- Member with the permission.
- Member of another community.
- Banned member.
- Timed-out member.
- Owner.
- Administrator.
- Instance admin who is not a member.
- Bot.

Expected status and body shape are asserted. `403` vs `404` behavior is intentional and tested.

### 4.4 Contract tests

- OpenAPI validates against 3.1.
- Every route has auth, error, and pagination documentation.
- Generated TypeScript types compile.
- Rust response DTOs serialize to the documented shape.
- WebSocket event names/payloads match `packages/shared-types`.
- Permission names match `PERMISSIONS.md` and the bit constants.
- No secret field appears in a public DTO.

### 4.5 Database tests

- Constraint tests: duplicate usernames, cross-community roles, overlapping overwrites, invalid scopes.
- Cascade tests: community purge removes channels/messages; user deletion sets message author null.
- Index usage for the critical queries (`EXPLAIN` assertions or plan review).
- Cursor pagination correctness under concurrent inserts.
- Transaction isolation for refresh rotation, invite usage, ban/kick, last-admin guard.
- Migration up/backward compatibility; no destructive changes without expand/contract.

---

## 5. Frontend tests

### 5.1 Unit/component

- Design-system components: buttons, dialogs, inputs, menus, tooltips, focus management.
- Message rendering: plain text, Markdown subset, mentions, links, code blocks, attachment cards.
- URL sanitization and XSS payloads.
- Permission-gated controls (hidden/disabled) while the backend remains authoritative.
- Store transitions: auth, instance switch, voice, notifications.
- Query cache updates for message create/update/delete and access loss.
- Localization: no missing keys in `en`/`tr`; pluralization and date formatting.
- Form validation and error mapping.
- PWA service worker registration and offline page (with mocks).

### 5.2 API integration

- Mock Service Worker against generated types.
- Error paths: 401 refresh, 403, 404, 429, 500, network failure, maintenance.
- Optimistic send rollback and retry.
- Reconnect/resync behavior with a fake gateway.
- Multi-instance cache isolation.

### 5.3 E2E (Playwright)

Scenarios:

1. Register → verify email (captured) → login.
2. Login → refresh on reload → logout.
3. Create community → create text/voice channel → send/edit/delete message.
4. Reply, mention, reaction, pin, search.
5. Invite → join from a second user → role assignment → permission gate.
6. Friend request → accept → DM → block.
7. Upload attachment → preview/download → quota/error.
8. Join voice (test LiveKit) → mute/deafen → leave.
9. Camera/screen share where the CI environment supports it; otherwise capability-gated test.
10. Notification settings and mute.
11. Admin disables a user → session revoked.
12. Maintenance mode.
13. Instance switch with separate sessions.
14. PWA offline shell and reconnect banner.
15. Accessibility: keyboard-only navigation of the main shell, message composer, and settings.

E2E tests use a seeded database and deterministic clocks where possible. They do not depend on external networks.

---

## 6. Voice/video testing

### 6.1 Automated

- Token issuance for every permission combination.
- Token expiry, revocation, and room mismatch.
- Webhook signature/replay/out-of-order.
- Projection rebuild after Redis flush.
- Capacity and password checks.
- Real LiveKit join with a headless test client; publish a synthetic audio track; verify another client receives it.
- TURN fallback with UDP blocked (real Coturn, not mocked).
- Reconnect after network interruption.

### 6.2 Manual

- Headset/speaker/Bluetooth device switching.
- Echo cancellation/noise suppression/AGC on real hardware.
- Push-to-talk with focus and global shortcuts.
- Screen share on Windows/Linux/browsers; X11 vs Wayland.
- Mobile browser voice limitations.
- Corporate firewall/NAT.

Real audio must be heard. A green token test is not voice completion.

---

## 7. Desktop testing

- Windows 10/11: install, launch, device auth, keychain, notifications, tray, PTT, deep links, update.
- Linux: Ubuntu 24.04 X11 and Wayland, AppImage/deb, Secret Service, tray fallback, PTT fallback.
- Multi-instance switching.
- Window state across monitors.
- Upgrade from previous release; uninstall leaves no secrets.
- Capability detection for screen share; fallback path.
- Logs/diagnostics redaction.
- Auto-update signature rejection.
- Accessibility (keyboard, screen reader where available).

CI builds are necessary but not sufficient; manual smoke tests are required before a release.

---

## 8. Security testing

- Authentication abuse: brute force, credential stuffing simulation, enumeration, timing.
- Authorization bypass/IDOR matrix.
- Privilege escalation: role hierarchy, overwrites, bot grants, owner-only actions.
- Session revocation and refresh reuse.
- CSRF/Origin/CORS.
- XSS in all user-controlled strings and rendered content.
- SQL injection attempts in search/sort/filter.
- File upload attacks: MIME confusion, double extensions, SVG/HTML, huge images, zip bombs, path traversal, polyglots.
- SSRF via webhooks/link previews.
- WebSocket: ticket theft/reuse, origin, payload limits, filter bypass, slow consumer.
- Embed: origin bypass, token audience confusion, postMessage injection, clickjacking.
- Rate-limit bypass via headers/IPv6/multiple sessions.
- Secret leakage in logs/errors/bundles.
- Dependency and container scanning.
- Backup access control.

Findings are tracked with severity, reproduction, fix, and regression test.

---

## 9. Load and performance testing

### 9.1 Targets to validate

- 500 concurrent WebSocket connections per API node.
- 200 concurrent text users.
- 100 messages/second sustained.
- REST p95 < 200 ms for common reads.
- Message commit p95 < 150 ms.
- Fan-out p95 < 500 ms.
- 100 voice participants / 20 video publishers per LiveKit node (media, not API).

### 9.2 Scenarios

- Login/logout churn.
- 500 idle WS connections with heartbeats.
- 100 users in 20 channels sending messages.
- Large channel history pagination and search.
- Community with 10k members; member list pagination.
- File upload/download burst with quotas.
- Notification/mention fan-out.
- Voice token issuance burst.
- Outbox backlog and recovery.
- Redis failure and recovery.
- Database failover/restart.
- Reconnect storm after a gateway restart (jitter).

### 9.3 Method

- Use k6 for HTTP/WS; a Rust or TypeScript client for protocol-specific scenarios.
- Measure server CPU, memory, DB connections, p95/p99 latency, error rate, outbox lag.
- Record hardware, dataset size, and configuration with every result.
- Do not publish “supports X users” without the scenario and hardware.
- Fix regressions; do not hide them with larger timeouts.

---

## 10. CI gates

A pull request must pass:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test` (unit + integration)
- SQLx migration check
- OpenAPI validation and TypeScript type generation
- `pnpm lint`, `pnpm typecheck`, `pnpm test`
- Playwright smoke suite
- `cargo deny` and dependency audit
- Secret scanning
- Frontend production build
- Desktop build (at least compile) for Windows/Linux

Nightly/periodic:

- Full Playwright suite.
- LiveKit/Coturn integration.
- Load tests.
- Dependency/container scans.
- Restore drill (staging).
- Desktop installer builds.

Coverage is a guide, not a target to game. Security-critical modules (auth, permissions, storage, webhooks, embeds) require near-complete branch coverage and adversarial cases. A percentage alone does not prove correctness.

---

## 11. Test data and isolation

- Seed scripts create deterministic users, communities, roles, channels, and messages.
- Test emails use a reserved domain and are captured locally.
- Test uploads use generated small files and a few malicious fixtures.
- Tests never touch production data or send real email.
- Each integration test uses a unique database/schema and Redis namespace.
- Cleanup is automatic; tests must not depend on execution order.
- Time is injectable for expiry/retention tests.

---

## 12. Flaky tests

- Treat flakiness as a bug.
- No arbitrary sleeps; use readiness signals and polling with timeouts.
- Quarantine only with an owner and an issue; do not silently retry forever.
- Network/media tests may be marked nightly if the environment is unreliable, but they still run before release.
- Retries are allowed for known infrastructure races only, with a comment and a follow-up.

---

## 13. Definition of done

A feature is done when:

- [ ] Behavior and edge cases are specified.
- [ ] Backend authorization is enforced and tested.
- [ ] Unit and integration tests pass.
- [ ] Frontend states (loading, empty, error, permission, offline) are implemented.
- [ ] Localization keys exist in `en` and `tr`.
- [ ] Accessibility is checked.
- [ ] Security implications are reviewed.
- [ ] Rate limits/resource bounds are defined.
- [ ] Migrations are included and tested.
- [ ] API/WebSocket/docs are updated.
- [ ] Manual verification is recorded where automation is insufficient.
- [ ] No secrets or debug artifacts are committed.
- [ ] The project builds from a clean checkout.

---

## 14. Phase exit criteria (testing view)

| Phase | Exit criteria |
|---|---|
| 0 | Documents consistent; no code claims; environment risks recorded |
| 1 | Auth/session integration tests; migrations from empty; health checks; OpenAPI; no secrets in logs |
| 2 | Authorization matrix for communities/roles/invites; social tests; DB constraint tests |
| 3 | Message/reaction/pin/search tests; WS reconnect/resync; real PostgreSQL/Redis; load smoke |
| 4 | Real LiveKit/Coturn audio tests; token permission matrix; TURN fallback; revocation |
| 5 | Camera/screen share on supported browsers; capability fallback; real media |
| 6 | Playwright core flows; accessibility; PWA; responsive matrix; i18n |
| 7 | Windows/Linux installs; keychain; notifications; PTT; deep links; update signature |
| 8 | Webhook/embed/API security tests; SDK contract tests; demo site |
| 9 | Load/security review; restore drill; upgrade rehearsal; production install on clean host |

---

## 15. Open testing decisions

| Decision | Current position | Needed by |
|---|---|---|
| CI provider | GitHub Actions assumed | Phase 1 |
| LiveKit CI | Nightly + pre-release; resource-heavy | Phase 4 |
| Coverage thresholds | Security modules high; global guide only | Phase 1 |
| Visual regression | Design system only | Phase 6 |
| Load test hardware | Document per run; staging VPS | Phase 3 |
| External security review | Not scheduled | Before v1 |
| Browser matrix automation | Chromium in CI; manual Safari/Firefox | Phase 6 |
| Desktop signing in CI | Release job only | Phase 7 |
