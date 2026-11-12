# Architecture Decision Records

**Status:** Phase 0 decisions. Each record is immutable once accepted; changes require a new ADR that supersedes the old one.

Format: context, decision, consequences, alternatives considered.

---

## ADR-0001: Modular monolith for the Rust backend

**Status:** Accepted

**Context:** Tulpar has many domains (auth, communities, messaging, voice authorization, files, notifications) but an initial target of hundreds of concurrent users. Microservices would add distributed transactions, service discovery, network failure modes, and operational overhead before the product is proven.

**Decision:** Build one Rust workspace with clearly separated crates and one deployable binary (`tulpar-server`) containing the API, gateway, and background workers. Enforce dependency direction and table ownership through review and tests.

**Consequences:**

- Simple local development and one transaction boundary.
- Clear module boundaries without network calls between domains.
- Scaling starts vertically and by running multiple stateless nodes.
- A crate boundary can later become a service boundary if a specific component justifies it.
- Requires discipline to prevent the monolith from becoming a coupled mess.

**Alternatives:** Microservices from day one (rejected: premature complexity); a single giant crate (rejected: poor boundaries); separate media service in Rust (rejected: LiveKit is mature and not our core).

---

## ADR-0002: PostgreSQL is the source of truth

**Status:** Accepted

**Context:** Messages, membership, permissions, and files must survive process crashes, Redis loss, and deployments. Multiple data stores create consistency problems.

**Decision:** Store all durable state in PostgreSQL 16+. Use constraints, foreign keys, partial unique indexes, and migrations. Redis is never the message database.

**Consequences:**

- Strong consistency and mature backup/restore.
- A single primary is initially a scaling limit; read replicas can be added.
- Schema changes require careful migrations.
- Search uses PostgreSQL full-text and trigram indexes initially.

**Alternatives:** MySQL (rejected: fewer useful features for this design); SQLite (rejected: weak multi-node/ops story); document databases (rejected: relational integrity matters here).

---

## ADR-0003: Redis is ephemeral and optional on the message path

**Status:** Accepted

**Context:** Presence, rate limits, tickets, typing, and pub/sub need low-latency shared state. Making them mandatory for messaging would turn a cache outage into a chat outage.

**Decision:** Use Redis for ephemeral state and cross-node fan-out. Message commits do not require Redis. Define degraded behavior for every Redis-dependent feature.

**Consequences:**

- Messaging continues if Redis fails.
- Gateway tickets fail closed; multi-node fan-out stops; rate limits weaken to per-process memory.
- Presence is best-effort and may show “unknown”.
- Operators must monitor Redis and understand the degraded modes.

**Alternatives:** In-memory only (rejected: no multi-node); Kafka/NATS (rejected: overkill for presence/tickets); Redis as durable storage (rejected: wrong tool and backup burden).

---

## ADR-0004: UUIDv7 public identifiers

**Status:** Accepted

**Context:** Public IDs must be unique without a central sequence, safe to expose, and friendly to time-ordered pagination. Future federation may need globally unique IDs.

**Decision:** Generate UUIDv7 in the application with the Rust `uuid` crate. Use them for public resources. Do not expose `BIGSERIAL` IDs.

**Consequences:**

- IDs sort approximately by creation time, improving index locality and cursors.
- No database round-trip to obtain an ID.
- UUIDs are larger than 64-bit integers; storage/index size is acceptable.
- Clients must treat IDs as opaque strings.
- Internal outbox ordering still uses a separate identity column.

**Alternatives:** BIGSERIAL (rejected: enumeration and federation friction); ULID (similar; UUID has broader tooling); random UUIDv4 (rejected: worse index locality).

---

## ADR-0005: REST commands + WebSocket push, no durable event replay

**Status:** Accepted

**Context:** Real-time delivery is needed, but clients must recover after disconnects, deployments, and missed events. A durable per-user event log duplicates the database and can drift from it.

**Decision:** REST is authoritative for commands and reads. The WebSocket gateway pushes committed events with a per-connection sequence number. On reconnect or gap, clients call `GET /sync` and per-channel `after` queries. No durable event replay in v1.

**Consequences:**

- Recovery is simple and always consistent with the database.
- Reconnect may require more REST calls than replay.
- The gateway does not need an event store.
- Event delivery is best-effort; the client must handle gaps.
- Future resume optimization can be added without changing the contract.

**Alternatives:** Kafka event sourcing (rejected: operational complexity); per-user durable event log (rejected: duplication/drift); polling only (rejected: poor latency and load).

---

## ADR-0006: 64-bit permission bitfield with a central evaluator

