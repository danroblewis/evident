#!/usr/bin/env bash
# .goalpost/bin/run-all.sh — refresh every compiler2 goalpost artifact.
# Expensive (hours at today's per-fixture compile speed); run from CI,
# a cron, or by hand. The measure scripts only read the artifacts.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$HERE/run-kernel-corpus.sh"
"$HERE/run-conformance.sh"
"$HERE/run-sample.sh"
"$HERE/run-selfhost.sh"
