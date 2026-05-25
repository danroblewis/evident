# Where the runtime finds `stdlib/`

The Evident runtime ships its standard library as plain `.ev` files on
disk (`stdlib/runtime.ev`, `stdlib/ast.ev`, `stdlib/passes/*.ev`, …). At
load time several places need to locate that directory: `effect-run`
loads `runtime.ev`; the self-hosted-pass drivers (`lint`, `desugar`,
`infer-types`) load `ast.ev` + `passes/*.ev`; the portable `Evident*`
impls load a pass. **All of them go through one resolver**,
`evident_runtime::stdlib_path::stdlib_dir()`, which returns the stdlib
*directory* — callers join `runtime.ev` / `ast.ev` / `passes/<x>.ev` onto
it.

## Why path-based, not embedded

The files stay separate on disk; the runtime does **not** `include_str!`
them into the binary. Embedding would force a recompile on every `.ev`
edit, which kills the dogfooding workflow (editing a self-hosted pass and
re-running with no rebuild). Path-based resolution keeps both properties:
a relocated/installed binary still finds its stdlib, *and* editing a pass
takes effect immediately.

## Resolution order

`stdlib_dir()` tries these in order and returns the first directory that
contains the marker file `runtime.ev`:

1. **`EVIDENT_STDLIB`** — explicit override (the PYTHONPATH analog).
   `EVIDENT_STDLIB_DIR` is accepted as a back-compat alias. This override
   is **authoritative**: if it's set but doesn't point at a stdlib
   directory, that's a hard error — a typo fails loudly instead of
   silently falling back.

2. **Install locations**, relative to the running executable:
   - `<exe_dir>/../share/evident/stdlib` — FHS-style install
     (`<prefix>/bin/evident` + `<prefix>/share/evident/stdlib`).
   - `<exe_dir>/stdlib` — self-contained / portable layout, stdlib next
     to the binary.
   - `$XDG_DATA_HOME/evident/stdlib`, falling back to
     `~/.local/share/evident/stdlib` — per-user data dir.

3. **Dev-tree fallback** (so `cargo test` and the dev binary work with
   zero config, regardless of CWD):
   - `$CARGO_MANIFEST_DIR/../stdlib` — a compile-time constant baked into
     the binary, pointing at the repo's `stdlib/`. This is the primary
     dev path.
   - A few exe-relative guesses up out of `target/{debug,release}[/deps]`.
   - `./stdlib` — CWD-relative, the historical behavior.

4. **Clear error** if none match: the message lists every path checked
   and names the `EVIDENT_STDLIB` override, so the failure is actionable
   (not a bare "No such file").

## Examples

Run from anywhere with an explicit stdlib:

```sh
cd /tmp
EVIDENT_STDLIB=/opt/evident/share/evident/stdlib \
  evident effect-run /path/to/program.ev
```

Install layout that resolves with no env var:

```
/usr/local/bin/evident
/usr/local/share/evident/stdlib/runtime.ev
/usr/local/share/evident/stdlib/ast.ev
/usr/local/share/evident/stdlib/passes/*.ev
```

A wrong override is rejected immediately:

```
$ EVIDENT_STDLIB=/nope evident effect-run program.ev
effect-run: $EVIDENT_STDLIB=/nope does not look like Evident's stdlib
(no `runtime.ev` at `/nope/runtime.ev`).
Point $EVIDENT_STDLIB at the directory that holds `runtime.ev` and `ast.ev`.
```

## Implementation

`runtime/src/stdlib_path.rs`. The search core (`resolve_candidates`) is
split from env/exe lookup so it's unit-testable without process-global
mutation; the public `stdlib_dir()` wraps it with the live candidate list.
This module replaced three inconsistent prior schemes: CWD-relative consts
(`"stdlib/runtime.ev"`), a portable-only `EVIDENT_STDLIB_DIR` env check
duplicated per file, and hard-coded `../stdlib` in tests.
