# STATE

_Output of `scripts/check-deletable.sh`._

```
BOOTSTRAP NOT YET DELETABLE.

Blockers:

compiler.smt2 does not exist at the repo root.
    This is the self-hosted compiler — written in Evident at
    compiler/compiler.ev, compiled once via bootstrap, and
    committed here. Until it exists, only bootstrap can compile .ev files.
test.sh still invokes bootstrap. Switch its 'evident' binary path
    to use kernel + compiler.smt2.
bootstrap/ directory still exists (11385 lines of Rust).
    When every blocker above is cleared, run: rm -rf bootstrap/

See CLAUDE.md, section 'The deletion path,' for how to clear these.
```
