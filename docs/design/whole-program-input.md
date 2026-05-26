# Whole-program-table input for self-hosted passes (Gap D / COUNTEREXAMPLES #27)

> **Status:** design (docs-only). No `.rs`/`.ev` written; the worked example
> in §4 is a sketch for an implementation session.
>
> **Scope:** the two `inject` sub-passes that stay in Rust after
> REVIVE-inject — `inject_claim_arg_types` and `inject_lhs_eq_types`
> (`runtime/src/runtime/inject.rs`) — plus the generics-orchestration lookup
> that stays in Rust for the same reason
> (`runtime/src/portable/generics.rs::monomorphize`). Both are blocked on what
> the prior sessions called *Gap D*: a pass "needs the whole-program schema
> map, not string ops — the same boundary `subscriptions`/`validate` keep."
>
> **One-line conclusion (so the reader can stop here if they only want the
> verdict):** the whole-program *encoder already exists and round-trips*; the
> real blocker is that the only way a pass could *index* a marshaled table
> by name is an **in-solve string equality**, which is the measured Z3
> string-theory blow-up (validate's `nm = "FFICall"` SIGSEGV, desugar's
> `FRef`). So the fix is **not** "marshal the table in" — it is the same
> Rust-owns-the-string-leaf split every other cutover ships: **resolve the
> by-name lookup in the Rust shim, hand the FSM pre-resolved facts, let the
> FSM do the structural decision + construction.** The "composite
> whole-program INPUT" capability is *not actually required* to finish
> `inject`.

---

## § 1 — Pin the gap precisely

The session brief asks which of three things is true:

> (a) **no encoder** for a schema map, (b) an encoder exists but **isn't
> wired as INPUT** to a `portable/` pass, or (c) the encoder exists and is
> wired but the pass **can't index it by name**.

The answer is **primarily (c), with (b) true as a downstream symptom and (a)
definitively false.** Evidence, with file + symbol:

### (a) is false — the encoder exists, in both representations, and round-trips

A whole `Program` — including its `Seq(SchemaDecl)` — already encodes two
ways:

| Representation | Encoder | Round-trip decoder |
|---|---|---|
| Z3 `Datatype` (the reflection / `given`-pin path) | `encode_program`, `encode_schema_list`, `encode_schema_decl` (`runtime/src/translate/encode_ast.rs:604, 275, 368`) | — (asserted into the solver, not decoded) |
| `Value::Enum` cons-list (the shape a stack-FSM consumes) | `program_to_value`, `schema_list_to_value`, `schema_decl_to_value` (`encode_ast.rs:817, 823, 841`) | `decode_program`, `decode_schema_list`, `decode_schema_decl` (`runtime/src/translate/decode_ast.rs:577, 267, 395`) |

