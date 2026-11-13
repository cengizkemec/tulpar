#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="${TULPAR_COMPOSE_FILE:-$ROOT_DIR/deployments/production/docker-compose.yml}"
ENV_FILE="${TULPAR_ENV_FILE:-$ROOT_DIR/deployments/production/.env}"

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required. Install Docker Engine and the Compose plugin first." >&2
  exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
  echo "The Docker Compose plugin is required." >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  cp "$ROOT_DIR/deployments/production/.env.example" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  echo "Created $ENV_FILE from the example. Fill every value, then re-run." >&2
  exit 1
fi

docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" up -d --build
docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" ps
echo "Tulpar is starting. Verify https://<TULPAR_DOMAIN>/health/ready."
