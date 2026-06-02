# STATE

_This file is the output of `scripts/check-deletable.sh`. It is
the project's brutal-truth status: what's blocking bootstrap deletion,
in concrete file counts. No prose, no aspirations._

_Updated by running `scripts/check-deletable.sh > STATE.md` from the
repo root whenever the state changes._

```
BOOTSTRAP NOT YET DELETABLE.

Blockers:

12 files still reference bootstrap/runtime/target:
    ./tests/conformance/conftest.py
    ./tests/conformance/features/README.md
    ./tests/conformance/features/runner.sh
    ./docs/plans/DELETION-CHECKLIST.md
    ./docs/briefings/tasks/02-conformance-architecture.md
    ./scripts/lexer-oracle.py
    ./scripts/run-kernel-tests.py
    ./scripts/run-lang-tests.py
    ./scripts/diff-test-selfhosted.sh
    ./scripts/bench-demo.sh
    ./scripts/bench-selfhosted.sh
    ./scripts/coordinator-results/02-conformance-architecture/prompt.md
compiler.smt2 does not exist at the repo root.
    This is the self-hosted compiler — written in Evident at
    compiler/compiler.ev, compiled once via bootstrap, and
    committed here. Until it exists, only bootstrap can compile .ev files.
17 Python files remain under scripts/ or tests/ (scheduled for removal):
    scripts/lexer-oracle.py
    scripts/run-kernel-tests.py
    scripts/run-lang-tests.py
    scripts/runtime-size.py
    scripts/strip-comments.py
    tests/conformance/__init__.py
    tests/conformance/conftest.py
    tests/conformance/test_claim_composition.py
    tests/conformance/test_cli.py
    tests/conformance/test_composite_elements.py
    tests/conformance/test_errors.py
    tests/conformance/test_evident_self.py
    tests/conformance/test_language.py
    tests/conformance/test_selfhosted_diff.py
    tests/conformance/test_selfhosted_perf.py
    tests/conformance/test_string_ops.py
    tests/conformance/test_syntax_sugar.py
test.sh still invokes bootstrap. Switch its 'evident' binary path
    to use kernel + compiler.smt2.
bootstrap/ directory still exists (11247 lines of Rust).
    When every blocker above is cleared, run: rm -rf bootstrap/

See CLAUDE.md, section 'The deletion path,' for how to clear these.
```
