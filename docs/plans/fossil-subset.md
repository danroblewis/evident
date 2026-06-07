# The fossil-compilable subset of Evident (empirical, 2026-06-07)

The committed `compiler.smt2` artifact ("the fossil", md5
`0e5a9f96b29196f4688efcc3cd1fc3df`, identical in this worktree and
main) was probed with 50 minimal fixtures, one suspect construct
each, in `tests/seam/probes/p*.ev`. This document is the resulting
subset map for writing compiler2 in fossil-compilable Evident.

## Method

Per fixture:

```
scripts/flatten-evident.sh tests/seam/probes/pNN_*.ev > flat.ev
printf '%s\nmain\n' flat.ev | kernel compiler.smt2 > out.smt2
```

(kernel = `kernel/target/release/kernel`; each compile ≈ 45–60 s.)
Judged on the emitted `out.smt2`: does it contain the form's
semantic content, and do a trailing `sentinel ∈ Int = 42` pin and
`effects = ⟨Exit(0)⟩` survive? The fossil's dominant failure mode
is the junk-drain: the offending line AND everything after it in
the claim body vanish from the emit, so the sentinel separates
"this line broke" from "this line is inert". Emits that looked
complete were also executed (`kernel out.smt2`) for exit-code
sanity.

Verdict categories:

- **COMPILES** — correct SMT shape emitted, body after it intact.
- **DROPS-SILENTLY** — line vanishes, remainder of body junk-drained,
  emit otherwise well-formed (worst class for missed constraints).
- **MISCOMPILES-LOUD** — malformed/ill-sorted SMT emitted
  (`(= flag )`, undeclared symbol); Z3 rejects at load, kernel
  exits 1 — at least it is loud.
