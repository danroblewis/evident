# legacy-rust/ — read-only reference (Rust extracts from old branches)

Same freeze rules as `bootstrap/` and `legacy-python/`: **read, do
not edit, do not bug-fix, do not compile into any crate.** These
files are not on any critical path. They are reference material for
ideas that exist nowhere else in the current tree.

## `functionizer/` — the Z3 "macro-finder" functionizer

Extracted from branch `feat/compile-constraints-to-programs`
(tip `5f0e066`, the `compile-constraints-to-programs` plan, rounds
20–25). This is **one** of the three functionizers that existed in
that lineage; the other two — **symbolic regression**
(`functionize/symbolic.rs`) and **LLM** (`functionize/llm.rs`) —
were deliberately NOT brought in. See `docs/plans/architecture-invariants.md`
("Functionizability over Z3-fast") and
`docs/plans/functionizer-integration.md` for why this one matters.

### What "macro-finder" means here

The PLAN (round 0) listed *"Z3 `macro_finder` + `elim-predicates` in
our tactic chain to catch quantified function definitions
automatically"* as a candidate technique. What actually shipped (rounds
22–24) is the **Z3-tactic functionizer**: it runs Z3's preprocessing
tactic chain over a claim body, then partitions the simplified
assertions into per-output function definitions — exactly the
"promote constraint-defined symbols to functions" idea the
macro-finder tactic embodies. `simplify` + `propagate-values` is the
shipped chain; `macro-finder` / `elim-predicates` / `solve-eqs` were
probed in `bench_tactics.rs` and the design notes. This is the
"macro-finder version" the extraction task means, as opposed to the
SR and LLM variants.

### Files

| File | Lines | Role |
|---|---|---|
| `src/functionizer.rs`   | 51   | The `Functionizer` / `CompiledFunction` trait. A strategy turns an extracted `Z3Program` into a callable artifact, or returns `None` to fall through to a full Z3 solve. |
| `src/z3_program.rs`     | 81   | The `Z3Program` IR — a claim body simplified by Z3 tactics, partitioned into per-output assignments (`Z3Step::Scalar`/`Seq`/`Guarded`) + consistency checks + residual predicates. This is "the function definition" the macro-finder produces. |
| `src/z3_eval.rs`        | 1208 | **The core.** `simplify_assertions` runs the tactic chain; `extract_program` partitions the result into a `Z3Program`; `eval_program` walks it natively (the slow-path interpreter). The macro-finder logic lives here. |
| `src/decompose.rs`      | 321  | Union-find over the free variables of the normalized assertions → independent separable sub-models. Structural, no `check()` calls. Lets each component functionize independently. |
| `src/cranelift.rs`      | 1536 | The default `Functionizer`: lowers a `Z3Program`'s ASTs to Cranelift IR → native machine code (round 24). The "compile to a real function" backend that consumes the extractor's output. |
| `src/mod.rs`            | 27   | The factory (`default() = Cranelift`). NOTE: declares `pub mod symbolic; pub mod llm;` — those modules were intentionally NOT extracted (SR + LLM variants). Kept verbatim only to document how strategy selection worked. |
| `src/bench_tactics.rs`  | 96   | In-process bench of tactic chains incl. `simplify,...,elim-predicates`. Shows the `macro-finder`-adjacent tactic exploration. |
| `src/tactic_probe.rs`   | 46   | Minimal `Tactic::new(ctx, "...")` probe harness. |
| `tests/*.rs`            | —    | Extraction + JIT fixtures: `decompose.rs`, `cranelift_jit_hello.rs`, `jit_minimal.rs`, `jit_gap_*` (div/mod, guarded-seq, str-concat), `per_component_jit.rs`, `seq_record_jit.rs`. |
| `docs/compile-claims-to-functions.md` | 1663 | The architectural design doc — the full menu of optimizations + the native-compile path in depth. The single most important doc here. |
| `docs/round-23-z3-functionizer-integration.md` | 146 | How the extractor wired into `rt.query` behind `EVIDENT_FUNCTIONIZE_Z3`. |
| `docs/round-24-cranelift-jit.md` | 158 | The Cranelift codegen round. |
| `docs/bench-functionize.md` | 151 | Measured numbers. |

### What is NOT here (intentionally dropped)

- `functionize/symbolic.rs` — symbolic-regression functionizer
  (genetic programming over sampled input→output pairs). Different
  variant; skipped per the extraction task.
- `functionize/llm.rs` — LLM-driven functionizer (prompt an LLM to
  write the Rust function, compile via `rustc`, validate). Skipped.
- The whole `runtime/` crate around these files. These are the
  load-bearing modules only, not the engine they plugged into.

## Why these are here

The current project is deleting the Rust compiler and self-hosting in
Evident. The functionizer is the *post-load optimizer we trust* — it
makes "what's slow in Z3 today" irrelevant if the Evident shape
functionizes cleanly. We keep its source as reference so future
implementation choices (cons-list vs Z3-Seq, fixed-arity match vs
variadic Seq ops, which pin mechanism) can be made
functionizability-aware. See `docs/plans/functionizer-integration.md`.

## Freeze status

Read-only reference. When the functionizer is re-implemented (against
the kernel + SMT-LIB pipeline, not this old `runtime/` crate) and the
relevant ideas have been transcribed or explicitly rejected, this
directory may be removed.
