# Z3 capabilities reference

Z3 **4.15.4** (the build on PATH + Python bindings). Theories, sorts, and
operations, verified against the installed solver. Columns: **SMT-LIB** name ·
**Python** API (`import z3`) · meaning. Where the two differ it's called out;
where an op is *exposed but unsupported by this build*, it's flagged ⛔.

> **The one decidability rule that governs everything:** a theory is fast and
> complete when its terms are **bounded / quantifier-free / linear**; it becomes
> semi-decidable (or undecidable) when you add unbounded quantifiers, nonlinear
> arithmetic, or unbounded sequence/string lengths. "Bound it and it's fast"
> holds across all theories below, not just sets.

---

## Theories at a glance

| Theory | Sorts | Decidable (QF)? | Notes |
|---|---|---|---|
| Core (Booleans) | `Bool` | yes | the propositional skeleton; `ite`, `=`, `distinct` |
| Uninterpreted functions (EUF) | user sorts | yes | `(declare-sort)`, `(declare-fun)`; only `=`/`distinct` |
| Linear integer arithmetic (LIA) | `Int` | yes | `+ - * (by const) div mod`, `< ≤` … |
| Nonlinear integer arithmetic (NIA) | `Int` | **no** (undecidable) | `*` of variables; Z3 tries, may not terminate |
| Linear real arithmetic (LRA) | `Real` | yes | exact rationals |
| Nonlinear real arithmetic (NRA) | `Real` | yes (decidable, costly) | polynomial; CAD |
| Bit-vectors (BV) | `BitVec(n)` | yes | fixed-width machine ints, all bit ops |
| Arrays (AX) | `Array(I,V)` | yes (QF) | `select`/`store`, extensional, `map`, const |
| **Sets** | `Set(T)`=`Array(T,Bool)` | yes (QF) | ∪ ∩ ∖ ∁ ∈ ⊆; **cardinality unsupported ⛔** |
| Sequences (Seq) | `Seq(T)` | semi-decidable | decidable when length-bounded |
| Strings | `String`=`Seq(Char)` | semi-decidable | decidable when length-bounded |
| Regular expressions | `RegLan` (`ReSort`) | yes | `str.in_re`, closure ops |
| Characters | `Char` | yes | building block of strings |
| Floating-point (FP) | `Float(eb,sb)`,`RoundingMode` | yes (QF) | IEEE-754, NaN/inf, rounding |
| Algebraic datatypes | enums/tuples/records/recursive | yes (QF) | constructors, selectors, recognizers |
| Quantifiers | — | semi-decidable | `forall`/`exists`, MBQI; incomplete in general |
| Lambdas / rec functions | — | — | `Lambda`, `define-fun-rec` |
| Special relations | binary relations | yes | transitive closure, partial/linear/tree orders |
| Pseudo-Boolean / cardinality | `Bool`s | yes | `AtMost`/`AtLeast`/`Pb*` over booleans |

Engines: **`Solver`** (SMT), **`Optimize`** (νZ — min/max + soft constraints),
**`Fixedpoint`** (μZ — Datalog/Horn/Spacer, least-fixpoint recursion),
`Tactic`/`Goal` (programmable preprocessing & quantifier elimination).

---

## Sorts (master list)

| Sort | SMT-LIB | Python | notes |
|---|---|---|---|
| Boolean | `Bool` | `BoolSort()` | |
| Integer | `Int` | `IntSort()` | unbounded mathematical ℤ |
| Real | `Real` | `RealSort()` | exact rationals ℚ (not floats) |
| Bit-vector | `(_ BitVec n)` | `BitVecSort(n)` | fixed width n |
| Array | `(Array I V)` | `ArraySort(I,V)` | total function I→V |
| Set | `(Set T)` | `SetSort(T)` | sugar for `Array(T,Bool)` |
| Sequence | `(Seq T)` | `SeqSort(T)` | finite sequences |
| String | `String` | `StringSort()` | = `(Seq (_ Char))` |
| Regex | `RegLan` | `ReSort(SeqSort)` | regular languages |
| Char | `(_ Char)` | (via strings) | unicode code point |
| Float | `(_ FloatingPoint eb sb)` | `FPSort(eb,sb)`, `Float16/32/64/128()` | IEEE-754 |
| RoundingMode | `RoundingMode` | `RoundNearestTiesToEven()` … | FP rounding |
| Enum / Datatype | `(declare-datatype …)` | `EnumSort`, `Datatype`, `TupleSort` | algebraic |
| Uninterpreted | `(declare-sort S)` | `DeclareSort("S")` | opaque, EUF only |
| Finite domain | — | `FiniteDomainSort(n)` | bounded-size sort |

---

## Core (Booleans)