The `Value::Enum` path is the shared marshaler (session UU). `SchemaList`
(`SchLNil` / `SchLCons`, `encode_ast.rs:823`) is a poppable cons-list — the
exact shape `subscriptions_walk` / `validate_walk` / `inject_collect` already
walk. `MakeSchemaDecl` carries `param_count` (4-field) since GAPB
(`encode_ast.rs:368-378`, `decode_ast.rs:395-415`), so a whole `SchemaDecl`
round-trips losslessly *for the fields the inject sub-passes read* (keyword,
name, param_count, and each body `Membership`'s name + type). The one
documented marshaler lossiness — nested/top-level `MatchPattern` collapsing to
`BindWildcard` (`encode_ast.rs:558-572`) — is irrelevant here: neither
sub-pass reads or rewrites match patterns; they read `Membership` name+type
and `Call`/`ClaimCall` names only.

**So there is no missing encoder. The table can be marshaled both
directions, faithfully, today.**

### (b) is true, but it is a symptom, not the cause

No shipping `portable/` pass feeds the FSM a whole table. Every one seeds
`run_nested` with **one** body-item or expr at a time:

- `subscriptions.rs` / `validate.rs` — one `Constraint`/`BodyItem` per
  `run_nested` (`self-hosting.md` §"What subscriptions reproduces", point 2:
  "per-item keeps it O(N)").
- `inject.rs::collect_refs` (`runtime/src/portable/inject.rs:132`) — walks
  per body item.
- `generics.rs::collect_uses` (`runtime/src/portable/generics.rs:255`) — loops
  over `schemas` **in Rust** and seeds each item; the by-name lookup
  (`schemas.get(&generic_head)`, `generics.rs:401`) is a Rust `HashMap` hit.

And the two Rust inject sub-passes index the table purely in Rust:
`schemas.get(type_name)` / `schemas.contains_key(name)` (`runtime/inject.rs:121,
143, 156, 294, 344`), `enums.by_variant.borrow().get(name)`
(`runtime/inject.rs:291`). They receive `&HashMap<String, SchemaDecl>` and
`&EnumRegistry` as Rust arguments (`runtime/inject.rs:40-43, 211-215`) — never
marshaled.

But "nobody wired it" is not the obstacle. Wiring it is one
`schema_list_to_value(...)` call. The reason nobody wired it is (c).

### (c) is the real gap — there is no by-name lookup a pass can do inside the solve

To use a marshaled table, a pass must answer "given this call name / this
field head, what are its param/field types?" — an **association lookup keyed
on a string**. In Evident that is one of:

```evident
-- linear scan over the table:
∀ s ∈ schemas : s.name = key ⇒ …
-- or a cons-list walk:
match schemas
    SchLCons(MakeSchemaDecl(_, nm, _, _), rest) ⇒ (nm = key ? … : recurse rest)
```

Both hinge on `nm = key` — **string equality between an enum-extracted
payload and a key, evaluated inside the per-tick Z3 solve.** That is exactly
the in-solve cousin of #18 that three sessions measured as catastrophic:

- **validate** (`COUNTEREXAMPLES.md` #18, `self-hosting.md` §validate): folding
  `ECall(nm,_) ⇒ nm = "FFICall" ? …` into the FSM translated and was faithful
  on small inputs, but on a walk state carrying unrelated string literals
  (`test_26_value_cache.ev::driver`'s `msg` ternary) Z3's string theory went
  from ~0.5 ms/constraint to **minutes + multi-GB, SIGSEGV**.
- **desugar** (`COUNTEREXAMPLES.md` #20c, `stdlib/passes/desugar.ev:179-192`):
  the `FRef(name)` → `⟨items⟩` lookup is a string-keyed map hit; doing it
  in-solve "hits the same Z3 string-theory blowup `validate` measured," so the
  FSM emits `FRef(name)` and **the Rust shim does the `HashMap` lookup out of
  the solve.**
- **subscriptions** (`self-hosting.md` §classification): the
  `world.`/`world_next.` split "stays in Rust" because there is "no
  substring/prefix builtin," and the FSM "emits the RAW dotted identifier
  strings."

And critically: **there is no map / assoc-lookup primitive in the language.**
The only "assoc" structure that exists is desugar's hand-rolled `Assoc`
cons-list (`desugar.ev:132-133`, `AEntry`/`ACons`) — and it deliberately does
**not** do an in-FSM lookup, for precisely this reason. So even with the table
threaded in, a pass has no non-catastrophic way to index it.

A second, independent cost wall reinforces (c): `run_nested` re-marshals the
**entire** FSM state through Z3 *every tick* — `given.insert(input,
current.clone())` per step, full output decode per step
(`runtime/src/effect_loop/nested.rs:431-435, 471`). Carrying the table on the
FSM's stack makes per-tick datatype marshaling O(table size) — the same
O(N²) blow-up the subscriptions shim explicitly avoids by feeding one item at
a time. This is a *setup-time* (load-time) wall, not a per-tick-runtime one,
but it independently rules out "just put the table in FSM state."

### Verdict

> **The gap is (c): no in-solve by-name lookup exists that doesn't trigger the
> Z3 string-theory blow-up, and there is no map/assoc primitive. (b) is true
> only because (c) makes wiring the table in pointless. (a) is false — the
> encoder exists and round-trips.**

This is the load-bearing correction the brief asked for: **"whole-program-table
composite INPUT" was the *assumed* mechanism; it is not the right one.** The
table never needs to enter the solve. The lookup is a string-keyed leaf that
belongs in Rust — the identical boundary `subscriptions` / `validate` /
`desugar` / `generics` all keep.

---

## § 2 — The lookup primitive

Three options for "given a name, get that schema's params."

### Option A — linear scan / cons-list walk in the FSM (`nm = key` in-solve)

**Rejected, with hard evidence.** This is the option the in-solve-string-eq
constraint forbids: it puts `nm = key` inside the per-tick solve. The validate
driver SIGSEGV and the desugar `FRef` decision are the measured proof that any
string compare on a state that also carries unrelated string literals blows up
Z3's string theory. The inject corpus carries exactly such literals (effect
type strings like `"Seq(Effect)"`, FFI signatures, `Println` payloads), so
this would reproduce the blow-up. Not viable.

### Option B — Rust-side index; the FSM receives pre-resolved per-site facts (RECOMMENDED)

The shim already **holds** the index — `&self.schemas: &HashMap<String,
SchemaDecl>` and `&self.enums` are right there on the load path
(`runtime/src/runtime/load.rs:101, 108, 155`). It resolves each call-site /
eq-site to its param/field type **in Rust** (one `HashMap` hit, no solve), and
hands the FSM only the resolved facts — small, per-claim, the cheap shape.
The FSM does the *structural* decision (which args are fresh, multi-use,
undeclared → inject) and constructs the `BodyItemList` to splice. The whole
table never crosses into the solve; the FSM state never carries it; no string
is ever *compared* in-solve — strings are only *carried* and *embedded* into
constructed `BIMembership` nodes (the cheap side of #18, already proven by
`fsm_params_build` / `prev_tick_build`).

This is the established "Evident owns the recursion / the decision, Rust owns
the string-keyed leaf" split:

| Pass | Rust owns (string leaf) | Evident owns |
|---|---|---|
| `subscriptions` | `world.`/`world_next.` prefix split | the tree walk + name-set fold |
| `validate` | the 4-element banned-set check (`is_banned`) | the tree walk + `ECall` collect |
| `desugar` | the `FRef`→`⟨items⟩` `HashMap` lookup | the `Concat` spine fold + chunk ordering |
| `generics` | the schema-map lookup + fixed-point | WALK + PARSE + SUBSTITUTE |
| **`inject` (Gap D, proposed)** | **the by-name type resolution against the table/registry** | **the fresh/multi-use/undeclared decision + membership construction** |

### Option C — a real Evident `Map(K, V)` / `lookup` primitive

**Not worth it now.** Scope: a `Map(K, V)` type lowered to a Z3 `Array K → V`,
or a `lookup(assoc, key)` builtin. Two problems: (1) lowering a string-keyed
map to Z3 arrays pushes the string compares into Z3 *array+string* theory —
the same string-theory cost, just relocated; (2) doing the lookup as a builtin
*evaluated outside the solve* is Option B mechanized into the language — same
result, far more surface. A language feature is justified only if a pass needs
a by-name lookup **whose result must then drive further in-FSM string
compares** — no current or proposed pass does. Revisit only if one appears.

### Recommendation

> **Option B, decisively.** It is the only choice consistent with the
> in-solve-string-eq constraint that shaped validate / subscriptions /
> desugar / generics. It requires *no new language feature, no whole-table
> marshal, no per-tick O(N²)* — only the same ~10-line Rust resolver leaf
> every other cutover already ships.

The deeper finding worth recording: **Gap D, as "whole-program-table composite
INPUT," is mis-framed.** The two inject sub-passes don't need the table *in the
FSM*; they need (i) a structural decision (self-hostable) and (ii) a
string-keyed type resolution (a Rust leaf). The composite-INPUT capability is
only ever needed by a *future* pass that must do a **structural whole-program
traversal that is not a by-name lookup** — and even then, per-item seeding
(generics' loop) sidesteps the O(N²) re-marshal.

---

## § 3 — The marshaling surface

Given Option B, the surface is minimal and entirely reuses what exists.

- **No new encoder.** The table is not marshaled. Body walking reuses
  `body_item_to_value` (`encode_ast.rs:872`), exactly as `inject.ev` already
  does (`portable/inject.rs:146`).
- **New small input enums** in `stdlib/passes/inject.ev` — the pass's
  self-contained cons-list AST copy — analogous to the existing `FPBInput`
  (`inject.ev:200`) and `PTBInput` (`inject.ev:259`). A single generic
  *fact* shape suffices for both sub-passes:

  ```evident
  -- A resolved injection fact: a membership to inject IF `eligible`.
  -- The Rust shim has already done every string-keyed decision
  -- (table lookup, fresh/declared check, multi-use count); the FSM
  -- only carries the strings into a constructed node — never compares.
  enum Fact     = MakeFact(String, String, Bool)   -- name, type, eligible
  enum FactList = FLNil | FLCons(Fact, FactList)
  enum FB =
      FBInit(FactList)
      FBStep(FactList, BodyItemList)
      FBDone(BodyItemList)
  ```

  These are `Bool`+`String` tuples — the cheap, faithful shape (#18's carry
  side). They never enter a comparison.
- **Output:** a `BodyItemList` of `BIMembership` to splice at `param_count`,
  decoded by the existing `decode_membership_list` (`portable/inject.rs:459`)
  and spliced by the existing `splice_at` (`portable/inject.rs:393`) — byte
  for byte the return path the two cut-over sub-passes already use.
- **Cached engine.** The WW per-thread cached `EvidentInject` already loads
  `inject.ev` once and JIT-caches each FSM (`portable/inject.rs:96-122,
  288-328`). Adding one `facts_build` FSM costs a one-time JIT compile; the
  per-claim cost is a JIT-cached `run_nested` of a small construction FSM —
  the same budget as `fsm_params_build`. New FSM names go in
  `is_self_hosted_pass_fsm` (`portable/inject.rs:355`) to keep the
  cross-engine-cascade guard correct.
- **AOT-over-runtime.** `inject` runs once per schema at **load**, never on the
  per-tick scheduler path. Per-tick runtime is structurally untouched; the only
  delta is one-time load cost (a JIT-cached construction-FSM call per
  claim-with-a-call/eq-site), consistent with the AOT-over-runtime priority.

For completeness: the *reflection* path (pinning a whole `Program` as a Z3
`given` over `stdlib/ast.ev`'s `Seq`-shaped enums — used by `literal_types.ev`)
*is* a genuine whole-program-INPUT surface. But it indexes via Z3 `Seq`-select
+ in-solve string match, i.e. Option A's blow-up; it is viable only for small
programs / non-string-keyed analysis. It is the wrong tool for a load-path pass
that runs on every claim of every program (Mario, the whole suite). Option B's
shim-resolve is correct here.

---

## § 4 — Worked example: completing the `inject` cutover

Both remaining sub-passes reduce to **the same construction FSM** the cutover
already has in spirit — "given a list of `(name, type, eligible)` facts, build
the membership list to splice." `fsm_params_build` and `prev_tick_build` are
already special cases of this. So the worked example needs **one generalized
`facts_build` FSM** plus two Rust shim methods that pre-resolve facts.

### `stdlib/passes/inject.ev` — one new FSM

```evident
-- Construct the membership list from pre-resolved facts. The Rust shim has
-- already filtered every string-keyed decision into the `eligible` Bool;
-- this FSM walks the fact list, keeps the eligible ones (the destructured-
-- Bool read, #18 keystone — already faithful), and emits BIMembership nodes
-- in list order. Order within the spliced block is whatever the shim
-- supplied (the shim sorts for determinism, as the Rust pass does today).
fsm facts_build(state ∈ FB, halt ∈ Bool)
    state = match _state
        FBInit(facts) ⇒ FBStep(facts, BILNil)
        FBStep(FLNil, acc) ⇒ FBDone(acc)
        FBStep(FLCons(f, rest), acc) ⇒ match f
            MakeFact(nm, ty, elig) ⇒
                (elig ? FBStep(rest, BILCons(BIMembership(nm, ty, PNone), acc))
                      : FBStep(rest, acc))
        FBDone(l) ⇒ FBDone(l)
    halt = match _state
        FBDone(_) ⇒ true
        _         ⇒ false
```

(Built tail-first; the shim supplies facts in reverse of the desired head
order — identical to `fsm_params_build`'s tail-first assembly,
`inject.ev:237-239`. Inline `sat_*` claims pin the construct, mirroring
`sat_build_all_three` etc.)

### `runtime/src/portable/inject.rs` — two shim methods

**`inject_claim_arg_types` (self-hosted).** The shim keeps every string-keyed /
table-keyed leaf in Rust — the exact code already in `runtime/inject.rs:40-194`
— and replaces only the final "filter + build memberships" with a `facts_build`
run:

```rust
pub fn claim_arg_types(&self, s: &mut SchemaDecl,
                       schemas: &HashMap<String, SchemaDecl>) {
    if s.external { return; }
    // 1. declared set + use counts — string work, off the Evident `inject_collect`
    //    walk output (uses = occurrence count of each reachable identifier).
    let declared = membership_names(s);
    let uses     = count_uses(&self.collect_refs(&s.body));   // reuses the walk
    // 2. per positional claim call, resolve receiver-prefix / subschema and the
    //    called claim's param types AGAINST THE TABLE — pure Rust HashMap hits
    //    (the `resolve` + `process_call` logic, verbatim from runtime/inject.rs).
    //    Produce one Fact per positional arg: (argname, paramtype,
    //    eligible = fresh ∧ ¬declared ∧ uses ≥ 2 ∧ not-a-schema-name).
    let facts = resolve_call_arg_facts(s, schemas, &declared, &uses);   // Rust leaf
    if facts.is_empty() { return; }
    // 3. construct + splice via the FSM (no table, no string compare in-solve).
    let injected = self.run_facts_build(facts);     // run_nested over `facts_build`
    splice_at(s, injected);
}
```

**`inject_lhs_eq_types` (self-hosted).** Same shape, simpler: walk constraints
for `Identifier = Expr`; resolve each RHS type via the recursive
`infer_type` resolver (field-chain lookups + enum-variant lookup against the
table/registry — all string-keyed, stays Rust, verbatim from
`runtime/inject.rs:264-333`); produce a `Fact` per eq-site (`eligible = fresh
∧ ¬declared ∧ ¬already-queued ∧ not-a-schema-name ∧ type-resolved`); run the
**same** `facts_build`; splice. Subclaim recursion stays in the shim (it
recurses per subclaim body, calling `facts_build` per level — the type
resolution per level is table-keyed, so per-level shim invocation is the
honest split; `runtime/inject.rs:359-363` already recurses this way).

### What stays in Rust, what moves

| Piece | Where | Why |
|---|---|---|
| receiver-prefix / subschema `resolve` | Rust | string split + table lookup |
| called-claim param-type lookup | Rust | `schemas.get(name)` — HashMap hit |
| RHS-type `infer_type` (field chains, enum-variant) | Rust | table + registry, string-keyed |
| `declared` set, `uses ≥ 2` typo-defense count | Rust | string-keyed counting |
| first-wins type dedup | Rust | string-keyed |
| **fresh/eligible filter + membership construction** | **Evident `facts_build`** | **structural decision on destructured Bools (#18 keystone) + node construction (#18 carry side)** |
| splice at `param_count` | Rust | index in hand on the `SchemaDecl` |

### Correctness pins

Extend `portable::inject::tests::GOLDEN` (`portable/inject.rs:527`) with corpus
sites that gain memberships from these two sub-passes — captured from the
current Rust impl **before** its deletion (the same golden-snapshot contract
the first two sub-passes use):

- `inject_claim_arg_types`: a `set_draw_color(win.renderer, Color(...), sky_eff)`
  + `effects = ⟨sky_eff, …⟩` site → `sky_eff ∈ Effect` (the doc example,
  `runtime/inject.rs:22-28`).
- `inject_lhs_eq_types`: `out = LibCall("...", "...", "...", ⟨…⟩)` → `out ∈ Effect`;
  `pos = IVec2(3, 4)` → `pos ∈ IVec2` (`runtime/inject.rs:203-207`).

Plus production-load-path pins in `runtime/tests/inject_correctness.rs`. The
test asserts the self-hosted pipeline reproduces the Rust golden byte-for-byte
on the corpus, exactly as `matches_golden_on_corpus` does today.

### Generics' orchestration lookup (the same fix)

`generics.rs::monomorphize`'s `schemas.get(&generic_head)` (`generics.rs:401`)
is the identical leaf and stays Rust by the same reasoning — it is "a
structural traversal over a mutable `HashMap` an FSM has no handle on"
(`generics.rs:24-29`). No change needed; this design simply confirms that
choice is correct and final, not a temporary gap.

---

## § 5 — First implementation slice + risks

### Smallest mergeable step

Do **`inject_lhs_eq_types` first** — it is the simpler shape (single eq-site,
no receiver-prefix/subschema resolution). One slice:

1. Add `facts_build` + the `Fact`/`FactList`/`FB` enums to
   `stdlib/passes/inject.ev`, with inline `sat_*` pins.
2. Add `EvidentInject::lhs_eq_types(&self, s, schemas, enums)` to
   `portable/inject.rs`: port the `infer_type` resolution to Rust-side fact
   production + a `facts_build` run; add `facts_build` to
   `is_self_hosted_pass_fsm`.
3. Capture the golden snapshot from the Rust `inject_lhs_eq_types`, then
   delete it and route `load.rs:101` to the new shim method.
4. `./test.sh` green; commit.

Then `inject_claim_arg_types` reusing the **same** `facts_build` FSM (only the
fact-resolution leaf differs), and delete the second Rust sub-pass + its
`schemas` argument from `load.rs:108`.

### Risks

- **Per-call cost:** low. Load-time only, JIT-cached construction FSM, same
  budget as the existing `*_build` FSMs. Per-tick scheduler runtime untouched
  (AOT-over-runtime honored).
- **In-solve string-eq:** **eliminated by construction.** No string is ever
  compared in the solve; strings are only carried and embedded into constructed
  nodes (the cheap #18 side, already proven faithful by `fsm_params_build`).
  This is the entire point of Option B.
- **LOC:** the honest one. Expect **net Rust roughly flat-to-slightly-positive**,
  not a shrink — the small-pure-pass self-host ceiling (REVIVE-generics: net
  Rust +~100 because the shim/marshaling tax exceeds the deleted pure pass). The
  string-keyed resolution leaves stay in Rust regardless; only the
  filter+construct moves. Don't sell this as a LOC win.

### Recommendation

**Do it — for the consistency / dogfooding close of Gap D and to make `inject`
fully self-hosted, not for a LOC reduction.** The durable deliverable is the
*reframing*: Gap D was never "marshal the whole table in" — that path
re-imports the in-solve string-eq blow-up and the per-tick O(N²) re-marshal.
The correct close is the shim-resolve split, which means **the composite
whole-program INPUT capability is not required to finish `inject` or to settle
generics' orchestration.** Build the `facts_build` slice; record that the
whole-program-table-as-FSM-state idea is the wrong mechanism and the by-name
lookup is a permanent Rust leaf; defer any `Map(K,V)` language feature until a
pass appears that needs a lookup result to drive further in-FSM string
compares (none does today).
