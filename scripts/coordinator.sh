#!/usr/bin/env bash
# scripts/coordinator.sh — spawn isolated Claude Code sessions
#
# Usage:
#   scripts/coordinator.sh launch <task-spec.md> [worktree-branch]
#   scripts/coordinator.sh status
#   scripts/coordinator.sh wait <task-name>
#   scripts/coordinator.sh tail <task-name>
#
# Each task gets a fresh `claude -p` invocation with its own context
# window. The parent coordinator (you) sees only status + result
# file paths, NOT the full session transcript. This is the only way
# to survive context exhaustion on a project this size.
#
# Result tree:
#   scripts/coordinator-results/<task-name>/
#     prompt.md       — what was sent to the session
#     stdout.log      — full session stdout (the agent's reply text)
#     pid             — background process id (while running)
#     status          — running|done|failed
#     worktree-branch — git branch the session worked on (if any)
#
# TODO: rewrite in Evident.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

RESULTS=scripts/coordinator-results
mkdir -p "$RESULTS"

cmd_launch() {
  local task_spec=${1:?usage: launch <task-spec.md> [branch]}
  local branch=${2:-}
  local name
  name=$(basename "$task_spec" .md)
  local dir="$RESULTS/$name"
  mkdir -p "$dir"

  if [ ! -f "$task_spec" ]; then
    echo "coordinator: task spec not found: $task_spec" >&2
    exit 2
  fi

  # Assemble the prompt: foundation briefing + freeze rules + task
  {
    echo "# Subordinate session briefing"
    echo
    echo "You are a subordinate Claude Code session spawned by a coordinator."
    echo "The coordinator has limited context budget and depends on you to"
    echo "complete this task with maximum independence."
    echo
    echo "## REQUIRED FIRST ACTIONS"
    echo
    echo "1. Read /Users/danroblewis/evident/CLAUDE.md fully."
    echo "2. Read /Users/danroblewis/evident/bootstrap/READ-ME-FIRST.md."
    echo "3. Read /Users/danroblewis/evident/docs/briefings/foundation.md."
    echo "4. Run \`bash /Users/danroblewis/evident/scripts/check-deletable.sh\` and read its output."
    echo
    echo "Do all four before you write a single character of code."
    echo "These tell you the project's state and the constraints you must operate under."
    echo
    echo "## YOUR TASK"
    echo
    cat "$task_spec"
    echo
    echo "## REPORTING BACK"
    echo
    echo "At the end of your session, print a self-contained report:"
    echo
    echo "- Branch name pushed to origin (if you committed)."
    echo "- Files added / modified / deleted (paths only)."
    echo "- The output of \`bash /Users/danroblewis/evident/scripts/check-deletable.sh\`"
    echo "  AFTER your changes."
    echo "- Anything you tried and abandoned, with a one-line explanation."
    echo "- Any docs/plans/blocked-<topic>.md you wrote (if you got blocked)."
    echo
    echo "Do NOT print a full diff. Do NOT describe every file's contents."
    echo "The coordinator can read the files directly; you save context by"
    echo "being terse and citing paths."
  } > "$dir/prompt.md"

  if [ -n "$branch" ]; then
    echo "$branch" > "$dir/worktree-branch"
  fi

  echo "running" > "$dir/status"

  # Spawn claude -p in background, dangerously skip permissions
  # so the session can write files without prompting.
  nohup claude -p "$(cat "$dir/prompt.md")" \
    --dangerously-skip-permissions \
    > "$dir/stdout.log" 2>&1 &
  local pid=$!
  echo "$pid" > "$dir/pid"

  echo "Launched: $name (pid $pid)"
  echo "  prompt: $dir/prompt.md"
  echo "  result: $dir/stdout.log"
  echo "  status: $dir/status"
}

cmd_status() {
  if ! ls "$RESULTS"/*/status >/dev/null 2>&1; then
    echo "(no coordinator tasks)"
    return
  fi
  printf "%-40s  %-8s  %s\n" "TASK" "STATUS" "PID/EXIT"
  for d in "$RESULTS"/*/; do
    local name; name=$(basename "$d")
    local status; status=$(cat "$d/status" 2>/dev/null || echo "?")
    local pid_or_exit=""
    if [ "$status" = "running" ]; then
      local pid; pid=$(cat "$d/pid" 2>/dev/null || echo "?")
      if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        pid_or_exit="pid $pid"
      else
        # Process is gone; figure out exit and update status
        pid_or_exit="(finished)"
        echo "done" > "$d/status"
        status=done
      fi
    fi
    printf "%-40s  %-8s  %s\n" "$name" "$status" "$pid_or_exit"
  done
}

cmd_wait() {
  local name=${1:?usage: wait <task-name>}
  local dir="$RESULTS/$name"
  if [ ! -d "$dir" ]; then
    echo "coordinator: no such task: $name" >&2
    exit 2
  fi
  local pid; pid=$(cat "$dir/pid" 2>/dev/null || true)
  if [ -z "$pid" ]; then
    echo "coordinator: no pid for $name; perhaps already finished"
    return
  fi
  echo "waiting on $name (pid $pid)..."
  wait "$pid" 2>/dev/null || true
  echo "done" > "$dir/status"
  echo "  $name finished. Output: $dir/stdout.log"
}

cmd_tail() {
  local name=${1:?usage: tail <task-name>}
  local f="$RESULTS/$name/stdout.log"
  if [ ! -f "$f" ]; then
    echo "coordinator: no log for $name" >&2
    exit 2
  fi
  tail -50 "$f"
}

case "${1:-}" in
  launch) shift; cmd_launch "$@" ;;
  status) cmd_status ;;
  wait)   shift; cmd_wait "$@" ;;
  tail)   shift; cmd_tail "$@" ;;
  *)
    cat >&2 <<EOF
usage: $0 <subcommand> [args]

  launch <task-spec.md>    spawn a session for the task
  status                   list all sessions and their state
  wait <task-name>         wait for a session to finish
  tail <task-name>         tail the session log
EOF
    exit 2
    ;;
esac
