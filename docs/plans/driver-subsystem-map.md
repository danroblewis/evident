# driver_main subsystem map + decomposition study

Status: analysis + probe-backed verdict (2026-06-08).
Subject: `compiler2/driver.ev` — `claim driver_main`, lines 926–6277
(6277-line file; the claim body is ~5350 lines). This is the real
self-hosted compiler (137/138 conformance). It is **oracle-compiled**
(`/usr/local/bin/evident-oracle emit compiler2/driver.ev driver_main`)
into `driver.smt2`, which the kernel runs as an FSM, one Z3-AST build
step per tick, over the source it is translating.

Everything below the `claim driver_main` line shares ONE flat
state-field list. 330 `_<name>` carry pairs (counted) live in that one
claim. This document maps the subsystems embedded in it, separates
shared from owned state, and answers — with an actual probe compile —
whether the subsystems can be factored into composed sub-claims.

---

## 0. Pre-`driver_main` helper claims (already extracted, pure)

These already-factored helper claims (lines 237–925) are the existing
proof that pure sub-claims compose cleanly. They take values in, return
values out, own no carry:

| Lines | Claim/enum | Role |
|---|---|---|
| 238–291 | `C2Item/C2Items/C2H/C2Binds/C2Frames` enums | work-item + composition-frame ADTs |
| 293–326 | `C2TokOp(t,o,rec)` | Token → Op classifier |
| 328–366 | `C2AtomE(t,e,ok)` | Token → atomic Expr |
| 367–437 | `FtiTok(tag,p0,s,t)` | FTI decode triple → Token |
| 438–474 | `C2Prec(o,n,right,chain)` | operator precedence ladder |
| 475–520 | `PrOp/PrOps` enums | Pratt op-stack ADT |
| 521–844 | `C2PrattStep(...)` | one shunting-yard action (the parser core) |
| 851–863 | `C2ChainLvl(tail,out)` | one fold level of comparison chains |
| 864–925 | `RtRecName/RtIdxOf/RtSortOf/RtFieldAcc` | record-registry pure lookups |

Plus six sibling modules already split into files: `compiler2/lex_fti.ev`,
`translate2_{bool,ctor,match,record,seq}.ev`. So the file already uses
helper-claim decomposition for everything that is a pure function. What
remains inlined in `driver_main` is the **stateful FSM** — and that is
the hard part this study is about.

---

## 1. Subsystem inventory (inside `driver_main`)

Each row: section, line range, owned carry-pair count (`_<name>`
declarations physically in that section), entry/exit gate, and purpose.
Counts are mechanical (`awk` over the section banners + carry-decl
pattern); banners are the file's own `-- ── … ──` headers.

