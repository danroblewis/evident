# Type-refactor log — Phase 1 (discovered TYPES into compiler2)

Branch `type-refactor` off `main`, worktree `/tmp/refactor-wt`.
Gate: `.goalpost/bin/run-conformance.sh` must read **137/138**
(the one known failure is `123-subschema-shadowing-quantifier`).
Records change the emit, so the gate is behavioral, not byte-identical.

Order: most-complex / most-lines-removed first, after a cheap
mechanism proof (FtiBuffer at one carried site).

## Mechanism proof — `FtiBuffer(base ∈ Int, count ∈ Int)`

The recurring base+cursor pair behind every FTI-resident buffer
(token buffer, symbol table, claim index). Wired at ONE real carried
site: the **claim index** (`ci_base` + `ci_cnt` → `cibuf`).

Validated facts (record-carry idiom in the real driver fsm):
- The fsm autocarry transform **synthesizes the record dual**
  `_cibuf ∈ FtiBuffer` automatically (no explicit `_cibuf` decl
  needed) — confirmed in the flattened unit and in isolation.
- A **forward type reference works**: `FtiBuffer` is declared in
  `compiler2/driver_ir.ev` (imported last), yet used in
  `driver_zinit.ev`/`driver_claimidx.ev` which flatten earlier. The
  oracle accepts the use-before-decl.
- A **bare bound constraint on a record field** is accepted:
  `cibuf.count < 2048` (was `ci_cnt ∈ Int < 2048`, a decl+bound; now
  the field is declared by the type, the bound is a plain constraint).
- `_record.field` reads in arbitrary expressions work
  (`_cibuf.count + 1`, `_cibuf.base`).
- Field carry across ticks: base pinned once, count climbs — proven
  by `tests/compiler2_units/types/fti_buffer_carry.ev` and by the
  updated `driver_claimidx/index_append.ev`.

### Call sites rewritten (8 code refs, 6 files)
- `compiler2/driver_ir.ev` — +1 type decl (`FtiBuffer`).
- `compiler2/driver_zinit.ev` — `ci_base` decl+carry → `cibuf`/`cibuf.base`.
- `compiler2/driver_claimidx.ev` — `ci_cnt` decl/bound/carry +
  write addr → `cibuf.count` / `cibuf.base`.
- `compiler2/driver_guard.ev` — `ci_base` read → `cibuf.base`.
- `compiler2/driver_compose.ev` — `ci_base` read → `cibuf.base`.
- `compiler2/driver_posbind.ev` — `ci_base` read → `cibuf.base`.
- `compiler2/driver_emit.ev` — `ci_base` free → `cibuf.base`.
- tests: `driver_claimidx/index_append.ev` updated; new
  `types/fti_buffer_carry.ev` added.

