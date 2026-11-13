#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="${TULPAR_COMPOSE_FILE:-$ROOT_DIR/deployments/production/docker-compose.yml}"
ENV_FILE="${TULPAR_ENV_FILE:-$ROOT_DIR/deployments/production/.env}"
BACKUP_DIR="${TULPAR_BACKUP_DIR:-/var/backups/tulpar}"
RETENTION_DAYS="${TULPAR_BACKUP_RETENTION_DAYS:-14}"
UPLOADS_VOLUME="${TULPAR_UPLOADS_VOLUME:-tulpar_uploads}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
TARGET="$BACKUP_DIR/$STAMP"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "missing environment file: $ENV_FILE" >&2
  exit 1
fi

mkdir -p "$TARGET"
echo "backing up PostgreSQL to $TARGET/postgres.dump"
docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" exec -T postgres \
  pg_dump -U tulpar -d tulpar --format=custom >"$TARGET/postgres.dump"

echo "backing up uploads volume to $TARGET/uploads.tar.gz"
docker run --rm \
  -v "$UPLOADS_VOLUME:/data:ro" \
  -v "$TARGET:/backup" \
  alpine:3.20 tar czf /backup/uploads.tar.gz -C /data .

(
  cd "$TARGET"
  sha256sum postgres.dump uploads.tar.gz >SHA256SUMS
)

find "$BACKUP_DIR" -mindepth 1 -maxdepth 1 -type d -mtime +"$RETENTION_DAYS" -exec rm -rf {} +

echo "backup complete: $TARGET"
