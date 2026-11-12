# Tulpar Deployment

**Status:** Phase 2. A development Docker Compose stack (`deployments/development`) builds and runs PostgreSQL, Redis, LiveKit, Coturn, and the API with health checks. It was verified from a local non-OneDrive build context because Docker Desktop cannot read the current OneDrive path directly. Production deployment remains Phase 9.

## 4.1 Verified development stack

From the repository root:

```bash
cp deployments/development/.env.example deployments/development/.env
docker compose --env-file deployments/development/.env \
  -f deployments/development/docker-compose.yml up -d --build
curl http://127.0.0.1:18080/health/ready
```

The `.env` file contains development-only values and must remain untracked. The API health endpoint reported database and Redis as `up`, and real community/message/upload/WebSocket flows succeeded through the Docker API. Use `down` without `-v` to preserve the PostgreSQL and `uploads` volumes.

The API container starts as root only to create and chown the uploads volume, then drops to the `tulpar` user through `gosu`. `TULPAR__STORAGE__ROOT` and `TULPAR__STORAGE__MAX_UPLOAD_BYTES` control the local storage path and per-file limit.

Voice uses `TULPAR__VOICE__LIVEKIT_URL` (client URL), `TULPAR__VOICE__LIVEKIT_API_URL` (internal admin URL), the LiveKit key/secret, and the Coturn shared secret for ephemeral TURN credentials. Coturn must run with `--use-auth-secret --static-auth-secret=<secret>`; static TURN users are not used. LiveKit webhooks are sent to `/webhooks/livekit` and verified against the API secret.

**Primary target:** Ubuntu Server 24.04, Docker Engine with Compose v2.

**Initial topology:** one VPS for the application and data services, with LiveKit/Coturn either co-located or on a separate media host.

---

## 1. Deployment principles

1. A single VPS must be enough for a small community, with a documented path to scale.
2. PostgreSQL, Redis, and the LiveKit API are never exposed publicly.
3. TLS is mandatory; Caddy obtains and renews certificates automatically.
4. Secrets are injected at deploy time and never baked into images or committed.
5. Data survives upgrades; migrations are explicit and backed up.
6. Media traffic does not pass through the Rust API.
7. Every service has a health check, restart policy, and bounded resources.
8. The operator can back up and restore without vendor tooling.
9. Optional components (monitoring, MinIO, email) can be enabled independently.
10. No central Tulpar cloud is required.

---

## 2. Reference topology

```
                         Internet
                            │
              ┌─────────────┴─────────────┐
              │                           │
        80/443 TCP                   3478 UDP/TCP
        5349 TCP                     50000-51999 UDP
              │                           │
        ┌─────▼─────┐               ┌─────▼─────┐
        │  Caddy    │               │  Coturn   │
        └──┬─────┬──┘               └───────────┘
           │     │
   /api,/gateway│ rtc.example.com
           │     │
     ┌─────▼──┐ ┌▼──────────┐
     │ Tulpar │ │  LiveKit  │
     │ Server │ │  SFU      │
     └─┬───┬──┘ └─────┬─────┘
       │   │          │
  ┌────▼─┐ ┌▼────┐    │
  │ PG   │ │Redis│    │
  └──────┘ └─────┘    │
       │              │
  ┌────▼──────────────▼─────┐
  │ Object storage (local/S3)│
  └──────────────────────────┘
```

Recommended domains:

| Hostname | Purpose |
|---|---|
| `tulpar.example.com` | Web app, REST API, gateway, embed pages |
| `rtc.example.com` | LiveKit signaling (proxied by Caddy) |
| `turn.example.com` | Coturn TLS endpoint |
| `usercontent.example.com` | Optional separate origin for uploaded files |

One main origin simplifies cookies, CSP, CORS, and WebSocket authorization. The user-content origin isolates untrusted files.

---

## 3. Port plan

### 3.1 Public ports

