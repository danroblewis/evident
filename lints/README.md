# Lint framework

Mechanical and agent-driven code reviews against a shared rulebook.

```
lints/
  README.md              this file
  agent-prompt.md        the brief for the code-review subagent
  rules/                 the rulebook — one markdown per anti-pattern
    AP-NNN-<name>.md
  checks.sh              mechanical runner; loads active rules,
                         applies their grep patterns, exits non-zero
                         on violation. Wired into test.sh as Phase 0.
  findings/              where agent runs deposit per-file reports.
                         Reviewed by humans; deletable once acted on.

runtime/tests/lints.rs   AST-based rules that need a real parser
                         (cross-file invariants, structural checks)
```

## How a rule is born

Two paths, same shape:

1. **Human or agent observes a recurring mistake.** They write a new
   `lints/rules/AP-NNN-<short-name>.md` describing it, then add the
   detection to `checks.sh` (grep) or `runtime/tests/lints.rs` (AST).
   Run `./test.sh --lints-only` to confirm the check fires on the
   offending file. Optionally fix existing offenders or document
   them as known violations.

2. **Code-review subagent surfaces a candidate pattern.** When run
   on a file, the agent reports findings in `lints/findings/`. If
   it spots something that looks like a real anti-pattern (not a
   one-off issue), it proposes a new rule by writing the rule file
   and editing `checks.sh`. Human reviews + commits or rejects.

## Rule file format

Every file in `lints/rules/` follows this template so both humans
and the agent can read/write them:

```markdown
# AP-NNN: <short-name>

**Status:** active | proposed | warn-only

**Pattern.** What to look for in concrete terms — symbol, syntax
shape, file location.

**Why.** Why it's bad. Cite a real past mistake if you have one.

**Fix.** What to do instead.

**Detection.** grep | ast | review-only

**Pattern (grep).** A regex literal, or a description if review-only.

**Scope.** Which paths the rule applies to (whitelist / blacklist).

**Exceptions.** Cases where a violation is acceptable (e.g.,
strings inside comments don't count).

**Examples.**
  - Past commit / file-line where this bit us.
  - The seed entries in this directory cite the SdlVertex
    intrusion in `ast.rs`, the inline `LibCall` in
    `examples/test_16_sdl_red.ev`'s first draft, etc.
```

## How `checks.sh` works

Each active rule has a corresponding shell function in `checks.sh`
named `check_<short-name>`. The runner enumerates these functions,
runs them, prints a per-rule pass/fail line, and exits 1 if any
failed. New rule = new function in `checks.sh` (it's not auto-loaded
from the rule file because grep patterns benefit from being
hand-tuned, and we want each check to be runnable / debuggable on
its own).

The `lints/rules/AP-NNN-*.md` file is the prose; the `check_*`
function in `checks.sh` is the executable form. Convention: the
shell function's first comment line cites the rule by ID.

## How the code-review agent runs

The agent's standing brief lives in `lints/agent-prompt.md`. To run
a review, an outer agent (typically Claude in an interactive
session) launches a subagent with that prompt + the file(s) to
review. The subagent:

1. Reads `lints/rules/` to learn the active rulebook.
2. Reads the assigned file.
3. Writes findings to `lints/findings/<basename-of-file>.md` in a
   fixed format (see agent-prompt.md).
4. If it identifies a new anti-pattern, writes a new rule file under
   `lints/rules/` AND adds a `check_*` function to `checks.sh`.
5. Reports back a brief summary.

Findings accumulate. A human (or the outer agent) walks them, fixes
or dismisses each, and deletes the corresponding `lints/findings/`
file when done.
