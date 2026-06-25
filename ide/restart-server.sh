#!/usr/bin/env bash
# restart-server.sh — restart the Evident web-IDE server (ide/web/server.py).
#
# Usage:   ./ide/restart-server.sh          (from the repo root, or anywhere)
#
# The cloudflared tunnel (→ localhost:5173) is a SEPARATE process and stays up
# across a server restart, so your public tunnel URL keeps working — this only
# bounces the local Python server. If the TUNNEL itself is down, restart that
# separately (a new `cloudflared tunnel --url` gives a fresh random URL).

set -uo pipefail                              # not -e: pkill's "no match" must not abort
cd "$(dirname "$0")/.." || exit 1             # repo root (script lives in ide/)

PORT=5173
LOG=/tmp/evident-ide-server.log

echo "Stopping any running IDE server…"
pkill -f "ide/web/server.py" 2>/dev/null || true
sleep 1

echo "Starting IDE server…"
nohup python3 ide/web/server.py > "$LOG" 2>&1 &
echo "  started pid $!   (log: $LOG)"

for _ in $(seq 1 25); do
  if curl -sf -o /dev/null --max-time 2 "http://localhost:$PORT/"; then
    echo "✓ IDE server up: http://localhost:$PORT/"
    exit 0
  fi
  sleep 1
done

echo "✗ server didn't come up in 25s — check: tail $LOG"
exit 1
