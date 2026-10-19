#!/usr/bin/env bash
# Build the web UI and the hub, stop the running hub, start the new one.
#
# Usage: tools/restart.sh [--no-web] [--debug]
#   --no-web  skip `trunk build` (reuse crates/oikos-web/dist)
#   --debug   build and run the debug profile instead of release
#
# The hub runs detached from this shell, from the repo root (so it reads
# ./oikos.toml), and logs to target/oikos-os.log (OIKOS_LOG overrides).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE=release
BUILD_WEB=1
for arg in "$@"; do
  case "$arg" in
    --no-web) BUILD_WEB=0 ;;
    --debug) PROFILE=debug ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

BIN="$ROOT/target/$PROFILE/oikos-os"
LOG="${OIKOS_LOG:-$ROOT/target/oikos-os.log}"
BIND="$(sed -n 's/^bind *= *"\(.*\)"/\1/p' "$ROOT/oikos.toml" 2>/dev/null | head -1)"
HEALTH="http://${BIND:-127.0.0.1:8081}/health"

cargo_flags=()
[ "$PROFILE" = release ] && cargo_flags+=(--release)

# 1. Build the web UI first: the hub embeds dist/ at compile time.
if [ "$BUILD_WEB" = 1 ]; then
  echo "==> trunk build ($PROFILE)"
  (cd "$ROOT/crates/oikos-web" && trunk build "${cargo_flags[@]}")
fi

# 2. Build the hub.
echo "==> cargo build -p oikos-os ($PROFILE)"
(cd "$ROOT" && cargo build "${cargo_flags[@]}" -p oikos-os)

# 3. Stop the previous hub: SIGTERM, wait up to 10 s, then SIGKILL.
pids="$(pgrep -f 'oikos-os run' || true)"
if [ -n "$pids" ]; then
  echo "==> stopping previous hub (pid $pids)"
  kill $pids 2>/dev/null || true
  for _ in $(seq 1 20); do
    pgrep -f 'oikos-os run' >/dev/null || break
    sleep 0.5
  done
  if pgrep -f 'oikos-os run' >/dev/null; then
    echo "    still alive, sending SIGKILL"
    pkill -9 -f 'oikos-os run' || true
    sleep 0.5
  fi
fi

# 4. Launch the new one, detached, logging to $LOG.
echo "==> starting $BIN run (log: $LOG)"
mkdir -p "$(dirname "$LOG")"
cd "$ROOT"
nohup "$BIN" run >>"$LOG" 2>&1 < /dev/null &
NEW_PID=$!
disown "$NEW_PID" 2>/dev/null || true

# 5. Wait for /health.
for _ in $(seq 1 40); do
  if ! kill -0 "$NEW_PID" 2>/dev/null; then
    echo "hub exited early; last log lines:" >&2
    tail -n 20 "$LOG" >&2
    exit 1
  fi
  if curl -fsS --max-time 1 "$HEALTH" >/dev/null 2>&1; then
    echo "==> hub up (pid $NEW_PID) at $HEALTH"
    exit 0
  fi
  sleep 0.5
done
echo "hub (pid $NEW_PID) did not answer $HEALTH within 20 s; last log lines:" >&2
tail -n 20 "$LOG" >&2
exit 1