| Port | Protocol | Service | Notes |
|---:|---|---|---|
| 80 | TCP | Caddy | Redirect to HTTPS; ACME HTTP challenge |
| 443 | TCP/UDP | Caddy | HTTPS, WSS, HTTP/3 |
| 3478 | UDP/TCP | Coturn | STUN/TURN |
| 5349 | TCP | Coturn | TURN over TLS |
| 50000–50999 | UDP | LiveKit | WebRTC media; size to expected concurrency |
| 51000–51999 | UDP | Coturn | TURN relay range |
| 7881 | TCP | LiveKit | WebRTC TCP fallback; publish only if needed |

The port ranges must not overlap. Each LiveKit participant typically consumes one UDP port; 1,000 ports support roughly 1,000 concurrent media sessions on that node, but CPU/bandwidth will limit first. Coturn relay ports are used only by relayed participants. Adjust both ranges to the host and firewall.

### 3.2 Internal ports

| Port | Service | Exposure |
|---:|---|---|
| 8080 | Tulpar API/gateway | Docker network only; Caddy proxies |
| 5432 | PostgreSQL | Docker network only; never published |
| 6379 | Redis | Docker network only; never published |
| 7880 | LiveKit signaling/API | Docker network; Caddy proxies signaling |
| 9100/9090/3000 | Monitoring | Docker network or admin VPN only |
| 9000/9001 | MinIO (optional) | Internal; console via admin tunnel only |

Do not publish PostgreSQL or Redis “for convenience”. If an operator needs remote access, use an SSH tunnel or VPN.

---

## 4. Services

| Service | Image (planned) | Persistent | Restart | Health |
|---|---|---|---|---|
| `caddy` | `caddy:2-alpine` | config, data, certs | unless-stopped | HTTP |
| `tulpar` | locally built Rust image | uploads if local storage | unless-stopped | `/health/ready` |
| `postgres` | `postgres:16-alpine` | `pgdata` | unless-stopped | `pg_isready` |
| `redis` | `redis:7-alpine` | optional AOF for tickets | unless-stopped | `redis-cli ping` |
| `livekit` | `livekit/livekit-server` pinned | config | unless-stopped | HTTP `/` |
| `coturn` | `coturn/coturn` pinned | config/certs | unless-stopped | TURN allocation test |
| `minio` (optional) | `minio/minio` pinned | `minio-data` | unless-stopped | `/minio/health/live` |
| `prometheus` (optional) | `prom/prometheus` | `prom-data` | unless-stopped | `/-/ready` |
| `grafana` (optional) | `grafana/grafana` | `grafana-data` | unless-stopped | `/api/health` |
| `loki` (optional) | `grafana/loki` | `loki-data` | unless-stopped | `/ready` |
| `backup` (optional) | small cron/restic image | backup target | scheduled | job status |

Production images are pinned by digest. The `tulpar` image runs as a non-root user with a read-only root filesystem except its data volume.

### 4.1 Development stack

Phase 1 creates a development Compose stack with PostgreSQL, Redis, LiveKit, and Coturn. Production hardening, monitoring, and backup automation are completed in Phase 9. The same Rust binary runs in both; only configuration differs.

---

## 5. Volumes and persistence

| Volume | Contents | Backup |
|---|---|---|
| `pgdata` | PostgreSQL data | Yes, logical + optional WAL |
| `redisdata` | Redis AOF/RDB if enabled | No; ephemeral |
| `uploads` | Local file storage | Yes, if used |
| `caddy_data` | Certificates, ACME state | Recommended |
| `caddy_config` | Caddy autosave | No |
| `livekit` | Config only | Config is in Git/templates |
| `coturn` | Config/certs | Config is in Git/templates |
| `minio-data` | S3 objects if using MinIO | Yes, via bucket replication/backup |
| `prom-data`, `grafana-data`, `loki-data` | Monitoring history | Optional |

Redis is not a durable message store. Losing it logs users out of gateway tickets, clears presence/typing, and may require re-login for refresh-ticket flows; messages remain in PostgreSQL.

Local uploads must be on a volume with enough disk and a backup path. S3/MinIO is recommended before adding multiple API nodes.

---

## 6. Configuration

Configuration is loaded from a TOML file plus environment overrides. Secrets come from environment variables or Docker secrets. `.env.example` contains placeholders only.

### 6.1 Core variables (planned names)

