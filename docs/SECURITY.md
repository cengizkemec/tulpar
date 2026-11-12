# Tulpar Security Model

**Status:** Phase 2. Authentication, sessions, rate limiting, CSRF/Origin checks, password hashing, community permission evaluation, friend/block privacy, and the local Docker deployment controls are implemented. Many controls in this document belong to later phases and are not yet exercised. This is not a certification or an audit.

Security is a release blocker, not a later hardening phase. This document defines the threat model, required controls, known limitations, and the review process for changes.

---

## 1. Reporting a vulnerability

Do not open a public issue, discussion, or pull request for a suspected vulnerability.

A private security contact and GitHub Security Advisories configuration will be announced before the first public release. Until then, contact the project owner through a private channel.

Include:

- Affected component and version/commit.
- Reproduction steps or a proof of concept.
- Impact and affected data.
- Whether the issue is already public.
- Suggested mitigation, if any.

Do not access other users' data, disrupt a running instance, or publish details before a fix is available. Good-faith research that follows these rules will not be pursued.

---

## 2. Supported versions

Before v1, only the latest commit on the main development line is supported. After v1:

- The latest minor release receives security fixes.
- The previous minor release receives critical fixes for 90 days.
- Unsupported versions are documented; operators are expected to upgrade.

There is no bug bounty program yet. This may change.

---

## 3. Threat model

### 3.1 Assets

| Asset | Classification |
|---|---|
| Passwords, password hashes | Secret / sensitive |
| Refresh tokens, API keys, bot tokens, webhook secrets | Secret |
| JWT signing key, LiveKit secret, TURN secret, SMTP/S3 credentials | Secret |
| Private messages, DMs, attachments | Confidential |
| Voice/video media | Confidential in transit; not E2EE |
| Email addresses, IP addresses, sessions | Personal data |
| Community membership, roles, audit logs | Confidential/internal |
| Public community metadata | Public |
| Backups | Confidential, contain all of the above |

### 3.2 Trust boundaries

| Boundary | Trust level |
|---|---|
| Browser, webview, embed host page | Untrusted |
| Public internet | Untrusted |
| Caddy | Trusted TLS terminator with a minimal config |
| Tulpar API/gateway | Trusted application boundary |
| PostgreSQL, Redis | Trusted internal services; never publicly exposed |
| LiveKit, Coturn | Trusted infrastructure with scoped secrets |
| Object storage | Trusted storage; stored user content is untrusted |
| Instance administrator | Powerful, audited, not trusted with secrets or private content by default |
| Host root / hypervisor | Fully trusted; compromise is outside the application threat model |

### 3.3 Adversaries

- Remote unauthenticated attackers.
- Authenticated users attempting IDOR, privilege escalation, spam, or harassment.
- Compromised user accounts and stolen tokens.
- Malicious community administrators or moderators.
- Malicious third-party websites using the embed SDK.
- Attackers who can send crafted files, URLs, messages, or WebSocket frames.
- Supply-chain attackers targeting dependencies, build artifacts, or container images.
- Network attackers on insecure networks.
- Curious or malicious instance operators (privacy threat, not a host-compromise threat).

### 3.4 Out of scope

- An attacker with root on the host or hypervisor.
- A compromised browser, OS, or malicious extension with full page access.
- Unlimited-resource denial of service.
- Physical access to an unlocked device.
- Coercion of the operator or legal compulsion.
- Vulnerabilities in LiveKit, PostgreSQL, Redis, or Coturn that are not reachable through Tulpar's configuration. These are still tracked and upgraded.

---

## 4. Transport and browser controls

- TLS 1.2+ only; TLS 1.3 preferred. HTTP redirects to HTTPS.
- HSTS after the deployment is stable; preload only after all subdomains are HTTPS.
- Secure, HttpOnly, SameSite refresh cookies; no tokens in URLs.
- `Referrer-Policy: strict-origin-when-cross-origin` or stricter.
- `X-Content-Type-Options: nosniff`.
- `X-Frame-Options` is not relied on; CSP `frame-ancestors` is authoritative.
- `Permissions-Policy` restricts camera, microphone, and display capture to the app and authorized embed frames.
- `Cross-Origin-Opener-Policy: same-origin-allow-popups` for the app where embed login requires popups; verify compatibility. The narrow `/embed/authorize` page may omit COOP (or use `unsafe-none`) solely to retain `window.opener` for the one-time code handshake, with a stricter CSP and no third-party content.
- `Cross-Origin-Resource-Policy: same-origin` for private API responses; user content uses a separate origin.
- Strict CSP, no inline scripts in production, `object-src 'none'`, `base-uri 'self'`, `form-action 'self'`.
- CORS uses exact origins and `Vary: Origin`; credentials are allowed only for configured web origins.

