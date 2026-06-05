# Task: Feasibility — AOT functionizer cache as binary (wave 5d)

## What this is

**A diagnostic / design wave.** No code changes. Deliverable = a
feasibility report in `docs/plans/wave-5d-aot-binary-cache.md`.

## Why

Once the functionizer lives in Evident (wave 5c) AND can emit
native code (some codegen option), the next question is *when* to
materialize: at runtime (JIT, today) vs at build time (AOT,
cached binaries).

**These are the same operation, named differently.** AOT
functionization with disk cache IS compilation to a binary. This
wave designs that cache: format, invalidation, distribution.

## Required reading

1. `CLAUDE.md` — Definition of done.
2. Memory: [[feedback-aot-over-runtime-disk-cache]] — the user's
   stated preference for AOT + cache.
3. Memory: [[project-constraint-model-compilation]] — the
   long-term native compilation plan.
4. `kernel/src/functionize/mod.rs` — what the JIT output looks
   like today (`Program` struct, JIT'd code blobs).
5. `compiler/compiler.ev`, `compiler/sample.ev` — the two driver
   programs whose binaries we'd produce.

## Scope

### Section 1: What does the cache key on?

The functionizer's input is the SMT-LIB body + manifest. If those
are byte-identical, the JIT output should also be byte-identical
(assuming deterministic codegen).

Propose a cache key: SHA256 of `body + manifest`? `(body, codegen
version)`? Discuss trade-offs.

### Section 2: Cache directory layout

Mirror Python's `__pycache__` (the memory entry's example):
- Where on disk? `~/.evident-cache/` or `./.evident/`?
- File naming convention?
- Versioning when codegen changes?

### Section 3: Binary format

The functionized program today is `Vec<Step>` + JIT code blobs in
memory. For the cache to be a STANDALONE BINARY, we need:
- A loader (probably the existing kernel's `run_program` logic).
- The Step shapes serialized (JSON? bincode? bespoke?).
- The native code blob (a `.o` file? raw bytes? a `.dylib`?).

Discuss three options:
- **Self-contained executable**: ELF/Mach-O that loads itself
  and runs. Maximum standalone-ness.
- **Side-car format**: a `.evidentc` file the kernel runs
  (`kernel foo.evidentc`). Easiest to ship.
- **Plain object file + small loader stub**: link-time embedded.

### Section 4: Distribution

Once a project's `compiler.ev` is AOT-compiled, the result is
`compiler.evidentc` (or whatever format). How does that get
distributed?
- Checked into the repo (like `compiler.smt2` is today)?
- Built on first run + cached locally?
- Both (CI builds the cache and uploads it)?

### Section 5: Invalidation

When `compiler.ev` changes, the cache must rebuild. When the
codegen module changes (new optimizations), the cache must
rebuild. Sketch the invalidation rules.

### Section 6: Verdict + roadmap

`feasibility: HIGH|MEDIUM|LOW|BLOCKED`. Likely HIGH given the
existing JIT machinery is mostly there. Sketch a 3-step roadmap.

## Forbidden

- Editing `kernel/`, `compiler/`, `stdlib/`, `bootstrap/`, Python.
- Implementing the cache.
- Picking a codegen option (that's wave 5c).

## Reporting back

- Branch (`agent-55-feasibility-aot-binary-cache`).
- Report at `docs/plans/wave-5d-aot-binary-cache.md`.
- Cite docs.

Less than 1500 words.