| Variable | Purpose | Secret |
|---|---|---|
| `TULPAR__PUBLIC_URL` | Canonical instance URL, used in emails and JWTs | No |
| `TULPAR__INSTANCE_NAME` | Display name | No |
| `TULPAR__BIND_ADDR` | API bind address (default `0.0.0.0:8080`) | No |
| `TULPAR__DATABASE_URL` | PostgreSQL URL | Yes |
| `TULPAR__REDIS_URL` | Redis URL | Yes |
| `TULPAR__JWT_KEYS` | JSON map of `kid` → signing secret | Yes |
| `TULPAR__JWT_ACTIVE_KID` | Active signing key | No |
| `TULPAR__EMBED_SIGNING_KEY` | Embed token key | Yes |
| `TULPAR__WEBHOOK_ENCRYPTION_KEY` | Webhook secret encryption | Yes |
| `TULPAR__SMTP_URL` | SMTP connection URL | Yes |
| `TULPAR__SMTP_FROM` | Sender address | No |
| `TULPAR__REGISTRATION_ENABLED` | Registration policy | No |
| `TULPAR__REQUIRE_EMAIL_VERIFICATION` | Verification policy | No |
| `TULPAR__STORAGE__PROVIDER` | `local`, `s3`, `minio` | No |
| `TULPAR__STORAGE__LOCAL_PATH` | Local upload path | No |
| `TULPAR__STORAGE__S3_*` | Bucket/endpoint/credentials | Partly |
| `TULPAR__LIVEKIT__URL` | Public LiveKit WSS URL | No |
| `TULPAR__LIVEKIT__API_KEY` | LiveKit key | Yes |
| `TULPAR__LIVEKIT__API_SECRET` | LiveKit secret | Yes |
| `TULPAR__TURN__URLS` | TURN URLs | No |
| `TULPAR__TURN__SECRET` | Shared Coturn secret | Yes |
| `TULPAR__CORS__ALLOWED_ORIGINS` | Web origins | No |
| `TULPAR__RATE_LIMIT__*` | Optional overrides | No |
| `TULPAR__LOG__FORMAT` | `json` or `pretty` | No |
| `TULPAR__METRICS__BIND_ADDR` | Internal metrics listener | No |

The exact schema is finalized in Phase 1 and documented in a generated configuration reference. Names above are the design intent, not a stable contract yet.

### 6.2 Secret handling

- Use Docker secrets or an environment file with `0600` permissions owned by the deploy user.
- Do not put secrets in `docker-compose.yml`, images, Git, shell history, or CI logs.
- Generate random secrets with a CSPRNG; never reuse between instances.
- Back up secrets separately from data, encrypted.
- Rotate the JWT key with `kid`; rotate LiveKit/TURN/SMTP/S3 through documented procedures.

---

## 7. Installation procedure (planned, Phase 9)

The following is the intended procedure. It is not tested yet and must not be presented as a working installer until Phase 9.

1. Provision Ubuntu Server 24.04 with a public IPv4 (and IPv6 if supported), SSH key access, and NTP.
2. Create a non-root deploy user; disable password SSH; enable the firewall.
3. Install Docker Engine and the Compose plugin from Docker's official repository.
4. Point DNS records for the app, RTC, TURN, and optional user-content hosts at the server.
5. Fetch a signed Tulpar release (source or images) and verify checksums/signatures.
6. Create the production environment file and secret files; restrict permissions.
7. Start PostgreSQL and Redis; wait for health.
8. Run the one-shot migration command.
9. Create the first instance administrator with the CLI (interactive password prompt).
10. Start the API, gateway, LiveKit, Coturn, and Caddy.
11. Verify `/health/live`, `/health/ready`, HTTPS, WebSocket ticket flow, login, message send, file upload, and a real voice join.
12. Configure backups and a restore drill.
13. Configure monitoring/alerts and log retention.
14. Record the instance URL, admin recovery procedure, and secret ownership.

Steps 5–13 become scripts in `scripts/` and `deployments/production/` in Phase 9. No script is considered complete until it has been run on a clean Ubuntu 24.04 host.

---

## 8. Upgrade procedure (planned)

