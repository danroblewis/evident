#!/usr/bin/env bash
# Supervised launch for the Evident web IDE — restart the server if it dies, so the solver
# backend never silently vanishes mid-session (Ana #202). Use this instead of running
# server.py directly when you want the IDE to stay up.
#
#   ./ide/web/run.sh
#
cd "$(dirname "$0")/../.." || exit 1
while true; do
    python3 ide/web/server.py
    code=$?
    echo "[run.sh] server exited (code $code) — restarting in 1s…" >&2
    sleep 1
done
