# Evident developer tooling (`tools/`)

IDE/LSP-style refactoring tooling for the self-hosted Evident compiler tree
(`compiler2/*.ev`, `stdlib/*.ev`, `compiler/*.ev`, `tests/**/*.ev`).

This is **separate dev tooling**. It does NOT touch `kernel/` (frozen), does
NOT touch the frozen oracle, and adds NO Python to the producing path. It is a
single dependency-free Rust binary set under `tools/`.

```
tools/
  evident-tools/     Rust crate — the engine + two binaries:
    evt              the CLI (index/defs/refs/rename/symbols/families/collisions)
    evident-lsp      a std-only Language Server over stdio
  vscode-evident/    minimal VS Code extension (TextMate grammar + LSP client)
  README.md          this file
```

## Why Rust (not Python/Go)

- The repo already ships `cargo`/`rustc` (the kernel is Rust); no new toolchain.
- "No Python in the producing path" — this tooling is dev-only, but Rust keeps
  it a single static binary with **zero external crates** (the LSP's JSON-RPC
  and JSON codec are hand-rolled in `src/json.rs`), so there is no dependency
  drift and nothing to `pip install`.
- A real tokenizer (not grep/sed) is mandatory here — see the traps below — and
  Rust makes the token-accurate byte-splice rewrite trivial and safe.

## Build

```sh
cd tools/evident-tools
cargo build --release
# binaries: target/release/evt  and  target/release/evident-lsp
```