- **MISCOMPILES-SILENT** — well-formed but WRONG constraint
  (e.g. `xs[0]` binding to the next token's variable). The worst
  class: loadable, solvable, wrong answers.
- **ERRORS(nyi)** — literal `<nyi>` marker in the emit, remainder
  drained; loud at Z3 load.

## Composition forms (the mission matrix)

| # | Form | Verdict | Evidence (fixture → emit) | Implication for compiler2 |
|---|------|---------|---------------------------|---------------------------|
| 0 | flat claim, no composition | COMPILES (with per-line shape limits, see next table) | p15: `(= x 7)`, sentinel, Exit 0 all present. p00's `y = x + 1` truncated — the control's failure was an expression-shape failure, not a claim-shape one | flat claims are the backbone |
| 1 | bare claim name (names-match) | **COMPILES**, including transitively | p01: `(= n 11)` inlined from `IsEleven`, sentinel + effects intact. p53: `main → SetsDerived → SetsBase` both bodies inlined (`(= b 5)`, `(= n 11)`) | **the ONLY working composition mechanism.** Decompose compiler2 into parameterless claims invoked by bare name; nesting is fine |
| 2 | `..ClaimName` passthrough | DROPS-SILENTLY | p02: emit stops at `(declare-fun n () Int)`; no `(= n 11)`, no sentinel, max-effects = 0 | never write `..Name`; use the bare name |
| 3 | `(a, b) ∈ ClaimName` positional | ERRORS(nyi) + miscompile | p03: `r` re-declared as `(Array Int p)` (!), then `<nyi>`, rest drained | never |
| 4 | `ClaimName(slot ↦ name)` (simple-name value) | DROPS-SILENTLY | p04: emit stops after `(declare-fun greeting () String)`; no `(= greeting "hi")` | never — even simple-name slot bindings are dropped, not just expression values. compiler.ev's own `TLHd(l ↦ x, out ↦ y)` style is NOT fossil-compilable |
| 5 | `recv.subclaim(args)` receiver dispatch | ERRORS(nyi) | p05: declares `x`, `result`, then `<nyi>`, rest drained | never |
| 6 | `cond ⇒ ClaimName` conditional inline | DROPS-SILENTLY | p29 (clean retry; p06 was confounded by its `= true` pin): emit stops after the two declares | never; conditionality only via `match` (below) |

## Statement / expression shapes (what a claim body line may be)

| Shape | Verdict | Evidence |
|-------|---------|----------|
| `x ∈ Int` / `∈ String` / `∈ Bool` / `∈ Nat` / `∈ Seq(T)` / `∈ UserEnum` decl | COMPILES | everywhere; p55 Nat adds `(>= n 0)` |
| `x ∈ Int = 7`, `s ∈ String = "ab"` membership pin | COMPILES | p00, p13 |
| `flag ∈ Bool = true` membership pin | MISCOMPILES-LOUD | p18: `(assert (= flag ))` (RHS empty); body after survives |
| `flag = true` bare assert | COMPILES | p19: `(= flag true)` |
| `x ∈ Int = -7` membership pin | MISCOMPILES-LOUD | p26: `(= x )` |
| `x = -7` bare assert | COMPILES | p37: `(= x (- 7))` |
| `y ∈ Int = a + 1` / `a - b` / `a * 2` membership pin (ONE bare binop, name/int-literal operands, NO parens) | COMPILES | p20 `(+ x 1)`, p63 `(- a b)`, p64 `(* a 2)` |
| `y = x + 1` bare assert (binop RHS) | MISCOMPILES-SILENT + drain | p00: emits `(= y x)` (!!), drops `+ 1` and the rest of the body |
| `y = (x + 1)` bare assert, parenthesized | MISCOMPILES-LOUD + drain | p33: `(= y )` |
| `y ∈ Int = ((x + 1) * 2)` nested arith | MISCOMPILES-LOUD + drain | p40: `(= y )` — parens break membership RHS too: exactly one binop, no grouping |
| `x ∈ Int = (0 - 7)` | MISCOMPILES-LOUD + drain | p38: `(= x )` |
| `y = x` / `t = "xy"` bare assert (atom RHS) | COMPILES | p23, p52 |
| standalone comparisons `x < 10`, `x > 0`, `x ≤ 10`, `x ≥ 0`, `x ≠ 3` | COMPILES | p21, p54 (`<= >= (not (=))`) |
| chained membership `x ∈ Int = 5`, `y ∈ Int < 10`, `0 < z ∈ Int < 10`, `a, b ∈ Int < 5` | COMPILES | p11: all four forms, sentinel + effects intact |
| ternary (bare or membership, parenthesized or not) | MISCOMPILES-LOUD + drain | p14, p31: `(= y )` plus a stray `(> x 3)` assert |
| Bool-expr RHS `flag ∈ Bool = (x > 3)`, `((a) ∧ (b))` | MISCOMPILES-LOUD + drain | p32, p41: `(= flag )` |
| `cond ⇒ (n = 11)`, `(¬flag) ⇒ …` | DROPS-SILENTLY / mangles | p22 drains; p30 re-declares `flag` as `(Array Int )` + `<nyi>` |
| `match` on enum var, arms `Variant(_) ⇒ literal-or-name`, default `_ ⇒` | **COMPILES** (clean `ite`) | p46: `(= n (ite ((_ is IntResult) e) 1 0))`; p49: `(ite … a b)` |
| `match` arm with payload var (`IntResult(v) ⇒ v`) | MISCOMPILES-LOUD | p34: `(ite ((_ is IntResult) e) v 0)` — `v` left unbound |
| `match` arm pattern payloadless (`EofResult ⇒ 1`) | MISCOMPILES-SILENT/LOUD | p66: `(ite ((_ is EofResult) e) 0 Int)` — arms misaligned, sort name leaks as constant |
| `match` arms = ctor applications or Seq literals | DROPS-SILENTLY / ERRORS(nyi) | p56 drains; p50 `<nyi>` |
| `e matches Variant(_)` (any position) | MISCOMPILES-LOUD | p08/p35: `(= ok )` + stray standalone recognizer assert |
| enum decl, payloaded variant FIRST (`AA(Int) \| BB`) | COMPILES | p16: `Pick` lands in the datatypes block, `(= e (AA 7))`. (stdlib's Effect/Result/LibArg are NOT evidence — they are a hardcoded preamble, identical in every emit) |
| enum decl, payloaded variant in any LATER position | MISCOMPILES catastrophically | p67 (inline), p69 (multi-line), p70 (cons-list `Nil2 \| Cons2(Int, LL2)`): enum parser shreds; the whole claim body is consumed into a garbage datatype named `Int`. Cons-list types — compiler.ev's own TokenList pattern — are NOT in the subset |
| ctor pin `e ∈ Result = IntResult(7)` / `StringResult("hi")` / `Exit(code)` | COMPILES | p07, p62, p24 (name payloads fine) |
| payload extraction `e = StringResult(s0)` (bare ctor equation) | DROPS-SILENTLY | p58 (isolated by p62) |
| accessor call `IntResult__f0(e)` | MISCOMPILES-SILENT | p57: emits `(= n 0)` — wrong and quiet |
| Seq literal `⟨1, 2⟩` / `⟨"a", "b"⟩` / `⟨⟩` / ctor elements / name elements | COMPILES | p09, p27, p65, p28, p59 |
| `xs = a ++ b` (Seq concat) | ERRORS(nyi) | p09: `<nyi>` after `xs` declares |
| `t ∈ String = s ++ u` (String concat, any operand kinds) | MISCOMPILES-LOUD | p42/p47: `(= t (+ s ))` — second operand dropped AND wrong operator |
| `n ∈ Int = #xs` (Seq) | COMPILES | p48: `(= n xs__len)` |
| `n ∈ Int = #s` (String) | MISCOMPILES-LOUD | p43: `(= n s__len)` — `s__len` undeclared for strings |
| `y ∈ Int = xs[0]` (Seq index) | **MISCOMPILES-SILENT** | p44: `(= y sentinel)` — binds the NEXT line's identifier. Never index |
| `r0 ∈ Result = last_results[0]` / `match last_results[0]` | DROPS whole body / garbage | p60: body empty; p51: `(ite ((_ is ) StringResult) _ sentinel)` |
| `∀ x ∈ xs : x > 0` | DROPS-SILENTLY | p10: seq pins emitted, no per-element constraint, rest drained |
| record `type IVec2(x, y ∈ Int)` + any use | MISCOMPILES-LOUD / ERRORS(nyi) | p12: `(declare-fun p () )`; p17: `<nyi>` | 
| two independent claims in one file | COMPILES | p15: target-claim selection clean, `Unused` body absent |
| effects `= ⟨Exit(0)⟩` / `⟨Exit(code)⟩` / `⟨LibCall("libc","puts",⟨ArgStr("hi")⟩), Exit(0)⟩` | COMPILES (and runs; prints) | smoke, p24, p28 (patched run printed `hi`, exit 0) |
| `cond ⇒ effects = ⟨…⟩` (guarded writer) | DROPS-SILENTLY | p25 |
| `effects ∈ Seq(Effect) = (cond ? ⟨…⟩ : ⟨⟩)` | ERRORS(nyi) | p36 |
| `effects ∈ Seq(Effect) = ⟨e0⟩` (name element) | COMPILES but **broken at runtime** | p59: emit fine, but manifest gains `effects:Seq(Effect)` as a state field → kernel "stuck", Exit never dispatched |
| `effects ∈ Seq(Effect)` then bare `effects = ⟨a⟩` | ERRORS(nyi) | p68: `<nyi>` after the declares (bare asserts cannot render Seq literals); manifest also polluted with `effects` |

## Runtime wall: `_name` carry companions are mandatory

Every top-level membership (including enum-typed ones) lands in the
manifest `state-fields`. When the emitted program is fully pinned
(the common case in this subset), the kernel's functionizer extracts
it and then requires a `_<name>` declaration per state field; the
fossil does not emit them, and the kernel dies on tick 1 with
`unknown constant _n` (exit 1). Observed on p01/p15/p16/p20/p23/p24/
p28/p37/p46/p49/p52/p53/p55/p62/p63; programs with an unpinned or
Seq-typed state field dodge it only because extraction bails
(p11/p21/p27/p48/p54/p65 ran exit 0 as-is).

The in-source fix is compiler.ev's own `_pmode` pattern and it
round-trips cleanly through the fossil: p39 declares `_x ∈ Int` and
`_sentinel ∈ Int`, compiles (companions excluded from state-fields),
and runs exit 0. Manually appending the same declares to
p01/p24/p28/p49 emits turned all of them into exit-0 runs (p28
printing `hi`). **Rule: for every top-level var `x ∈ T`, also
declare `_x ∈ T`.**

## What this means for compiler2

Style rules (each backed by a row above):

1. **Composition**: parameterless claims + bare-name invocation
   (names-match), nesting allowed. Nothing else. No `↦` calls, no
   `..`, no tuples, no receivers, no guarded inlines — all are
   silent or near-silent drops.
2. **One constraint per line**, RHS restricted to: atom (int/string
   literal, name; `true`/negatives only via bare assert), exactly
   one unparenthesized binop (`+ - *` over names/int literals) in a
   membership pin, `#seq`, a Seq literal, a ctor application with
   atom args, or a `match`.
3. **Branching = `match` only**, scrutinee a plain enum-typed name,
   every non-default arm pattern `Variant(_)` (parenthesized;
   payloadless variants cannot be arm patterns — give FSM-phase
   variants a dummy `Int` payload), arms restricted to literals or
   names. Build complex conditionals by cascading matches through
   intermediate enum/name variables.
4. **No payload reads.** Match payload vars, `Variant__f0(e)`
   accessors, ctor-equation extraction, Seq indexing and
   `last_results` reads are all broken (several silently). Data can
   flow INTO enums/ctors, never back out.
5. **Enums**: at most one payload-carrying variant, and it must be
   the FIRST variant; any later payloaded variant (inline or
   multi-line form) shreds the parser and eats the claim body
   (p67/p69). This rules out cons-list types in the subset.
6. **Strings**: literals, equality pins and Seq(String) literals
   only. No `++`, no `#s`. compiler2 cannot build strings at
   runtime under the fossil.
7. **Effects**: a single unconditional ctor-literal Seq
   (`⟨LibCall(…), …, Exit(n)⟩`) is the only shape that both compiles
   and runs.
8. Declare a `_x` companion for every top-level variable.

### The honest bottom line

Within these rules a program can: declare and pin typed state,
inline shared constraint blocks, do single-binop arithmetic,
compare, select between precomputed atoms with match-ite, build
enum/Seq data, and emit one fixed effects sequence (print + exit
works end-to-end, p28). That is enough for one-shot,
statically-determined-output programs.

It is NOT currently enough for compiler2 itself: there is no
working conditional effects writer (p25/p36/p50/p59 — the p59 shape
compiles but the manifest miscategorizes `effects` and the kernel
sticks), no `last_results` read (p51/p60), no string concatenation
(p42/p47), no payload extraction (p34/p57/p58), and no recursive
data types (p70 — cons lists shred the enum parser). A compiler must
read source (ReadFile → last_results), branch per tick, build output
text, and exit conditionally — all four sit behind these gaps. The
narrowest fossil capabilities to fix first, in dependency order:
(a) an effects writer that stays out of state-fields with non-ctor
elements (p59/p68), (b) `match last_results[0]` or any equivalent
read (p51/p60), (c) String `++` (p42), (d) match payload binding
(p34). Until at least (a)+(b)+(c) exist, compiler2-in-the-subset is
not writable, and fixing them means regenerating the fossil — the
same wave-5 dependency documented in
`docs/plans/expr-slot-binding-port-notes.md` §4–5.

## Probe inventory

71 fixtures in `tests/seam/probes/` (p00–p70, no gaps; numbers
p18+ were added in later rounds as earlier results forced
disambiguation, e.g. p06 → p29, p67 → p69 → p70). Each file's header
comment states its suspect and expected emit. Compile artifacts
were not committed; rerun via the command at the top (~45 s each,
parallelizes cleanly — 18 concurrent compiles ran fine in 156 GB /
24 cores).