**Status:** Accepted

**Context:** Permission checks are on every request, message, gateway event, and voice token. Channel overwrites need allow/deny masks. Role permissions must be compact and fast.

**Decision:** Store permissions as a `bigint` bitfield (bits 0–62), expose names in the API, and implement a pure centralized evaluator with role hierarchy and overwrite rules.

**Consequences:**

- Fast checks and simple storage.
- Adding a permission requires a new bit and documentation; bits are never reused.
- API clients cannot send raw integers, avoiding JavaScript bitwise limitations and bit 63 issues.
- The evaluator must be extensively property-tested.
- Effective permissions are not denormalized initially; cache only after measurement.

**Alternatives:** Row-per-permission (rejected: more joins and complex overwrites); string arrays in the database (rejected: slow evaluation); JSON permissions (rejected: weak constraints).

---

## ADR-0007: Browser cookies for web, device flow for desktop

**Status:** Accepted

**Context:** Refresh tokens must not be accessible to JavaScript in a browser. Desktop apps need durable credentials without storing them in webview storage.

**Decision:** Web uses a short-lived in-memory access token plus an `HttpOnly`, `Secure`, `SameSite=Lax` refresh cookie. Desktop uses the OAuth device authorization flow; the Tauri Rust layer stores the refresh token in the OS keychain and only returns access tokens to the webview.

**Consequences:**

- Web XSS cannot read the refresh token.
- Desktop refresh tokens stay out of the renderer.
- Desktop login requires a browser round-trip.
- Linux without Secret Service cannot persist the session; it must re-authenticate.
- Access tokens remain valid until expiry (≤15 minutes) after revocation.

**Alternatives:** localStorage refresh tokens (rejected: XSS); embedded password form in desktop (rejected: weaker and worse UX); OAuth authorization code + PKCE with deep links (viable, but device flow is simpler and more portable).

---

## ADR-0008: LiveKit for media, Coturn for TURN

**Status:** Accepted

**Context:** Voice/video is a core feature. Building an SFU or TURN server is a multi-year effort and a security risk.

**Decision:** Use LiveKit as the SFU and Coturn for STUN/TURN. Deploy them independently of the Rust backend. Tulpar authorizes and mints short-lived tokens; LiveKit routes media.

**Consequences:**

- Proven WebRTC behavior and SDK support.
- Extra infrastructure and networking complexity.
- LiveKit/Coturn must be monitored, upgraded, and firewalled.
- The backend must handle token revocation and webhook reconciliation.
- No custom codecs or media pipeline.

**Alternatives:** mediasoup/Janus (viable but more integration work); custom SFU (rejected); LiveKit built-in TURN only (rejected to keep TURN independent and explicit).

---

## ADR-0009: Storage abstraction with local, S3, and MinIO providers

**Status:** Accepted

**Context:** Self-hosters have different storage capabilities. Messaging code should not be rewritten to switch from local disk to object storage.

**Decision:** Define an `ObjectStore` abstraction with local filesystem and S3-compatible implementations. Use two-phase upload intents and finalize. Access is authorized through message/channel permissions, not object keys.

**Consequences:**

- Small instances can start with local disk.
- Larger deployments can move to S3/MinIO without changing message APIs.
- Local and S3 have different upload flows (signed local endpoint vs presigned S3).
- Thumbnails and integrity checks are provider-independent.
- Backups differ by provider.

**Alternatives:** Local only (rejected: poor scaling); S3 only (rejected: self-hosting barrier); storing blobs in PostgreSQL (rejected: bloat and backup cost).

---

## ADR-0010: Tauri 2 desktop wrapping the shared web SPA

**Status:** Accepted

**Context:** Desktop and web must share UI and features. Electron is heavy; a native UI would duplicate work.

**Decision:** Use Tauri 2 with Rust native capabilities and the shared React/TypeScript SPA. Add a narrow platform adapter for auth, notifications, keychain, PTT, tray, deep links, and updates.

**Consequences:**

- Small binaries and one UI codebase.
- WebView differences (WebView2/WebKitGTK) affect voice/video/capture.
- Some features require capability detection and fallbacks.
- Rust IPC must be least-privilege and validated.
- No macOS in v1.

**Alternatives:** Electron (rejected: footprint); Flutter/native UI (rejected: duplication); browser-only PWA (rejected: no global PTT/keychain/tray).

---

## ADR-0011: Single main origin plus a separate user-content origin

**Status:** Accepted

**Context:** Cookies, CSP, CORS, and WebSocket auth are simpler on one origin. User-uploaded content is the highest XSS risk.