CSP values are deployment-specific and must be tested against the built frontend. A permissive `default-src *` is not acceptable.

---

## 5. Authentication and sessions

The full model is in [AUTHENTICATION.md](AUTHENTICATION.md). Non-negotiable points:

- Argon2id with per-password salt and versioned parameters.
- Passwords never logged, never stored reversibly, never returned.
- Refresh tokens are 256-bit random, hashed at rest, rotated on every use, and bound to a server-side session.
- Reuse of a rotated refresh token revokes the session family.
- Browser refresh tokens are `HttpOnly`, `Secure`, `SameSite=Lax`, path-scoped.
- Desktop refresh tokens are stored in the OS keychain through the device authorization flow.
- Access tokens are short-lived JWTs. Revocation is immediate for the gateway and voice; existing REST tokens expire within 15 minutes.
- Login and reset are rate limited and return generic errors to reduce enumeration.
- Session management lists devices and allows revocation.
- Sensitive changes require the current password or recent re-authentication.
- No MFA in v1. This is a known gap for high-risk deployments.

---

## 6. Authorization

- Every resource lookup is scoped by the actor's membership and permissions.
- The backend is the only authorization authority.
- Channel/message/member/role IDs are never sufficient authorization.
- Private resources return `404` when the actor must not know they exist.
- The permission evaluator is centralized and property-tested; see [PERMISSIONS.md](PERMISSIONS.md).
- Role hierarchy and anti-escalation rules are enforced server-side.
- Instance administration is separate from community administration.
- Instance admins cannot read private messages/DMs by default.
- The gateway filters every event by current permissions.
- LiveKit tokens are minted only after permission, membership, ban, timeout, capacity, and password checks.
- DMs are participant-only and block-aware.
- Bots use ordinary roles and cannot be instance admins or community owners.

### 6.1 IDOR and enumeration

- Every endpoint that accepts an ID performs a scoped lookup, not a global lookup followed by a check.
- List endpoints filter by membership at the SQL predicate level.
- Invite previews reveal only public community information.
- User search respects privacy and block settings.
- Error timing and status codes avoid leaking private resource existence.

### 6.2 Multi-instance isolation

- Each instance has its own database, Redis namespace, keys, LiveKit secret, and storage.
- Tokens are audience-bound to an instance public URL (`iss`) and API audience.
- A token from instance A is rejected by instance B even if the signing secret were accidentally shared; `iss` and key separation must both hold.
- The client stores credentials per instance URL and never reuses them across instances.
- Embed tokens are audience-bound and cannot be used as general API tokens.

---

## 7. Input, output, and injection

### 7.1 Validation

- Explicit request DTOs with `serde` and a validation library; unknown fields rejected.
- Length, range, enum, and format checks on every field.
- Unicode normalization where appropriate; reject NUL and invalid control characters.
- IDs parsed as UUIDs, never interpolated into SQL.
- Pagination limits enforced.
- JSON depth and body size limits.
- WebSocket frame size and rate limits.
- Upload size, type, and dimension limits.

### 7.2 SQL injection

- SQLx parameterized queries.
- No string interpolation of user input.
- Dynamic sort/filter clauses use a small allowlist, never raw input.
- Database errors are mapped to generic responses; SQL is never returned to clients.

### 7.3 XSS

- React escapes text by default.
- No `dangerouslySetInnerHTML` for user content.
- Markdown is rendered by a strict sanitizer with an allowlist of elements and attributes.
- URLs are limited to `https`, `http`, and `mailto`; `javascript:`, `data:`, `vbscript:`, and `file:` are rejected.
- HTML, SVG, and script attachments are served as downloads from a separate user-content origin with `Content-Disposition: attachment`, `nosniff`, and a restrictive CSP.
- Images are decoded and re-encoded where practical; EXIF metadata is stripped.
- No user-controlled `style` attributes or CSS injection.
- CSP is defense in depth, not the primary XSS control.

