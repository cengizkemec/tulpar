# Deployments

- `development` — the Phase 1 local Compose stack for PostgreSQL, Redis, LiveKit, Coturn, and the API.
- `docker-compose` — shared Compose definitions and profiles.
- `production` — the Phase 9 production stack, environment templates, and runbooks.

The reference production target is Ubuntu Server 24.04 with Docker Engine and Compose v2. No Compose file is present yet; creating an untested stack would be misleading. See [docs/DEPLOYMENT.md](../docs/DEPLOYMENT.md) and [docs/BACKUP.md](../docs/BACKUP.md). Never commit `.env` files or secrets.
