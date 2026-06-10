# carry-preserving fsm composition

Status: landed (2026-06-08). Extends `scripts/passes/expand-fsm-autocarry.sh`
(see `docs/plans/fsm-autocarry.md` for the single-fsm base case).

## The problem this overturns

`docs/plans/driver-subsystem-map.md` §3 concluded — probe-backed — that a
composed sub-claim **cannot own FSM carry state**: the frozen oracle inlines
`Sub(x ↦ y)` as a scoped value substitution (x→y in Sub's body) but knows
nothing of the kernel's `_<name>` carry convention. Sub's carry sibling `_x`
is α-renamed to a dead `Helper__x__callN`, the parent's `_y` is never
synthesized, and the carry breaks (UNSAT / garbage). That verdict held *for
the oracle alone*. It is overturned **at the transform layer**: the
pre-oracle source transform now rewrites composition so that the carry pair
lands as the **parent's own** top-level `y` / `_y` — which the kernel does
carry. The oracle still does only value substitution; the transform makes
the substitution carry-correct before the oracle ever sees it.

This is the architectural unlock for decomposing `driver_main`'s giant
single claim into self-contained, unit-testable fsm sub-claims that own
their carry state.

## Carry travels with the logic

The kernel builds its manifest `state-fields` list from the **top claim's**
primitive memberships, and carries a field `y` across ticks iff a sibling
`_y` is also a top-level membership. Composition inlines a sub-fsm's body
into the parent. So for a composed carry to survive, the carry pair `y`/`_y`
must be the **parent's** fields, and the inlined transition (`y = … _y …`)
must be written over them. The transform guarantees exactly that: after it
runs, the carry declaration and the logic that reads it are co-located in the
parent, and the kernel carries the value. Carry travels WITH the logic.

## The registry (fixpoint)

Pass 1 scans every `fsm` and records, per fsm `F`:
- bare field decls `x ∈ T` (the carry anchor + its `base(T)`),
- the set of `_x` tokens referenced in `F`'s code (comments stripped),
- every composition call `Sub(slot ↦ value, …)` with its parsed bindings.

Pass 2 computes `carry(F, x)` to a **fixpoint**:

> `carry(F, x)` ⟺ `x` has a bare decl `x ∈ T` in `F`
>   AND ( token `_x` appears in `F`'s code
>         OR `F` has a call `Sub(s ↦ x, …)` where `Sub` is a registered fsm
>            and `carry(Sub, s)` ).

The second disjunct is what makes a field a carry *by virtue of composition*:
binding a parent var to a sub's carry slot makes that parent var a carry too.
Because `carry(Sub, s)` may itself be composition-induced, the rule is
iterated to a fixpoint — so nested `fsm → fsm → fsm` composition propagates
carries transitively (Inner.iv → Mid.mv → Outer.ov).

## The two forms

### 1. Slot-bind — `Sub(x ↦ y, …)`

The `_x ↦ _y` injection rule:

> For each binding `s ↦ v` in a call `Sub(…)` where `Sub` is a registered
> fsm, `carry(Sub, s)`, `v` is a bare identifier, and no `_s` binding is
> already present: append `, _s ↦ _v` to the call.

The injected `_v` makes the parent reference `_v`; the parent has a bare decl
`v ∈ T`; so the autocarry pass synthesizes `_v ∈ base(T)` in the parent.
Result for the canonical example:

```
fsm Counter                         claim Counter
    n ∈ Int                             n ∈ Int
    n = (is_first_tick ? 0 : _n+1)      _n ∈ Int
fsm Main                ── expand ──►    n = (is_first_tick ? 0 : _n+1)
    count ∈ Int                     claim Main
    Counter(n ↦ count)                  count ∈ Int
                                        _count ∈ Int
                                        Counter(n ↦ count, _n ↦ _count)
```

The oracle inlines `count = (is_first_tick ? 0 : _count + 1)` over Main's own
`count`/`_count`; manifest `state-fields = count:Int`; the kernel carries it.
Run: `0, 1, 2`.

**Multi-call-site** falls out naturally: `Sub(x↦a)` and `Sub(x↦b)` inject
`_x↦_a` and `_x↦_b`, synthesizing independent `_a`/`_b` because each binds a
distinct parent var.

### 2. Lift — `..Sub` / bare `Sub`

No injection. The single-fsm autocarry already synthesizes Sub's `_x` inside
Sub; the oracle's names-match `..` lift copies **both** `x` and `_x` into the
parent as the parent's own fields, so `x` carries for free. (Verified
empirically: the parent need not even pre-declare `x` — the lift brings it.)
The transform deliberately leaves lift lines alone.

### Positional `(a, b) ∈ Sub`

Not yet handled (no required fixture exercises it). The first-line-param
mapping would need the transform to read Sub's header params and match them
positionally to a/b before deciding which carry siblings to inject. Slot-bind
is the recommended form for carrying sub-fsms until this lands.

## Regression safety

Injection fires only when the callee is a **registered fsm**. Every
composition call in `compiler2/driver.ev` targets a `claim` helper, and the
file's one `fsm` (`driver_main`) is never composed — so no injection occurs
and the expanded source is byte-for-byte unchanged. Verified: stage1
`.smt2` from `expand | oracle emit driver_main` is **byte-identical** to the
pre-change baseline (11417 lines, empty `diff`).

## Tests

`tests/fsm_compose/` + `run.sh` (pipeline: `flatten` → `oracle emit` →
`kernel` run, diffed against each fixture's `-- expect:` header):

| fixture | form | result |
| --- | --- | --- |
| `no_carry_slot.ev` | slot-bind, no carry | `b = 5`, exit 0 |
| `counter_slot.ev` | slot-bind + carry | `0,1,2` (the carry proof) |
| `counter_lift.ev` | `..Sub` lift | `0,1,2` |
| `multi_counter.ev` | two call sites | `0,1,2` / `0,2,4` independent |
| `nested.ev` | 3-level fsm→fsm→fsm | `0,1,2` end-to-end |

All 5 pass.
