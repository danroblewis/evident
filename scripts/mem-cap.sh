#!/usr/bin/env bash
# mem-cap.sh — run a command; SIGKILL it if RSS exceeds a cap.
#
# Usage:
#   MEM_CAP_MB=3000 scripts/mem-cap.sh kernel compiler.smt2 < src
#
# Default cap: ${EVIDENT_MEM_CAP_MB:-3000} MB. Exit 137 + a stderr line
# if the watchdog kills the child; otherwise transparent (exit code +
# stdout/stderr of the child are preserved).
#
# Why this exists: macOS does not enforce RLIMIT_AS (`ulimit -v`), so
# runaway Z3/kernel processes can swap the whole machine. This wrapper
# polls `ps -o rss=` every 0.5 s and kills the child when it grows past
# the cap. Use it around kernel invocations in lang/kernel test phases.

set -u -o pipefail

CAP_MB="${MEM_CAP_MB:-${EVIDENT_MEM_CAP_MB:-3000}}"
CAP_KB=$(( CAP_MB * 1024 ))

"$@" &
pid=$!

cleanup() { kill -TERM $pid 2>/dev/null; }
trap cleanup INT TERM

while kill -0 $pid 2>/dev/null; do
    rss_kb=$(ps -o rss= -p $pid 2>/dev/null | tr -d ' ')
    if [ -n "$rss_kb" ] && [ "$rss_kb" -gt "$CAP_KB" ]; then
        kill -KILL $pid 2>/dev/null
        echo "mem-cap: killed pid $pid (RSS ${rss_kb}KB > cap ${CAP_MB}MB)" >&2
        wait $pid 2>/dev/null
        exit 137
    fi
    sleep 0.5
done

wait $pid
exit $?
