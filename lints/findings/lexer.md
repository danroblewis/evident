# Findings: runtime/src/lexer.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): lexer.rs is in
  scope. Scan for `Sdl[A-Z]`, `SDL_`, `\bGl[A-Z]`, `Glsl`,
  `Audio[A-Z]`, `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
  `/usr/lib/lib` — zero hits. The file mentions no C library by
  name and contains no platform paths. Clean.
- AP-002, AP-003, AP-006, AP-007, AP-008: examples-only scope; not
  applicable.
- AP-004: conformance-only scope; not applicable.
- AP-005: applies to `runtime/tests/**.rs`. The in-file
  `#[cfg(test)] mod tests` carries no `#[ignore]` annotations.
  Clean.

## Per-file-invariant check

The lexer's invariants (purpose: source `&str` → flat `Vec<Token>`;
owns the `Token` enum and Unicode/word-keyword/literal/indentation
recognizers; never builds nested structure, never references Z3 /
runtime / effects / FTI / C libraries; errors are character-level
only; zero `use crate::*` imports) all hold:

- File defines `Token`, `LexError`, and a single `pub fn tokenize`
  returning `Vec<Token>` — flat.
- Zero `use` statements; depends only on `std`.
- Errors are all character-level (`unterminated string literal`,
  `unknown escape`, `unexpected character`, `unexpected '!'`,
  `invalid integer/real`). No grammar-level diagnostics.
- `paren_depth` tracking exists to suppress `Newline` / `Indent`
  emission inside `(`, `[`, `{`, `⟨` groups. This is layout/
  whitespace bookkeeping, which is the lexer's concern (Newline +
  Indent are its outputs). It does not amount to parsing — no
  decision is made about which tokens form which constructs.

## Candidate new rules

None worth promoting. One small observation that did NOT clear the
proposing bar:

**Observation (review-only).** The file uses repeated
`chars.next(); col += 1;` after every successful single-char
recognition (15+ sites). A small `bump(&mut chars, &mut col)`
helper would make the recognizer arms one-liners and remove a
class of "forgot to bump col" bugs. This is style/refactor advice,
not an anti-pattern; not adding to the rulebook.

## Clean

The file is clean against all 8 active rules and against its
runtime-invariants brief. No findings to fix.