| op | SMT-LIB | Python | meaning |
|---|---|---|---|
| and / or / not | `and or not` | `And, Or, Not` | |
| implies / iff | `=> =` | `Implies`, `==` | |
| xor | `xor` | `Xor` | |
| if-then-else | `ite` | `If(c,a,b)` | typed conditional |
| equality | `=` | `==` | any sort |
| distinct | `distinct` | `Distinct(a,b,…)` | pairwise ≠ |
| true / false | `true false` | `BoolVal(True/False)` | |

## Uninterpreted functions (EUF)
`DeclareSort("S")` makes an opaque sort; `Function("f", S, …, T)` an opaque
function. Only `=`/`distinct` reason about them (congruence). Decidable, cheap,
and the basis for modeling "things we don't want to give arithmetic meaning."

## Integer & Real arithmetic

| op | SMT-LIB | Python | notes |
|---|---|---|---|
| + − × | `+ - *` | `+ - *` | `*` of two variables ⇒ nonlinear |
| int division / mod / rem | `div mod rem` | `a/b`, `a%b`, `Rem` | Euclidean; `div` rounds toward −∞ |
| real division | `/` | `a/b` (Real) | exact rational |
| comparisons | `< <= > >=` | `< <= > >=` | |
| negation / abs | `-` / `abs` | `-x`, `Abs(x)` | |
| power / sqrt | `^` | `x**k`, `Sqrt(x)` | nonlinear |
| Int↔Real | `to_real to_int is_int` | `ToReal, ToInt, IsInt` | |
| divisibility | `(_ divisible k)` | `x % k == 0` | |

## Sets — the headline (set = `Array(T, Bool)`)

| op | Python | meaning |
|---|---|---|
| sort | `SetSort(T)` | the powerset of T |
| empty / universe | `EmptySet(T)` / `FullSet(T)` | ∅ / U |
| add / remove element | `SetAdd(s,e)` / `SetDel(s,e)` | s∪{e} / s∖{e} |
| union | `SetUnion(a,b)` (= `Union`) | a ∪ b |
| intersection | `SetIntersect(a,b)` (= `Intersect`) | a ∩ b |
| difference | `SetDifference(a,b)` | a ∖ b |
| complement | `SetComplement(s)` (= `Complement`) | U ∖ s |
| membership | `IsMember(e,s)` | e ∈ s |
| subset | `IsSubset(a,b)` | a ⊆ b |
| equality | `a == b` | extensional set equality |
| **cardinality** | `SetHasSize(s,n)` | **⛔ unsupported in 4.15.4** ("set-has-size is not supported") |