Lines removed (net): ~0 for the proof (one field-pair unified; the
proof's value is establishing the idiom, not deleting lines). The
base+count pair was already split across decls; the win comes from
unifying the remaining two FtiBuffer instances (token buffer, symtab)
and the larger flattened registries.

### Finding — every field of a carried record must be constrained
The kernel carries a record-typed fsm member by carrying each flattened
field const. If a consumer references only ONE field (e.g. DriverGuard
uses `cibuf.base` but never `cibuf.count`), the unused field has no
covering assignment and the kernel aborts at runtime with
`state var \`cibuf.count\` not in model` (functionizer: "an output had
no covering assignment"). Fix: any fsm that pulls in a record member
must constrain ALL its fields, even with an identity carry
(`cibuf.count = (is_first_tick ? 0 : _cibuf.count)`). In the real driver
this is automatic (zinit pins base, claimidx drives count); it only
bites module-isolation fixtures that stub a subset. Implication for
later types: a wide record (e.g. a 42-field Z3 handle bank) forces
EVERY field to be live every tick — fine for a genuinely co-traveling
group, but a reason NOT to lump weakly-related fields into one record.

Unit tests: PASS — 6/6 (types/fti_buffer_carry, driver_claimidx,
driver_compose, driver_emit, driver_guard, driver_posbind).
Gate: **PASS — 137/138** (only known failure
`123-subschema-shadowing-quantifier`; 0 timeouts). COMMITTED `0829183`.

## FtiBuffer instance 2 — token buffer (`tbase` + `lx_count` → `tbuf`)

The highest-traffic FTI buffer: the 65536×32 token arena written by the
lexer and read by the window. Same proven idiom; second instance of the
already-declared `FtiBuffer` type. 12 code refs across 4 modules:
- `driver_zinit.ev` — `tbase` decl+carry → `tbuf`/`tbuf.base`.
- `driver_lex.ev` — bound (`tbuf.count < 65534`), carry, the three
  float-token writes (`tbuf.base + _tbuf.count*32 …`), the EOF write,
  and the `LexFtiPlan(base ↦ tbuf.base, count ↦ _tbuf.count)` call.
- `driver_window.ev` — fetch addr `tbuf.base + _tcur*32`.
- `driver_emit.ev` — free `tbuf.base`.
Fixtures updated: driver_lex/{lex_idents,lex_twochar_op},
driver_window/fetch_burst, driver_emit/estep_walk,
driver_zinit/latch_isort (+ driver_ir import where the module didn't
already pull it; + identity `.count` stubs where a fixture exercises
only `.base`).

Unit tests: PASS — 29/29 (full compiler2_units suite).
Gate: **PASS — 137/138** (only known `123-subschema`; 0 timeouts).
COMMITTED `d3971ed`.

## FtiBuffer instance 3 — symbol table (`st_base` + `st_cnt` → `stbuf`)

Third and final FtiBuffer instance, completing the "instantiated three
times" thesis (token buffer / symbol table / claim index). The 8192×8
handle table. 8 code refs across 5 modules (incl. the big `driver.ev`):
- `driver_zinit.ev` — `st_base` decl+carry → `stbuf`/`stbuf.base`.
- `driver_symtab.ev` — bound (`stbuf.count < 8192`) + carry (init `2`,
  the two kernel-seeded slots).
- `driver_buildeff.ev` — the two D3 seed writes (`stbuf.base`, `+8`).
- `driver.ev` — symtab read (`stbuf.base + 8*(pos/32)`) + the decl
  istep-2 write (`stbuf.base + 8*_stbuf.count`).
- `driver_emit.ev` — free `stbuf.base`.
Fixtures updated: driver_buildeff/{select_w2,select_w5},
driver_emit/estep_walk, driver_zinit/latch_isort,
driver_symtab/decode_peel (declare `stbuf` + stub `.base`, since
DriverSymtab constrains only `.count`; + driver_ir import where absent).

Unit tests: PASS — 29/29.
Gate: **PASS — 137/138** (artifact: total 138, passed 137, failed 1 =
only known `123-subschema`, 0 timeouts, wall 418s, builder oracle).
COMMITTED.

---

## Summary — FtiBuffer fully landed (3/3 instances)

`type FtiBuffer(base ∈ Int, count ∈ Int)` now unifies all three FTI
base+cursor pairs the analysis flagged (Appendix A.1): the token buffer
(`tbuf`), the symbol table (`stbuf`), and the claim index (`cibuf`).
28 base/cursor code references collapsed onto one declared record type;
the three loose `_base`/`_cnt` decl+carry pairs became three records.

### Record-carry idiom — established facts (the reusable recipe)
1. Declare `x ∈ T` ONCE (in the module that owns the base pin); the fsm
   autocarry transform synthesizes the prev-tick dual `_x ∈ T`. No
   explicit `_x` decl is needed.
2. Field constraints may live in DIFFERENT module fsms (base pinned in
   driver_zinit, cursor driven in driver_lex/symtab/claimidx) — they all
   merge into driver_main via `..Module` and share the one declaration.
3. A bound that was a decl+bound (`cnt ∈ Int < N`) becomes a plain field
   constraint (`x.count < N`); the type already declares the field.
4. Forward type reference is fine — `FtiBuffer` is declared in
   driver_ir.ev (imported last) yet used in earlier-flattened modules.
5. **Every field of a carried record must have a covering assignment
   each tick**, or the kernel aborts (`state var X not in model`). In
   the full driver this is automatic; module-isolation fixtures that use
   only one field must stub the others with an identity carry. This is
   the one real constraint the type system imposes — see the "Finding"
   above. It is also a design guardrail: do NOT lump weakly-related
   fields into one record, because each forces a live value every tick.

---

## Z3SolverCtx — the solver-lifecycle handle triple (`z_cfg/z_ctx/z_sol`)

First of the cohesive `z_*` sub-records (the prior "god-record" was
rejected; the split is the right shape). `type Z3SolverCtx(cfg, ctx,
sol ∈ Int)` unifies the three Z3 lifecycle handles created consecutively
during zinit (zsteps 1/2/3): the config, the context, the solver. They
are a genuinely co-traveling group — every `Build*Z3` effect threads
`ctx`, every assert goes through `sol` — so all three are driven every
tick by DriverZInit (the every-field-live guardrail is satisfied in the
full driver automatically).

Sites rewritten: 93 references (`z_ctx` alone = 77) across 4 modules,
all READS (slot args `ctx_h ↦ z3ctx.ctx` / `ArgInt(z3ctx.ctx)`):
- `driver_ir.ev` — +1 type decl (`Z3SolverCtx`).
- `driver_zinit.ev` — the three decl+latch lines → `z3ctx` record +
  three field latches (`z3ctx.cfg = is_first_tick ? 0 : zstep=1 ? … :
  _z3ctx.cfg`, etc.). Doc header updated.
- `driver_buildeff.ev` — ~40 `z_ctx` slot args + `z_cfg`/`z_sol`.
- `driver.ev` — ~25 build-step slot args.
- `driver_record.ev`, `driver_emit.ev` — the remaining reads.
Fixtures: driver_buildeff/{select_w2,select_w5}, driver_emit/estep_walk
(their stub `z_cfg/z_ctx/z_sol ∈ Int` carries → one `z3ctx ∈
Z3SolverCtx` stub driving all three fields each tick, per the guardrail).
New carry unit test `types/z3_solverctx_carry.ev` (step-latched triple;
proves the autocarry synthesizes `_z3ctx ∈ Z3SolverCtx` so each field's
latched value persists).

Unit tests: PASS — 30/30. Gate: **PASS — 137/138** (only known
`123-subschema`; 0 timeouts; wall 416s). COMMITTED.

## Z3Sorts — the four base-sort handles (`z_isort/z_bsort/z_ssort/z_rsort`)

Second cohesive `z_*` sub-record. `type Z3Sorts(isort, bsort, ssort,
rsort ∈ Int)` unifies the Int/Bool/String/Real sort handles latched
consecutively in zinit (zsteps 5/6/7/8). Every type-directed Build*Z3
step picks one by the field/atom type string; all four are driven each
tick by DriverZInit (guardrail satisfied in the full driver).

Sites rewritten: 71 references across 4 modules, all READS (slot args
`int_sort_h ↦ z3sorts.isort` and ternary RHS values `… = "Real" ?
z3sorts.rsort`):
- `driver_ir.ev` — +1 type decl (`Z3Sorts`).
- `driver_zinit.ev` — four decl+latch lines → `z3sorts` record + four
  field latches. Doc header updated.
- `driver_buildeff.ev`, `driver.ev`, `driver_enum.ev`,
  `driver_record.ev` — slot-arg / ternary reads.
Fixtures: driver_enum/floor_walk (4 sort stubs → one `z3sorts` record
stub + driver_ir import), driver_buildeff/{select_w2,select_w5} (their
isort+bsort stubs → full `z3sorts` record driving all four fields, per
the guardrail), driver_zinit/latch_isort (read + comments →
z3sorts.isort; the module drives all four). New carry unit test
`types/z3_sorts_carry.ev`.

Unit tests: PASS — 31/31. Gate: **PASS — 137/138** (only known
`123-subschema`; 0 timeouts; wall 416s). COMMITTED.

## Z3Numerals — the cached 0..4 int-numeral handles (`z_zero..z_four`)

Third cohesive `z_*` sub-record. `type Z3Numerals(zero, one, two, three,
four ∈ Int)` unifies the five small int-numeral handles minted once in
zinit (zsteps 16/17/18/33/34 — z_three/z_four live in the later D2
section but latch the same way) so effects-array indices/lengths reuse
them instead of re-minting. All five driven each tick by DriverZInit.

Sites rewritten: 25 references across 2 modules, all READS (effects-index
ternaries `d_sel_i = 0 ? z3nums.zero : …`, length ternaries, the geq
floor `r_h ↦ z3nums.zero`, a `C2PushH(z3nums.zero)`):
- `driver_ir.ev` — +1 type decl (`Z3Numerals`).
- `driver_zinit.ev` — five decl+latch lines → `z3nums` record + five
  field latches (zero/one/two at zsteps 16-18; three/four at 33/34).
  Doc header updated.
- `driver_buildeff.ev`, `driver.ev` — the read sites.
Fixtures: driver_buildeff/{select_w2,select_w5} (their single `z_zero`
stub → full `z3nums` record driving all five fields, per the guardrail).
New carry unit test `types/z3_numerals_carry.ev`.

Unit tests: PASS — 32/32. Gate: **PASS — 137/138** (only known
`123-subschema`; 0 timeouts). COMMITTED.

## rt_* → cons-list enum — DEFERRED (big rewrite), NOT infeasible

> CORRECTION (post-hoc, orchestrator): the original conclusion below
> claimed enum-typed members "do not carry" and that a cons-list is
> "off the table." **That is wrong — it was a flawed probe**, the same
> trap the record-carry investigation hit earlier (a missing prev-tick
> dual). A cons-list **enum carries fine** when its `_xs ∈ T` dual is
> declared (or synthesized by `fsm` autocarry):
>
> ```
> enum Stack = Nil | Push(Int, Stack)
> claim main
>     s ∈ Stack
>     _s ∈ Stack                 -- the dual the probe below omitted
>     s = (is_first_tick ? Push(0, Nil) : Push(_depth + 1, _s))
>     ...
> ```
> → carries correctly (verified: depth reaches 2, exit 0). The driver's
> own work-item stacks are carried cons-list enums too. The probes below
> failed only because they wrote `xs ∈ IntList = (… _xs …)` / `c ∈ Col =
> (… _c …)` with **no `_xs`/`_c` declaration** — so the dual was unbound,
> not because enum carry is unsupported.
>
> So `rt_*` → a cons-list enum of `RecTypeEntry` **is feasible**. It is
> still a large registry rewrite (the append/probe logic) and was
> rightly deferred for time, but as a SCOPE call, not an impossibility.
> The record-unrolled-×3 form is the other viable shape. Either is a
> focused-session task.

--- original (flawed-probe) finding, retained for the record: ---

The analysis proposed restructuring the record registry into a cons-list
enum of a `RecTypeEntry` element. The agent's probes (`xs ∈ IntList =
(is_first_tick ? ICons(7,INil) : ICons(step, _xs))` and `c ∈ Col =
(is_first_tick ? Red : _c)`) failed at emit with `_xs`/`_c` unbound — but
those probes omitted the `_xs ∈ IntList` / `_c ∈ Col` dual, so the
failure was the missing declaration, not an enum-carry limit. The viable
shapes are (a) a cons-list enum of `RecTypeEntry` with the carried-list
dual, or (b) a `RecTypeEntry` record unrolled ×3; both are large
self-contained rewrites of the `RtIdxOf`/`RtRecName`/`RtSortOf`/
`RtFieldAcc` probes and their ~25 call sites.

### Other candidates — assessed, deferred (with measured reasons)
- **`z_*` → a Z3 handle-bank record. DONE as three cohesive sub-records**
  (`Z3SolverCtx`, `Z3Sorts`, `Z3Numerals` — see the three sections
  above; 93 + 71 + 25 = 189 of the 345 sites collapsed). The original
  single-god-record idea was correctly rejected (it would force every
  weakly-related handle live every tick, violating fact #5). The
  remaining `z_*` handles (decl handles `z_lc_decl/z_argint_decl/…`, the
  effects-array consts `z_effs/z_elen/…`, `z_true/z_false`, the
  last_results/is_first_tick build-context consts) are weakly related to
  each other and to the three landed groups; any further split should be
  another cohesion pass, not one bank.
- Smaller element records (`Window8`/`ww_*`, `Frame`/`CFCons` payload,
  `MatchPinCtx`/`mp_*`). `ww_*` is cohesive but wide-surface (the token
  window is matched in nearly every classifier module). `mp_*` (65
  members) is a context, not a clean tuple. Tractable but lower value
  than completing FtiBuffer; left for follow-on.
