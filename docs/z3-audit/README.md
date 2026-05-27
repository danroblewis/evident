# Z3-Replaceability Audit — ranked summary (all 120 runtime/src files)

> **One line:** Of **120** `runtime/src/*.rs` files, **0** are genuine *new*
> high-confidence `replaceable-alone` candidates. The Z3-solve-replaceable
> surface has already been harvested by the prior self-hosting sessions —
> what remains is overwhelmingly the **compile pipeline that produces Z3
> input** (57 `circular`), **data/IO/dispatch plumbing** (39 `not-a-CSP`),
> and **glue / hot-path** (12 `trivial` + `hot-path`). The 12 "replaceable"
> verdicts are *already-self-hosted passes* (their Rust residual shims) plus
> two partial ports gated on known, tracked blockers.

This file ranks every per-file report in `docs/z3-audit/`. Each row links to
its report. The audit asks one question per file: **could this file's job be
replaced by a Z3 constraint *solve* (expressed as an Evident claim Z3
satisfies), or is it something else?**

## The honest top-line

The estimate in the session brief was "~128 files"; the repository actually
contains **120** `.rs` files under `runtime/src/`, all now audited (63 from
the cut-off run + 57 completed here).

| Verdict | Count | What it means |
|---|---:|---|
| `circular` | 57 | IS the compile pipeline that produces/runs Z3 input (parser, lexer, AST→Z3 translate, inline-expansion, functionize/JIT, IR extraction, solver-driver, CHC binding). A solve can't replace the thing that builds solves. |
| `not-a-CSP` | 39 | Pure data definitions, IO/effect dispatch, FFI marshaling, CLI entry points, async event sources, model-value extraction. No decision problem. |
| `replaceable-as-group` | 11 | Replaceable only *with* a related file — and in every case here, the solve **already lives in Evident** (`stdlib/passes/*.ev`); the Rust file is the residual swap-interface shim, or a partial port with named blockers. |
| `trivial` | 8 | Tiny module (mod.rs re-exports, small helpers/glue) — too small to be worth a solve. |
| `hot-path` | 4 | Runs per-tick in the scheduler inner loop; a solve-per-tick adds overhead with no benefit. |
| `replaceable-alone` | 1 | `translate/preprocess.rs` — a load-time fixed-point propagation that *could* be a solve, but **medium** confidence and the report's own conclusion is "self-hosting buys little." |

**Net: there is no remaining high-confidence `replaceable-alone` + small +
load-time opportunity to act on.** This is the expected result: the
self-hosting program (pretty, validate, subscriptions, toposort, generics,
desugar) has already moved every "Rust algorithm that decides a property"
into Evident that was tractable. What's left in Rust is, by construction, the
part that *can't* be a solve (the compiler) or *shouldn't* be (the per-tick
loop, the IO plumbing).

## Ranked opportunities (the 12 "replaceable" verdicts, tiered)

### Tier A — already exploited: the solve runs in Evident, Rust is the residual shim
No action. These report `replaceable-as-group` because the constraint/walk
**already lives** in `stdlib/passes/*.ev`; the named Rust file is the thin
swap-interface shim (cache/marshaling/guard glue) that the cutover left
behind. Listed for completeness — they are evidence the harvest happened, not
a backlog.

| File | Cut-over session | Residual Rust is… |
|---|---|---|
| [`runtime/src/portable/pretty.rs`](portable__pretty.md) | pretty-evident | ~60 lines of trait + engine glue; `RustPretty` deleted |
| [`runtime/src/runtime/validate.rs`](runtime__validate.md) | VALIDATE-recursive | 22-line adapter → `portable::validate` → `validate.ev` |
| [`runtime/src/subscriptions.rs`](subscriptions.md) | XX/ZZ (merged) | `AccessSets` type + fd-conflict detector; inference is in `subscriptions.ev` |
| [`runtime/src/effect_loop/toposort.rs`](effect_loop__toposort.md) | PORT-toposort | memo cache + cycle recovery; the int-rank solve is `ToposortRanks` |
| [`runtime/src/portable/generics.rs`](portable__generics.md) | REVIVE-generics | fixed-point driver + presence guard; walk/string-ops in `generics.ev` |
| [`runtime/src/portable/desugar.rs`](portable__desugar.md) | REVIVE-desugar | Concat-presence guard + rewrite-walk; gather/flatten in `desugar.ev` |

