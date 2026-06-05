#!/usr/bin/env bash
# mem-cap.sh — run a command; SIGKILL it if RSS exceeds a cap.
#
# Usage:
#   MEM_CAP_MB=8000 scripts/mem-cap.sh kernel compiler.smt2 < src
#
# Default cap: ${EVIDENT_MEM_CAP_MB:-12000} MB. The kernel running
# compiler.smt2 routinely needs 3-6 GB per process for Z3 state on
# real fixtures (the smoke fixture peaks around 4 GB), so the cap is
# 12 GB by default to protect against runaways without truncating
# legitimate compiles. Tune down only when you know the fixture is
# small. Exit 137 + a stderr line
# if the watchdog kills the child; otherwise transparent (exit code +
# stdout/stderr of the child are preserved).
#
# Why this exists: macOS does not enforce RLIMIT_AS (`ulimit -v`), so
# runaway Z3/kernel processes can swap the whole machine. This wrapper
# polls `ps -o rss=` every 0.5 s and kills the child when it grows past
# the cap. Use it around kernel invocations in lang/kernel test phases.

set -u -o pipefail

CAP_MB="${MEM_CAP_MB:-${EVIDENT_MEM_CAP_MB:-12000}}"
CAP_KB=$(( CAP_MB * 1024 ))

# `cmd &` redirects stdin from /dev/null for the backgrounded job in
# bash's default behavior — so without the explicit `<&0`, a child
# that reads stdin (e.g. kernel's ReadLine) sees instant EOF and
# silently produces a truncated program. Forward our own stdin
# explicitly.
"$@" <&0 &
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
