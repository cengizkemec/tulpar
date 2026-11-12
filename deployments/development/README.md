# Development Docker stack

This stack runs PostgreSQL 16, Redis 7, LiveKit, Coturn, and the Tulpar API. Images are pinned by digest.

## Start

1. Copy `.env.example` to `.env`.
2. Replace `TULPAR_POSTGRES_PASSWORD`, `TULPAR_DEV_JWT_SECRET`, `TULPAR_LIVEKIT_SECRET`, and `TULPAR_TURN_PASSWORD` with random development-only values.
3. Run from the repository root:

```bash
docker compose --env-file deployments/development/.env \
  -f deployments/development/docker-compose.yml up -d --build
```

The API is exposed on port `18080` by default. PostgreSQL, Redis, LiveKit, and Coturn ports are defined in the Compose file.

## Verify

```bash
curl http://127.0.0.1:18080/health/ready
curl http://127.0.0.1:18080/api/v1/openapi.json
```

## Stop

```bash
docker compose --env-file deployments/development/.env \
  -f deployments/development/docker-compose.yml down
```

The PostgreSQL volume is preserved unless `-v` is used.

## Limitations

- This is a development stack, not a production deployment.
- Docker Desktop could not read the repository directly from the current OneDrive path. Run from a local copy outside OneDrive, or move the repository.
- The LiveKit/Coturn configuration is for local testing. Production requires public IP/TLS configuration and pinned, tested versions.
