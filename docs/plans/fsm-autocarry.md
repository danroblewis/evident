# fsm auto-carry — prev-tick `_<name>` synthesis

## Problem

`compiler2/driver.ev`'s `driver_main` was a single `claim` that hand-declared
**347 prev-tick carry pairs**: for a state field `x ∈ T` whose value must
survive into the next tick, the body referenced `_x`, and `_x` had to be
declared by hand as `_x ∈ T` right next to the field. That is ~347 lines of
pure boilerplate — roughly half the field-declaration noise in a 6277-line
file.

We want to write `fsm driver_main` with bare fields and have each needed
`_<name>` carry synthesized automatically.

## Feasibility — why this is a source transform, not a keyword feature

`fsm` is, in the frozen bootstrap **oracle**, just a synonym for `claim`
(CLAUDE.md schema-keywords table: "fsm — currently a synonym for claim").
The oracle is a frozen binary; we cannot teach it FSM semantics. The
self-hosted `compiler.smt2` is likewise a committed artifact. So the carry
expansion has to happen as a **preprocessor / source transform that runs
before any compiler sees the text**. Input uses `fsm` + bare fields; the
transform rewrites it to `claim` + explicit `_x` decls; the compiler
compiles the rewritten form.

## The expansion rule (validated, exact)

In an `fsm <Name>` claim:

> A field `x` declared with a **bare** membership `x ∈ T` gets a synthesized
> declaration `_x ∈ base(T)` **iff the token `_x` is referenced somewhere in
> the claim's code** (comments excluded).
>
> `base(T)` is the underlying sort of `T` — its first whitespace-delimited
> token. A refined `Int < 65534` therefore carries as plain `Int`.

`claim` (no `fsm`) is passed through untouched, so pure helpers
(`translate2_*.ev`, stdlib) are unaffected.

## Validation against the real driver

Measured against the 347 hand-written pairs in `driver_main`
(pre-conversion):

| check | result |
| --- | --- |
| carry decls referenced in code | **347 / 347** (0 declared-but-unreferenced) |
| over-generation (field `x`, `_x` in code, no manual carry) | **none** |
| every carry's base field has exactly one bare decl (clean anchor) | **yes** |
| carry type `== base(field type)` | **347 / 347** |
| `_x` referenced but with no carry / wrong place | only inside `--` comments and SMT string literals (`__len`, `__mem`, …); correctly ignored |

The rule reproduces the hand-written set **exactly**: no more, no fewer, same
types.

### Edge cases the rule handles

1. **Refined field types.** `lx_count ∈ Int < 65534`, `ci_cnt ∈ Int < 2048`,
   `st_cnt ∈ Int < 8192` carry as plain `Int` (the human wrote them that
   way). `base(T)` = first token strips the bound. Without this the carry
   would re-impose a bound on the *previous* value, which is wrong.

2. **Non-primitive carry types.** Not all carries are `Int/Bool/Real/String`.
   `driver_main` carries `Expr`, `EnumVariantList`, `TokenList`, `C2Items`,
   `C2H`, `C2Frames`, `C2Binds`, `ExprList`, `PrOps`. The rule is purely
   syntactic (`base(T)` = first token), so these Just Work; it does not
   assume a primitive sort.

3. **Stray `_x` tokens.** Single-underscore names appearing only in `--`
   comments (`_count`, `_saw_result`, …) and double-underscore SMT
   identifiers in string literals (`__len`, `__mem`, `__SeqOf_LibArg`, …)
   are not real references. Comment stripping + the "field `x` must have a
   bare decl" anchor both exclude them — confirmed by the zero
   over-generation result.

## Where the transform lives

`scripts/passes/expand-fsm-autocarry.sh` (awk; `# TODO: rewrite in Evident`). It
reads Evident source on stdin and writes the expanded source on stdout:
rewrites `fsm <Name>` → `claim <Name>` and inserts `_x ∈ base(T)` after each
anchoring bare field decl whose `_x` is referenced.

It is wired as the **final pass of `scripts/flatten-evident.sh`**, so every
call site that flattens (the whole self-hosted runtime path:
`flatten | kernel compiler.smt2`) gets the expansion for free.

The bootstrap-oracle build path does **not** go through flatten — it does
`oracle emit compiler2/driver.ev` directly — so the same expander is also
invoked in `.goalpost/bin/lib.sh:gp_build_stage1` (expand driver.ev to a
temp, then `oracle emit` it; the imports still resolve against the repo-root
cwd, unchanged). Awk/no-Python keeps the producing path Python-free per
CLAUDE.md.

## End-to-end proof

- `expand-fsm-autocarry.sh < driver.ev(fsm) | oracle emit` produces a
  stage1 `.smt2` that is **byte-identical** to the stage1 built from the
  original hand-written `claim` driver (11417 lines, `diff` empty).
- Conformance fixtures 026 / 006 / 002 / 045 / 094 / 052 run through the
  fsm-derived stage1 give **identical** exit/stdout to the baseline stage1
  (006 → `go` exit 0, 002 → `conformance` exit 0; 026/045/094/052 exit 1 —
  pre-existing compiler2 limitations, identical under both). **Zero
  regression.**

## Line-count impact

`compiler2/driver.ev`: 6277 → 5930 lines (**−347**, the deleted carry decls);
`claim driver_main` → `fsm driver_main`.

## Composition (follow-up)

The single-fsm rule above is now extended to carry-preserving fsm
**composition** — slot-bind (`Sub(x ↦ y)` injects `_x ↦ _y` and synthesizes
the parent's `_y`), lift (`..Sub` carries for free), multi-call-site, and
nested fsm→fsm→fsm via a fixpoint registry. The same expander does both;
composition only fires when the callee is a registered fsm, so the driver's
stage1 stays byte-identical. See `docs/plans/fsm-composition.md`.
