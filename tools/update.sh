#!/usr/bin/env bash
# Upgrade a Docker install: pull the latest code, then rebuild and restart the hub.
#
# Usage: tools/update.sh
#
# Runs from the repo root whatever the current directory, so compose.yml
# and ./oikos.toml are the ones next to this checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> git pull --rebase"
git pull --rebase

echo "==> docker compose up -d --build"
docker compose up -d --build
