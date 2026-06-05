# Wave 5d — Feasibility: AOT functionizer cache as binary

**Type:** diagnostic / design. No code changes.
**Verdict:** `feasibility: MEDIUM` (HIGH for the side-car format on
all-scalar programs; the residual-step dependency on a live Z3
context is the one thing that keeps it from HIGH across the board).

## Premise

AOT functionization with a disk cache **is** compilation to a
binary — the same operation as today's JIT, moved from per-run to
build time and persisted. This is the user's stated direction
([[feedback-aot-over-runtime-disk-cache]]: front-load setup, keep
per-tick fast; a `__pycache__`-style cache is explicitly desired)
and the materialization step of the long-term native-compile plan
([[project-constraint-model-compilation]]).

This report designs that cache by reading what the functionizer
actually produces today.

## What the functionizer produces today (ground truth)

From `kernel/src/functionize/mod.rs` and `jit.rs`:

- A `Program` is `{ steps: Vec<Step>, predicates: Vec<Z3_ast>,
  jit_count, interp_count, _keepalive: Vec<Z3_ast> }`
  (`mod.rs:79`).
- Each `Step` is `{ var, body: StepBody, result_is_bool,
  is_effects, jit: Option<JitStep> }` (`mod.rs:65`). `StepBody` is
  `Scalar(Z3_ast) | Seq(Vec<Z3_ast>) | Guarded(Vec<Branch>)`.
- A `JitStep` is `{ _module: JITModule, func: extern "C"
  fn(*const i64) -> i64, inputs: Vec<String> }` (`jit.rs:108`).
  Cranelift emits the native blob; `inputs` names the `i64` slots
  the blob reads (`jit.rs:184-187`).
- The exit-line diagnostic classifies every assertion as **JIT /
  interp / residual** (`mod.rs:239`).

The crucial split for caching: a `Step` is one of three
serializability classes.

| Class | Holds | Serializable standalone? |
| --- | --- | --- |
| **JIT** scalar | Cranelift native blob + input names | **Yes** — emit object code instead of in-memory `JITModule`. |
| **interp** | `Z3_ast` pointers, evaluated by `eval.rs` | Only as SMT-LIB text; needs a live Z3 ctx to rebuild. |
| **residual** | `predicates: Vec<Z3_ast>` re-solved each tick | **No** — requires Z3 at run time, every tick. |

`_keepalive` (`mod.rs:79`) exists precisely because the `Z3_ast`s
are process-lifetime pointers into a Z3 context — they are not
values, so they cannot be frozen to disk as-is. **This is the load-
bearing fact of the whole wave.**

## Section 1 — What does the cache key on?

The functionizer's input is the SMT-LIB body + manifest header
(the kernel parses the manifest, then hands the body to extraction;
see CLAUDE.md §"Manifest header"). If those bytes are identical and
codegen is deterministic, the output blob is identical.

**Proposed key:**
`SHA256(canonical_smtlib_body) ⊕ SHA256(manifest) ⊕ codegen_version`.

- Key on the **emitted SMT-LIB**, not the `.ev` source. The `.ev` →
  `.smt2` step is itself the compiler under construction; two
  different `.ev` files (or two compiler versions) that emit the
  same `.smt2` *should* hit the same cache entry. Keying on
  `.smt2` makes the cache compiler-agnostic.
- `codegen_version` must be in the key, not just metadata: a
  Cranelift upgrade or a new lowering rule in `jit.rs` changes the
  blob for identical input. Bump it on any change under
  `kernel/src/functionize/`.