1. Read the release notes and migration warnings.
2. Verify a recent backup exists and can be restored.
3. Optionally enable maintenance mode; announce the window.
4. Pull the new images by digest.
5. Run the migration job. It acquires an advisory lock and refuses to continue on failure.
6. Start the new API/gateway; keep old workers from processing the same outbox rows (they use `SKIP LOCKED`, but avoid mixed schema versions).
7. Verify health, login, message send, upload, WebSocket, and voice.
8. Leave maintenance mode.
9. Monitor errors and latency for at least 30 minutes.
10. Keep the previous images and backup until the next successful release.

Rollback:

- Application rollback is possible if the schema is backward compatible (expand/contract).
- Destructive migrations are not run automatically; rollback may require restoring the pre-upgrade backup.
- Never “roll back” by running untested down migrations in production.
- Document the exact rollback command in each release note.

---

## 9. Sizing guidance

These are starting points, not guarantees.

| Deployment | vCPU | RAM | Disk | Notes |
|---|---:|---:|---:|---|
| Development | 2 | 4 GB | 40 GB | All services, no media load |
| Small text community | 2 | 4 GB | 80 GB | Up to ~100 concurrent text users; voice limited |
| Target reference | 4 | 8 GB | 160 GB SSD | 200 text users / ~500 WS; validate voice separately |
| Media-heavy | 8+ | 16 GB+ | 200 GB+ | LiveKit on its own host preferred |

- PostgreSQL: 25% of RAM for `shared_buffers`; leave headroom for the OS and connections.
- Redis: `maxmemory` sized for presence/tickets/rate limits, not messages; `allkeys-lru` is acceptable for ephemeral data.
- API: 20–30 DB connections per node; more connections is not faster.
- LiveKit: CPU and bandwidth dominate. Test 100 voice participants and 20 video publishers; do not assume.
- Disk: attachments grow faster than text. Set quotas and monitor.
- Network: 1 Gbps is recommended for any real voice/video usage.

---

## 10. TLS and reverse proxy

Caddy:

- Automatic HTTPS for the app, RTC, TURN, and user-content hostnames.
- Proxies `/api/*`, `/gateway`, and `/embed/*` to the Tulpar server.
- Serves the built web app with SPA fallback.
- Proxies `rtc.example.com` to LiveKit's signaling port.
- Adds security headers (HSTS, CSP, `nosniff`, referrer policy, permissions policy).
- Enforces request body limits and timeouts.
- Does not proxy TURN media; Coturn is reached directly.

Coturn TLS uses a certificate mounted read-only. Certificate renewal must reload Coturn or use a renewal hook; this is validated in Phase 9.

WebSocket proxying must preserve upgrade headers, set a long read timeout, and not buffer. The gateway sends heartbeats; idle proxy timeouts must exceed the heartbeat interval.

---

## 11. Email

- Configure SMTP with TLS and a dedicated sender.
- Publish SPF, DKIM, and DMARC records for the sending domain.
- Use a transactional email provider or a well-configured local relay.
- Monitor bounces and delivery failures.
- Do not send passwords or tokens in logs.
- Verification/reset links use the configured public URL, never the request `Host` header.
- If email is unavailable, registration with required verification is delayed; the account is preserved.

---

## 12. Monitoring and logging

Recommended:

- Prometheus scrapes the internal Tulpar metrics listener, PostgreSQL exporter, Redis exporter, LiveKit metrics, and node exporter.
- Grafana dashboards: API latency/errors, DB pool, Redis, gateway connections, outbox lag, webhook failures, LiveKit participants, disk, TLS expiry.
- Loki or compatible log aggregation; JSON logs with request IDs.
- Alerts: readiness failure, 5xx rate, p95 latency, DB pool exhaustion, outbox backlog, webhook failure spike, disk > 80%, certificate expiry, LiveKit/TURN unreachable, backup failure.
- Audit logs are separate from application logs and have their own retention.

No mandatory telemetry leaves the instance. Monitoring is operator-controlled.

---

## 13. Hardening checklist