### 7.4 CSRF

- General API mutations use Bearer tokens, not cookies.
- Cookie-authenticated auth endpoints validate `Origin` and use `SameSite=Lax`.
- Any future cookie-authenticated mutation must add double-submit CSRF protection before shipping.
- CORS never uses `*` with credentials.

### 7.5 Open redirects and URL handling

- Redirect targets are server-constructed or validated against an allowlist.
- `return_to` parameters are not accepted for auth.
- External links open with `noopener noreferrer`.
- Deep links are parsed and validated; no shell invocation.
- Webhook and link-preview URLs are SSRF-checked.

---

## 8. File uploads and user content

- Upload intents are authenticated, size-limited, purpose-scoped, and expire.
- The server sniffs MIME type from bytes; the client's `Content-Type` is not trusted.
- Extension/MIME pairs are allowlisted per purpose.
- SVG, HTML, JavaScript, and executable formats are rejected or forced to download.
- Images are dimension/pixel-limited to prevent decompression bombs, decoded, and re-encoded.
- Filenames are display-only; object keys are generated by the server. Path traversal is impossible by construction.
- Per-user, per-community, and instance quotas are enforced.
- Attachments inherit message/channel visibility. Avatars are public by design.
- Signed URLs are short-lived and audience-scoped.
- Object storage buckets are private; no public listing.
- Optional ClamAV scanning can quarantine files. It is not required for v1 but is recommended for public communities.
- Orphaned pending uploads and deleted files are purged.
- No archive extraction in v1.
- User content is served from a dedicated origin, not the main app origin.

---

## 9. Webhooks and SSRF

- HTTPS only.
- DNS resolution rejects loopback, private, link-local, multicast, metadata-service, and reserved ranges.
- DNS is re-resolved on every delivery to prevent rebinding.
- No redirects.
- Connect/read timeouts and a capped response body.
- Webhook creation requires `MANAGE_WEBHOOKS` and is rate limited.
- Signing secrets are generated server-side, shown once, and stored encrypted with XChaCha20-Poly1305 under a versioned application key.
- Deliveries are signed with HMAC-SHA256 over `timestamp.body`; receivers must reject stale timestamps and deduplicate event IDs.
- Delivery logs store only a short response excerpt.
- Optional egress allowlists for high-security deployments.

Link previews, if implemented, use the same SSRF controls and never fetch arbitrary user URLs from the server without them.

---

## 10. Voice and video

- LiveKit API secret is server-side only.
- Tokens are short-lived and least-privilege (`canPublishSources` by permission).
- Room names are derived server-side.
- Membership, ban, timeout, capacity, and room password are checked before token issuance.
- TURN credentials are ephemeral and derived from a shared secret; no static TURN password in clients.
- LiveKit webhooks are signature-verified and replay-protected.
- Moderation actions use the LiveKit server API and are audited.
- Revocation removes the participant and refuses new tokens.
- No recording or E2EE in v1; users must be told the server can access media metadata and content.
- Screen sharing requires a user gesture and a visible indicator.

---

## 11. Embed security

See [EMBED_SDK.md](EMBED_SDK.md). Key controls:

- Exact origin allowlist, enforced server-side.
- Public key is not a secret.
- Short-lived, audience-scoped tokens; never general API tokens.
- Popup login on the instance origin; no third-party cookie dependency.
- `postMessage` source/origin/type validation; `targetOrigin` never `*`.
- iframe sandbox and per-request `frame-ancestors`.
- Member permissions re-checked on every request.
- No private channels, DMs, audit data, or LiveKit tokens in embeds.

---

## 12. Rate limiting and denial of service

- Reverse proxy timeouts and body limits.
- Per-IP, per-user, per-key, and per-route token buckets.
- Argon2 concurrency semaphore; login floods cannot exhaust memory.
- WebSocket connection limits, frame limits, backpressure, and slow-consumer disconnects.
- Upload quotas and streaming limits.
- Search and embed rate limits.
- Webhook delivery concurrency limits and backoff.
- Database pool limits and statement timeouts.
- Redis-backed limits with a bounded in-memory fallback when Redis is unavailable.
- No endpoint performs unbounded work based on a client-supplied count.

