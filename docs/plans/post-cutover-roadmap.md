# Post-cutover roadmap — what to do AFTER `rm -rf bootstrap/`

The bootstrap-deletion cutover is gated on one compiler fix
(`compiler/translate_ctor.ev` — see STATE.md "THE single ctor-arg
blocker"). This file is the plan for the *next* phase: how the
kernel itself shrinks toward zero.

Today the kernel is ~880 LOC of Rust: trampoline + libffi + Z3
wrapper. Each piece has a feasibility study; this roadmap names
them, sequences them, and quotes their verdicts.

## The four phases (in dependency order)

### Phase 1 — Z3 wrapper in Evident
**Plan:** [`wave-5a-z3-in-evident.md`](wave-5a-z3-in-evident.md)
**Verdict:** `MEDIUM` (split — solve half HIGH, model-readback half
BLOCKED on two named capabilities).

Replace the Rust calls to the ~70 Z3 C functions in
`kernel/src/{tick,functionize/*}.rs` with `LibCall("libz3", "...")`
from Evident. Solve loop (parse → assert → check → read sat int) is
HIGH; decoding the model AST into effect values needs new primitives.

### Phase 2 — Trampoline + libffi in Evident
**Plan:** [`wave-5b-trampoline-ffi-in-evident.md`](wave-5b-trampoline-ffi-in-evident.md)
**Verdict:** Path A `HIGH` (libffi stays a pure C dep, just call its
entry points from Evident). Path B `MEDIUM` (replace libffi entirely
with mmap+mprotect+codegen — bigger lift, Apple Silicon W^X / MAP_JIT
story is the real cost).

Ship A first: it depends only on phase 1's `Z3_solver_check`
machinery plus a trivial `dlsym_addr` addition. B reuses A's
handle-passing substrate and is the prerequisite for phase 3 option Z.

### Phase 3 — Functionizer in Evident
**Plan:** [`wave-5c-functionizer-in-evident.md`](wave-5c-functionizer-in-evident.md)
**Verdict:** Recognizer half `HIGH` (tree-walk over Z3 ASTs — same
shape as validate_walk and desugar). Codegen half splits:
| Codegen option | feasibility | one-line note |
| -------------- | ----------- | -------------- |
| X — emit asm, shell out to `as`, dlopen | `HIGH` | fastest to "no Rust"; needs phase-2 `dlopen` |
| Y — link libLLVM via FFI | `LOW` | heavy permanent dep, only edge is quality |
| Z — self-hosted ISA models | `MEDIUM` | "Evident all the way down" endgame; needs phase-2 exec pages |

Prototype X first. Z is the final form.

### Phase 4 — AOT binary cache
**Plan:** [`wave-5d-aot-binary-cache.md`](wave-5d-aot-binary-cache.md)
**Verdict:** `MEDIUM` (HIGH for the side-car format on all-scalar
programs; residual-step dependency on a live Z3 context is what keeps
it from HIGH across the board).

AOT functionization with a disk cache **is** compilation to a binary
— same operation as today's JIT, moved from per-run to build time and
persisted. Once phase 3 lands, this is what materializes the
generated code into something the kernel just loads.

## Cross-wave dependency notes

- Phase 1's FFI sugar must cover the Z3 **tactic/goal** API
  (`Z3_mk_goal`, `Z3_mk_tactic`, `Z3_tactic_apply`) in addition to
  context/solver lifecycle. The functionizer recognizer needs it.
- Phase 2 must expose `dlopen`/`dlsym` (for phase 3 option X) and
  `mmap`+`mprotect` (for option Z).
- Phase 3 has a chicken-and-egg property: the recognizer needs to
  *functionize itself*. Solve by keeping the Rust stage-0
  functionizer until phase 4 closes the loop.

## End state

Kernel/ has been shaved down to the trampoline shim and a couple of
syscalls; everything that used to be `Z3_*` or `libffi_*` is now an
Evident program calling `LibCall("libz3", "...")` / `LibCall("libc",
"dlsym")`. The "kernel + compiler.smt2" pair is no longer just the
compiler — it is the whole runtime.

## Where to pick up

The ordering above is a *suggestion*, not a constraint. Phase 1 is
the obvious next step because everything else depends on it, but a
session that just wants to ship the cutover should focus on
`compiler/translate_ctor.ev` first and not touch any of phases 1-4.
The feasibility plans are durable reference; the actual ordering of
phase work will follow what unblocks the most LOC reduction per
session.

---

## Self-hosting deliverable: reify the AST + a pass phase (host passes in Evident again)

**Decision (2026-06-09):** the compiler-pass infrastructure that used to
live in `stdlib/passes/*.ev` — self-hosted AST→AST transforms (`desugar`,
`generics`, `validate`, `seq_chains`, …) — should be **rebuilt as part of
self-hosting**, in `compiler2/passes/`, not as transitional shell scripts
and not as a one-off bolt-on.

### Why this is a self-hosting item, not a refactor
compiler2 today is a **streaming translator**: it fuses parse → lower →
translate, lowering each body line straight into the `C2Items` work-item
stream. It never reifies a full `BodyItemList`/`SchemaDecl` AST (only
transient `Expr` payloads of single work-items), so there is **no
parse/passes/translate seam** for a pass to slot into. The old design
*had* that seam (in the deleted bootstrap Rust): parse → marshal AST →
run pass-programs (driven by a `run()` nested-FSM-to-fixed-point driver)
→ translate. Reviving passes means re-introducing that seam.

### What survives / what's gone
- LIVE: the AST mirror — `compiler/parser.ev` still defines the cons-list
  `Expr`/`ExprList`/`BodyItem`/`BodyItemList`/`SchemaDecl` enums, and
  compiler2 parses into them.
- LIVE: the pass template — `git show <pre-deletion>:stdlib/passes/seq_chains.ev`
  is the canonical shape: a `fsm` stack-machine (Seed→Step→Done) that pops
  the AST list head, transforms, recurses on the tail, with inline `claim`
  unit tests that seed an AST and assert the output. `desugar.ev` is a
  Seq-concat *flattening* pass — the closest precedent to a bounded-Seq
  lowering. Discipline: Evident owns the recursion; string-leaf keying
  stays out of the per-tick solve (the Z3 string-theory blow-up).
- GONE: the `run()` driver (nested-FSM-to-fixed-point), the AST↔Value
  marshaler (Rust), and the orchestrator that staged parse→passes→translate.

### The deliverable
Give compiler2 a real phase structure — `parse → reify BodyItemList AST →
run passes → lower → emit` — and a place for passes (`compiler2/passes/`).
The first citizens are the two shell transforms that today run pre-oracle
in `flatten-evident.sh` (both carry `# TODO: rewrite in Evident` headers
pointing here — they are interim by charter, not by accident):
- **`scripts/passes/expand-fsm-autocarry.sh`** — `fsm` → `claim` + `_x` carry-dual
  synthesis + composition threading.
- **`scripts/passes/lower-bounded-seq.sh`** — bounded-`Seq` → flat-scalar lowering.
  No longer a PoC: as of 2026-06-09 it is the production encoding of every
  compiler2 registry (evt/uev/stv + 13 more families), with a real rule
  inventory the Evident port must reach parity with: decl/dual/literal/
  append/hold, range-`∀` slot instantiation, len-guarded + literal-range
  `∃`, index/card substitution, Int `∀`/membership unrolls, the
  **keyed-projection pair** (`∀`-pin + `¬∃`-default → covered select chain;
  the functionizer-safe lowering of registry lookups), the **guarded pin
  family** (the pair generalized to scalar/mixed same-out families with a
  mandatory negated-disjunction default and loud refusals — missing
  default, hold-in-disguise, double cover;
  `docs/plans/guarded-pin-family-lowering.md`), and the loud
  completeness check. Conformance for the port: `tests/compiler2_units/
  seq_lowering/` + `tests/seq/` + byte-identical flatten output on
  `compiler2/driver.ev`.
- the old `desugar`/`generics`/`validate` family, ported onto the new seam.

Either re-add a minimal `run()` kernel capability (clean standalone
`AST → AST` pass-programs, the proven model) or express passes as phases
inside compiler2's tick loop — that execution-model choice is the first
sub-task. **Note the chicken-and-egg:** a compiler2 pass can lower bounded
`Seq`s in *user programs* immediately, but cannot touch compiler2's *own*
registries (compiled by the frozen oracle) until self-hosting closes the
loop — so the registry cleanup is gated on this work, or done as interim
`Array`+`len` until then.

### Measured: carried registries resist records / Seq / passes alike (don't re-probe)
Under the frozen oracle + functionizer (probed 2026-06-09), a *carried*
collection of records or a *carried* `Seq`/Array+len cannot replace the
hand-unrolled numbered scalars (`evt_n0..5`, `uev_*`, …) without a
regression:
- **Records as carried state** (`Seq(EnumVariantVal)` or 6 record slots) →
  the oracle drops the `w_need` ternary (manifest already ~1505
  state-fields; more carried field-consts tip the flatten/expand translator).
- **`Seq` membership `∈` is dropped** by the oracle ("couldn't translate to
  Bool") — and membership is the registry's core lookup. (`#`, `xs[i]`,
  `∀`, `++` DO translate.)
- **A carried `Seq` won't functionize** — "extract_program: an output had no
  covering assignment", 9/9 residual, Z3 invoked every tick. Fatal in the
  compiler's hot loop.
So the numbered-scalar unroll is the ONLY carried-registry encoding that
stays on the functionizer fast path today.

**Resolution (2026-06-09, later the same day):** the registry cleanup DID
land — not by changing the encoding, but by putting the unroll behind a
pre-oracle source transform. `scripts/passes/lower-bounded-seq.sh` lets the
source say `evt ∈ Seq(EnumVariantVal)` / `∀ k ∈ {0..5} : …` while the
oracle still sees the numbered scalars. The measured walls above are
walls of the *oracle-visible encoding*, and they stand; the *surface* is
free. What remains for this roadmap item is debt relocation: the
transform is ~450 lines of awk, and self-hosting means porting it (and
`expand-fsm-autocarry.sh`) into `compiler2/passes/` per the deliverable
above.
