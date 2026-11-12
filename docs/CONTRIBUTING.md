# Contributing to Tulpar

**Status:** Phase 1. The Rust workspace, auth/API foundation, and local test harness exist. The project is not public-release ready and has no CI pipeline yet.

Tulpar is a long-term, security-sensitive, self-hosted communication platform. Contributions must be deliberate, tested, documented, and consistent with the architecture.

---

## 1. Before you start

Read, in order:

1. [README.md](../README.md)
2. [ARCHITECTURE.md](ARCHITECTURE.md)
3. The contract document for your area: [DATABASE.md](DATABASE.md), [API.md](API.md), [WEBSOCKET.md](WEBSOCKET.md), [AUTHENTICATION.md](AUTHENTICATION.md), [PERMISSIONS.md](PERMISSIONS.md), [VOICE.md](VOICE.md), [SECURITY.md](SECURITY.md).
4. [TESTING.md](TESTING.md) and [DECISIONS.md](DECISIONS.md).

If a change conflicts with an accepted decision, open a design discussion and add a new ADR. Do not silently change the architecture in a pull request.

---

## 2. Development setup

Phase 1 establishes the verified toolchain. The intended prerequisites are:

- Rust stable (rustup).
- Node.js 24 LTS.
- pnpm 10.
- Docker Engine/Desktop with Compose v2.
- PostgreSQL 16 and Redis 7 (via Compose).
- Tauri 2 system dependencies for desktop work.
- Git.

On Windows, prefer a local, non-OneDrive path for the repository. Build outputs (`target/`, `node_modules/`, `dist/`) should not be synced.

Do not commit generated build output, local databases, `.env` files, or secrets.

---

## 3. Repository conventions

- Rust crates live in `crates/` and follow the ownership rules in [ARCHITECTURE.md](ARCHITECTURE.md).
- Web and desktop UI share the SPA in `apps/web`; desktop-specific code lives in `apps/desktop`.
- Shared TypeScript packages live in `packages/`.
- Infrastructure and deployment assets live in `infrastructure/` and `deployments/`.
- Documentation lives in `docs/`.
- Tests live next to the code where practical; cross-cutting suites live in `tests/`.

Do not create a new crate/package unless it has a clear responsibility and does not duplicate an existing one. Explain the boundary in the PR.

---

## 4. Workflow

1. Create a focused branch from the current development branch.
2. Make the smallest coherent change.
3. Add or update tests.
4. Update the relevant documentation and contracts.
5. Run formatting, linting, tests, and a production build.
6. Open a PR with a clear description, risk assessment, and verification steps.

Suggested branch names:

- `feat/message-search`
- `fix/refresh-rotation`
- `docs/voice-failure-modes`
- `chore/upgrade-axum`

Suggested commit style: conventional commits, imperative mood.

```
feat(messaging): add cursor-based message search
fix(auth): revoke session family on refresh reuse
docs(security): document TURN credential lifetime
```

Do not mix unrelated refactors into a feature PR.

---

## 5. Rust standards

- `cargo fmt` and `clippy -D warnings` are mandatory.
- Prefer explicit error types (`thiserror`) over stringly errors.
- No `unwrap`/`expect` on request paths; use typed errors and map to problem+json.
- No blocking I/O or CPU-heavy work on the async runtime; use `spawn_blocking` for Argon2 and image processing.
- Keep handlers thin; put business logic in domain services.
- Every SQL query is parameterized; no user input in SQL strings.
- Do not log secrets, tokens, message content, or full personal data.
- Use `#![forbid(unsafe_code)]` unless a reviewed, documented exception is required.
- New dependencies need a justification: why it is needed, maintenance status, license, transitive size, and security history.
- Public APIs and event types are documented.

### Module boundaries

- A crate owns writes to its tables.
- Read-only joins are allowed for projections; cross-domain writes go through the owning service.
- `tulpar-api` and `tulpar-gateway` compose; domains do not depend on them.
- No cyclic crate dependencies.

### Database changes

- Every schema change is a new SQLx migration.
- Never edit an applied migration after the first public release.
- Use expand/contract for destructive changes.
- Add indexes for every foreign-key lookup and list query.
- Test migrations from an empty database and from the previous release.
- Do not put secrets or large blobs in the database.
- Update [DATABASE.md](DATABASE.md) in the same PR.

---

## 6. TypeScript / frontend standards

- Strict TypeScript; no `any` without a comment and a narrow reason.
- Functional components and hooks; no class components.
- Server state via TanStack Query; client state via Zustand. Do not duplicate server state in Zustand.
- Query keys include the instance URL.
- No `dangerouslySetInnerHTML` for user content; use the sanitized Markdown renderer.
- No tokens in localStorage, IndexedDB, URLs, or logs.
- All user-facing strings go through i18n; add both `en` and `tr` keys.
- Components must handle loading, empty, error, permission, and offline states.
- Use the shared design system; do not hardcode one-off colors/spacing.
- Accessibility is part of the definition of done: keyboard, focus, labels, contrast.
- Avoid unnecessary dependencies and large bundles; check the route chunk budget.
- No third-party analytics or tracking scripts by default.