No system is immune to a resource-exhaustion attack. The goal is bounded impact, fast recovery, and clear alerts.

---

## 13. Secrets and key management

Secrets include:

- Database URL/password.
- Redis password.
- JWT signing keys.
- Embed signing key.
- LiveKit API key/secret.
- TURN shared secret.
- SMTP credentials.
- S3/MinIO credentials.
- Webhook secret-encryption key.
- Update signing private key (offline).
- Code signing certificates.

Rules:

- Never committed to Git. `.env` is ignored; `.env.example` contains placeholders only.
- Injected through environment variables or Docker secrets, with restrictive file permissions.
- Never returned by the admin API, logged, included in crash reports, or embedded in frontend bundles.
- Separate secrets per environment and per instance.
- JWT keys support `kid` rotation; old keys verify until token expiry.
- LiveKit/TURN/SMTP/S3 secrets rotate through documented procedures.
- Webhook secrets are encrypted at rest because they must be recoverable for signing.
- CI runs secret scanning (e.g. gitleaks) on every change.
- Backups containing secrets are encrypted and stored offline or in a separate trust domain.

---

## 14. Logging, privacy, and audit

- Structured logs include request ID, route, status, latency, and actor ID where appropriate.
- Never log passwords, tokens, cookies, authorization headers, message content, attachment contents, full email addresses, or webhook secrets.
- IP addresses are personal data; retention is limited and documented.
- Audit logs record security-relevant actions with actor, target, action, IP, and metadata. They do not contain message bodies.
- Audit logs are append-only for the application role.
- Optional crash reporting is opt-in, self-hostable, and redacted.
- No mandatory telemetry is sent to the Tulpar project.
- Data minimization: collect only what the feature needs. No contact lists, location, or device fingerprinting.
- Account deletion and data export are supported as documented; legal retention policy is reviewed before public release.

---

## 15. Administration

- Instance administrators are a separate table; the first admin is created by CLI, never by “first registrant” auto-promotion.
- Admin actions are audited.
- Destructive actions require recent re-authentication.
- The last remaining admin cannot be removed through the API.
- Admins cannot read passwords, refresh tokens, API keys, webhook secrets, or private message content by default.
- Community administrators are ordinary community members with roles; they cannot bypass instance policy.
- Maintenance mode blocks new sessions and non-admin mutations while preserving read-only health information.
- Admin UI is lazy-loaded and permission-gated; secrets are never rendered.

---

## 16. Supply chain and dependencies

- Commit lockfiles (`Cargo.lock`, `pnpm-lock.yaml`).
- Pin Docker images by digest for production; use minimal base images.
- Run `cargo deny` (licenses, advisories, bans, sources) and `cargo audit`.
- Run `pnpm audit` and a dependency review.
- Dependabot/Renovate with review; no automatic merge of major updates.
- Review new dependencies for maintenance, license, transitive size, and necessity.
- Avoid `unsafe` Rust unless justified and reviewed; prefer `#![forbid(unsafe_code)]` in application crates.
- Build release artifacts in CI with pinned toolchains and reproducible settings where possible.
- Generate an SBOM and publish checksums/signatures for releases.
- Sign desktop updates and, where available, Windows binaries.
- No post-install scripts from untrusted packages; pnpm can be configured to ignore scripts and allowlist exceptions.
- Container images run as non-root, read-only where possible, with dropped capabilities.
- Database and Redis are not exposed publicly; LiveKit API is internal; Coturn exposes only required ports.

---

## 17. Backup and disaster recovery security

- Backups are encrypted at rest and in transit.
- Access is limited to operators; restore requires authorization.
- Restore is tested, not assumed.
- Backups are stored off-host and retained according to policy.
- Redis is not backed up as durable data; sessions and presence are ephemeral.
- Secrets are backed up separately from data and never in the same plaintext archive.
- A compromised backup is treated as a data breach.

See [BACKUP.md](BACKUP.md).

---

## 18. Incident response

