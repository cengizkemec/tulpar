# Production deployment

This directory contains a single-host production stack for Ubuntu Server 24.04:
PostgreSQL, Redis, LiveKit, Coturn, the Tulpar API, the web container, and Caddy for
TLS and security headers. The main app is frame-denied; `/embed/*` intentionally allows
framing for third-party widgets while keeping the token/origin allowlist server-side.

## 1. DNS and firewall

Create DNS records:

- `tulpar.example.com` → server IPv4/IPv6
- `livekit.tulpar.example.com` → server IPv4/IPv6

Allow inbound:

| Port | Protocol | Purpose |
|---|---|---|
| 80 | TCP | ACME HTTP challenge and redirect |
| 443 | TCP/UDP | HTTPS, HTTP/3, web app, API, gateway, LiveKit signaling |
| 7881 | TCP | LiveKit TCP fallback |
| 50000-50100 | UDP | LiveKit media |
| 3478 | TCP/UDP | Coturn STUN/TURN |
| 51000-51100 | UDP | Coturn relay |

Do not expose PostgreSQL or Redis publicly.

## 2. Configure

```bash
cd deployments/production
cp .env.example .env
chmod 600 .env
```

Fill every value. `TULPAR_CORS_ALLOWED_ORIGINS` must be a JSON array. Include the web
origin and the desktop origins:

```json
["https://tulpar.example.com","http://tauri.localhost","https://tauri.localhost","tauri://localhost"]
```

The SMTP URL uses the `smtps://user:password@host:465` form.

## 3. Start

```bash
docker compose --env-file .env up -d --build
docker compose --env-file .env ps
```

The API runs migrations at startup. Caddy obtains TLS certificates automatically.

## 4. Verify

```bash
curl -fsS https://tulpar.example.com/health/ready
curl -fsSI https://tulpar.example.com/ | head
```

Open the web app and register the first account, then create the first instance
administrator:

```bash
docker compose --env-file .env exec -T api tulpar-server admin create \
  --email admin@example.com --password-stdin <<< 'replace-with-a-strong-password'
```

## 4.1 Monitoring

Enable the optional Prometheus + Grafana monitoring profile:

```bash
docker compose --env-file .env --profile monitoring up -d prometheus grafana
```

Prometheus binds to `127.0.0.1:9090`, Grafana to `127.0.0.1:3000` (admin password from
`TULPAR_GRAFANA_PASSWORD`), and the provisioned `Tulpar overview` dashboard reads the
internal `/metrics` endpoint. Do not expose either port publicly. See
[../../docs/MONITORING.md](../../docs/MONITORING.md).

## 5. Backups

Run from the repository root:

```bash
sudo TULPAR_BACKUP_DIR=/var/backups/tulpar scripts/backup.sh
```

Schedule it daily with cron or a systemd timer. Restore with:

```bash
docker compose --env-file deployments/production/.env stop api web caddy
sudo scripts/restore.sh --yes /var/backups/tulpar/<timestamp>
docker compose --env-file deployments/production/.env start api web caddy
```

See [../../docs/BACKUP.md](../../docs/BACKUP.md) for retention and verification guidance.

## 6. Updates

```bash
git pull
docker compose --env-file .env build api web
docker compose --env-file .env up -d
```

Check `/health/ready` and the logs after every update. Database migrations are applied by
the API on startup; take a backup before upgrading.

## 7. Known production follow-ups

- Coturn TLS on 5349 with a certificate is not configured in this stack yet.
- Prometheus/Grafana dashboards and a metrics endpoint are not included yet.
- Load-test results and multi-node gateway fan-out are tracked in the roadmap.
- Desktop release signing and updater feeds are separate from this server stack.