(`evident-tools` is its own cargo workspace, so it never participates in the
kernel's build graph.)

Optionally put them on PATH:

```sh
export PATH="$PWD/target/release:$PATH"
```

## The CLI: `evt`

All commands scan the whole `.ev` tree by default (so the `_x` carry dual is
never missed — see traps). `--scope <dir>` narrows it; `--root <dir>` overrides
repo-root discovery.

| Command | What it does |
| --- | --- |
| `evt index` | Build + print the symbol index: every `claim`/`fsm`/`type`/`schema`/`enum`/`subclaim` decl + header slots, with `file:line:col`. |
| `evt defs <name>` | Where `<name>` is **declared** (membership `∈`, header slot, or top-level decl). |
| `evt refs <name>` | **All** references to `<name>` incl. the `_name` carry dual, classified `decl` / `assign-lhs` / `read`. |
| `evt rename <old> <new>` | **Tree-wide, token-accurate** rename of `<old>` AND `_<old>`→`_<new>`. Refuses on target-name collision (the merge trap) unless `--force`. `--dry-run` shows the diff. |
| `evt symbols [file.ev]` | Symbol outline (one file, or the whole tree) with member variables nested under their schema. |
| `evt families [--min N]` | The two hand-run refactoring probes, productized: **numbered** families (`xN` siblings → candidate `Seq`), **prefix** families (`<word>_` ≥N decls → candidate record/namespace), and **cons-peel** residue (`after`/`next`/`tail`/`*_rest` — the "skip a bank" list-walk clusters). |
| `evt collisions [entry] [claim]` | The **authoritative** collision oracle: flatten → `evident-oracle emit driver_main` → parse top-level `declare-fun` names → report duplicates. Defaults `compiler2/driver.ev` / `driver_main`. Needs `evident-oracle` on `/usr/local/bin` (override with `EVIDENT_ORACLE`). |

### Examples

```sh
evt defs zstep
evt refs enum_hold
evt rename enum_hold zinit_enum_hold --dry-run
evt families --scope compiler2 --min 4
evt collisions                      # declare-fun names of emitted driver_main
```

## Evident semantics the tools respect (why naive grep/sed fails)

These are the traps the manual de-prefix work hit (encoded in the project
memories); each is handled:

1. **The `_x` carry dual.** `_x` is the prev-tick dual of `x`; a rename of `x`
   must also rename `_x`. The lexer makes `_x` a single token; rename rewrites
   `_old`→`_new`, preserving the leading underscore. `refs`/`defs` group the
   dual with its base. Because `\b` does **not** match before a leading `_`, a
   `\bNAME` file-filter would miss files that contain only `_NAME` — so the
   tools always operate on the **whole tree**, never a grepped subset.
2. **Substring corruption.** Renaming `x_h` must not touch `ctx_h`. The rename
   matches whole **identifier tokens**, not substrings, so longer names that
   merely contain `old` are untouched.
3. **The merge trap.** Renaming `A→B` where `B` already exists silently MERGES
   two distinct symbols under names-match composition (and can explode the
   solver). `evt rename` **refuses** when the target already occurs, printing
   how many times and pointing at `evt refs`/`evt collisions`; `--force`
   overrides.
4. **The authoritative collision oracle is NOT a source grep.** Header-param
   and hidden-internal names appear in source but get **zero** top-level
   declares (scoped away), so a `.ev` grep over-reports collisions. `evt
   collisions` emits `driver_main` and inspects the real `declare-fun` set —
   the ground truth. (`evt rename`'s built-in guard is the cheap source-level
   front line; run `evt collisions` for the definitive check.)
5. **Strings and comments are never identifiers.** The tokenizer skips `--`
   line comments and `"..."` literal contents, so a rename can't corrupt them.

The tokenizer matches `compiler/lexer.ev`: ASCII identifiers
`[A-Za-z_][A-Za-z0-9_]*`, the unicode operators (`∈ ∀ ∃ ⇒ ↦ ¬ ∧ ∨ ≤ ≥ ≠ ⟨ ⟩`
…) and multi-char ASCII ops (`++ => <= >= != ..`) each as single tokens.

## What's robust vs heuristic (be honest)

- **Robust:** tokenization, the `_x` dual, substring safety, the byte-accurate
  rewrite, the `declare-fun` collision oracle, the families analyses, the
  symbol outline. Decl recognition (membership `∈`, header slots, enum
  variants) is structural and exact.
- **Heuristic / lexical (no full name-resolution):** `defs`/`refs` are
  **name-scoped**, not **join-resolved**. Evident composition is names-match
  with `..`-lift (shares the whole namespace), bare-mention (joins only header
  slots; internals freshened per call site), and `slot ↦ value` binding. The
  tools do **not** model which occurrences of a given name are actually the
  *same* SMT variable after join resolution — that needs the oracle. So `refs
  foo` returns every `foo` token tree-wide (including independent `foo`s in
  unrelated claims / fixtures). For rename this is the **safe** behavior (a
  tree-wide consistent rename is what you want); for "find references to *this*
  binding" it can over-report across distinct scopes. The `collisions` command
  is the oracle-backed escape hatch when you need ground truth.
- The LSP's Seq-membership diagnostic is a conservative **shape** heuristic
  (it can't know a RHS is a `Seq` without the oracle); treat it as a hint.

## The LSP: `evident-lsp`

A std-only Language Server over stdio (`Content-Length` framing + JSON-RPC).
Capabilities:

- `textDocument/definition` — go to a symbol's declaration(s).
- `textDocument/references` — all references (incl. duals).
- `textDocument/documentSymbol` — outline (schemas + nested members/variants).
- `textDocument/hover` — decl signature (header slots) or variable occurrence count.
- `textDocument/prepareRename` + `textDocument/rename` — same engine as the CLI,
  with the **same collision refusal** (returns no edit + a `window/showMessage`
  if the target exists; use `evt rename --force` if intended).
- `textDocument/publishDiagnostics` — the Seq-membership lint hint.

Test it by hand:

```sh
printf 'Content-Length: 52\r\n\r\n{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  | evident-lsp
```

## VS Code extension (`vscode-evident/`)

Provides:

- **Syntax highlighting** via a TextMate grammar
  (`syntaxes/evident.tmLanguage.json`) — decls, keywords, the unicode/ASCII
  operators, strings, numbers, and the `_x` carry dual as a distinct scope.
- **LSP client** (`client/extension.js`) that spawns `evident-lsp`.

Wire it up:

```sh
cd tools/vscode-evident
npm install            # pulls vscode-languageclient
# point the extension at the built server:
#   Settings → "evident.lspPath": "/abs/path/to/target/release/evident-lsp"
# then F5 in VS Code to launch an Extension Development Host,
# or `vsce package` to build a .vsix and install it.
```

Any LSP-capable editor works — Neovim/Emacs/Helix can launch
`evident-lsp` directly as the server command for `*.ev`.

## Survey: refactor tools other languages have, and what we built

Mature languages expose, via LSP and ctags-style indexers:

| Capability | Status here |
| --- | --- |
| Rename symbol | **Done** (CLI + LSP), token-accurate, dual-aware, collision-guarded |
| Find references | **Done** (CLI `refs`, LSP references) |
| Go-to-definition | **Done** (CLR `defs`, LSP definition) |
| Find declarations | **Done** (`defs`) |
| Document / workspace symbols | **Done** (`symbols`, LSP documentSymbol) |
| Hover | **Done** (LSP hover) |
| Diagnostics | **Partial** (Seq-membership lint hint; more lints can wrap `scripts/lint-*.sh`) |
| ctags-style index | **Done** (`index`) |
| Codemod / family analysis | **Done** (`families` — Evident-specific: Seq/record/cons-peel candidates) |
| LSP protocol | **Done** (`evident-lsp`, std-only) |
| Extract (variable/record/claim), Inline | **Deferred** — these need join-aware semantic resolution (which member writes cover an output, where a `..`-lift shares a name) that is only sound with oracle integration. The `families` command surfaces the *candidates* for the record/Seq extractions the user does by hand, which is the high-value 80%; the mechanical edit is still manual because doing it wrong silently changes solver behavior. |
| Call hierarchy / type hierarchy | **Deferred** — composition is names-match, not calls; a faithful hierarchy needs join resolution. |

### Why extract/inline were deferred

In Evident a "variable" is a constraint membership and identity across files is
decided by **names-match join resolution**, not by an import graph. Extracting a
record or inlining a claim safely requires knowing, per occurrence, whether two
same-named tokens are the *same* SMT binding after `..`-lift / bare-mention /
slot-bind resolution — exactly the thing the lexical engine deliberately does
*not* claim to know (see "robust vs heuristic"). Productizing the *analysis*
(`families`) without auto-applying the *edit* is the honest line: it points the
user at every cluster worth extracting, and `evt rename` + the `collisions`
oracle make the manual edit safe, without a tool silently mis-joining symbols.
