#!/usr/bin/env bash
# .goalpost/bin/run-all.sh — refresh every compiler2 goalpost artifact.
# Expensive (hours at today's per-fixture compile speed); run from CI,
# a cron, or by hand. The measure scripts only read the artifacts.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$HERE/run-conformance.sh"
"$HERE/run-selfcompile-sweep.sh"   # selfcompile.sh measure (minutes)
"$HERE/run-invariant-gate.sh"      # carried_invariants.sh measure (~15s)
"$HERE/run-selfhost.sh"
