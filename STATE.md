# STATE

_Output of `scripts/check-deletable.sh`._

```
BOOTSTRAP NOT YET DELETABLE.

Blockers:

11 files still reference bootstrap/runtime/target:
    ./tests/conformance/conftest.py
    ./tests/conformance/features/README.md
    ./tests/conformance/features/runner.sh
    ./docs/plans/DELETION-CHECKLIST.md
    ./docs/briefings/tasks/12-kernel-A-vs-B-largebody.md
    ./docs/briefings/tasks/02-conformance-architecture.md
    ./scripts/diff-test-selfhosted.sh
    ./scripts/bench-demo.sh
    ./scripts/run-kernel-tests.sh
    ./scripts/bench-selfhosted.sh
    ./scripts/run-lang-tests.sh
compiler.smt2 does not exist at the repo root.
    This is the self-hosted compiler — written in Evident at
    compiler/compiler.ev, compiled once via bootstrap, and
    committed here. Until it exists, only bootstrap can compile .ev files.
1 Python files remain under scripts/ or tests/ (scheduled for removal):
    tests/conformance/conftest.py
test.sh still invokes bootstrap. Switch its 'evident' binary path
    to use kernel + compiler.smt2.
bootstrap/ directory still exists (11247 lines of Rust).
    When every blocker above is cleared, run: rm -rf bootstrap/

See CLAUDE.md, section 'The deletion path,' for how to clear these.
```
