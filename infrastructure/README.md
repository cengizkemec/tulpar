# Infrastructure

Service configuration and deployment assets:

- `docker` — Dockerfiles and shared container conventions.
- `postgres` — init scripts, tuning, and migration notes.
- `redis` — configuration and eviction policy notes.
- `livekit` — SFU configuration templates.
- `coturn` — TURN/STUN configuration and certificate notes.
- `caddy` — reverse proxy, TLS, and security headers.
- `monitoring` — Prometheus, Grafana, Loki, and alert rules.

Configuration templates and secrets are separate. Never commit real credentials, private keys, or production environment files. No service configuration exists yet; Phase 1 creates the development stack and Phase 9 the production stack. See [docs/DEPLOYMENT.md](../docs/DEPLOYMENT.md).
