#!/usr/bin/env bash
# Upgrade a Docker install: pull the latest code, move a legacy ./oikos.toml
# and ./secrets/ into ./data/, then rebuild and restart.
#
# Usage: tools/update.sh [--hub-only|--all]
#   --hub-only  rebuild and restart only the hub (you run your own broker and database)
#   --all       rebuild and restart the whole compose stack (hub, Mosquitto, TimescaleDB)
#
# Without a flag the script picks: the whole stack when data/bundled.json or
# compose.override.yml exists, the hub only when data/oikos.toml or
# data/hub.json exists (own broker and database), the whole stack otherwise.
#
# Runs from the repo root whatever the current directory, so compose.yml and
# ./data are the ones next to this checkout.
set -euo pipefail

# Everything lives in main so bash has read the whole script before
# `git pull` rewrites this file; the pulled version then runs the rest.
main() {
  ROOT="$(cd "$(dirname "$0")/.." && pwd)"
  cd "$ROOT"

  if [ "${1:-}" != "--pulled" ]; then
    echo "==> git pull --rebase"
    git pull --rebase
    exec "$ROOT/tools/update.sh" --pulled "$@"
  fi
  shift

  mode=""
  for arg in "$@"; do
    case "$arg" in
      --hub-only) mode=hub ;;
      --all) mode=all ;;
      *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
  done

  migrate_legacy_files
  warn_old_key_paths

  reason=""
  if [ -n "$mode" ]; then
    reason="requested on the command line"
  elif [ -e data/bundled.json ] || [ -e compose.override.yml ]; then
    mode=all
    reason="bundled stack or compose.override.yml found"
  elif [ -e data/oikos.toml ] || [ -e data/hub.json ]; then
    mode=hub
    reason="own broker and database"
  else
    mode=all
    reason="fresh install"
  fi

  if [ "$mode" = hub ]; then
    echo "==> $reason: rebuilding the hub only"
    echo "==> docker compose up -d --build hub"
    docker compose up -d --build hub
  else
    echo "==> $reason: rebuilding the whole stack"
    echo "==> docker compose up -d --build"
    docker compose up -d --build
  fi

  echo "==> docker compose logs --tail 20 hub"
  docker compose logs --tail 20 hub
}

# Old installs kept ./oikos.toml and ./secrets/*; the hub now reads ./data/.
# Never overwrite anything already in ./data/.
migrate_legacy_files() {
  if [ -f oikos.toml ]; then
    if [ -e data/oikos.toml ]; then
      echo "data/oikos.toml already exists; ./oikos.toml left in place"
    else
      mkdir -p data
      mv oikos.toml data/oikos.toml
      echo "moved oikos.toml -> data/oikos.toml"
    fi
  fi

  if [ -d secrets ]; then
    for f in secrets/*; do
      [ -f "$f" ] || continue
      name="$(basename "$f")"
      [ "$name" = README.md ] && continue
      if [ -e "data/secrets/$name" ]; then
        echo "data/secrets/$name already exists; $f left in place"
      else
        mkdir -p data/secrets
        mv "$f" "data/secrets/$name"
        echo "moved $f -> data/secrets/$name"
      fi
    done
  fi
}

warn_old_key_paths() {
  if [ -f data/oikos.toml ] && grep -q '/etc/oikos/secrets' data/oikos.toml; then
    echo "WARNING: data/oikos.toml names key files under /etc/oikos/secrets; change them to /data/secrets/... or delete those lines (the defaults find data/secrets/)" >&2
  fi
}

main "$@"