**Decision:** Serve the web app, API, gateway, and embed pages from one origin. Serve uploaded files from a separate `usercontent` origin with restrictive headers.

**Consequences:**

- Simpler auth and realtime configuration.
- The user-content origin can have a different CSP and no cookies.
- One more DNS name and certificate.
- Caddy must keep the two origins isolated.
- Embed `frame-ancestors` is per-request and never applies to the main app.

**Alternatives:** Separate API domain (more CORS/cookie complexity); all content on the app origin (rejected: stored XSS blast radius).

---

## ADR-0012: Transactional outbox

**Status:** Accepted

**Context:** A database commit can succeed while the real-time event, webhook, or email is lost. Dual writes are unsafe.

**Decision:** Write durable domain events to an `outbox_events` table in the same transaction as the state change. A worker publishes/delivers and marks them processed, with retries and idempotency.

**Consequences:**

- No lost events after a crash.
- At-least-once delivery; consumers must deduplicate by `event_id`.
- Extra database writes and a worker to monitor.
- Event latency includes dispatcher scheduling.
- Webhooks and email reuse the same reliability mechanism.

**Alternatives:** Direct publish after commit (rejected: loss window); transactional messaging broker (rejected: operational complexity); polling only (rejected: latency).

---

## ADR-0013: Webhooks signed with HMAC and secrets encrypted at rest

**Status:** Accepted

**Context:** Webhook receivers need to verify authenticity. The server must be able to sign future deliveries, so it cannot store only a hash.

**Decision:** Generate a per-endpoint secret, show it once, store it encrypted with XChaCha20-Poly1305 under a versioned application key, and sign `timestamp.body` with HMAC-SHA256. Enforce SSRF protections and retries.

**Consequences:**

- Receivers can verify integrity and reject replays.
- The application encryption key is a critical secret.
- SSRF is a real risk and must be defended on every delivery.
- Delivery logs and retries are required.
- Secret rotation is an explicit operation.

**Alternatives:** Store plaintext (rejected); store only a hash (cannot sign); platform-wide shared secret (rejected: blast radius).

---

## ADR-0014: Embed as an iframe with scoped short-lived tokens

**Status:** Accepted

**Context:** Third-party sites need a safe integration without exposing a general API token or the user's session.

**Decision:** Host widgets on the instance origin, mount them in an iframe, validate the host origin against an allowlist, and issue short-lived `tulpar-embed` audience tokens. Authenticated widgets use a first-party popup approval flow and one-time authorization codes.

**Consequences:**

- Host pages never receive general tokens or cookies.
- The iframe and postMessage protocol must be strictly validated.
- Third-party cookie blocking is avoided for the token flow.
- Public widgets are read-only and rate-limited.
- Host CSP must allow the instance script/frame.

**Alternatives:** Inline widget JS (rejected: host DOM/XSS risk); API key in the host page (rejected: secret exposure); OAuth provider for third parties (rejected for v1 scope).

---

## ADR-0015: Expand/contract database migrations

**Status:** Accepted

**Context:** Upgrades must not destroy data, and rolling deployments may run old and new code briefly. Down migrations are risky and often untested.

**Decision:** Use SQLx migrations. After the first public release, migrations are immutable and additive. Destructive changes use expand/contract across releases. No automatic destructive migration. Back up before upgrades.

**Consequences:**

- Safer upgrades and rollback.
- More releases for a single schema cleanup.
- Migrations must be tested against the previous release.
- Some operational discipline is required.
- Pre-release squashing is allowed until the first public release.

**Alternatives:** Auto-generated down migrations (rejected: untested rollback); ORM auto-sync (rejected: dangerous); no migration versioning (rejected).

---

## ADR-0016: Structured observability from day one

**Status:** Accepted

**Context:** Debugging real-time, permissions, and media issues without traces/metrics is guesswork. Logging secrets is unacceptable.

**Decision:** Use `tracing` with JSON logs, request IDs, Prometheus metrics, and optional OpenTelemetry OTLP traces. Separate audit logs from application logs. No message content or secrets in either.

**Consequences:**

- Operable from the first deployment.
- Some overhead and configuration.
- Dashboards/alerts are part of the deployment.
- Log retention and privacy must be managed.
- No mandatory telemetry to the Tulpar project.

**Alternatives:** Plain text logs (rejected: poor production diagnostics); hosted APM only (rejected: self-hosting/privacy); no metrics (rejected).

---

## ADR-0017: No end-to-end encryption in v1

**Status:** Accepted

