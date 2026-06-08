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
`123-subschema-shadowing-quantifier`; 0 timeouts). COMMITTED.
