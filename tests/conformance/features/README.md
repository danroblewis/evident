# Conformance features — implementation-agnostic spec corpus

Each subdirectory here defines **one language capability** as an
input/output spec, independent of *which* compiler produces the
output. That independence is the whole point: it lets us compile the
same spec with the **bootstrap** compiler today and the
**self-hosted** compiler (`kernel + compiler.smt2`) once it exists,
and ask mechanically — *do both produce equivalent output?*

This is the strangler-fig scaffold for the self-hosting transition
(see the repo `CLAUDE.md`). When every feature here passes under both
`IMPL=bootstrap` and `IMPL=selfhost`, the self-hosted compiler is
feature-complete and `bootstrap/` can be deleted.

The legacy Python conformance tests (`tests/conformance/test_*.py`)
stay frozen and keep running during the transition; features are
migrated *out* of them into this directory over time.

## A feature directory

```
001-int-arithmetic-add/
  source.ev               # the Evident program (an emittable claim)
  claim.txt               # name of the top-level claim to compile (e.g. "main")
  expected/
    smt2-contains         # required: substrings that must appear in the .smt2
    stdout                # optional: kernel stdout when the .smt2 is run
    exit                  # optional: kernel exit code (default 0 when run)
```

### `source.ev`

A complete Evident program. Because the runner compiles via
`evident emit`, the program must be **kernel-runnable**: it declares a
single top-level `effects ∈ Seq(Effect)` constraint (import
`stdlib/kernel.ev` for `LibCall` / `Exit` / `ArgStr` etc.). Keep each
spec focused on one capability — one binop, one literal kind, one enum
+ match — so a failure points at exactly one feature.

### `claim.txt`

The name of the claim to compile (the argument passed to
`evident emit <source> <claim>`). One line. Defaults to `main` if the
file is absent.

### `expected/smt2-contains` (required)

One substring per line. Each non-empty line must appear **somewhere**
in the generated `.smt2` (plain substring match, not regex). This is
the implementation-agnostic assertion: it pins the *translation* a
correct compiler must produce — e.g. `(+ 3 4)` for integer addition,
`str.++` for string concatenation, `(Green)` for an enum variant —
without caring how the compiler got there. Blank lines are ignored.

### `expected/stdout` and `expected/exit` (optional)

If either file is present, the runner additionally **runs** the
compiled `.smt2` through the kernel and checks behaviour:

- `stdout` — exact kernel stdout, trailing newlines stripped from both
  sides before comparison. Multi-line stdout is fine (one line per
  printed line).
- `exit` — expected process exit code. If `stdout` is present but
  `exit` is absent, the exit code is checked against the default `0`.

If neither file is present, the feature is a translation-only check
(smt2-contains only) and the kernel is not invoked.

## Running

```sh
# bootstrap backend (default) — what ./test.sh runs today
tests/conformance/features/runner.sh

# self-hosted backend — kernel + compiler.smt2 (reports BLOCKED until it exists)
IMPL=selfhost tests/conformance/features/runner.sh

# both — compile under each, compare stdout + exit for equivalence
IMPL=both tests/conformance/features/runner.sh
```

The runner prints one line per feature and a summary:

```
N passed / M failed / K blocked  (of T)
```

It exits `0` only when **every** feature passed. A *blocked* feature
(e.g. `selfhost` before `compiler.smt2` exists) means the run is
incomplete, not green, so it exits non-zero.

### Backends

| `IMPL`      | How `source.ev` is compiled                                         |
| ----------- | ------------------------------------------------------------------- |
| `bootstrap` | `bootstrap/runtime/target/release/evident emit source.ev <claim>`   |
| `selfhost`  | `kernel/target/release/kernel compiler.smt2 < source.ev`            |
| `both`      | compile under each, then compare kernel stdout + exit               |

`selfhost` requires `compiler.smt2` at the repo root. Until the
self-hosted compiler is built and committed there, `selfhost` and
`both` report every feature as `BLOCKED: no compiler.smt2`.

### How `IMPL=both` interprets results

For each feature, `both` compiles via bootstrap and via selfhost, runs
each resulting `.smt2` through the kernel, and asserts the two agree on
**stdout and exit code**. If the bootstrap leg fails to compile or
mismatch its own `expected/`, that's a `✗` for the feature. If
selfhost can't compile yet (no `compiler.smt2`), the feature is
`BLOCKED`. Otherwise the feature passes only when the two backends are
behaviourally equivalent — which is the signal we need before flipping
the default backend from `bootstrap` to `selfhost`.

## Writing a new feature

1. Pick one capability that the bootstrap compiler already handles
   (a binop, a literal kind, an enum, a match, a string op).
2. Make a directory `NNN-short-name/`.
3. Write a minimal `source.ev` that exercises exactly that capability
   and ends in an `effects` constraint so `evident emit` accepts it.
   The smallest runnable shapes:
   - print:  `effects = ⟨LibCall("libc","puts",⟨ArgStr(msg)⟩), Exit(0)⟩`
   - exit code only: `effects = ⟨Exit(<int expr>)⟩`
4. Add `claim.txt` (usually `main`).
5. Run `tests/conformance/features/runner.sh` once and read the actual
   `.smt2` to choose tight `expected/smt2-contains` substrings, plus
   `expected/stdout` / `expected/exit` for a behavioural check.
6. Confirm it passes under `IMPL=bootstrap`. That's the baseline the
   self-hosted compiler must later match.
