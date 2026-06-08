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

### Next candidates — assessed, deferred (with measured reasons)
- **`rt_*` → `RecTypeEntry`** (43 members; cohesion 61 — the highest of
  any registry). Highest line-removal, but NOT a mechanical rename: the
  registry is a hand-unrolled array-of-records (`rt_cnt`, `rt_n0..rt_n2`,
  `rt_f0…`) and the target shape is a record ELEMENT inside a cons-list
  enum (the `CFCons` "Frame" pattern). That is a structural rewrite of
  the registry's append/probe logic (`RtIdxOf`/`RtRecName`/`RtFieldAcc`),
  not a field-for-field substitution. High risk; should be its own
  focused session with a fresh gate budget. NOT attempted (would not
  fit safely before the harness window closes).
- **`z_*` → a Z3 handle-bank record.** MEASURED: 30+ distinct handles,
  **345 total reference sites** across every compiler2 module (`z_ctx`
  alone = 77). They are also weakly related — numerals (`z_zero…z_four`),
  sort handles (`z_isort/z_rsort/z_bsort/z_ssort`), solver handles
  (`z_ctx/z_sol/z_cfg`), decl handles (`z_lc_decl/z_argint_decl/…`) —
  not one co-traveling tuple. Lumping them into a single ~30-field
  record both (a) violates the fact-#5 guardrail (a god-record forces
  every weakly-related field live every tick) and (b) is a 345-site
  blast radius with dozens of isolation fixtures stubbing individual
  handles. A better future move is several SMALL cohesive records
  (`Z3Sorts`, `Z3Numerals`, `Z3SolverCtx`), each gated separately — not
  one bank. NOT attempted as a single type.
- Smaller element records (`Window8`/`ww_*`, `Frame`/`CFCons` payload,
  `MatchPinCtx`/`mp_*`). `ww_*` is cohesive but wide-surface (the token
  window is matched in nearly every classifier module). `mp_*` (65
  members) is a context, not a clean tuple. Tractable but lower value
  than completing FtiBuffer; left for follow-on.
