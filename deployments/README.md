# Deployments

- `development` — a verified Docker Compose stack with PostgreSQL, Redis, LiveKit, Coturn, and the Tulpar API.
- `docker-compose` — shared Compose conventions and future production profiles.
- `production` — the Phase 9 production stack, environment templates, and runbooks.

The reference production target is Ubuntu Server 24.04 with Docker Engine and Compose v2. The development stack was verified from a local, non-OneDrive build context because Docker Desktop cannot read the current OneDrive source path directly.

## Development stack

From the repository root:

```bash
cp deployments/development/.env.example deployments/development/.env
# replace every placeholder with a random development-only value
docker compose --env-file deployments/development/.env \
  -f deployments/development/docker-compose.yml up -d --build
curl http://127.0.0.1:18080/health/ready
docker compose --env-file deployments/development/.env \
  -f deployments/development/docker-compose.yml down
```

Never commit the populated `.env` file. The development Compose secrets are not production credentials. See [docs/DEPLOYMENT.md](../docs/DEPLOYMENT.md) and [docs/BACKUP.md](../docs/BACKUP.md).
