# compiler.ev perf diagnostic — wave 4e (measure before refactoring)

**Diagnostic-only session. No source touched** (`compiler/`, `kernel/`,
`bootstrap/`, `stdlib/` all unchanged). Deliverable = this doc.

Headline: **The functionizer extracts NOTHING (0/37 123 steps) — the whole
monolithic `main` body is re-solved by Z3 every tick at a flat ~490 ms,
independent of input. The recommended refactor is to cut LEX tick COUNT
(bulk whitespace/comment skip), not per-tick cost — because functionizing
this body has a hard ceiling (only 27.5 % of its assertions are JIT-eligible;
72.5 % are functionizer-hostile String / cons-list / datatype-match shapes).
Hypothesized speedup on a real comment-heavy file: ~3×.**

Cites: `CLAUDE.md` §"Functionizer diagnostics"; `docs/plans/grammar-wave4d.md`;
`docs/plans/blocked-grammar-wave4d.md` (Blocker 5); `kernel/src/functionize/mod.rs`
§"Diagnostics" + `extract_program`; `scripts/build-compiler-smt2.sh`;
`tests/kernel/test_hello.ev`; `compiler/compiler.ev`.

---

## Section 1 — Setup

`compiler.smt2` rebuilt from the current `compiler/compiler.ev` via
`scripts/build-compiler-smt2.sh` (bootstrap, one final time):

```
── build-compiler-smt2 ──
  source : compiler/compiler.ev          (593 lines of Evident)
  output : compiler.smt2
  size   : 9 603 701 bytes / 200 348 lines
```

Body shape (`grep` over `compiler.smt2`):

| metric                 | value   |
| ---------------------- | ------- |
| `(assert …)` lines     | 37 122  |
| `(declare-fun …)`      | 37 286  |
| manifest state-fields  | **162** (+ `effects` ⇒ 163 outputs) |
| `max-effects`          | 16      |

A 593-line compiler expands to ~37 k assertions — a ~60× blow-up, because every
composition site (`MembershipStep`, `CtorMembershipStep`, `SeqMembershipStep`,
`MatchMembershipStep`, `VariantText`, `EnumDeclSmtlib`, the six lexer
char-classifier claims, …) inlines its full sub-claim body. This is the static
"code size" of the compiler in SMT-LIB, and Z3 re-evaluates **all** of it every
tick.

The compiler reads its input from `/tmp/compiler-input.ev` via a `ReadFile`
effect on tick 0 (NOT stdin — `compiler/compiler.ev:64`). All runs below write
the input there, then `./kernel/target/release/kernel compiler.smt2`.

---

## Section 2 — Functionizer load report (short input)

Input (`/tmp/compiler-input.ev`, 28 chars):

```
claim foo
    x ∈ Int = 5
```

```bash
EVIDENT_FUNCTIONIZE_STATS=verbose ./kernel/target/release/kernel compiler.smt2
```

Verbose load report (stderr), quoted verbatim:

```
[functionizer] load:
  body asserts: 37123
  not functionized — fast path disabled; all 37123 asserts run on Z3 each tick
  reason: extract_program: an output had no covering assignment
[functionizer] not functionized (extract_program: an output had no covering
  assignment); 37123 total / 0 JIT / 0 interp / 37123 residual;
  21203.2 ms total (0.0 ms func / 20469.5 ms z3)
```

Reading it:

- **Total assertions extracted: 37 123** (after `simplify` + `propagate-values`
  + conjunction-flatten).
- **JIT = 0 / interp = 0 / residual = 37 123.** The functionizer **refuses the
  whole program** — there are no per-step rows because `extract_program`
  returned `None` before any step was built.
- **No per-step shape categories are emitted** (`binop` / `ite` / `select` /
  `accessor` / `guarded-seq` / `seq-literal` / `unfunctionizable`) precisely
  *because* extraction never produced a step list. Every assertion is residual:
  all 37 123 go to Z3 each tick.
- **Why residual:** `extract_program` (kernel/src/functionize/mod.rs:682) returns
  `None` when any manifest output lacks a covering assignment **or** the output
  dependency graph has a cycle (`topo_order`, line 793/828). Both map to the
  same `refuse!` string.

### Which output refuses? (investigation)

`EVIDENT_FUNCTIONIZE_DUMP=1` dumps the simplified+flattened assertions. Over the
reconstructed 37 123 assertions:

- **All 163 outputs DO carry a covering `(= var expr)` (or guarded, for
  `effects`).** A naive substring check flags only `manifest` (its body contains
  the *literal* `";; manifest:"` — a false positive; the kernel matches Z3 AST
  app-names, not substrings), `effects` (covered via the guarded shape, by
  design), and `has_input` / `was_collecting_str` (reversed `(= expr var)`
  forms, covered by the `r`-side branch at mod.rs:719). So **no output is
  genuinely uncovered by the scalar/guarded rule.**
