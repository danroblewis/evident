# Floor enum ctors (`Exit` / `LibCall`) as general expressions

**Type:** compiler gap / design. Found 2026-06-12 during the rc=7/9
self-compile sweep. NOT bodyless-record-ordering (the A′ pre-scan does
not touch it). NOT a source-reorder fix.

## The gap

The self-hosted compiler resolves a 0 Z3 handle (→ `Exit(9)`,
"unresolved identifier: `Exit`" / `LibCall`) whenever an **Effect-floor
enum constructor is used as a general expression** — i.e. assigned by
`=` to a scalar `Effect`-typed const, or appearing as a bare ternary
branch:

```evident
eff ∈ Effect = Exit(7)                              -- rc=9 unresolved Exit
eff ∈ Effect = (bad ? Exit(7) : LibCall(…))         -- rc=9
tag_cell ∈ Effect = LibCall("__mem", "write_long", ⟨…⟩)   -- rc=9 unresolved LibCall
```

Only the **effect-literal `⟨…⟩` chain** form works:

```evident
effects ∈ Seq(Effect) = ⟨Exit(7)⟩                   -- rc=0 (works)
effects ∈ Seq(Effect) = (bad ? ⟨Exit(7)⟩ : ⟨Exit(8)⟩) -- rc=0 (works)
```

User-enum ctors as scalar expressions work fine (`c ∈ Color = Green`,
`b ∈ Box = Mk(5)` — both rc=0); the gap is **specific to the floor
variants** `Exit` / `LibCall` (and the rest of the Effect floor).

## Root cause (two layers)

1. **Pratt never forms the call.** `call_shift` (driver_expr.ev:372)
   only treats `Name(` as a call when `Name` is in the `calls` registry
   = `callable_names` = builtin callables ++ `variant_names`
   (driver_pratt.ev:36, 40). `variant_names` is built ONLY for
   `_is_user_enum` declarations (driver_enum.ev:402, 408) — the **floor
   enum is excluded**. So `Exit(7)` Pratt-parses as a bare `EIdent("Exit")`
   followed by a parenthesized group, never `ECall1("Exit", …)`. The
   bare ident then gets `push_ident` with a 0 handle → the rc=9
   diagnostic names `Exit`.

2. **The generic ctor dispatch never resolves the floor decl.**
   `ctor_decl` (driver_exprdecomp.ev:70-72) scans `user_variants` only;
   floor variants aren't there, so even if Pratt formed the call,
   `ctor_decl = 0` and call1/3_items fall to `C2PushH(0)`. The floor
   ctor decls live in DriverEnum as dedicated registers
   (`exit_decl` / `libcall_decl`, driver_enum.ev:334-339) and are
   consulted ONLY by the effect-literal parser in DriverClaimIdx
   (`head_ident = "Exit"` / `"LibCall"`, driver_claimidx.ev:170-175).
   The design comment at driver_exprdecomp.ev:69 ("the legacy
   dropped-Exit(3+4) class is impossible by construction") records the
   assumption that floor ctors only ever appear inside `⟨…⟩`.

## Why it matters — it is a REAL driver_main gap

The compiler's OWN source uses the scalar-Effect form pervasively, so
the full `driver_main` self-compile would hit it too (not just the unit
wrappers):

- `compiler2/driver_lex.ev:133-138` — `tag_cell / payload_cell /
  op_cell ∈ Effect = LibCall("__mem", "write_long", ⟨…⟩)`.
- `compiler2/translate2_bool.ev:60-110` — `eff = (bad ? Exit(7) :
  is_eq ? LibCall("libz3", "Z3_mk_eq", …) : …)` (a long ternary chain
  of LibCall branches with an `Exit(7)` null-guard arm).
- `compiler2/translate2_ctor.ev:250` — `eff = (ctor_decl_h = 0 ?
  Exit(7) : app_eff)`.

Unit wrappers blocked by this (rc=9 `Exit`/`LibCall`, or rc=7 from the
downstream 0-handle guard firing): `driver_lex/{lex_idents,lex_twochar_op}`
(after their `next_char` ordering is also resolved),
`driver_workitems/ternary_null_guard`, `argref/argref_error`,
`driver_symtab/decode_peel`.

## Fix sketch (a dedicated, gated effort — NOT done here)

A partial attempt in this session (register `Exit` in `builtin_callables`
+ map `ctor_decl → exit_decl`) made Pratt form `ECall1("Exit", n)` and
routed to `C2App(exit_decl, 1)`, but the scalar build then emitted
`Error: Sorts Int and Effect are incompatible` — the 1-arg C2App path
does not produce a correctly-sorted scalar `Effect` const. And it does
nothing for `LibCall`, whose 3-arg `Seq(LibArg)` shape has no generic
builder (only the efflit parser constructs the LibArg cells). The
partial change was reverted (it made no module clean and added risk).

A complete fix needs all of:

1. Add the floor callable names to `callable_names` so Pratt forms the
   call (cheap; bump `builtin_callables`).
2. Resolve the floor decl in `ctor_decl` (`Exit → exit_decl`,
   `LibCall → libcall_decl`).
3. A correct scalar-Effect C2App builder for `Exit(n)` (the current
   `C2App(exit_decl, 1)` mis-sorts — investigate the arg/result sort
   threading vs. the working efflit `C2App(exit_decl, 1)` inside the
   chain).
4. A general `LibCall(lib, fn, ⟨args⟩)` builder reachable from a
   ternary branch — the hard part: it must construct the `Seq(LibArg)`
   cells the way DriverClaimIdx's `libcall_items` does, but in
   expression position rather than the efflit chain.

It touches the hot Pratt dispatch + ctor lowering, so it must be gated
per step on `tests/compiler2_units/run.sh`, `functionization-gate.sh`,
and the full conformance run (137/138), and bisected if a batch bails.
Given the conformance risk and that the structural A′ declaration
pre-scan is in flight in parallel, this is scoped as its own effort.

## Workaround until then

Write floor effects through the effect-literal `⟨…⟩` chain (the working
surface), not as scalar `Effect`-typed consts. Where a named scalar is
wanted for composition, keep building it inside `effects = ⟨…⟩` rather
than `name ∈ Effect = <ctor>`.