- [ ] SSH keys only; root login disabled; non-root deploy user.
- [ ] Firewall allows only 22 (restricted), 80, 443, 3478, 5349, LiveKit media, Coturn relay.
- [ ] PostgreSQL and Redis not published; strong passwords; no default users.
- [ ] Docker daemon access restricted; containers non-root, `no-new-privileges`, dropped capabilities.
- [ ] Secrets in secret files, `0600`, not in shell history.
- [ ] TLS enabled; HSTS after verification; certificate renewal tested.
- [ ] Security headers and CSP verified in a browser.
- [ ] Rate limits and body limits configured.
- [ ] Backups configured and restore tested.
- [ ] Log rotation and disk alerts configured.
- [ ] Unattended security upgrades enabled for the OS.
- [ ] NTP synchronized.
- [ ] Admin access restricted (VPN/allowlist where possible).
- [ ] Registration policy and email verification chosen deliberately.
- [ ] Public community discovery disabled unless intended.
- [ ] File quotas and allowed types reviewed.
- [ ] LiveKit/TURN ports tested from outside the network.
- [ ] Maintenance mode and upgrade procedure documented.
- [ ] Incident contact and secret rotation plan documented.

---

## 14. Troubleshooting

| Symptom | Likely cause | Check |
|---|---|---|
| `502` from Caddy | API not healthy | `docker compose ps`, API logs, `/health/ready` |
| WebSocket closes immediately | Ticket expired, Redis down, Origin mismatch | Gateway logs, Redis, ticket TTL, allowed origins |
| Voice joins but no audio | Mic permission, device, server mute, UDP blocked | Browser console, LiveKit room, firewall, TURN |
| Voice fails on corporate network | UDP blocked, TURN unreachable | Test TURN/TLS 5349, coturn logs |
| Screen share unavailable | WebView/browser/OS limitation | Capability detection, browser test, PipeWire portal |
| Uploads fail | Storage quota, MIME rejection, size limit, S3 credentials | API logs, storage health, quota, file type |
| Emails not delivered | SMTP/TLS/DNS/SPF/DKIM/DMARC | Mail logs, provider, DNS records |
| Migration fails | Schema drift, lock, disk | Migration logs, advisory lock, backup |
| Disk full | Attachments/logs/backups | `df`, volume usage, retention jobs |
| Presence wrong | Redis down or stale TTL | Redis health, gateway heartbeat, reconciler |
| Webhooks failing | Receiver down, signature mismatch, SSRF block | Delivery log, receiver logs, DNS/IP validation |

Logs must not include secrets. When asking for help, redact tokens, cookies, email addresses, and IPs.

---

## 15. Scaling path

1. **Tune the single host:** PostgreSQL, Redis, Caddy, disk, kernel limits.
2. **Separate LiveKit/Coturn:** move media to a dedicated host with a public IP and bandwidth.
3. **Separate workers:** run the binary with `worker` role for outbox/webhooks/email.
4. **Multiple API/gateway nodes:** put them behind Caddy/HAProxy; share PostgreSQL/Redis; use object storage; no sticky sessions required.
5. **PostgreSQL read replica:** for read-heavy discovery/search, with explicit staleness rules.
6. **LiveKit cluster:** distributed mode with Redis; shared TURN.
7. **Object storage:** S3/MinIO before multiple API nodes.
8. **Backup/monitoring scale:** dedicated host or managed services.

Do not add Kubernetes, a service mesh, or Kafka until measurements justify them.

---

## 16. Open deployment decisions

| Decision | Current position | Needed by |
|---|---|---|
| Primary domain names | Example placeholders only | Phase 1 |
| TLS/ACME | Caddy automatic | Phase 1 |
| Object storage default | Local for small installs; MinIO/S3 recommended | Phase 1/9 |
| SMTP provider | Operator choice; no bundled provider | Phase 1 |
| LiveKit host | Co-located initially; separate for media load | Phase 4 |
| TURN host | Co-located initially; separate for scale | Phase 4 |
| Monitoring stack | Optional Compose profile | Phase 9 |
| Backup tool | pg_dump + restic/S3; pgBackRest optional | Phase 9 |
| Container registry | Operator-hosted or GHCR; decide | Phase 9 |
| Release signing | Required for desktop; server images checksummed | Phase 9 |
| Commercial support | Not defined | Post-v1 |