- **No cycle was found among the output scalar-defs** by an external DFS.

The exact failing output cannot be pinned from outside the kernel — the refuse
string is generic and the only way to name it is a one-line kernel diagnostic
(`eprintln!` the output that `build_body` returned `None` for, or the residual
cycle), which is a frozen-tree edit out of scope here. **Strong suspects** are
the two documented functionizer-hostile shapes that live in `main`:

- `read_result = match last_results[0] { StringResult(s) ⇒ s ; _ ⇒ … }`
  — a `last_results` datatype-decode. The FTI honesty audit
  (`memory: project_fti_honesty_audit_result`) records the functionizer
  *refuses* `last_results` `DT_IS` decode.
- `manifest`'s `str_from_int(_mxe)` — the same audit records `str_from_int` as a
  refuse shape.

Either is enough to make `extract_program` reject the whole program, since the
gate is **all-or-nothing** (mod.rs:733 — one uncovered output ⇒ refuse the
entire fast path).

---

## Section 3 — Per-tick trace (short input)

```bash
EVIDENT_FUNCTIONIZE_STATS=verbose EVIDENT_FUNCTIONIZE_TRACE=1 \
  ./kernel/target/release/kernel compiler.smt2
```

40 ticks total. Sample:

```
[functionizer] tick 0:  0.00ms func / 613.31ms z3 / 0.05ms dispatch
[functionizer] tick 1:  0.00ms func / 473.80ms z3 / 0.11ms dispatch
[functionizer] tick 2:  0.00ms func / 473.29ms z3 / 0.02ms dispatch
...
[functionizer] tick 38: 0.00ms func / 487.19ms z3 / 0.02ms dispatch
[functionizer] tick 39: 0.00ms func / 539.25ms z3 / 0.03ms dispatch
[functionizer] … 37123 total / 0 JIT / 0 interp / 37123 residual;
  20601.8 ms total (0.0 ms func / 19894.4 ms z3)
```

- **Z3 is ~96.5 % of every tick** (19 894 ms z3 / 20 602 ms total). `func` and
  `dispatch` are ~0 ms — there is no functionized work and effect dispatch is
  trivial.
- **Per-tick cost is FLAT at ~490 ms** (tick 0 is 613 ms — it also runs the
  `ReadFile`). There is **no per-tick growth**: the cost is the fixed-size
  monolith re-solve, not a function of accumulated state size.

---

## Section 4 — Real-shaped, larger input

Input (`/tmp/compiler-input.ev`, 124 chars / 6 lines — an enum + a claim with
four memberships):

```
enum Color = Red | Green | Blue
claim demo
    a ∈ Int = 5
    b ∈ Int = 10
    c ∈ Int = 15
    flag ∈ Bool = true
```

It compiles to correct SMT-LIB (manifest + prelude + `Color` datatype +
declares/asserts for `a`/`b`/`c`/`flag`). Timings:

| metric                | value                    |
| --------------------- | ------------------------ |
| ticks                 | **166**                  |
| wall clock            | **84 s**                 |
| total Z3              | 80 922 ms                |
| mean ms/tick (Z3)     | **487 ms** (80 922 / 166)|
| per-tick growth       | none (flat)              |

Cross-check with Section 3 (28 chars → 40 ticks; 124 chars → 166 ticks): tick
count scales **linearly** at ≈ 1.3 ticks/source-char (lex ≈ 1 tick/char +
reverse ≈ 1 tick/token + parse ≈ 1 tick/membership), and per-tick cost is
constant. So:

```
total_time ≈ 490 ms × (≈1.3 × source_chars)
```

**Extrapolation to `test_hello`** (4137 chars flattened): ≈ 5000+ ticks ×
490 ms ≈ **40+ minutes** — matching the "intractable" call in
`docs/plans/blocked-grammar-wave4d.md` (Blocker 5).

---

## Section 5 — The bottleneck + ONE recommendation

### Bottleneck

The dominant cost is **Z3 re-solving the entire 37 123-assertion monolithic
`main` body every tick**, at a flat ~490 ms, with **0 % functionized**. Total
wall-clock = `per-tick (irreducible) × tick-count (∝ source size)`.

Mapping to the task's framing:

- **Most steps residual?** Yes — *all* 37 123 (the functionizer refuses to
  extract a single step). This is the headline.
- **Interp-only?** No — 0 interp (extraction never ran).
- **Z3 dominant despite extraction?** N/A — there is no extraction.
- **Per-tick growth (state-size penalty)?** **No** — cost is flat. So
  cons-list-vs-Array+len for the carried lexer state is **not** the lever
  (carried state size does not drive per-tick cost here).

### Why "make the shapes extract" has a hard ceiling

