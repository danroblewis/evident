# Feasibility — Functionizer in Evident (wave 5c)

**Diagnostic / design wave. No code changed.** Deliverable = this report.

Headline: **the recognizer half is HIGH-feasibility — it is a tree-walk
over Z3 ASTs, the exact shape Evident already self-hosts (validate_walk,
desugar stack-FSMs, translate_ctor). The codegen half is the hard part:
Option X (emit asm → `as` → `dlopen`) is HIGH and ships "no Rust"
fastest; Option Z (self-hosted ISA models) is MEDIUM and is the
"Evident all the way down" endgame but needs wave-5b exec pages; Option
Y (libLLVM) is LOW given our constraint — its only edge is codegen
quality, which we are explicitly told not to chase (parity is the
target). Prototype Option X first.**

Cites: `kernel/src/functionize/{mod,jit,eval}.rs`;
`docs/plans/functionizer-integration.md`;
`docs/plans/grammar-wave4e-perf-diagnostic.md`,
`docs/plans/wave-4r-pertick-hot-shapes.md`; `compiler/translate_ctor.ev`;
memory [[project_constraint_model_compilation]],
[[project_smtlib_compile_target]], [[project_nested_fsm_implementation_plan]],
[[project_validate_recursive_cutover]], [[project_functionizer_walk_result]].
Assumes wave-5a's Z3 FFI sugar (LibCall → libz3, ASTs as opaque Int
handles) is in place.

---

## What the functionizer is, mechanically

Two halves, both load-time / one-shot (never per-tick):

1. **Recognizer** (`functionize/mod.rs`): `simplify_assertions` (Z3
   tactic chain `simplify` + `propagate-values`) → `flatten_conjunctions`
   → `extract_program` (partition body assertions, keyed by manifest
   outputs, into `Step`s: `Scalar` / `Seq` / `Guarded`) → reachability +
   `topo_order`. Pure data manipulation over Z3 ASTs. Refuses (`None` →
   fall through to Z3) on any uncovered output or cycle.
2. **Codegen** (`functionize/jit.rs`): `compile_step` lowers a scalar
   Int/Bool `Step` AST to Cranelift IR → native `fn(*const i64) -> i64`.
   Scope: `+ - * unary-`, comparisons, `and/or/not/=>`, `ite`, and
   `select`/accessor over fixed-size record-Seqs. Everything else
   interprets (`eval.rs`) or stays on Z3.
3. **Verify**: before committing, run the extracted program against a
   real Z3 solve on tick-0 and tick-1; any mismatch disables the fast
   path. This is the soundness net (§4).

---

## Section 1 — Recognizer half in Evident

### Feasibility: HIGH

`extract_program` is a tree-walk with pattern matching that reads
`decl_kind`, `children`, `ast_app_name`, `numeral_i64`, sort kind, and
datatype-accessor metadata off each Z3 AST node, then bins the node into
a `Step`. Evident already does exactly this shape:

- `translate_ctor.ev`'s `RenderExprL0/L1/L2` walk a `TokenList` with
  `match` on token variants — the same dispatch as `match decl_kind`.
- `validate_walk` ([[project_validate_recursive_cutover]]) and the
  desugar gather/flatten passes ([[project_desugar_selfhost_result]])
  are full **stack-FSM** AST walks already cut over to Evident-only.

The one structural difference: a Z3 AST has **unbounded depth**, and
Evident claims can't self-recurse. So the recognizer is **not**
depth-unrolled levels (the translate_ctor `L0/L1/L2` ceiling) — it must
be the **stack-FSM walk** ([[project_nested_fsm_implementation_plan]]):
a work-stack of AST handles, one node classified per tick, children
pushed. `extract_program` is itself flat (it iterates a list of
top-level assertions, not a deep recurse), so the unbounded recursion is
only in the *guard/equality sub-expression* inspection
(`mentions_name`, `split_not_eq_bool_both`) and in `topo_order`'s
reachability — both already proven self-hostable as stack walks.

