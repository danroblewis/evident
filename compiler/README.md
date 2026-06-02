# compiler/ — the self-hosted Evident compiler (WIP)

This directory holds the Evident-in-Evident compiler that replaces
`bootstrap/runtime/` (the ~10,500-line Rust compiler). The project's
only goal is to delete `bootstrap/`; everything here is a step on
that path. When these `.ev` files compose into a working driver
(`compiler/compiler.ev`), compile to `compiler.smt2`, and verify
equivalent to bootstrap on the conformance corpus, `bootstrap/` is
deleted. Files here ship *as the compiler*; contrast with
`stdlib/`, which is stable runtime library code that user programs
depend on (`kernel.ev`, `combinatorics.ev`, `toposort.ev`).

## Status: WIP — no real driver yet; per-pass fixtures only.

Each file below demonstrates how one AST shape maps to SMT-LIB,
exercised one level deep by a hardcoded fixture under
`tests/kernel/`. They do **not** yet compose into a pipeline: no
file reads a `.ev` from disk via `ReadFile`, nothing wires
lex → parse → translate end to end, and no conformance test compares
output to bootstrap. Building the driver is Phase 3 of the deletion
checklist.

## What each file replaces

| compiler file              | replaces (bootstrap/runtime/src/…)        | pass |
| -------------------------- | ----------------------------------------- | ---- |
| `lexer.ev`                 | `lexer.rs`                                 | lex  |
| `parser.ev`                | `parser/mod.rs`                            | parse |
| `translate.ev`             | `translate/datatypes.rs`                   | C1 enum → datatype decls |
| `translate_declare.ev`     | `translate/declare.rs`                     | C2 declare-fun emission |
| `translate_bool.ev`        | `translate/exprs/bool.rs`                  | C3 Boolean atoms |
| `translate_arith.ev`       | `translate/exprs/scalar.rs`                | C4 integer arithmetic |
| `translate_compose.ev`     | `translate/inline/mod.rs`                  | C5 claim composition |
| `translate_quant.ev`       | `translate/exprs/quant.rs`                 | C6 ∀/∃ expansion |
| `translate_seq.ev`         | `translate/exprs/seq_eq.rs`                | C7 Seq → sequence theory |
| `translate_match.ev`       | `translate/exprs/match_expr.rs`            | C8 match → nested ITE |
| `translate_record.ev`      | `translate/exprs/record_lift.rs`           | C9 record lifts |
| `translate_string.ev`      | `translate/exprs/string_ops.rs`            | C10 string ops → str.* |
| `translate_generics.ev`    | `runtime/generics.rs`                      | C11 monomorphization |
| `translate_infer.ev`       | `runtime/inject.rs`                        | C12 LHS-type inference |
| `translate_concat.ev`      | `runtime/desugar.rs`                       | C13 `++` flattening |
| `translate_manifest.ev`    | `emit.rs`                                  | C14 manifest header |

Each file's first line carries the same mapping plus a one-line
status, in the form
`-- WIP: replaces bootstrap/runtime/src/<file>. STATUS: <one line>.`

## Where this fits

See `docs/plans/DELETION-CHECKLIST.md`, **Phase 2** (this
restructure) and **Phase 3** (the driver that composes these passes
into a real `compiler/compiler.ev`).