### Tier B — partial port, full cutover gated on named blockers (the real "next" work)
These are where genuine self-hosting LOC remains, but each is blocked by a
specific, already-documented gap — not a fresh green-field opportunity. They
are the highest-value *future* targets, ranked by how close they are.

1. **[`runtime/src/portable/inject.rs`](portable__inject.md)** + [`runtime/src/runtime/inject.rs`](runtime__inject.md) — walk + build already self-hosted in `inject.ev`. Cutover blockers (substring ops, `param_count` marshaling) were **closed** by sessions GAPC + SEED-marshal; the residual blocker is the in-solve string-set membership (#18-cousin Z3 string blowup) and the whole-program-table input. Closest to a clean finish.
2. **[`runtime/src/runtime/desugar.rs`](runtime__desugar.md)** — `desugar_seq_concat` done; `unify_world_syntax` / `unify_state_syntax` remain. Blocked on a **string-construction op** (generating `world_next.{field}` names) — the same recurring "no format!" wall.
3. **[`runtime/src/portable/introspect.rs`](portable__introspect.md)** + [`runtime/src/runtime/introspect.rs`](runtime__introspect.md) — AST rebuild already self-hosts; residual is the `cons_to_seq` list-representation asymmetry between FSM output and `decode_schema_decl`. Lowest criticality (only `--infer-types` path).

**The recurring unlock across Tier B is the same one prior sessions named:**
a string-construction / decomposition primitive in `translate/`, plus the
in-solve-string-equality avoidance pattern (collect-in-FSM, check-in-Rust).
Neither is a per-file change — they are language/runtime capabilities. See
[`docs/design/self-hosting-inventory.md`](../design/self-hosting-inventory.md).

### Tier C — `replaceable-alone`, but low value
- **[`runtime/src/translate/preprocess.rs`](translate__preprocess.md)** — `collect_seq_lengths` / `collect_pinned_ints` are integer-equality fixed-point propagation (Z3's home turf), so a solve *could* express them. But: **medium** confidence, it runs pre-translate on *every* query (a solve-per-query load tax), and the Rust is a fast, trivially-maintained fixed-point loop. The report's own verdict: "self-hosting them buys little." **Does not meet the change bar** (needs `replaceable-alone` + *high* confidence + small + load-time).

## Change made this session
**None.** The change bar (`replaceable-alone` + **high** confidence + small +
load-time + outside the off-limits set) is met by **zero** files. The single
`replaceable-alone` (`preprocess.rs`) is medium-confidence and explicitly
net-negative to port. Per the honest-fallback guidance, no change was made —
weakening a correct, fast Rust pass into a slower solve to manufacture a
"change made" would be exactly the wrong move. The deliverable is the map.

## What feeds SMT-LIB-front-end prioritization
The `circular` 57 ARE the compile pipeline the SMT-LIB north star
([`docs/design/smtlib-as-compile-target.md`](../design/smtlib-as-compile-target.md))
plans to refactor *Rust → SMT-LIB → Evident*. They are not "replaceable by a
solve" (that's circular) — but they are precisely the surface the SMT-LIB
emitter must eventually cover. The ranked Tier-B list above is the mode-1
self-hosting backlog; the `circular` translate/ + inline/ + exprs/ cluster is
the mode-2 (function→constraint, compile-target) frontier.

## Full table (all 120, grouped by area)

| File | Criticality | Verdict | Confidence |
|---|---|---|---|
| [`runtime/src/chc.rs`](chc.md) | peripheral (additive — on no existing runtime path; not yet selector-wired into compose.rs) | circular | high |
| [`runtime/src/commands.rs`](commands.md) | does-little | not-a-CSP | high |
| [`runtime/src/decompose.rs`](decompose.md) | peripheral | not-a-CSP | high |
| [`runtime/src/effect_dispatch.rs`](effect_dispatch.md) | critical | not-a-CSP | high |
| [`runtime/src/ffi.rs`](ffi.md) | critical | not-a-CSP | high |
| [`runtime/src/fti.rs`](fti.md) | critical | not-a-CSP | high |
| [`runtime/src/lexer.rs`](lexer.md) | critical | circular | high |
| [`runtime/src/lib.rs`](lib.md) | critical | circular | high |
| [`runtime/src/main.rs`](main.md) | peripheral | not-a-CSP | high |
| [`runtime/src/pretty.rs`](pretty.md) | peripheral | circular | high |
| [`runtime/src/stdlib_path.rs`](stdlib_path.md) | peripheral | not-a-CSP | high |
| [`runtime/src/subscriptions.rs`](subscriptions.md) | peripheral | replaceable-as-group | high |
| [`runtime/src/translate.rs`](translate.md) | critical | circular | high |
| [`runtime/src/value_builders.rs`](value_builders.md) | critical | not-a-CSP | high |
| [`runtime/src/z3_eval.rs`](z3_eval.md) | critical | circular | high |
| [`runtime/src/z3_profile.rs`](z3_profile.md) | does-little | not-a-CSP | high |
| [`runtime/src/core/api.rs`](core__api.md) | critical | not-a-CSP | high |
| [`runtime/src/core/ast.rs`](core__ast.md) | critical | not-a-CSP | high |
| [`runtime/src/core/functionizer.rs`](core__functionizer.md) | critical | not-a-CSP | high |
| [`runtime/src/core/mod.rs`](core__mod.md) | critical | not-a-CSP | high |
| [`runtime/src/core/seq_helpers.rs`](core__seq_helpers.md) | peripheral | trivial | high |
| [`runtime/src/core/value.rs`](core__value.md) | critical | not-a-CSP | high |
| [`runtime/src/core/z3_program.rs`](core__z3_program.md) | critical | not-a-CSP | high |
| [`runtime/src/core/z3_types.rs`](core__z3_types.md) | critical | not-a-CSP | high |
| [`runtime/src/parser/atoms.rs`](parser__atoms.md) | critical | circular | high |
| [`runtime/src/parser/body_item.rs`](parser__body_item.md) | critical | circular | high |
| [`runtime/src/parser/exprs.rs`](parser__exprs.md) | critical | circular | high |
| [`runtime/src/parser/mod.rs`](parser__mod.md) | critical | circular | high |
| [`runtime/src/parser/patterns.rs`](parser__patterns.md) | critical | circular | high |
| [`runtime/src/parser/program.rs`](parser__program.md) | critical | circular | high |
| [`runtime/src/parser/schema.rs`](parser__schema.md) | critical | circular | high |
| [`runtime/src/parser/tests.rs`](parser__tests.md) | peripheral | not-a-CSP | high |
| [`runtime/src/parser/types.rs`](parser__types.md) | critical | circular | high |
| [`runtime/src/translate/datatypes.rs`](translate__datatypes.md) | critical (load-time, on the translate pipeline) | circular | high |
| [`runtime/src/translate/declare.rs`](translate__declare.md) | critical (load-time, core of the translate pipeline) | circular | high |
| [`runtime/src/translate/decode_ast.rs`](translate__decode_ast.md) | critical (load-time, on the self-hosted pass pipeline) | circular | high |
| [`runtime/src/translate/encode_ast.rs`](translate__encode_ast.md) | critical (load-time, on the self-hosting bridge) | circular | high |
| [`runtime/src/translate/eval/cached.rs`](translate__eval__cached.md) | critical (this is the innermost per-tick hot path for all FSM queries in the scheduler loop) | circular | high |
| [`runtime/src/translate/eval/core.rs`](translate__eval__core.md) | peripheral (diagnostic/debugging path, not on the normal per-tick solve path) | circular | high |
| [`runtime/src/translate/eval/decode.rs`](translate__eval__decode.md) | critical (called by every evaluate* path after every successful solve to produce the `QueryResult::bindings` map) | not-a-CSP | high |
| [`runtime/src/translate/eval/decompose.rs`](translate__eval__decompose.md) | peripheral (used only by diagnostic/analysis APIs — `runtime::analysis.rs`, explore examples, probe_mario — never on the per-tick scheduler path) | circular | high |
| [`runtime/src/translate/eval/extra.rs`](translate__eval__extra.md) | peripheral (used by self-hosted pass paths in `runtime/reflection.rs` and `introspect.rs`, not on the normal per-tick FSM scheduler path) | circular | high |
| [`runtime/src/translate/eval/mod.rs`](translate__eval__mod.md) | critical (re-exports `evaluate`, `build_cache`, `run_cached`, etc. — everything the rest of the runtime calls into) | circular | high |
| [`runtime/src/translate/eval/solver.rs`](translate__eval__solver.md) | critical (called at the start of every solve path via every `evaluate*` variant) | circular | high |
| [`runtime/src/translate/exprs/bool.rs`](translate__exprs__bool.md) | critical | circular | high |
| [`runtime/src/translate/exprs/enums.rs`](translate__exprs__enums.md) | critical | circular | high |
| [`runtime/src/translate/exprs/mapping.rs`](translate__exprs__mapping.md) | critical | circular | high |
| [`runtime/src/translate/exprs/match_expr.rs`](translate__exprs__match_expr.md) | critical | circular | high |
| [`runtime/src/translate/exprs/mod.rs`](translate__exprs__mod.md) | critical | trivial | high |
| [`runtime/src/translate/exprs/quant.rs`](translate__exprs__quant.md) | critical | circular | high |
| [`runtime/src/translate/exprs/range.rs`](translate__exprs__range.md) | critical | circular | high |
| [`runtime/src/translate/exprs/record_lift.rs`](translate__exprs__record_lift.md) | critical | circular | high |
| [`runtime/src/translate/exprs/scalar.rs`](translate__exprs__scalar.md) | critical | circular | high |
| [`runtime/src/translate/exprs/seq_eq.rs`](translate__exprs__seq_eq.md) | critical | circular | high |
| [`runtime/src/translate/exprs/seq_field.rs`](translate__exprs__seq_field.md) | critical | circular | high |
| [`runtime/src/translate/exprs/string_ops.rs`](translate__exprs__string_ops.md) | critical | circular | high |
| [`runtime/src/translate/extract.rs`](translate__extract.md) | critical (load-time and tick-level result extraction) | circular | high |
| [`runtime/src/translate/inline/calls.rs`](translate__inline__calls.md) | critical | circular | high |
| [`runtime/src/translate/inline/dispatch.rs`](translate__inline__dispatch.md) | critical | circular | high |
| [`runtime/src/translate/inline/guards.rs`](translate__inline__guards.md) | critical | circular | high |
| [`runtime/src/translate/inline/membership.rs`](translate__inline__membership.md) | critical | circular | high |
| [`runtime/src/translate/inline/mod.rs`](translate__inline__mod.md) | peripheral | trivial | high |
| [`runtime/src/translate/inline/recursion.rs`](translate__inline__recursion.md) | critical | circular | high |
| [`runtime/src/translate/inline/rewrite.rs`](translate__inline__rewrite.md) | critical | circular | high |
| [`runtime/src/translate/inline/subschema.rs`](translate__inline__subschema.md) | critical | circular | high |
| [`runtime/src/translate/inline/walk.rs`](translate__inline__walk.md) | critical | circular | high |
| [`runtime/src/translate/preprocess.rs`](translate__preprocess.md) | critical (load-time, runs on every query before translation) | replaceable-alone | medium |
| [`runtime/src/translate/smtlib.rs`](translate__smtlib.md) | peripheral | circular | high |
| [`runtime/src/functionize/cranelift.rs`](functionize__cranelift.md) | critical | circular | high |
| [`runtime/src/functionize/glsl.rs`](functionize__glsl.md) | peripheral | circular | high |
| [`runtime/src/functionize/llm.rs`](functionize__llm.md) | peripheral | circular | high |
| [`runtime/src/functionize/mod.rs`](functionize__mod.md) | peripheral | trivial | high |
| [`runtime/src/functionize/satisfier.rs`](functionize__satisfier.md) | peripheral | circular | high |
| [`runtime/src/functionize/symbolic.rs`](functionize__symbolic.md) | peripheral | circular | high |
| [`runtime/src/fsm_unroll/compose.rs`](fsm_unroll__compose.md) | critical | circular | high |
| [`runtime/src/fsm_unroll/detector.rs`](fsm_unroll__detector.md) | peripheral | not-a-CSP | high |
| [`runtime/src/fsm_unroll/mod.rs`](fsm_unroll__mod.md) | peripheral | trivial | high |
| [`runtime/src/runtime/analysis.rs`](runtime__analysis.md) | peripheral (diagnostic/test path — `query_with_core` used by `evident test`; decomposition/classify used by analysis commands) | circular | high |
| [`runtime/src/runtime/autotune.rs`](runtime__autotune.md) | peripheral (load-time warmup; only affects the first ~60 ticks per schema until locked) | not-a-CSP | high |
| [`runtime/src/runtime/desugar.rs`](runtime__desugar.md) | critical (load-time; all three transforms run on every schema load and are prerequisites for translation) | replaceable-as-group | medium |
| [`runtime/src/runtime/inject.rs`](runtime__inject.md) | critical (load-time; every schema load runs both passes; without them undeclared names fail translation) | replaceable-as-group | medium |
| [`runtime/src/runtime/introspect.rs`](runtime__introspect.md) | peripheral (used by the `--infer-types` interactive inference pipeline; not on the standard load or tick path) | replaceable-as-group | medium |
| [`runtime/src/runtime/lenient.rs`](runtime__lenient.md) | peripheral (used at specific call sites to enable lenient mode temporarily) | trivial | high |
| [`runtime/src/runtime/load.rs`](runtime__load.md) | critical (load-time; this is the entry point for all schema loading — nothing can be queried without it) | circular | high |
| [`runtime/src/runtime/mod.rs`](runtime__mod.md) | critical (load-time and tick-0; this is the top-level API struct — every path through the runtime touches it) | not-a-CSP | high |
| [`runtime/src/runtime/nested.rs`](runtime__nested.md) | critical (load-time + query-time, in the constraint-building pipeline) | circular | high |
| [`runtime/src/runtime/query.rs`](runtime__query.md) | critical (every solve — both load-time and per-tick — flows through here) | circular | high |
| [`runtime/src/runtime/reflection.rs`](runtime__reflection.md) | critical (load-time; gating path for all self-hosted passes) | circular | high |
| [`runtime/src/runtime/register_enums.rs`](runtime__register_enums.md) | critical (load-time; must run before any schema that references an enum can be translated) | circular | high |
| [`runtime/src/runtime/sample.rs`](runtime__sample.md) | peripheral (used only by the `evident sample` CLI subcommand, not on any per-tick path) | trivial | high |
| [`runtime/src/runtime/scheduler_api.rs`](runtime__scheduler_api.md) | critical (hot per-tick path — every FSM tick calls through here) | hot-path | high |
| [`runtime/src/runtime/stats.rs`](runtime__stats.md) | peripheral (diagnostic/observability only; no effect on correctness) | not-a-CSP | high |
| [`runtime/src/runtime/validate.rs`](runtime__validate.md) | critical (load-time; blocks unsafe programs from loading) | replaceable-as-group | high |
| [`runtime/src/effect_loop/collect.rs`](effect_loop__collect.md) | critical | not-a-CSP | high |
| [`runtime/src/effect_loop/fsm.rs`](effect_loop__fsm.md) | critical | not-a-CSP | high |
| [`runtime/src/effect_loop/mod.rs`](effect_loop__mod.md) | critical | hot-path | high |
| [`runtime/src/effect_loop/nested.rs`](effect_loop__nested.md) | critical | hot-path | high |
| [`runtime/src/effect_loop/scheduler.rs`](effect_loop__scheduler.md) | critical | hot-path | high |
| [`runtime/src/effect_loop/state.rs`](effect_loop__state.md) | critical | not-a-CSP | high |
| [`runtime/src/effect_loop/timing.rs`](effect_loop__timing.md) | peripheral | trivial | high |
| [`runtime/src/effect_loop/toposort.rs`](effect_loop__toposort.md) | critical (on the tick-0 path; cached thereafter) | replaceable-as-group | high |
| [`runtime/src/event_sources/declarative_install.rs`](event_sources__declarative_install.md) | critical | not-a-CSP | high |
| [`runtime/src/event_sources/file_line_reader.rs`](event_sources__file_line_reader.md) | peripheral | not-a-CSP | high |
| [`runtime/src/event_sources/file_watcher.rs`](event_sources__file_watcher.md) | peripheral | not-a-CSP | high |
| [`runtime/src/event_sources/frame_timer.rs`](event_sources__frame_timer.md) | critical | not-a-CSP | high |
| [`runtime/src/event_sources/mod.rs`](event_sources__mod.md) | critical | not-a-CSP | high |
| [`runtime/src/event_sources/reflection.rs`](event_sources__reflection.md) | peripheral | not-a-CSP | high |
| [`runtime/src/event_sources/sigint.rs`](event_sources__sigint.md) | peripheral | not-a-CSP | high |
| [`runtime/src/event_sources/stdin.rs`](event_sources__stdin.md) | peripheral | not-a-CSP | high |
| [`runtime/src/event_sources/wall_clock.rs`](event_sources__wall_clock.md) | peripheral | not-a-CSP | high |
| [`runtime/src/portable/desugar.rs`](portable__desugar.md) | critical (load-time — runs on every schema that contains `++` before translation) | replaceable-as-group | high |
| [`runtime/src/portable/generics.rs`](portable__generics.md) | critical (load-time — runs on every program with generic type instantiations before translation) | replaceable-as-group | high |
| [`runtime/src/portable/inject.rs`](portable__inject.md) | critical (load-time — runs on every `fsm` schema before translation; a bootstrapping guard prevents re-entrant engine loads) | replaceable-as-group | high |
| [`runtime/src/portable/introspect.rs`](portable__introspect.md) | peripheral (load-time — called only from the passthrough-desugar auto-apply in `commands/common.rs`; never on per-tick, translate, or scheduler path) | replaceable-as-group | high |
| [`runtime/src/portable/mod.rs`](portable__mod.md) | critical (load-time — every self-hosted pass uses this infrastructure; the bootstrapping guard makes it load-path safe) | not-a-CSP | high |
| [`runtime/src/portable/pretty.rs`](portable__pretty.md) | peripheral (load-time / diagnostic only — used for UNSAT diagnostics and `evident check` output; never on the per-tick scheduler path) | replaceable-as-group | high |
| [`runtime/src/commands/common.rs`](commands__common.md) | peripheral | not-a-CSP | high |
| [`runtime/src/commands/effect_run.rs`](commands__effect_run.md) | peripheral | not-a-CSP | high |
| [`runtime/src/commands/sample.rs`](commands__sample.md) | peripheral | not-a-CSP | high |
| [`runtime/src/commands/test.rs`](commands__test.md) | peripheral | not-a-CSP | high |