**Context:** E2EE conflicts with server-side search, bots, moderation, multi-device history, and voice moderation. Implementing it badly is worse than not claiming it.

**Decision:** Do not implement E2EE in v1. Encrypt in transit (TLS, DTLS/SRTP for media) and rely on operator-controlled infrastructure. Document clearly that the server can read messages and media metadata.

**Consequences:**

- Server-side search, bots, and moderation are possible.
- Users must trust the instance operator.
- E2EE remains a future design decision with a different product model.
- The UI and documentation must not imply E2EE.

**Alternatives:** Signal-style E2EE (rejected for v1 scope and feature conflict); server-side encryption at rest only (not E2EE, and not a substitute).

---

## ADR-0018: No federation in v1

**Status:** Accepted

**Context:** Federation adds identity, trust, moderation, spam, and consistency problems. The product first needs a solid single-instance experience.

**Decision:** Each instance is independent. Do not implement cross-instance federation. Keep globally unique IDs and avoid numeric public IDs so additive federation is possible later.

**Consequences:**

- Simpler security and data model.
- Users join each instance separately.
- A future bridge/federation layer will need explicit trust and policy.
- No speculative federation columns in v1.

**Alternatives:** ActivityPub-style federation (rejected for v1); central identity (rejected: violates self-hosting philosophy).

---

## ADR-0019: Instance admin UI integrated into the web app

**Status:** Accepted

**Context:** A separate admin SPA adds build/deploy complexity. The admin surface is small at first and benefits from the same design system and auth flow.

**Decision:** Serve `/admin` as a lazy-loaded, permission-gated section of the web app. Instance admin checks are backend-enforced. A separate admin bundle can be introduced later if the attack surface or deployment model requires it.

**Consequences:**

- One build and one origin.
- Admin code ships in the same artifact but is lazy-loaded.
- The backend must still enforce every admin action.
- A future split is additive.
- Community settings remain within the community context.

**Alternatives:** Separate admin app now (rejected: duplicate infra); CLI-only admin (rejected: poor operator UX for moderation/ops).

---

## ADR-0020: Baseline versions and toolchain

**Status:** Accepted (versions pinned in Phase 1)

**Context:** The project needs stable, supported versions that match Ubuntu 24.04 and current browsers.

**Decision:** PostgreSQL 16+, Redis 7+, Node 24 LTS, pnpm 10, Rust stable, Tauri 2, React 19, TypeScript 5.x, Vite 7+, Axum 0.8+, SQLx 0.8+, LiveKit and Coturn pinned by release/digest. Exact versions are locked in manifests and image digests.

**Consequences:**

- Supported, widely deployed versions.
- Some local tools are newer/older; CI uses the pinned versions.
- Upgrades are deliberate and tested.
- No reliance on an unpinned `latest` tag in production.

**Alternatives:** Newest non-LTS Node (rejected: churn); PostgreSQL 17/18 immediately (rejected: 16 is stable and available); unpinned images (rejected: supply-chain risk).

---

## ADR-0021: PostgreSQL search with `simple` + trigram initially

**Status:** Accepted

**Context:** Tulpar must support Turkish and English. Language-specific stemming can produce wrong results, and a separate search engine adds operational cost.

**Decision:** Use PostgreSQL full-text search with the `simple` dictionary for messages and trigram indexes for usernames/display names/community names. Add partial-content trigram search for messages after measuring volume. Do not add Elasticsearch/Meilisearch in v1.

**Consequences:**

- No extra service.
- Search is adequate for the initial scale.
- Relevance is simpler than a dedicated engine.
- Turkish stemming is limited; trigram helps.
- Search must always be permission-scoped.

**Alternatives:** Elasticsearch (rejected: operational cost); language-specific configs only (rejected: mixed-language communities); no search (rejected: requirement).

---

## ADR-0022: No message queue or service mesh in v1

**Status:** Accepted

**Context:** Kafka/RabbitMQ/NATS and a service mesh are common “serious project” additions, but they add operational burden without a current requirement.

**Decision:** Use PostgreSQL outbox + Redis pub/sub for events. Do not add a message broker, service mesh, Kubernetes, or GraphQL in v1. Revisit only with measured need.

**Consequences:**

- Fewer moving parts and failure modes.
- Outbox lag must be monitored.
- Redis pub/sub is fire-and-forget; REST recovery covers gaps.
- Horizontal scaling is possible without a mesh.
- Future extraction of a service can introduce a broker if justified.

**Alternatives:** Kafka (rejected for now); NATS (possible later); service mesh (rejected); GraphQL (rejected: REST + typed SDK is sufficient).
