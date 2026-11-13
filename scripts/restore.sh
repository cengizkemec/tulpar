#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 || "$1" != "--yes" ]]; then
  echo "usage: $0 --yes <backup-directory>" >&2
  echo "This replaces the live database and uploads. Stop the stack first." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="${TULPAR_COMPOSE_FILE:-$ROOT_DIR/deployments/production/docker-compose.yml}"
ENV_FILE="${TULPAR_ENV_FILE:-$ROOT_DIR/deployments/production/.env}"
UPLOADS_VOLUME="${TULPAR_UPLOADS_VOLUME:-tulpar_uploads}"
SOURCE="$2"

if [[ ! -f "$SOURCE/postgres.dump" || ! -f "$SOURCE/uploads.tar.gz" ]]; then
  echo "backup is missing postgres.dump or uploads.tar.gz" >&2
  exit 1
fi

if [[ -f "$SOURCE/SHA256SUMS" ]]; then
  (cd "$SOURCE" && sha256sum -c SHA256SUMS)
fi

echo "restoring PostgreSQL from $SOURCE/postgres.dump"
docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" exec -T postgres \
  pg_restore -U tulpar -d tulpar --clean --if-exists --no-owner <"$SOURCE/postgres.dump"

echo "restoring uploads volume from $SOURCE/uploads.tar.gz"
docker run --rm \
  -v "$UPLOADS_VOLUME:/data" \
  -v "$SOURCE:/backup:ro" \
  alpine:3.20 sh -c 'rm -rf /data/* && tar xzf /backup/uploads.tar.gz -C /data'

echo "restore complete; start the API and web services again"