### API client

- Types are generated from OpenAPI; do not hand-write duplicate resource types.
- The client maps stable error codes; it does not parse human-readable messages.
- Mutations that create resources send an `Idempotency-Key`.
- Refresh is single-flight; failed refresh logs out cleanly.
- WebSocket events are validated before updating caches.

---

## 7. Security rules

- Never commit secrets, private keys, certificates, `.env` files, or production data.
- Never weaken authorization to make a test or UI easier.
- Every resource lookup must be scoped by membership/permission.
- Private resources return `404` when existence must not leak.
- Validate inputs, bound sizes, normalize text, and encode outputs.
- New network fetches require SSRF analysis, timeouts, redirect policy, and size limits.
- New file types require MIME sniffing, extension checks, and safe serving.
- New background jobs must be idempotent and safe to retry.
- Security-relevant actions need audit records without sensitive data.
- Add negative and escalation tests, not just happy paths.
- If you find a vulnerability, follow [SECURITY.md](SECURITY.md); do not open a public issue.

---

## 8. API and WebSocket contracts

- Public API changes require an update to [API.md](API.md) and the OpenAPI schema.
- Breaking changes require a new API version or a new event name/version.
- Additive fields are allowed in v1; removing/renaming is not.
- Stable error codes and event names; localized text is not a contract.
- Permission names in payloads must match [PERMISSIONS.md](PERMISSIONS.md).
- WebSocket changes require type updates in `packages/shared-types` and contract tests.
- Document rate limits and resource bounds for new endpoints.

---

## 9. Testing

Follow [TESTING.md](TESTING.md).

Minimum for a feature PR:

- Unit tests for pure logic.
- Integration tests for database/API behavior.
- Authorization matrix tests for new resources.
- Frontend component tests for new UI states.
- One E2E path for a user-visible flow where practical.
- Manual verification notes for voice/video/desktop features that CI cannot cover.

A PR that says “tested manually” without steps is not sufficient for security-sensitive code.

---

## 10. Localization

- `en` and `tr` are required from the start.
- No hardcoded user-facing strings.
- Use `Intl` for dates, numbers, and relative time.
- Do not concatenate translated fragments; use complete messages with placeholders.
- Check layout with longer Turkish strings.
- Server error codes are stable; client maps them to localized messages.

---

## 11. Documentation

Update the relevant document when you change:

- Architecture or crate boundaries.
- Database schema or retention.
- API/WebSocket contracts.
- Authentication or permissions.
- Deployment, ports, or configuration.
- Security behavior or limitations.
- Testing strategy or phase status.

Documentation must describe what is true, not what is planned as if it were implemented. Mark planned work clearly.

---

## 12. Pull request checklist

- [ ] The change is focused and explained.
- [ ] No secrets, debug artifacts, or generated output.
- [ ] Rust: fmt, clippy, tests pass.
- [ ] Frontend: lint, typecheck, tests, production build pass.
- [ ] Migrations are new, tested, and documented.
- [ ] Authorization is enforced and tested.
- [ ] Inputs/outputs/limits/rate limits are defined.
- [ ] Error codes and states are handled.
- [ ] `en` and `tr` localization added.
- [ ] Accessibility checked.
- [ ] OpenAPI/WS types updated.
- [ ] Docs updated.
- [ ] Manual verification recorded where needed.
- [ ] Security implications reviewed.
- [ ] The project builds from a clean checkout.

---

## 13. Review and merge

- At least one maintainer review is required.
- Security-sensitive changes require a second review focused on auth/permissions/data exposure.
- Reviewers check behavior, boundaries, tests, failure modes, and documentation, not just style.
- Do not merge with failing required checks.
- Squash or rebase according to the repository policy; keep history readable.
- Do not force-push a shared branch without coordination.

---

## 14. Releases

- Releases follow the phase exit criteria in [ROADMAP.md](ROADMAP.md).
- Server images are pinned by digest and checksummed.
- Desktop releases are signed; update manifests are signed.
- Database migrations are applied as a one-shot job with a backup.
- Release notes list breaking changes, migrations, and rollback steps.
- No release is made from a dirty working tree.
- The changelog is updated.

---

## 15. Conduct

Be direct, respectful, and constructive. Harassment, discrimination, doxxing, and disclosure of private user data are not acceptable. Security research must follow [SECURITY.md](SECURITY.md). Maintainers may remove contributions that violate these rules.

---

## 16. Questions

Until the repository has public discussions, contact the project owner through the private channel used for the project. Do not post secrets or vulnerability details in public.