The obvious refactor target ("change shapes so they extract") was measured and
has a **low ceiling**. Classifying the 37 123 simplified assertions by shape
(`EVIDENT_FUNCTIONIZE_DUMP`, reconstructed):

| assertion shape                                         | count   | %      |
| ------------------------------------------------------- | ------- | ------ |
| pure Int/Bool (JIT-eligible)                            | 10 196  | 27.5 % |
| mention String ops (`str.++`, `str.substr`, literals)   | 8 931   | 24.1 % |
| mention datatype / match / seq (`TLCons`, `Token`, `Result`, enum ctors) | 22 577 | 60.8 % |

(String and datatype overlap, so rows exceed 100 %.) The JIT compiles **only
Int/Bool scalar steps**. Even in the best case — extract gate opened AND every
pure-Int/Bool step JITs — at most **27.5 %** of assertions leave Z3, and Z3
must still solve the **72.5 % String / cons-list / datatype mass** (the bulk of
the inlined translate/parse machinery). Several of those shapes
(`last_results` datatype-decode, `str_from_int`) are *documented refuses*. So
functionizing cannot rescue per-tick cost — its ceiling is well under 1.5×.

### Recommended refactor (wave 4f): cut LEX tick COUNT via bulk skip

Per-tick cost (~490 ms) is irreducible at the `compiler.ev` level. **Tick count
is the only compiler-side lever with a proportional payoff.** Tick count is
dominated by the char-by-char LEX phase (≈ 1 tick/char), and a real `.ev` file
is mostly whitespace and comments — `test_hello` flattened is ~70 % `--`
comment lines (`docs/plans/blocked-grammar-wave4d.md`, Blocker 1).

**Change:** make the LEX FSM consume *runs* of trivially-classified characters
in a single tick instead of one char per tick:

- a comment run (`--` … to `\n`) — consumed in **one** tick (this also closes
  Blocker 1's correctness gap: the lexer currently has no comment mode);
- a whitespace run — collapsed to one tick.

**Hypothesis:** on a comment-heavy real file (~70 % comments + whitespace),
bulk-skipping those runs cuts lex ticks by roughly the comment+whitespace
fraction, i.e. **~3× fewer total ticks ⇒ ~3× wall-clock** (e.g. `test_hello`
~40 min → ~13 min). It is the highest-leverage change that stays inside
`compiler/*.ev`, and it does double duty as the Blocker-1 correctness fix.

**Secondary lever (same class):** the REVERSE phase pops the reverse-order token
list one token per tick (`compiler/compiler.ev:200`). For a 4137-char input
that is ~800 ticks (~6.5 min) of pure cons-list shuffling. Collapsing REVERSE
(lex into forward order, or reverse in fewer ticks) removes those ticks too.

### The structural ceiling (be honest)

Even with both tick-count cuts, per-tick stays ~490 ms, so the result is "minutes,"
not "seconds." The **order-of-magnitude** win is not a `compiler.ev` refactor at
all: it requires the kernel-side work-stack walker that fires only the
constraints the current step needs (Blocker 5's stated unblock in
`docs/plans/blocked-grammar-wave4d.md`; the nested-FSM tier-3 blocking-interpret
in `memory: project_nested_fsm_implementation_plan`). That is a `kernel/` task
and out of scope for wave 4f.

---

## Section 6 — Open questions

1. **Which output exactly refuses?** All 163 outputs appear covered and no cycle
   was found externally, yet `extract_program` returns `None`. Naming it needs a
   one-line kernel diagnostic (`eprintln!` the `build_body`-`None` output or the
   residual cycle in `topo_order`). Suspects: `read_result`'s `last_results`
   datatype-decode and `manifest`'s `str_from_int`. Worth a throwaway
   instrumented kernel build (NOT committed) at the start of wave 4f to confirm.

2. **If the gate were opened, what actually leaves Z3?** The 27.5 % ceiling
   assumes every pure-Int/Bool step both extracts *and* JITs *and* survives
   verification. The interp path's handling of `str.++` / `TLCons` / datatype
   `match` is unverified here — some "extracted" String/datatype steps may fail
   verification and re-refuse. Needs measurement once the gate is open.

3. **Does Z3 cost scale with active state or with static body size?** The flat
   ~490 ms across a tiny input strongly implies *static body size* dominates
   (inactive, phase-guarded passes are still processed). If true, shrinking the
   static body (less aggressive inlining of translate/parse sub-claims) would
   help per-tick cost — but Evident composition *is* inlining, so this likely
   needs a kernel/representation change, not a source refactor.

4. **`flag ∈ Bool = true` emitted `(assert (= flag ))`** in the Section-4 output
   — a dropped RHS (the lowercase-`true` footgun per `CLAUDE.md`, or a bool-pin
   gap). Unrelated to perf, but noted: a separate correctness bug in the
   self-hosted bool-pin path.