Z3 ASTs marshal as **opaque Int handles** (wave-5a). The recognizer
never dereferences them in Evident; it asks libz3 via LibCall sugar
(`Z3_get_decl_kind(h) → Int`, `Z3_get_app_arg(h, i) → Int`,
`Z3_get_numeral_int64`, …). `decl_kind` becomes an Int compared against
the `Z3_decl_kind` enum constants.

### Sketch: `MatchFunctionizableStep`

```evident
enum StepBody =
    SScalar(Int)              -- one AST handle
    SSeq(Seq(Int))            -- length-pinned element handles
    SGuarded(Seq(Branch))

type Branch(guard ∈ Int, neg ∈ Bool, body ∈ StepBody)
type Step(var ∈ String, body ∈ StepBody)

-- Classify ONE simplified, flattened top-level assertion `a` (an opaque
-- Z3 ast handle) against the output set, emitting a (var, body) or
-- ok=false (→ residual predicate, the caller keeps it for eval).
claim MatchFunctionizableStep
    a ∈ Int                              -- Z3_ast handle
    outputs ∈ String                     -- "name;name;…" output set
    out ∈ Step
    ok ∈ Bool

    dk ∈ Int = Z3DeclKind(a)             -- LibCall → libz3
    -- (=> P Q) or (or X Q): a guarded consequent constraining an output
    is_impl ∈ Bool = (dk = DK_IMPLIES)
    is_or   ∈ Bool = (dk = DK_OR)
    -- (= var expr): scalar / seq-pin / len-pin
    is_eq   ∈ Bool = (dk = DK_EQ)

    lhs ∈ Int = Z3Arg(a, 0)
    rhs ∈ Int = Z3Arg(a, 1)
    l_name ∈ String = Z3AppName(lhs)     -- "" if not a 0-arity const
    is_out ∈ Bool = (sub(l_name) ∈ outputs)

    -- scalar: (= out expr), out ∉ expr  (mentions-check = a sub-walk)
    is_scalar ∈ Bool = (is_eq ∧ is_out ∧ ¬MentionsName(rhs ↦ rhs, nm ↦ l_name))
    out = (is_scalar ? Step(l_name, SScalar(rhs)) : …guarded/seq arms…)
    ok  = (is_scalar ∨ …)
```

The full claim mirrors `extract_program`'s arm order: guarded first
(`try_record_guarded`), then the `(not (= bv …))` XOR rewrite
(`split_not_eq_bool_both`), then len-pin / select-pin (`match_len_pin`,
`match_select_pin`), then scalar, else residual. `MentionsName` and the
seq-collection (`collect_seq_in_and`) are sub-walks over the same handle
mechanism. `topo_order` is already a solved Evident shape — it is
`stdlib/toposort.ev` ([[project_toposort_cutover_result]]), applied to
the output dependency edges the reachability walk emits.