1. **Triage:** confirm scope, affected versions, data, and active exploitation.
2. **Contain:** revoke sessions/keys, disable affected features, isolate the instance, block abusive accounts.
3. **Eradicate:** patch, rotate secrets, rebuild compromised hosts if needed.
4. **Recover:** restore from a verified backup, monitor, verify integrity.
5. **Notify:** affected users/operators according to legal and ethical obligations.
6. **Review:** blameless postmortem, timeline, root cause, preventive actions.
7. **Document:** update this file and the runbooks.

Operators should have an out-of-band contact path and a pre-written user notification template.

---

## 19. Known limitations

These are explicit and must appear in operator/user documentation where relevant:

1. **No end-to-end encryption.** The server and instance operator can access message content and media metadata. Voice/video is encrypted in transit (DTLS/SRTP) but not E2EE.
2. **Access-token revocation window.** Revoked REST access tokens remain valid until expiry (≤15 minutes by default). Gateway and voice revocation are immediate.
3. **No MFA in v1.** High-risk deployments should use strong unique passwords and restrict admin access at the network layer.
4. **Redis degraded mode.** When Redis is down, rate limits are weaker, presence is local/unknown, and multi-node fan-out stops. This is documented and monitored.
5. **Instance admin content access.** Admins do not get private message access by default; a legal/abuse process is a separate policy decision.
6. **Embed limitations.** Third-party cookie blocking, popup blockers, host CSP, and iframe constraints can degrade widgets.
7. **Desktop capture.** Screen sharing depends on the WebView; not all Linux/Wayland environments work.
8. **Mobile voice.** Mobile browser background audio is restricted; no native mobile app in v1.
9. **No recording/transcription.** Not available, so no related consent/storage risks yet.
10. **No formal certification.** Tulpar is not certified for HIPAA, SOC 2, PCI, or similar. Operators are responsible for their compliance obligations.

---

## 20. Security review checklist for changes

Every PR that touches auth, permissions, files, network access, or admin features must answer:

- [ ] What is the trust boundary and who can call this?
- [ ] Is authorization enforced on the backend for every resource?
- [ ] Are IDs scoped by membership, not just validated?
- [ ] Are inputs validated, bounded, and normalized?
- [ ] Are outputs encoded/sanitized and served with the right headers?
- [ ] Does this introduce a new secret? How is it stored, rotated, and kept out of logs?
- [ ] Does this add a network fetch? Is it SSRF-protected with timeouts and size limits?
- [ ] Does this add a file type or parser? What are the decompression/parse limits?
- [ ] Does this change session/token behavior? Is revocation still correct?
- [ ] Does this add a dependency? Is it necessary, maintained, and licensed acceptably?
- [ ] Are rate limits and resource bounds defined?
- [ ] Are security-relevant actions audited without sensitive data?
- [ ] Are tests covering the negative and escalation cases?
- [ ] Is the documentation updated, including limitations?

---

## 21. Security testing

- Authentication abuse and enumeration tests.
- Authorization bypass/IDOR matrix for every endpoint.
- Privilege escalation tests for roles, hierarchy, overwrites, bots, and embeds.
- Upload validation: MIME confusion, double extensions, SVG/HTML, zip bombs, huge images, path traversal.
- XSS payloads in names, topics, messages, reactions, custom statuses, and embed data.
- CSRF/Origin/CORS tests.
- Rate-limit and lockout tests.
- Session revocation and refresh reuse tests.
- WebSocket origin/ticket/filter/backpressure tests.
- Webhook SSRF and signature/replay tests.
- Embed origin/postMessage/token tests.
- Dependency and container scanning.
- Load tests with realistic connection churn.
- External security review before a public production release, if feasible.

A feature is not complete until its security tests pass. “Works in the happy path” is not completion.

---

## 22. Open security decisions

| Decision | Current position | Needed by |
|---|---|---|
| Private security contact / advisories | To be announced | Before public release |
| MFA | Not in v1 | Post-v1 |
| E2EE | Not in v1; conflicts with search/bots/moderation | Future design |
| At-rest encryption | Disk/volume encryption by operator; optional app-level later | Phase 9 |
| Malware scanning | Optional ClamAV integration | Phase 3/9 |
| Egress allowlist | Optional operator feature | Phase 9 |
| Audit retention | 365 days default | Phase 2 |
| Legal retention/erasure policy | Needs review | Before public release |
| Bug bounty | None yet | Post-v1 |
| Security audit | Not yet scheduled | Before v1 |