| # | Subsystem | Lines | Carry pairs | Gate (entry→exit) | Purpose |
|---|---|---|---|---|---|
| 1 | INPUT | 927–960 | 4 | `is_first_tick` / `got_path` | read source path + content + target claim from stdin/files |
| 2 | ZINIT (Z3 lifecycle + Effect/Result floor) | 961–1090 | 34 | `zstep` -2→60 (holds at 9 for ED) | mk config/ctx/solver, sorts, numerals, Effect/Result floor consts |
| 3 | ED (enum-declaration machine) | 1091–1570 | 66 | `ed_act≠0 ∨ ed_src<4`; pmode 4 for user enums | one FSM that declares any datatype (LibArg, Effect, Result, user enums) via `translate2_ctor` steps |
| 4 | G2 record registry + RD machine | 1571–2206 | 58 | pmode 13 / `rd_*` | `type Name(...)` record sorts + field accessors |
| 5 | LEX (fossil FTI lexer) | 2207–2447 | 14 | gated after ZINIT (`_zstep<39`); `lx_count`/`tbase` | char-stream → FTI token buffer (write_long triples) |
| 6 | PHASE control | 2448–2456 | 1 | `phase` 0 lex · 2 parse · 3 emit | top-level mode selector |
| 7 | TOKEN WINDOW | 2457–2739 | 19 | `tcur`/`wend`/`tok_ready` | random-access FTI cursor + 8-token decoded lookahead (`wt0..wt7`, `wq*`) |
| 8 | PARSE dispatch | 2740–2785 | 3 | `pmode` 0 dispatch · 1 skip · 2 claim | top of the per-line dispatcher |
| 9 | F2 claim index | 2786–3043 | 10 | skip pass; `ci_base`/`ci_cnt`/`ci_names` | records every skipped `claim`/`type` top + body-start cursor (composition target table) |
| 10 | pmode 6 match-pin walk | 3044–3334 | 17 | `pmode=6` | `match` arms → nested ite + binds |
| 11 | pmode 7 set-literal walk | 3335–3367 | 2 | `pmode=7` | `x ∈ {a,b,c}` set membership |
| 12 | pmode 8 Seq(Int) literal walk | 3368–3396 | 3 | `pmode=8` | `⟨10,20,30⟩` array literals |
| 13 | G2 set-var registry | 3397–3574 | 11 | pmode 14 / `sv_*` | ≤2 `Set(T)` vars |
| 14 | CLAIM WALK state | 3575–3595 | 5 | shared walk cursor | per-claim body walk bookkeeping |
| 15 | FTI symbol table | 3596–4270 | 7 | `st_base`/`st_cnt`/`st_names` (1024 entries) | name → (handle, sort) table; the symbol environment |
| 16 | LINE CLASSIFIER | 4271–4356 | 0 | runs when `witems` empty | pure: head tokens → work-item program |
| 17 | F2 composition-line detection | 4357–4477 | 1 | classifier sub-case | recognize `Helper(slot↦…)` / bare / `..` heads |
| 18 | G1 infix-contains | 4478–4489 | 0 | classifier sub-case | `"lit" ∈ s` |
| 19 | G1 type splice | 4490–4513 | 0 | classifier sub-case | `p ∈ Point` record flatten |
| 20 | G1 bounded quantifiers | 4514–4638 | 10 | `fl_*`, pmode 11 | `∀/∃ v ∈ {lo..hi}` / `∀ v ∈ seq` |
| 21 | G2 rb record-pin broadcast | 4639–4755 | 5 | `rb_*` | componentwise record pin loop |
| 22 | F1 pmode-9 GROUP walk | 4756–4850 | 6 | `pmode=9` | first-line param lists |
| 23 | F2 composition inlining | 4851–5169 | 19 | pmode 10/12; `il_frames`/`il_binds`/`il_pfx` | call walk + inline-frame push/pop (the composition engine) |
| 24 | PRATT FSM state | 5170–5276 | 11 | `pmode=3`; `hstk`/`pk_kind` | drives `C2PrattStep` over the window |
| 25 | G1 conditional inline | 5277–5320 | 2 | `gc_*`, `il_guard` | `cond ⇒ ClaimName` |
| 26 | G1 positional binding | 5321–5647 | 19 | pmode 12; `pt_*`/`pj_*`/`pn*` | `(a,b) ∈ Claim`, method calls |
| 27 | state transitions | 5648–5782 | 0 | — | the `pmode`/`pphase` next-value mux (reads all of the above) |
| 28 | token consumption | 5783–5955 | 0 | — | cursor advance + window-tail refill arithmetic |
| 29 | per-item build effects | 5956–6152 | 0 | — | dispatch one work-item to its build pass (the effect emitter) |
| 30 | EMIT | 6153–6200 | 3 | `phase=3` | serialize the built context to `.smt2` |
| 31 | effects schedule | 6201–6277 | 0 | — | the single-writer `effects = …` ternary (the FSM's output) |

Total owned carry pairs: 330.

The three largest state owners — ED (66), G2 record registry (58), and
ZINIT (34) — are the Z3-object-handle machines (each `z_*`/`ed_*`/`rd_*`
field latches a Z3 func_decl/sort/ast handle produced by a prior tick's
`d_cap_int`). The three subsystems that own ZERO carry (16, 27, 28, 29,
31) are pure combinational logic that READS the shared state and muxes
the next values / effects — these are the natural first extraction
targets (see §4).

---

## 2. Shared vs owned state

Reference counts (`grep -cE '\b_?field\b'` over the whole file):

**Genuinely shared (read across many subsystems) — the FSM bus:**

| Field | refs | Read by |
|---|---|---|
| `d_cap_int` | 130 | EVERY latch (the per-tick `last_results[0]` Int capture — every Z3-handle field reads it) |
| `pmode` | 86 | dispatch + every pmode-N walk + state transitions |
| `zstep` | 85 | ZINIT, ED hold, LEX gate, transitions |
| `pk_kind` | 38 | PRATT + every caller that arms a parse |
| `ed_act`/`ed_src` | 33/21 | ED machine + ZINIT hold + transitions |
| `il_pfx`/`il_frames`/`il_binds` | 28/9/9 | composition engine + walker name-resolution + transitions |
| `tok_ready`/`tcur`/`wend`/`witems` | 20/17/7/15 | window + classifier + consumption + every walk |
| `st_base`/`st_cnt` | 13/7 | symbol table + walker resolve (read everywhere a name resolves) |
| `tbase`/`lx_count` | 12/12 | LEX ↔ window handoff |
| `phase`/`fl_on` | 8/8 | top dispatch / quantifier suppression |

**Subsystem-local (owned, read only within section):** the `z_*`
handle consts (ZINIT), `ed_*` step regs (ED), `rd_*`/record-registry
fields (G2), `rb_*`, `sv_*`, `fl_*`, `pt_*`/`pj_*`/`pn*` (positional),
`gc_*` (cond-inline), the `wt0..wt7`/`wq*` decoded-window registers.
These are written by exactly one machine and consumed by it (or by the
shared transition mux at §27).

**Consequence for extractability:** a subsystem is cleanly extractable
to the degree its OUTPUTS feed the shared bus through few channels.
ZINIT, ED, G2-record produce Z3 handles that are read back only via
their own `z_*`/`ed_*`/`rd_*` fields — high local cohesion. But they all
LATCH off the single shared `d_cap_int` and are sequenced by the single
shared `zstep`/`ed_*` counters, so they cannot be lifted as independent
FSMs; only their per-field *transition functions* can move (see §3–4).

---

## 3. The scoped-variable question (probe-backed verdict)

**Question:** can these subsystems become composed sub-claims, given
Evident's current composition — specifically, does the kernel's
`_<name>` state carry survive across ticks through a composed claim?

**Method:** four oracle compiles (the same oracle that builds the real
driver), each run through the kernel for the real tick count. Probe
sources are inline below; all compile with
`evident-oracle emit <flat> main` and run with the release kernel.

### Probe A — carry pair OWNED by a composed sub-claim → BREAKS

```evident
claim Counter(out ∈ Int)
    count ∈ Int
    _count ∈ Int
    count = (is_first_tick ? 5 : _count + 1)
    out = count
claim main
    c ∈ Int
    Counter(out ↦ c)
    done ∈ Bool ; _done ∈ Bool
    done = (is_first_tick ? false : true)
    effects = (done ? ⟨Exit(c - 6)⟩ : ⟨LibCall("libc","getpid",⟨⟩)⟩)
```

Result: the sub-claim's carry pair is α-renamed to
`Counter__count__call0` / `Counter___count__call0`. Manifest
`state-fields = c:Int done:Bool` — **the renamed field is NOT a state
field**, so the kernel never re-asserts `_count = <prev>`. Two-tick run:
**exit 2 (UNSAT)** — carry broken. This matches the F2-descope note in
`compiler2-driver-notes.md` ("Carry memberships INSIDE inline frames:
the prefixed-name pairing doesn't line up with the kernel's `_field`
convention").

### Probe B — carry in `main`, PURE helper composed → WORKS

```evident
claim Bump(inp ∈ Int, out ∈ Int)
    out = inp + 1
claim main
    count ∈ Int ; _count ∈ Int
    base ∈ Int = (is_first_tick ? 4 : _count)
    c ∈ Int
    Bump(inp ↦ base, out ↦ c)
    count = c
    done ∈ Bool ; _done ∈ Bool
    done = (is_first_tick ? false : true)
    effects = (done ? ⟨Exit(count - 6)⟩ : ⟨LibCall("libc","getpid",⟨⟩)⟩)
```

Result: `state-fields = base:Int c:Int count:Int done:Bool`. Two-tick
run **exit 0**. A stateless helper composes correctly across ticks.

### Probe C — carry pair in `main`, TRANSITION FUNCTION in a composed helper → WORKS, byte-identical

```evident
claim CounterStep(first ∈ Bool, prev ∈ Int, next ∈ Int)
    next = (first ? 5 : prev + 1)
claim main
    count ∈ Int ; _count ∈ Int
    CounterStep(first ↦ is_first_tick, prev ↦ _count, next ↦ count)
    done ∈ Bool ; _done ∈ Bool
    done = (is_first_tick ? false : true)
    effects = (done ? ⟨Exit(count - 6)⟩ : ⟨LibCall("libc","getpid",⟨⟩)⟩)
```

Result: emitted assert is
`(assert (= count (ite is_first_tick 5 (+ _count 1))))` — **identical**
to the hand-inlined `carry_fixture` form; `state-fields = count:Int
done:Bool`. Two-tick run **exit 0**.

### Probe D — slot-count cap

A 9-slot composition call (`Many(a↦…,…,h↦…,out↦…)`) compiles and runs
**exit 0** under the oracle. The "≤6 slots" cap noted in
`compiler2-driver-notes.md` is the *driver's own* composition support;
the **oracle** that compiles `driver.ev` has no such limit. Slot plumbing
is therefore not a practical bound on refactoring `driver.ev`.

### Verdict

Composition is a **scoped value substitution, not a state-scoping
mechanism.** The kernel carries state by matching a top-level primitive
membership `x` with its `_x` sibling and re-asserting `_x = <prev x>`
each tick; the manifest state-field list is built only from the TOP
claim's memberships. A composed sub-claim's memberships are α-renamed
(`Helper__x__callN`) and never enter that list.

Therefore:

- A sub-claim **cannot own FSM carry state.** Every one of the 330
  `_<name>` carry pairs MUST keep its `x ∈ T` / `_x ∈ T` declaration in
  `driver_main` to remain a manifest state field. (Probe A.)
- A sub-claim **can be a pure transition/decode function**: take
  `is_first_tick`, the relevant `_x` (previous values), and inputs as
  slots; return the next value(s) as output slots. The carry-pair
  declaration stays in `driver_main`; only the ternary body moves.
  (Probe B, C — and the 10 existing helper claims + 6 translate2
  modules already do exactly this for the pure-function parts.)

So the honest answer is **partial**: you cannot lift a subsystem as a
self-contained FSM, but you CAN move the bulk of `driver_main`'s text —
the transition-function bodies — into per-subsystem helper claims,
leaving behind only the carry-pair declarations and the slot-call that
wires them. The declarations are one line each; the bodies are where the
complexity (and the comprehension cost) lives.

---

## 4. Refactor proposal (dependency-ordered)

Given §3, the refactor is **"thin the body, keep the declarations"**:
for a chosen subsystem, leave its `x ∈ T` / `_x ∈ T` pairs in
`driver_main`, and replace the inlined transition ternaries with a call
to a new pure helper claim that returns the next values through output
slots. This is the Probe-C pattern at scale. It is mechanical, oracle-
verifiable per step, and reversible.

Extraction order (most self-contained first → most entangled last):

1. **Pure muxes that own ZERO carry (§27 transitions, §28 consumption,
   §29 per-item build effects, §31 effects schedule).** These read the
   bus and compute next-values / effects with no owned state. They are
   the cleanest: a helper takes the read fields as input slots and
   returns the muxed result. *Risk: low (no carry semantics involved).
   Worth it: high — these are large, dense, and pure.* Caveat: §29/§31
   build *effects* (the `effects = …` single-writer); a helper returning
   a `Seq(Effect)` value bound into `main`'s `effects` must preserve the
   single-writer rule — verify the emitted `effects` assert is unchanged.

2. **ZINIT z_* latches (§2).** 34 fields, each `z_x = (zstep = N ?
   d_cap_int : _z_x)`. Keep the 34 declarations; move the 34 ternaries
   into one `ZInitLatch(step ↦ zstep, cap ↦ d_cap_int, prev_x ↦ _z_x,
   …, x ↦ z_x)` helper (or a few, grouped). *Risk: low — uniform shape,
   each verifiable against the current emit. Worth it: medium — shrinks
   ~130 lines to ~40 + one helper.*

3. **ED machine step bodies (§3, 66 fields) and G2 record registry
   (§4, 58).** Same pattern, larger. The `ed_*`/`rd_*` step *logic*
   moves to helpers (they already delegate to `translate2_ctor`/
   `translate2_record`); the carry declarations stay. *Risk: medium —
   the ED/RD step functions are intricate and share `d_cap_int`
   sequencing. Worth it: high — these two are 124 carry fields and the
   densest part of the file.*

4. **The pmode-N walk bodies (§10–§13, §20–§26).** Each pmode walk's
   per-tick body becomes a helper keyed on the read fields; the `pt_*`/
   `fl_*`/`sv_*`/`gc_*` carry declarations stay. *Risk: medium-high —
   these read deep into the shared bus (`st_*`, window, `il_*`); the
   slot lists get wide (Probe D shows the oracle allows it, but wide
   slot lists are their own readability cost). Worth it: medium.*

5. **Leave the shared bus in place.** `d_cap_int`, `pmode`, `zstep`,
   `tcur`/`wend`/window, `st_*`, `il_*` are read by everything. They
   stay as `driver_main` declarations and are passed as input slots to
   whatever helpers need them. Do NOT try to "own" them in a subsystem.

What this buys: `driver_main` becomes ~330 carry-pair declarations + a
slot-call per subsystem + the shared bus — readable as *wiring*, with
each subsystem's logic in a named, independently-testable helper claim
(the style the CLAUDE.md "compact entry-point reads as wiring; logic
lives in claims" guidance asks for). The carry semantics are provably
preserved because the declarations never move (Probe C).

What it does NOT buy: true encapsulation. A helper that needs 12 shared
fields takes 12 input slots; the coupling is explicit but not reduced.
And carry pairs cannot be co-located with their logic — the declaration/
logic split is inherent to the kernel's state model.

### Honest bottom line

Refactoring is **feasible and worthwhile for the pure-body subsystems
(§27–§31, ZINIT, ED, G2)** via the Probe-C "declarations stay, body
moves to a helper" pattern, done one subsystem at a time with an
oracle-compile + conformance check between each. It is **NOT possible**
to express a subsystem as a self-contained FSM-owning sub-claim — the
kernel's `_<name>` state model forbids it (Probe A). If a future kernel
change taught the manifest builder to harvest state fields THROUGH
composition frames (mapping `Helper__x__callN` ↔ a stable carry name),
full subsystem extraction would open up; that is a kernel change, out of
scope here, and noted only as the unlock.

Recommended first extraction: **§31 effects schedule + §27 state
transitions** (zero owned carry, pure muxes, smallest blast radius),
then **ZINIT (§2)** as the first carry-bearing subsystem to validate the
Probe-C pattern on real driver state before tackling ED/G2.
