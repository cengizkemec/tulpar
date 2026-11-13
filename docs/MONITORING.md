# Monitoring and operations

**Status:** Baseline health checks, structured logs, and backup verification are implemented. Metrics dashboards and alert rules are planned.

## 1. Health checks

| Endpoint | Purpose |
|---|---|
| `/health/live` | Process is alive |
| `/health/ready` | PostgreSQL and Redis are ready |
| `/api/v1/version` | API version |

Docker Compose health checks call `/health/ready`. Caddy and the web container are
healthy when their upstream responds.

## 2. Logs

Production runs the API with `TULPAR__LOG__FORMAT=json` and `TULPAR__LOG_LEVEL=info`.
Collect container logs with the host's log driver or a shipper such as Vector,
Promtail, or Fluent Bit.

Useful commands:

```bash
docker compose --env-file deployments/production/.env logs -f --tail=200 api
docker compose --env-file deployments/production/.env ps
docker stats --no-stream
```

Never log request bodies, tokens, passwords, or private message content.

## 3. Baseline alerts

Configure alerts for:

- `/health/ready` failing for more than 2 minutes;
- container restart loops;
- disk usage above 80% on the database, uploads, and backup volumes;
- backup job missing or failing;
- TLS certificate renewal failure;
- unusual 5xx or 429 rates;
- Redis or PostgreSQL connection errors;
- LiveKit/TURN unreachable from the public network.

## 4. Backup verification

The backup script writes `SHA256SUMS`. Verify the latest backup and record the result:

```bash
cd /var/backups/tulpar/<timestamp>
sha256sum -c SHA256SUMS
pg_restore --list postgres.dump >/dev/null
```

Run a full restore drill quarterly and before major upgrades. See
[BACKUP.md](BACKUP.md).

## 5. Metrics

The API exposes a minimal Prometheus endpoint at `/metrics` on the internal API
listener. It reports process liveness and table/queue gauges:

- `tulpar_up`
- `tulpar_users`
- `tulpar_communities`
- `tulpar_messages`
- `tulpar_outbox_pending`
- `tulpar_webhook_pending`

Enable the optional Prometheus and Grafana services:

```bash
docker compose --env-file deployments/production/.env --profile monitoring up -d prometheus grafana
```

Prometheus listens on `127.0.0.1:9090` and scrapes `api:8080/metrics` over the internal
network. Grafana listens on `127.0.0.1:3000` with a provisioned `Tulpar overview`
dashboard and alert rules. Do not expose either port publicly. Request latency
histograms, gateway/voice metrics, and Alertmanager routing are still follow-ups.