- **Trade-off:** SMT-LIB is not canonical out of the box (term
  ordering, whitespace, gensym names). Either hash post-`simplify`
  (the functionizer already simplifies — `mod.rs:962` "assemble,
  JIT, verify") or define a canonical printer. Hashing the
  pre-simplify text risks false misses (same program, different
  spelling → recompile); hashing post-simplify risks coupling the
  key to Z3's simplifier version, which then also belongs in
  `codegen_version`. **Recommend: hash post-simplify body +
  include Z3 version in `codegen_version`.**

## Section 2 — Cache directory layout

Mirror `__pycache__`, with a global tier for shared artifacts:

- **Per-project:** `./.evident/cache/` next to the source, for the
  common dev loop. Git-ignored.
- **Global:** `~/.cache/evident/` (respect `$XDG_CACHE_HOME`) for
  cross-project reuse of identical bodies (e.g. stdlib programs).
- **Naming:** `<sha256-prefix>.<codegen_version>.evidentc` — the
  version in the *filename* (Python's `cpython-313` tag) so a
  codegen bump simply misses every old entry; stale files are
  harmless and reaped by mtime, never read.
- **Override:** `EVIDENT_CACHE_DIR` env var, matching the existing
  `EVIDENT_FUNCTIONIZE_*` env-gate convention (`mod.rs:119`).

## Section 3 — Binary format

Three options, evaluated against the three Step classes above.

1. **Self-contained executable (Mach-O/ELF).** Maximum
   standalone-ness. Requires linking the kernel's `run_program`
   loop (`mod.rs` `RunOut`/`build_inputs`, `mod.rs:1100`) and the
   effect dispatcher into the artifact. **Only viable for programs
   that functionize to 0 interp + 0 residual steps** — otherwise
   the executable must embed Z3, defeating "standalone." High
   value, narrow applicability today.

2. **Side-car `.evidentc` (recommended).** `kernel foo.evidentc`
   instead of `kernel foo.smt2`. The file is a container:
   - a header (magic + `codegen_version` + manifest);
   - the serialized `Vec<Step>` *shape* (var, `result_is_bool`,
     `is_effects`, body-kind, `inputs`) — bincode is the natural
     fit (no AST pointers to chase);
   - one native code section per JIT step (Cranelift's
     `ObjectModule` / `cranelift-object` instead of `JITModule` —
     a one-line backend swap of `jit.rs:155`);
   - **the original SMT-LIB body, retained verbatim**, so interp
     and residual steps can be rebuilt against a fresh Z3 ctx at
     load. The cache then re-extracts only the non-JIT steps and
     re-attaches the cached blobs to the JIT ones, skipping the
     expensive Cranelift compile.
   Easiest to ship; degrades gracefully (a program with residual
   steps still caches its JIT half).

3. **Plain object file + loader stub.** `.o` per program, linked
   against a small kernel stub at first run. More moving parts than
   (2) for no extra standalone-ness on mixed programs; useful only
   as the back half of option (1).

**Recommendation: build option 2 first.** It is the only one whose
benefit (skip Cranelift compile + verify, `mod.rs:1062`) applies to
*every* program regardless of residual count, and it keeps Z3 in
the loop where the IR still needs it.

## Section 4 — Distribution

`compiler.smt2` is already checked into the repo root and shipped
as a build artifact (CLAUDE.md §"Definition of done," item 3). The
AOT cache should follow the **same precedent** for the two driver
programs (`compiler/compiler.ev`, `compiler/sample.ev`):

- **Checked-in `compiler.evidentc`** alongside `compiler.smt2` —
  the warm-start artifact, so `kernel + compiler.evidentc` runs the
  self-hosted compiler with zero Cranelift cost on a clean checkout.
- **Built-on-first-run** for every *other* `.ev` the compiler
  processes — populated lazily in `./.evident/cache/`.
- **CI builds + uploads** the checked-in `.evidentc` so it is never
  hand-stale; CI is the single producer, the same role it would
  play for `compiler.smt2`. Treat a mismatch between checked-in
  `.evidentc` and a fresh rebuild as a CI failure.

## Section 5 — Invalidation

A cache entry is valid iff its key still matches. Concretely, miss
(rebuild) when **any** of:

1. The emitted **SMT-LIB body** changes (source edit, or a compiler
   change that alters emission) → SHA differs.
2. The **manifest** changes (state fields, effect/result enum
   names, `max-effects`) → SHA differs.
3. The **`codegen_version`** bumps — any change under
   `kernel/src/functionize/`, a Cranelift upgrade, or a Z3 upgrade
   (because interp/residual steps and post-simplify hashing both
   depend on Z3's behavior).

Because the version is in the filename, invalidation is "miss and
write a new file," never "mutate in place." Reaping is by mtime; a
stale entry is never *read* (its filename can't match a current
key), so a botched reap cannot produce a wrong run — it can only
waste disk. There is no time-based expiry: correctness is keyed on
content, not age.

**Self-verification backstop:** the functionizer already verifies
JIT output against Z3 before trusting it
(`EVIDENT_FUNCTIONIZE_SKIP_VERIFY`, `mod.rs:1062`). Keep that check
on the *first* materialization of a cache entry; subsequent loads
trust the key. This bounds the blast radius of a determinism bug to
one machine's first build.

## Section 6 — Verdict + roadmap

**`feasibility: MEDIUM`**, trending HIGH. The JIT machinery is
already there: Cranelift compiles scalar steps today (`jit.rs`), the
exit diagnostic already separates JIT/interp/residual (`mod.rs:239`),
and the input ABI is a flat `*const i64` (`jit.rs:110`) that
serializes trivially. The single thing standing between "cache the
JIT half" and "ship a standalone binary" is that **interp and
residual steps hold live `Z3_ast` pointers** (`_keepalive`,
`mod.rs:79`) — those programs will always carry their SMT-LIB and a
Z3 context to run. So: HIGH for the side-car format and for
all-scalar programs; MEDIUM as a blanket "standalone binary" claim.

This wave does **not** pick a codegen option — that is wave 5c. The
cache design above is codegen-agnostic: whatever 5c chooses to
emit, it lands in the `.evidentc` code section and the key/layout/
invalidation rules are unchanged.

**3-step roadmap**

1. **Persist the JIT half.** Swap `jit.rs`'s `JITModule` for
   `cranelift-object` behind an `EVIDENT_AOT=1` gate; write/read a
   bincode `Vec<Step>` shape + per-step `.o` blobs in
   `./.evident/cache/`, keyed per Section 1. Reattach cached blobs;
   re-extract only non-JIT steps from the retained SMT-LIB. Measure
   the Cranelift-compile saving (the AOT win the user asked to
   front-load).
2. **`kernel foo.evidentc`.** Teach the kernel's loader to accept
   the side-car container (Section 3, option 2) in addition to
   `.smt2`. Check `compiler.evidentc` into the repo next to
   `compiler.smt2`; have CI produce and diff it.
3. **Standalone for the all-scalar case.** For programs the
   diagnostic reports as `0 interp / 0 residual`, link
   `run_program` + the blob into a Mach-O/ELF (Section 3, option 1)
   — a true compiled binary, no Z3 at run time. Gate strictly on
   the residual count so mixed programs fall back to the side-car.

## Citations

- `kernel/src/functionize/mod.rs` — `Program`/`Step`/`StepBody`
  (`:43-89`), `_keepalive` Z3_ast lifetime (`:79`), diagnostic
  (`:239`), JIT/verify gates (`:962`,`:1062`), `build_inputs`
  (`:1100`), env convention (`:119`).
- `kernel/src/functionize/jit.rs` — `JitStep`/`JITModule`/`extern
  "C" fn(*const i64)->i64` (`:108-110`), `compile_step` (`:184-187`),
  Cranelift backend (`:23-26`).
- `compiler/compiler.ev`, `compiler/sample.ev`; CLAUDE.md
  (`compiler.smt2` precedent, manifest contract).
- [[feedback-aot-over-runtime-disk-cache]],
  [[project-constraint-model-compilation]].