**Caveat (honest):** the recognizer is itself an Evident program. Run
unfunctionized it inherits the string/seq-theory blowup that makes
`compiler.smt2` minutes-slow (wave-4e/4r). But it runs **once at load**,
which fits the AOT-over-runtime priority
([[feedback_aot_over_runtime_disk_cache]]), and the bootstrap escape is
standard: AOT-functionize the recognizer itself with the Rust **stage-0**
functionizer ([[project_smtlib_compile_target]]: "the functionizer is
our bootstrap compiler"). Name this circularity; don't pretend it isn't
there.

---

## Section 2 — Codegen half: three options

**Option X — emit asm text → `as` → load.** Evident emits assembly text
for a scalar step (the `emit()` lowering in `jit.rs` rewritten as a
text-producing tree-walk — Evident already produces SMT-LIB text the
same way), writes a `.s` to a temp path (`WriteFile`), invokes the
system assembler via `LibCall("libc","system",⟨"as -o step.o step.s"⟩)`
+ a link to a `.dylib`, then `LibCall(libz3-style)` → `dlopen`/`dlsym`
to get the function pointer the trampoline calls. *Viability:* high —
the encoding knowledge stays in `as`; the only new Evident is text
emit, which is the recognizer's twin. Batch all steps of one program
into a **single** `.s` to amortize the one process spawn. *Dev time:*
~2–3 sessions (text emit + the assemble/dlopen glue, gated on wave-5b's
dlopen sugar).

**Option Y — libLLVM via FFI.** LibCall into the **LLVM-C** API
(`LLVMContextCreate`, `LLVMBuildAdd`, `LLVMOrcLLJIT…`) — note this is a
stable **C ABI**, not C++, so FFI-reachable in principle. Full
optimizer for free. *Viability:* low **for us** — the dep is huge and
version-fragile, the C API surface is hundreds of pointer-heavy calls
(far past wave-5a's scope), and its sole advantage is codegen *quality*,
which the task forbids chasing (parity, not improvement). It buys
nothing X doesn't, at much higher cost. *Dev time:* ~6–10 sessions and
a permanent heavy dependency.

**Option Z — self-hosted ISA models.** A per-architecture Evident model
of the instruction encoding (x86-64 ModR/M / REX bytes; aarch64 fixed
32-bit words), emitting a byte `Seq`, loaded into `mmap`'d executable
pages (`mmap` + `mprotect(PROT_EXEC)` via LibCall). This is precisely
[[project_constraint_model_compilation]]'s vision — "asm is a compiled
view of a constraint model" — and the cleanest "Evident all the way
down" story (no external assembler, no LLVM). *Viability:* medium — a
large undertaking, one model per arch, and it depends on wave-5b's
trampoline giving us executable pages. But the scalar-Int/Bool ISA
subset the JIT actually needs (`add/sub/imul/neg/cmp/and/or/xor/csel` +
load) is small — a few dozen encodings per arch, not the whole ISA.
*Dev time:* ~4–6 sessions for the first arch, +2–3 per additional arch.

---

## Section 3 — Hybrid path (you don't pick one)

1. **Ship X first.** It clears "no Rust" — the Cranelift dep leaves the
   kernel, codegen logic moves to `compiler/*.ev`. Correctness rides on
   the verify gate (§4), so a weak/slow X is still *sound*.
2. **Land Z for the hot arch later.** Once wave-5b exposes exec pages,
   replace X's `as`+`dlopen` round-trip with direct byte-emit + mmap on
   the host arch (x86-64 or aarch64), eliminating the per-load process
   spawn and disk write. X stays the fallback for arches without a Z
   model. This is the "Evident all the way down" milestone.
3. **Y stays a documented non-choice.** Given parity-not-quality, Y
   earns its keep only if a future requirement adds real optimization
   (vectorization, etc.). Until then, "complex shapes" don't go to Y —
   the recognizer already refuses them to the **interpreter / Z3**
   (`eval.rs`), which is the correct home for non-scalar bodies.

The interpreter (`eval.rs`) is the floor under all three: any step
codegen declines simply interprets, exactly as today.

---

## Section 4 — Verification story

Today (`mod.rs:1062`): after building the program, run it AND a real Z3
solve on tick-0 and tick-1, compare state-fields + effects with
`compare_sv`; one mismatch → `refuse!` → whole run reverts to Z3. This
soundness net is **non-negotiable** and ports directly:

- The Z3 solve is reachable from Evident via the same libz3 LibCall
  sugar (`Z3_solver_check` + model reads) the recognizer uses — this is
  wave-5a's surface.
- Express the check as a `VerifyStep` claim: build the two sample input
  envs (`is_first_tick`=true/false, `_<name>` carries from the tick-0
  result, `last_results` empty per `build_inputs`), evaluate the
  candidate program and the Z3 solve, and assert per-output equality.
  On any inequality, emit a **refuse marker** and the kernel keeps the
  Z3 path — identical fall-through discipline.
- Output comparison (`compare_sv`) is scalar/string/datatype equality —
  Evident `=` already covers these.

The known refusal triggers (`str_from_int`, `last_results` decode,
symbolic-Seq state, `div`/`mod`) stay refusals: the recognizer never
emits a step for them, so the verify gate never sees them. The behavior
contract is already encoded portably in `runtime-contract/`
([[project_behavior_contract_oracle]]) — reuse those fixtures as the
equivalence corpus.

---

## Section 5 — Verdict + roadmap

| Half / option | feasibility | note |
| --- | --- | --- |
| Recognizer | **HIGH** | tree-walk; proven shape (validate/desugar/toposort already self-hosted). Needs wave-5a Z3 FFI sugar incl. the **tactic/goal API** (`Z3_mk_goal`, `Z3_mk_tactic`, `Z3_tactic_apply`), which is *additional* surface beyond 5a's lifecycle focus — flag for 5a. |
| Codegen X (`as`+dlopen) | **HIGH** | fastest to "no Rust"; needs wave-5b `dlopen` sugar. |
| Codegen Y (libLLVM) | **LOW** | only edge is quality, which is out of scope; heavy permanent dep. |
| Codegen Z (ISA models) | **MEDIUM** | endgame "Evident all the way down"; needs wave-5b exec pages; per-arch. |

**Recommend prototyping Option X first** — it reaches the project's
actual goal ("no Rust beyond a minimal kernel") with the least new
machinery, and the verify gate makes a rough first cut sound.

**3-step roadmap:**

1. **Recognizer in Evident.** Implement `MatchFunctionizableStep` +
   `topo_order` reuse + reachability as stack-FSM walks over libz3
   handles. Verify it produces **byte-identical `Step` partitions** vs
   the Rust `extract_program` on `tests/kernel/test_functionizer_basic.ev`
   and `test_functionizer_seqs.ev`. (Blocked until wave-5a adds the
   tactic API to the FFI sugar.)
2. **Codegen Option X + verify gate.** Emit asm text for scalar steps,
   batch-assemble + `dlopen`, wire the `VerifyStep` tick-0/tick-1 gate,
   prove parity on the `runtime-contract/` fixtures. (Blocked until
   wave-5b exposes `dlopen`.)
3. **Replace the Rust `functionize/` module**, then add **Option Z** for
   the host arch once wave-5b's trampoline lands executable pages,
   keeping X as the cross-arch fallback and `eval.rs`'s interpreter as
   the universal floor.

**Named cross-wave blockers:** (a) wave-5a must cover the Z3 *tactic/goal*
API, not just context/solver lifecycle; (b) wave-5b must expose
`dlopen`/`dlsym` (Option X) and `mmap`+`mprotect` (Option Z); (c) the
recognizer-functionizes-itself bootstrap requires the Rust stage-0
functionizer to remain until AOT closes the loop
([[project_smtlib_compile_target]]).

---

## Note: port the diagnostic + perf instrumentation too

The functionizer that moves into Evident in this wave currently carries a
set of **diagnostic env-var hooks** implemented in the Rust kernel
(`kernel/src/functionize/`, `kernel/src/tick.rs`):

- `EVIDENT_FUNCTIONIZE_STATS` (summary/verbose) — the per-run `[functionizer]`
  totals + per-step shape report.
- `EVIDENT_FUNCTIONIZE_TIMING` (+ `_BANDS`, `_REPS`) — the per-constraint
  *marginal* tick-0 solve-cost band profiler, with variable attribution.
- `EVIDENT_FUNCTIONIZE_DUMP` — the flat `flat[i] = <expr>` constraint listing.
- `EVIDENT_FUNCTIONIZE_WHY` / `_TRACE` — refusal reasons / per-tick timing.

These are not cosmetic: the perf toolchain depends on them —
`scripts/perf-profile.sh` (per-constraint cost ranking + `--bisect`) and
`scripts/functionization-gate.sh` (the `≠`-disequality regression gate, see
[[types-carry-invariants]] / the type-invariant perf caveat in CLAUDE.md).
**When the Rust `functionize/` module is replaced, re-expose equivalent
instrumentation in the Evident implementation, or both scripts go dark and
the project loses its only window into per-constraint solve cost.** The
`z3 -st` search-space half of `perf-profile.sh` is external (the `z3` CLI)
and survives the transition; only the kernel-side TIMING/DUMP/WHY hooks
need re-adding.
