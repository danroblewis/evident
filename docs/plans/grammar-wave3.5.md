# compiler.ev grammar coverage — wave 3.5 (integrate enum/match/Seq into the monolithic driver)

Status: **landed.** Closes the two integration gaps wave 3
(docs/plans/grammar-wave3.md §"Integration status (honest)") explicitly
deferred: enum / match / Seq were proven by dedicated driver fixtures that
inline a *shape-specific* pipeline, but the canonical disk-reading
`compiler/compiler.ev` driver only dispatched scalar memberships (+ the
`matches` recognizer, which wave 3 folded into the shared MembershipStep).

After wave 3.5, a `compiler.smt2` built from `compiler.ev` via bootstrap
compiles real `.ev` files that use a top-level `enum`, a `match`
membership pin, or `Seq` memberships — through the **same unified FSM**, no
inline plumbing.

## What landed

### Item 1 — top-level head-token dispatch for `enum`

`compiler.ev` now reads `_fwd`'s head token after REVERSE and computes
`is_enum_program ∈ Bool = (prog_head matches KwEnum)` (mirrors bootstrap's
`program.rs`: each top-level item dispatched by head token). Phase 2
branches:

- `KwEnum` → the enum-decl parse machine (the `ephase 1..8` state machine
  from `test_compiler_driver_enum.ev`, embedded and gated on
  `is_enum_program`), rendering each variant via `translate.ev`'s
  `VariantText` and assembling the block with `EnumDeclSmtlib`.
- otherwise → the claim head (`<kw> Ident`) + membership walk (unchanged
  shape, gated on `¬is_enum_program`).

Both sub-machines share phases 0 (lex) / 1 (reverse). The phase transition
and `emit_now` branch on `is_enum_program`; the enum path emits a
`(declare-datatypes …)` block, the claim path the manifest + declares.
The two sub-machines' carries are mutually gated so neither perturbs the
other (the inactive one's state stays inert: `_plist`/`_rem` stay `TLNil`,
so `plist_nil` / `step_ok` resolve harmlessly).

### Item 2 — per-membership RHS dispatch for `match` and `Seq`

The claim-path membership walk now classifies each membership off `_rem` by
peeking tokens t3 / t4:

| shape                       | discriminator              | step claim            |
| --------------------------- | -------------------------- | --------------------- |
| `name ∈ Type = match …`     | t3=`Eq`, t4=`KwMatch`      | `MatchMembershipStep` |
| `name ∈ Seq(T) = …`         | t3=`LParen` (compound)     | `SeqMembershipStep`   |
| `name ∈ Int = #src`         | t3=`Eq`, t4=`Hash`         | `SeqMembershipStep`   |
| everything else (waves 1–2) | —                          | `MembershipStep`      |

All three step-claims are composed on `_rem` each tick and the driver
selects `decl`/`assert`/`field`/`rest`/`ok`/`pinned` by shape. For a scalar
membership the selection is byte-identical to the wave-1/2 path
(`MembershipStep`'s outputs), so existing behavior is preserved.

Crucially this also works **interleaved**: a single claim body mixing
scalar + match + Seq memberships dispatches each line correctly (verified
manually, see "Verification").

### New pass: `compiler/parse_body_match.ev` — `MatchMembershipStep`

A single-tick, bounded peel (15 tokens) of the dominant
`match last_results[0]`-shape membership (one constructor arm with
single-name binds + one wildcard arm):

```
n ∈ Int = match e        →  (declare-fun n () Int)
    Ok(v) ⇒ v               (assert (= n (ite ((_ is Ok) e) v 0)))
    _     ⇒ 0
```

It assembles the nested ITE **inline from the lexer tokens** (the proven
`MembershipStep` `matches`-recognizer shape), reproducing
`translate_match.ev`'s `MatchArmSmtlib` output contract exactly (wildcard =
innermost else; binds render as their NAME).

**Why not compose `translate_match.ev` directly:** `MatchArmSmtlib` /
`ExprAsText` `match` on a claim param of enum type whose value is a
constructed payload (`match arm` over `MArm(_, b)`). When that claim is
instantiated by composition, the bootstrap translator silently *drops* the
constraint:

```
error: dropped constraint (couldn't translate to Bool):
  Binary(Eq, Identifier("body"), Match(Identifier("arm"),
    [MatchArm { pattern: Ctor { name: "MArm", binds: [Wildcard, Bind("b")] }, … }]))
```

`MatchTranslateStep` (used by the wave-3 match fixture) dodges this by
matching on a *locally derived* value rather than the input param, which is
why the fixture compiles. `MatchMembershipStep` matches on the lexer TOKENS
(`mt_t11 matches IntLit(_)`) — the same gap-free shape `MembershipStep`
already uses — and emits the identical string. This is a single-tick
bounded path; the unbounded N-arm walk (`MatchTranslateStep`) is a later
wave.

### Bind-substitution still deferred

As in wave 3, `Ok(v) ⇒ v` renders the bind as its NAME `v`, not the payload
accessor `(Ok__f0 e)` (a bootstrap-only refinement). Out of scope here.

## Canonical fixtures (item 3)

Three new fixtures drive the **unmodified `compiler.ev` FSM** with a
constant input (instead of the ReadFile seed — the established
`test_compiler_driver_mvp.ev` pattern, see
`test_compiler_driver_readfile.ev`'s header), proving the *unified
dispatch*, not just the per-pass mechanics:

- `tests/kernel/test_compiler_driver_canonical_enum.ev`
- `tests/kernel/test_compiler_driver_canonical_match.ev`
- `tests/kernel/test_compiler_driver_canonical_seq.ev`

Each emits byte-identical SMT-LIB to its wave-3 dedicated sibling. The
wave-3 dedicated fixtures are kept (they prove the per-pass mechanics).

## Verification

- `./test.sh`: **all phases passed.**
- Kernel tests: **88 (was 85), 0 failed**, green under default /
  `EVIDENT_FUNCTIONIZE=0` / `EVIDENT_FUNCTIONIZE_JIT=1`.
- The 3 canonical fixtures emit + run byte-identical to the wave-3
  expectations.
- End-to-end against the real `compiler.ev` (write `/tmp/compiler-input.ev`
  → bootstrap emit `compiler.ev` → kernel run): enum / match / Seq each
  byte-identical, AND a **mixed** body (`x ∈ Int = 5` ; `n ∈ Int = match …`
  ; `xs ∈ Seq(Int) = ⟨1,2⟩`) dispatches all three shapes correctly in one
  claim.
- Smoke test (item 4): `scripts/flatten-evident.sh compiler/compiler.ev`
  (**3441 lines**, was 2281 at wave 3) → `bootstrap emit` → /tmp/orig.smt2
  (2563 lines), **exit 0**.

## No frozen files touched

No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python. Diff is
`compiler/compiler.ev` + new `compiler/parse_body_match.ev` + three new
`tests/kernel/test_compiler_driver_canonical_*.ev` + this doc.

## Out of scope (wave 4+)

- Unbounded N-arm `match` (the `MatchTranslateStep` work-stack), and
  payload-accessor bind substitution.
- `match scrut[i]` index scrutinees (the corpus `last_results[0]` shape:
  `[` / `]` aren't yet lexed).
- Multi-top-level-item files (an `enum` *and* a `claim` in one source) —
  the driver still handles exactly one top-level item.
- Quantifiers, generics, records, subclaims, `..` / positional composition,
  imports-as-runtime-resolved.
