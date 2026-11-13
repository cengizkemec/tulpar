# Phase 9 Report — Production hardening

**Status:** Production deployment artifacts, runbooks, a Prometheus/Grafana monitoring profile, alert rules, and HTTP/k6 load harnesses are implemented. Clean-host installation, restore drills, Alertmanager routing, WebSocket/media load tests, release signing, and external security review remain pending.

**Date:** 2026-11-13

---

## 1. Implemented

### Production stack

- `deployments/production/docker-compose.yml` runs PostgreSQL, Redis, LiveKit, Coturn, the API, the web container, and Caddy.
- Caddy terminates TLS, enables HTTP/3, applies HSTS, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, and a restrictive `Permissions-Policy`.
- Caddy proxies the main domain to the web container and `livekit.<domain>` to LiveKit.
- PostgreSQL and Redis are internal-only; only Caddy, LiveKit media, and Coturn are exposed.
- Redis runs with append-only persistence.
- The API runs with production settings: secure cookies, SMTP email, JSON logs, trusted proxy headers, and production CORS origins.
- `deployments/production/.env.example` documents every required secret and setting.
- `deployments/production/README.md` covers DNS, firewall ports, startup, verification, updates, and first-admin creation.

### Backup and restore

- `scripts/backup.sh` creates a custom-format PostgreSQL dump and a compressed uploads volume archive, writes SHA-256 checksums, and prunes old backups.
- `scripts/restore.sh` verifies checksums, restores the database with `pg_restore --clean --if-exists`, and replaces the uploads volume.
- `docs/BACKUP.md` documents retention and verification.

### Monitoring baseline

- The API exposes `/metrics` with liveness and table/queue gauges.
- An optional `monitoring` Compose profile runs Prometheus on `127.0.0.1:9090` and scrapes the internal API endpoint.
- Health checks, JSON logs, and backup verification remain the primary operational signals.

### CI and security posture

- CI already runs `cargo audit` and gitleaks.
- The desktop shell is compiled and linted in a Windows CI job.
- The web typecheck/build and embed SDK build run in CI.
- Security headers are applied at the edge; the API keeps its body limits, timeouts, rate limits, CORS allowlist, and origin checks.

---

## 2. Verification performed

- `cargo check -p tulpar-api` passes.
- `cargo test -p tulpar-api --test embed_flow --no-run` passes.
- `pnpm typecheck` and `pnpm build:embed-sdk` pass.
- Production Compose and Caddy files are syntactically reviewed; a clean-host install is not yet recorded.

---

## 3. Known gaps

1. Clean Ubuntu 24.04 installation and firewall verification are not recorded.
2. The backup/restore drill on a clean host is pending.
3. Grafana dashboards and Prometheus alert rules are provisioned; Alertmanager routing, request-latency histograms, and gateway/voice metrics are still pending.
4. Load tests, soak tests, and multi-node gateway fan-out are not implemented.
5. Coturn TLS on 5349 is not configured.
6. Image signing, checksums, and a release workflow are not implemented.
7. Desktop code signing and signed updater feeds are not configured.
8. Incident-response and disaster-recovery runbooks are incomplete.

---

## 4. Next steps

1. Install on a clean Ubuntu 24.04 host and record the firewall/TLS/health checks.
2. Run a full backup → destroy → restore drill.
3. Add Alertmanager routing and richer latency/gateway metrics.
4. Add WebSocket and LiveKit media load tests.
5. Configure Coturn TLS, image signing, and signed desktop releases.