**Cardinality / measure is the gap.** `SetHasSize` is exposed but the solver
rejects it. To count, you must work in a **finite/bounded** domain and either
(a) pseudo-Boolean over the characteristic bits (`AtMost`/`AtLeast`, below), or
(b) sum an `Int` indicator `If(IsMember(e,s),1,0)` over an enumerated domain.
General cardinality over infinite domains, and "what fraction of the space," are
**not** native (that's `#SAT` / model counting — external tools).

Everything else — segment, intersect, difference, complement, membership,
subset, equality — is native and decidable **quantifier-free**. A *relation* is
a set over a tuple sort (`SetSort(TupleSort(...))`) or a `Function(... Bool)`.

## Arrays (the substrate under Sets)

| op | SMT-LIB | Python | meaning |
|---|---|---|---|
| read | `select` | `Select(a,i)` / `a[i]` | a(i) |
| write | `store` | `Store(a,i,v)` | a with i↦v |
| constant array | `((as const …) v)` | `K(IndexSort, v)` | everywhere v |
| map a function | `(_ map f)` | `Map(f, a, …)` | pointwise |
| extensionality / default | — | `Ext(a,b)`, `Default(a)` | |

## Bit-vectors (`BitVecSort(n)`)
Arithmetic (`+ - * `, `UDiv/SDiv`, `URem/SRem/SMod`), bitwise (`& | ^ ~`),
shifts (`<<`, `LShR` logical, `>>` arithmetic, `RotateLeft/Right`), compares
(unsigned `ULT ULE UGT UGE`, signed `< <= > >=`), structural (`Concat`,
`Extract(hi,lo,x)`, `SignExt`, `ZeroExt`, `Repeat`), reductions (`BVRedAnd/Or`),
overflow predicates (`BVAddNoOverflow`, `BVMulNoOverflow`, …), `BV2Int`/`Int2BV`.
Fully decidable, bit-blasted — fast for modest widths.

## Sequences (`SeqSort(T)`)

| op | Python | meaning |
|---|---|---|
| empty / unit | `Empty(seq)` / `Unit(e)` | ⟨⟩ / ⟨e⟩ |
| concat | `Concat(a,b,…)` | a ++ b |
| length | `Length(s)` | #s |
| index | `s[i]` / `SubSeq(s,o,l)` / `Extract(s,o,l)` | element / slice |
| contains / prefix / suffix | `Contains(s,t)`, `PrefixOf(p,s)`, `SuffixOf(q,s)` | |
| indexof / replace | `IndexOf(s,t,o)`, `Replace(s,a,b)` | |

Decidable when lengths are **bounded**; semi-decidable otherwise (the same
trap as Evident's Seq theory).

## Strings (`StringSort` = `Seq(Char)`) and Regex (`ReSort`)
Strings get all Seq ops plus `StrToInt`/`IntToStr`, `SubString`, `string_at`,
`StringVal("…")`. Regex sort `Re`: build with `Star, Plus, Option, Range,
Loop, Repeat, Union, Intersect, Concat`; match with `InRe(s, re)`; lift a
string to a singleton language with `Re(StringVal("…"))`. Regex membership is
decidable.

## Floating-point (`FPSort(eb,sb)` / `Float16/32/64/128`)
IEEE-754: arithmetic with explicit rounding (`fpAdd(rm,a,b)`, `fpMul`, `fpDiv`,
`fpFMA`, `fpSqrt`, `fpRem`), comparisons (`fpLT fpLEQ fpGT fpGEQ fpEQ fpNEQ`,
`fpMin/Max`), predicates (`fpIsNaN fpIsInf fpIsZero fpIsNormal fpIsSubnormal
fpIsNegative/Positive`), specials (`fpNaN`, `fpPlusInfinity`, `fpMinusZero`),
conversions (`fpToFP`, `fpToReal`, `fpToSBV/UBV`, `fpToIEEEBV`). Rounding modes
are a sort: `RoundNearestTiesToEven/Away`, `RoundToward{Positive,Negative,Zero}`.

## Algebraic datatypes
- **Enums:** `EnumSort("Color", ["R","G","B"])` → sort + constants.
- **Tuples / records:** `TupleSort("P", [IntSort(), StringSort()])` → constructor,
  accessors.
- **Recursive / sum types:** `Datatype("List")` with `declare("cons", ("hd",Int),
  ("tl","List"))` + `declare("nil")`, then `.create()`. Gives constructors,
  **selectors** (field access), and **recognizers** (`is_cons(x)`). Decidable QF.

## Quantifiers, lambdas, recursion
`ForAll([x], P)`, `Exists([x], P)` — semi-decidable, solved by E-matching / MBQI;
complete only on restricted fragments. `Lambda([x], body)` builds an array/function
value (first-class). `RecFunction` + `RecAddDefinition` define recursive functions
(`define-fun-rec`). Quantifier **elimination** is available as a tactic
(`Tactic('qe')`) for some theories — this is how you'd compute a function *image*
eagerly (vs. keeping it lazy).

## Special relations (relation *rules*, native)
Over a binary relation `R: T×T→Bool`:
`TransitiveClosure(R)`, `PartialOrder(R, id)`, `LinearOrder(R, id)`,
`TreeOrder(R, id)`, `PiecewiseLinearOrder`. Verified: transitive closure works
(`1→2, 2→3 ⊢ 1→3` without asserting it). This is the built-in route to
reachability / ordering constraints on relations.

## Pseudo-Boolean / cardinality (over Booleans)
`AtMost(b1,…,bn, k)`, `AtLeast(…, k)`, `PbLe([(b,w),…], k)`, `PbGe`, `PbEq` —
weighted/unweighted counting **over Boolean variables**. This is the practical
way to do "how many of these hold" and to encode finite-set cardinality.

## Engines
- **`Solver()`** — SMT decision/witness; `check()` → `sat`/`unsat`/`unknown`,
  `model()` gives one witness. `push()/pop()` for incremental.
- **`Optimize()`** — `maximize/minimize`, soft constraints (`add_soft`). Verified:
  picks the *boundary* of a solution space (max x in 0<x<10 → 9). This is how you
  choose a *specific* arbitrary point instead of any point.
- **`Fixedpoint()`** — Datalog/Horn (`register_relation`, `rule`, `query`),
  Spacer (PDR/IC3). Least-fixpoint recursive relations / reachability. Separate
  solving mode from the main SMT core.
- **`Tactic`/`Then`/`Goal`** — programmable preprocessing, quantifier elimination,
  simplification; `SolverFor(logic)` picks a tuned solver per SMT-LIB logic.

---

## What's *not* here (gaps to design around)
- **Set / general cardinality and measure** — `SetHasSize` unsupported; no
  "fraction of the space." Counting is `#SAT` (external: ApproxMC/UniGen).
- **Uniform / random sampling** — `check()` gives *an* arbitrary witness, not a
  fair one; fairness needs external samplers or hand-rolled blocking loops.
- **Unbounded everything** — Seq/String/quantifier/nonlinear-int are
  semi-decidable; keep domains **bounded** to stay in the fast, complete region.

## Reproduce / extend
`python3 prototype/00_smoke.py` exercises the core. To regenerate the exact API
name lists, introspect `dir(z3)` grouped by prefix (as this file was built).
