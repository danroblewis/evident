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
eff ∈ Effect = Exit(7)                              -- FIXED 2026-06-12 (rc=0)
eff ∈ Effect = (h = 0 ? Exit(7) : app)              -- FIXED 2026-06-12 (rc=0)
eff ∈ Effect = (bad ? Exit(7) : LibCall(…))         -- partial: LibCall arm still rc=9
tag_cell ∈ Effect = LibCall("__mem", "write_long", ⟨…⟩)   -- rc=9 unresolved ArgInt (see Status)
```

> **UPDATE 2026-06-12:** `Exit(n)` is LANDED (all three forms above
> compile, rc=0). `LibCall` as a general expression remains blocked on
> the `Seq(LibArg)` cons-list builder — see "## Status" below for the
> precise remaining blocker and the three-layer root cause (the prior
> agent's `Sorts Int and Effect` was a THIRD layer: a missing `Effect`
> sort-code mapping, now fixed).

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

## Status (2026-06-12): `Exit` LANDED; `LibCall` remainder documented

`Exit(n)` as a general expression now compiles cleanly through the
self-hosted compiler — scalar pin (`eff ∈ Effect = Exit(7)`), claim-body
binding, and the ternary-branch form (`eff = (h = 0 ? Exit(7) : app)`,
the `translate2_ctor.ev:250` shape). Verified by self-compiling minimal
fixtures (rc=0, emits `(assert (= a (Exit 3)))`) and by the
`driver_workitems/ternary_null_guard` sweep advancing past `Exit` to the
`LibCall` arg gap. Fast gates green (functionization, invariant, the
three touched-module unit suites); conformance held the baseline.

### The fix had THREE layers, not two

The original two layers were correct but incomplete. A third, deeper
layer was the actual cause of the prior agent's `Sorts Int and Effect
are incompatible`:

1. **(landed)** Floor ctors absent from the Pratt call registry —
   seeded `Exit`/`LibCall` into `callable_names` (`driver_pratt.ev`,
   `floor_ctor_names`, appended after `variant_names`).
2. **(landed)** `ctor_decl` only scanned `user_variants` — added the
   `Exit → exit_decl` / `LibCall → libcall_decl` special-cases
   (`driver_exprdecomp.ev`), mirroring `matches_tester`'s
   `IntResult`/`StringResult` cases.
3. **(landed — the real `Sorts Int and Effect` cause)** The type name
   `Effect` had **no sort code** in `DriverClassify.line.sort`
   (`driver_classify.ev`), so `e ∈ Effect` declared the const at the
   default **Int** sort. Then `Z3_mk_eq(e:Int, Exit-app:Effect)` raised
   the sort error. Added `Effect → 7` and wired sort-code `7 →
   effect_sort` in the `mkconst` dispatch (`driver.ev`), exactly
   paralleling `Result → 4 → result_sort` (the precedent — `Result`
   already worked this way, `Effect` was simply never added). The trace
   tool that pinned this: `EVIDENT_EFFECT_TRACE=1` showed
   `Z3_mk_const(…, sort=isort)` for `e` against `Z3_mk_app(exit_decl,…)`
   producing Effect.

### `LibCall` remainder — the precise remaining blocker

`LibCall(lib, fn, ⟨args⟩)` as a general expression now Pratt-parses as a
3-arg call (layer 1) and resolves `ctor_decl = libcall_decl` (layer 2),
but the **third argument** — the `⟨ArgInt(c), …⟩` `Seq(LibArg)` literal —
cannot be lowered by the generic `C2Process` path. Self-compiling
`eff = LibCall("libz3", "Z3_mk_eq", ⟨ArgInt(c)⟩)` halts at **rc=9
`unresolved identifier: ArgInt`**. Two sub-gaps remain, both inside the
3rd-arg build:

- **(a) `ArgInt`/`ArgStr`/`ArgRef` not callable ctors in expr position.**
  Their decls exist (`argint_decl`/`argstr_decl`) but aren't in
  `callable_names`/`ctor_decl`. Adding them is the same cheap pattern as
  layers 1–2.
- **(b) The `⟨…⟩` literal must build the `__SeqOf_LibArg` CONS-LIST, not
  a Z3 Seq.** `LibCall__f2` is typed `__SeqOf_LibArg` (a cons-list
  datatype: `__Cell_LibArg(arg, tail)` / `__Empty_LibArg`), but the
  generic seq-literal lowering in the Pratt/`C2Process` path builds a Z3
  `seq` value — an INCOMPATIBLE representation. The efflit chain's
  `libcall_items` (`driver_claimidx.ev:203`) constructs the cons-list by
  hand with `C2App(cell_decl, 2)` + `C2PushH(empty_val)`; a general-expr
  `LibCall` needs the same cons-list assembly reachable from
  `call3_items`' 3rd-arg processing. This is the hard part: it is a new
  lowering for "seq-of-LibArg literal in expression position," and it
  touches the hot Pratt/`C2Process` seq path with real conformance risk.
  Note the current efflit `libcall_items` also only handles a SINGLE
  arg cell — a general N-arg builder must fold the cons-list.

The clean next step: do (a), then add a dedicated `call3_items` branch
for `call_name = "LibCall"` that, instead of `C2Process(arg2)`, walks
the arg2 seq-literal elements emitting `C2App(argN_decl,1)` per cell and
folding them with `C2App(cell_decl, 2)` over a `C2PushH(empty_val)` tail
(N-ary generalization of `libcall_items`), then `C2App(libcall_decl, 3)`.
Gate per step on the three touched-module unit suites,
`functionization-gate.sh`, `invariant-gate.sh`, and full conformance.

## Workaround for `LibCall` until (b) lands

Write `LibCall` floor effects through the effect-literal `⟨…⟩` chain
(the working surface, unaffected by these changes — the efflit parse
mode handles `LibCall` before Pratt sees it), not as scalar
`Effect`-typed consts. `Exit` no longer needs the workaround.
