# `runtime-c/` — a minimal SMT-LIB-first Evident runtime in C++

A from-scratch, **thin native shell over Z3** that runs a subset of Evident by
the strategy in [`docs/design/smtlib-as-compile-target.md`](../docs/design/smtlib-as-compile-target.md):
parse Evident → emit **SMT-LIB text** → hand it to Z3 → solve → extract a model.
It is fully self-contained and **additive** — it does not touch the Rust
`runtime/`, which remains the language spec and the cross-check oracle.

The full design, the C-vs-Evident-vs-SMT-LIB split, and the roadmap live in
[`docs/design/c-runtime.md`](../docs/design/c-runtime.md). This file is just how
to build and run it.

## Dependency: Z3

The one unavoidable dependency. Built and tested against **Z3 4.15.4** from
Homebrew:

- header: `/opt/homebrew/include/z3.h`
- library: `/opt/homebrew/lib/libz3.dylib`

`brew install z3` if absent. CMake auto-probes Homebrew; override with
`-DZ3_ROOT=<prefix>`. We use the Z3 **C API** directly (`Z3_solver_from_string`
to load SMT-LIB text, `Z3_solver_check`, `Z3_model_eval`), the same calls the
Rust prototype in `runtime/src/translate/smtlib.rs` makes through the `z3` crate.

## Build

```sh
cmake -S runtime-c -B runtime-c/build
cmake --build runtime-c/build
```

Produces three binaries in `runtime-c/build/`:

| binary | what it is |
|---|---|
| `evidentc`      | the CLI: `evidentc <file.ev> <claim>` |
| `z3_link_proof` | M0 — hands a hardcoded SMT-LIB string to Z3, prints sat + model |
| `seed_tests`    | lexer / parser / emit / solve unit + integration tests |

## Run

```sh
# Solve one claim: prints sat/unsat and (when sat) the model bindings.
runtime-c/build/evidentc runtime-c/tests/fixtures/forced.ev forced_real_half
#   sat
#   x = 1.5

# Sat-check every claim in a file (cross-check aid).
runtime-c/build/evidentc runtime-c/tests/fixtures/scalars.ev --all

# Dump the generated SMT-LIB to stderr alongside the solve.
runtime-c/build/evidentc runtime-c/tests/fixtures/scalars.ev sat_int_div --smtlib
```

## Test + cross-check against the Rust runtime

```sh
# Seed unit tests (no Z3 model framework — just asserts).
./runtime-c/build/seed_tests

# Verdict + forced-model parity vs the Rust oracle (needs both binaries built;
# build the Rust one with: cargo build --release --manifest-path runtime/Cargo.toml)
./runtime-c/tests/crosscheck.sh
```

`crosscheck.sh` runs every fixture through both `evidentc --all` and
`evident sample --all` and asserts the sat/unsat verdicts match, then checks the
forced-model fixtures produce identical model values on both runtimes.
