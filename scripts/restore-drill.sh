#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <backup-directory>" >&2
  exit 1
fi

SOURCE="$1"
CONTAINER="tulpar-restore-drill-$$"
PORT="${TULPAR_DRILL_PORT:-15433}"

if [[ ! -f "$SOURCE/postgres.dump" ]]; then
  echo "missing $SOURCE/postgres.dump" >&2
  exit 1
fi
if [[ -f "$SOURCE/SHA256SUMS" ]]; then
  (cd "$SOURCE" && sha256sum -c SHA256SUMS)
fi

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker run -d --rm --name "$CONTAINER" \
  -e POSTGRES_PASSWORD=drill \
  -p "127.0.0.1:$PORT:5432" \
  postgres:16 >/dev/null

for _ in $(seq 1 30); do
  if docker exec -e PGPASSWORD=drill "$CONTAINER" pg_isready -U postgres >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

docker exec -e PGPASSWORD=drill "$CONTAINER" createdb -U postgres tulpar
docker exec -e PGPASSWORD=drill "$CONTAINER" psql -U postgres -d tulpar \
  -c "CREATE EXTENSION IF NOT EXISTS citext; CREATE EXTENSION IF NOT EXISTS pg_trgm;"
docker cp "$SOURCE/postgres.dump" "$CONTAINER:/tmp/postgres.dump"
docker exec -e PGPASSWORD=drill "$CONTAINER" \
  pg_restore -U postgres -d tulpar --no-owner /tmp/postgres.dump

users="$(docker exec -e PGPASSWORD=drill "$CONTAINER" psql -U postgres -d tulpar -tAc 'SELECT count(*) FROM users')"
communities="$(docker exec -e PGPASSWORD=drill "$CONTAINER" psql -U postgres -d tulpar -tAc 'SELECT count(*) FROM communities')"
messages="$(docker exec -e PGPASSWORD=drill "$CONTAINER" psql -U postgres -d tulpar -tAc 'SELECT count(*) FROM messages')"

echo "restore drill ok: users=$users communities=$communities messages=$messages"
echo "The temporary PostgreSQL container will be removed."
