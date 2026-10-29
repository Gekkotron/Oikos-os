#!/usr/bin/env bash
# Proves the bundled stack with the real images: compose.yml parses, the hub
# does not depend on the bundled services, and Mosquitto and TimescaleDB
# start on the credentials init-bundled wrote and enforce them.
# Run from the repo root on a Linux host with Docker, jq and cargo.
set -euo pipefail

cd "$(dirname "$0")/../.."

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    docker compose logs mosquitto timescaledb || true
  fi
  docker compose down -v || true
  rm -f compose.override.yml
  exit "$status"
}
trap cleanup EXIT

echo "== compose file"
docker compose config -q
config=$(docker compose config --format json)
echo "$config" | jq -e '.services.hub.depends_on == null' >/dev/null
echo "$config" | jq -e '.services | has("init") and has("mosquitto") and has("timescaledb")' >/dev/null

echo "== init-bundled"
cargo build -p oikos-os
target/debug/oikos-os --data ./data init-bundled

# The migration guide's override shape: a second port for Postgres.
cat > compose.override.yml <<'YAML'
services:
  timescaledb:
    command: postgres -c port=5433
YAML

# --no-deps: init already ran natively; starting it would build the hub image.
docker compose up -d --no-deps mosquitto timescaledb

mqtt_password=$(jq -er .mqtt_password data/bundled.json)
db_password=$(jq -er .db_password data/bundled.json)

echo "== timescaledb"
ready=0
for _ in $(seq 1 60); do
  if docker exec oikos-timescaledb pg_isready -h localhost -p 5433 >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
[ "$ready" -eq 1 ] || { echo "timescaledb not ready after 60 s" >&2; exit 1; }

extension=$(docker exec oikos-timescaledb psql \
  "postgresql://oikos:${db_password}@localhost:5433/oikos" \
  -tAc "select extname from pg_extension where extname='timescaledb'")
[ "$extension" = "timescaledb" ] || { echo "timescaledb extension missing: '$extension'" >&2; exit 1; }

echo "== mosquitto"
docker exec oikos-mosquitto mosquitto_pub -h localhost \
  -u oikos -P "$mqtt_password" -t oikos/ci -m ok
if docker exec oikos-mosquitto mosquitto_pub -h localhost \
  -u oikos -P wrong -t oikos/ci -m ok >/dev/null 2>&1; then
  echo "mosquitto accepted a wrong password" >&2
  exit 1
fi

echo "stack ok"
