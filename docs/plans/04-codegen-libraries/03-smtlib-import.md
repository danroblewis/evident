# Phase 4.3: SMT-LIB import → stdlib/smtlib/import/

## Goal

Replace the import half of `smtlib.rs` (~450 lines, parses SMT-LIB
S-expression syntax → Evident AST). This needs an Evident-side
string-tokenizer + recursive descent parser.

## Prereqs

- Phase 3 done.
- Phase 4.2 (export) gives a baseline for what the round-trip
  invariant looks like.

## What to build

`stdlib/smtlib/import.ev` — tokenize an SMT-LIB string into
Seq(Token), then recursive-descent parse into Program.

This is the trickiest Phase 4 task — building a parser in Evident.
The grammar is simpler than Evident's own (only S-expressions), so
it's a reasonable proving ground for "Evident can parse things."

## Files touched

- `runtime/src/smtlib.rs` — delete the import half
- `runtime/src/commands/import_smt2.rs` — call Evident library
- `stdlib/smtlib/import.ev` (new)

## Acceptance

- [ ] Round-trip: Evident program → export-smt2 → import-smt2 →
      identical Evident program (modulo cosmetic diffs)
- [ ] LOC: -450 Rust, +~300 Evident

## Notes

If this works, it suggests the parser bootstrap (out-of-scope for
this roadmap) is feasible later. Useful proof point.
