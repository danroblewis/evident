# Syntax: Mathematical / Type-Theoretic

## Philosophy

This syntax treats Evident programs as collections of inference rules in the style of natural deduction and Martin-Löf type theory. Every claim is a proposition; establishing a claim produces an evidence term — a first-class value encoding the derivation. The horizontal-bar rule form makes the logical structure explicit: premises above the line, conclusion below, rule name at the right. Unicode notation (⊢, ∧, ∀, →) is used throughout, with ASCII fallbacks provided. The intended audience is someone fluent in proof assistants (Coq, Agda, Lean). Readability for casual programmers is deliberately sacrificed in exchange for precision and a direct correspondence to proof-theoretic semantics.

---

## 1. Factorial

```
(* Type declaration *)
Factorial : ℕ → ℕ → Prop

(* Evidence type — the derivation witnesses *)
data FactEvidence : ℕ → ℕ → Type where
  | FactZero : FactEvidence 0 1
  | FactSucc : ∀ n f f', FactEvidence n f
                        → Mult (n + 1) f f'
                        → FactEvidence (n + 1) f'


──────────────────────── Fact-Zero
  ⊢ Factorial 0 1 [FactZero]


  ⊢ Factorial n f [ev]     ⊢ Mult (n+1) f f' [ev']
  ──────────────────────────────────────────────────── Fact-Succ
        ⊢ Factorial (n+1) f' [FactSucc n f f' ev ev']


(* Evidence annotation syntax: the term in [·] is the proof witness *)
(* Querying: *)
? ⊢ Factorial 4 f [w]
(* Runtime finds: f = 24, w = FactSucc 3 6 24 (FactSucc 2 2 6 (FactSucc 1 1 2 (FactSucc 0 1 1 FactZero ev₃) ev₂) ev₁) ev₀ *)
```

**ASCII fallback:**
```
-- Type declaration
Factorial : N -> N -> Prop

--                              Fact-Zero
-- --------------------------------
--   |- Factorial 0 1 [FactZero]

--   |- Factorial n f [ev]     |- Mult (n+1) f f' [ev']
--   ----------------------------------------------------- Fact-Succ
--         |- Factorial (n+1) f' [FactSucc n f f' ev ev']
```

**Assessment:** The inference rule form maps cleanly onto the recursive structure of factorial — base case and inductive step correspond directly to rules with zero and two premises respectively. The evidence term annotation `[·]` makes the proof object explicit at the point of conclusion, which is powerful for inspection. What feels awkward is that `Mult` must itself be defined as a separate judgment, so a complete program requires a pile of auxiliary rules that a friendlier syntax would hide.

---

## 2. Sorted List

```
(* Type declarations *)
List : Type → Type
Sorted : List ℕ → Prop

(* Evidence type *)
data SortedEvidence : List ℕ → Type where
  | SortedNil  : SortedEvidence []
  | SortedOne  : ∀ x. SortedEvidence [x]
  | SortedCons : ∀ x y ys.  x ≤ y
                           → SortedEvidence (y :: ys)
                           → SortedEvidence (x :: y :: ys)


───────────────────────── Sorted-Nil
  ⊢ Sorted [] [SortedNil]


───────────────────────────── Sorted-One
  ⊢ Sorted [x] [SortedOne x]


  ⊢ x ≤ y [le-ev]     ⊢ Sorted (y :: ys) [ev]
  ──────────────────────────────────────────────── Sorted-Cons
     ⊢ Sorted (x :: y :: ys) [SortedCons x y ys le-ev ev]


(* Deriving a specific instance: *)
(* ⊢ 1 ≤ 2 [Le-12]   ⊢ 2 ≤ 5 [Le-25]   ⊢ Sorted [5] [SortedOne 5]     *)
(*   ────────────────────────────────────────────────────────────────── Sorted-Cons *)
(*         ⊢ Sorted [2,5] [SortedCons 2 5 [5] Le-25 (SortedOne 5)]                 *)
(*   ───────────────────────────────────────────────────────────────── Sorted-Cons  *)
(*     ⊢ Sorted [1,2,5] [SortedCons 1 2 [5] Le-12 (SortedCons ...)]                *)

? ⊢ Sorted [1, 2, 5] [w]
(* Succeeds; w encodes the full derivation tree above *)

? ⊢ Sorted [3, 1, 2] [w]
(* Fails: no derivation exists *)
```

**ASCII fallback:**
```
-- Sorted : List N -> Prop

-- ------------------------- Sorted-Nil
--   |- Sorted [] [SortedNil]

-- ----------------------------- Sorted-One
--   |- Sorted [x] [SortedOne x]

--   |- x <= y [le-ev]    |- Sorted (y :: ys) [ev]
--   ------------------------------------------------ Sorted-Cons
--     |- Sorted (x :: y :: ys) [SortedCons x y ys le-ev ev]
```

**Assessment:** Inference rules elegantly capture the inductive structure of `Sorted`. The evidence term `SortedCons x y ys le-ev ev` records precisely which pair was compared and the sub-derivations for the ordering and tail, making it easy to extract a machine-checkable certificate. The notation gets crowded when showing a full nested derivation tree in comments — horizontal space is a real constraint of the bar-form style.

---

## 3. HTTP Request Validation

```
(* Type declarations *)
Request  : Type    -- a raw HTTP request record
Method   : Type    -- GET | POST | PUT | DELETE | ...
Path     : Type    -- string
Headers  : Type    -- map String String
Body     : Type    -- bytes option

ValidRequest  : Request → Prop
ValidMethod   : Method → Prop
ValidPath     : Path → Prop
AuthPresent   : Headers → Prop
ContentType   : Headers → Prop
BodyWellFormed : Body → Prop

(* Evidence record type — what a "valid request" proof looks like *)
data ValidRequestEvidence : Request → Type where
  | MkValidRequest :
      ∀ (r : Request).
        ValidMethod   r.method  [mev]
      → ValidPath     r.path    [pev]
      → AuthPresent   r.headers [aev]
      → ContentType   r.headers [cev]
      → BodyWellFormed r.body   [bev]
      → ValidRequestEvidence r


(* Axioms — self-evident base claims *)

──────────────────────── Valid-GET
  ⊢ ValidMethod GET [VM-GET]

──────────────────────── Valid-POST
  ⊢ ValidMethod POST [VM-POST]

──────────────────────── Valid-PUT
  ⊢ ValidMethod PUT [VM-PUT]

──────────────────────── Valid-DELETE
  ⊢ ValidMethod DELETE [VM-DELETE]

(* Path validation — must begin with "/" *)
  ⊢ HasPrefix "/" p [ev]
  ────────────────────── Valid-Path
  ⊢ ValidPath p [VP ev]

(* Auth header present *)
  ⊢ "Authorization" ∈ dom(h) [ev]
  ─────────────────────────────── Auth-Present
  ⊢ AuthPresent h [AP ev]

(* Content-Type must be "application/json" for POST/PUT *)
  ⊢ h["Content-Type"] = "application/json" [ev]
  ──────────────────────────────────────────────── Content-Type-JSON
  ⊢ ContentType h [CT ev]

(* Body must parse as valid JSON *)
  ⊢ ParsesAsJSON b [ev]
  ──────────────────── Body-JSON
  ⊢ BodyWellFormed b [BJ ev]

(* The top-level validation rule *)
  ⊢ ValidMethod   r.method  [mev]
  ⊢ ValidPath     r.path    [pev]
  ⊢ AuthPresent   r.headers [aev]
  ⊢ ContentType   r.headers [cev]
  ⊢ BodyWellFormed r.body   [bev]
  ────────────────────────────────────────────────────────────────── Valid-Request
  ⊢ ValidRequest r [MkValidRequest r mev pev aev cev bev]


(* Constructing and inspecting a full evidence term *)

(* Given a concrete request: *)
let r₀ : Request := {
  method  = POST,
  path    = "/api/users",
  headers = { "Authorization": "Bearer tok", "Content-Type": "application/json" },
  body    = b"{ \"name\": \"alice\" }"
}

? ⊢ ValidRequest r₀ [w]

(* Runtime produces: *)
(*   w = MkValidRequest r₀                           *)
(*         VM-POST                                   *)
(*         (VP (HasPrefix-pf "/" "/api/users"))       *)
(*         (AP (InDom-pf "Authorization" r₀.headers)) *)
(*         (CT (Eq-pf r₀.headers["Content-Type"]     *)
(*                    "application/json"))            *)
(*         (BJ (JSON-pf r₀.body))                    *)

(* Inspecting sub-evidence: *)
let auth-proof : AuthPresent r₀.headers [aev] := w.auth-ev
(* auth-proof = AP (InDom-pf "Authorization" r₀.headers) *)
```

**ASCII fallback:**
```
-- ValidRequest : Request -> Prop
-- data ValidRequestEvidence r where
--   MkValidRequest : ValidMethod r.method [mev]
--                  -> ValidPath r.path [pev]
--                  -> AuthPresent r.headers [aev]
--                  -> ContentType r.headers [cev]
--                  -> BodyWellFormed r.body [bev]
--                  -> ValidRequestEvidence r

--   |- ValidMethod r.method [mev]  |- ValidPath r.path [pev]
--   |- AuthPresent r.headers [aev] |- ContentType r.headers [cev]
--   |- BodyWellFormed r.body [bev]
--   ---------------------------------------------------------------- Valid-Request
--   |- ValidRequest r [MkValidRequest r mev pev aev cev bev]
```

**Assessment:** The evidence record type `MkValidRequest` is a powerful feature here — it gives the full validation result a precise type, and field projections (`w.auth-ev`) let callers extract specific sub-proofs without re-running validation. The notation scales reasonably to five simultaneous premises. The most awkward part is that "real" auxiliary predicates like `ParsesAsJSON` and `HasPrefix` need their own axiom/rule stacks that would balloon the file in a complete implementation.

---

## 4. Graph Reachability

```
(* Type declarations *)
Node  : Type
Edge  : Node → Node → Prop
Reach : Node → Node → Prop

(* Axioms: specific edges in the graph *)

──────────────────────── Edge-AB
  ⊢ Edge A B [EAB]

──────────────────────── Edge-BC
  ⊢ Edge B C [EBC]

──────────────────────── Edge-CD
  ⊢ Edge C D [ECD]

──────────────────────── Edge-AC
  ⊢ Edge A C [EAC]

(* Reachability rules *)

  ⊢ Edge u v [e]
  ──────────────────────────── Reach-Step
  ⊢ Reach u v [Step u v e]


  ⊢ Reach u w [ev₁]    ⊢ Reach w v [ev₂]
  ──────────────────────────────────────── Reach-Trans
  ⊢ Reach u v [Trans u w v ev₁ ev₂]


(* Querying: *)
? ⊢ Reach A D [w]

(* One derivation: *)
(*   ⊢ Edge A B [EAB]          ⊢ Edge B C [EBC]          ⊢ Edge C D [ECD]      *)
(*   ─────────────────         ─────────────────         ─────────────────      *)
(*   ⊢ Reach A B [Step A B EAB]  ⊢ Reach B C [Step B C EBC]  ⊢ Reach C D [...]  *)
(*   ──────────────────────────────────────────          ─────────────────      *)
(*              ⊢ Reach A C [Trans A B C ...]            ⊢ Reach C D [...]      *)
(*              ──────────────────────────────────────────────────────          *)
(*                           ⊢ Reach A D [Trans A C D (Trans A B C              *)
(*                                           (Step A B EAB) (Step B C EBC))     *)
(*                                        (Step C D ECD)]                       *)


(* Alternative shorter derivation via direct edge: *)
(*   ⊢ Edge A C [EAC]          ⊢ Edge C D [ECD]      *)
(*   ─────────────────         ─────────────────      *)
(*   ⊢ Reach A C [Step A C EAC]  ⊢ Reach C D [Step C D ECD] *)
(*   ────────────────────────────────────────────────        *)
(*        ⊢ Reach A D [Trans A C D (Step A C EAC) (Step C D ECD)]               *)

(* Both are valid evidence terms; the runtime may return either *)

(* All-reachability: collect all evidence *)
? ∀ v. ⊢ Reach A v [w(v)]
(* Yields a function from nodes to reachability proofs *)
```

**ASCII fallback:**
```
-- Node : Type
-- Edge : Node -> Node -> Prop
-- Reach : Node -> Node -> Prop

-- axiom  |- Edge A B [EAB]
-- axiom  |- Edge B C [EBC]
-- axiom  |- Edge C D [ECD]
-- axiom  |- Edge A C [EAC]

--   |- Edge u v [e]
--   ------------------------- Reach-Step
--   |- Reach u v [Step u v e]

--   |- Reach u w [ev1]   |- Reach w v [ev2]
--   ----------------------------------------- Reach-Trans
--   |- Reach u v [Trans u w v ev1 ev2]
```

**Assessment:** Graph reachability is a natural fit for inference rules — the `Reach-Step` and `Reach-Trans` rules read almost identically to their textbook definitions. The evidence terms `Step` and `Trans` form a proof tree that is itself a path through the graph, so the evidence term *is* the path, which is elegant. The ambiguity of multiple valid derivations is handled gracefully by the semantics (clause ordering is irrelevant), though it raises the practical question of which evidence term gets returned when multiple proofs exist.

---

## 5. FizzBuzz

```
(* Type declarations *)
FizzBuzz   : ℕ → String → Prop
DivBy3     : ℕ → Prop
DivBy5     : ℕ → Prop
DivBy15    : ℕ → Prop
NotDivBy3  : ℕ → Prop
NotDivBy5  : ℕ → Prop

(* Divisibility — via modular arithmetic judgments *)
  ⊢ n mod 3 = 0 [ev]
  ───────────────────── Div3
  ⊢ DivBy3 n [D3 ev]

  ⊢ n mod 5 = 0 [ev]
  ───────────────────── Div5
  ⊢ DivBy5 n [D5 ev]

  ⊢ DivBy3 n [ev₃]    ⊢ DivBy5 n [ev₅]
  ──────────────────────────────────────── Div15
  ⊢ DivBy15 n [D15 ev₃ ev₅]

  ⊢ n mod 3 ≠ 0 [ev]
  ────────────────────── Not-Div3
  ⊢ NotDivBy3 n [ND3 ev]

  ⊢ n mod 5 ≠ 0 [ev]
  ────────────────────── Not-Div5
  ⊢ NotDivBy5 n [ND5 ev]

(* FizzBuzz cases — four mutually exclusive rules *)

  ⊢ DivBy15 n [ev]
  ─────────────────────────────────────── FB-FizzBuzz
  ⊢ FizzBuzz n "FizzBuzz" [FB-FZ n ev]


  ⊢ DivBy3 n [ev₃]    ⊢ NotDivBy5 n [ev₅]
  ──────────────────────────────────────────── FB-Fizz
  ⊢ FizzBuzz n "Fizz" [FB-F n ev₃ ev₅]


  ⊢ DivBy5 n [ev₅]    ⊢ NotDivBy3 n [ev₃]
  ──────────────────────────────────────────── FB-Buzz
  ⊢ FizzBuzz n "Buzz" [FB-B n ev₅ ev₃]


  ⊢ NotDivBy3 n [ev₃]    ⊢ NotDivBy5 n [ev₅]
  ───────────────────────────────────────────────── FB-Num
  ⊢ FizzBuzz n (show n) [FB-N n ev₃ ev₅]


(* Deriving FizzBuzz 15: *)
(*   ⊢ 15 mod 3 = 0 [M3]   ⊢ 15 mod 5 = 0 [M5]                         *)
(*   ─────────────────────  ─────────────────────                        *)
(*   ⊢ DivBy3 15 [D3 M3]   ⊢ DivBy5 15 [D5 M5]                         *)
(*   ────────────────────────────────────────────── Div15                *)
(*            ⊢ DivBy15 15 [D15 (D3 M3) (D5 M5)]                        *)
(*   ──────────────────────────────────────────────────── FB-FizzBuzz    *)
(*   ⊢ FizzBuzz 15 "FizzBuzz" [FB-FZ 15 (D15 (D3 M3) (D5 M5))]         *)

? ⊢ FizzBuzz 15 s [w]
(* s = "FizzBuzz", w = FB-FZ 15 (D15 (D3 M3-pf) (D5 M5-pf)) *)

? ⊢ FizzBuzz 7 s [w]
(* s = "7", w = FB-N 7 (ND3 (NEq-pf (7 mod 3) 0)) (ND5 (NEq-pf (7 mod 5) 0)) *)

(* Generating all outputs: *)
? ∀ n ∈ {1..20}. ⊢ FizzBuzz n s(n) [w(n)]
(* Yields a dependent map n ↦ (s(n), w(n)) for each n *)
```

**ASCII fallback:**
```
-- FizzBuzz : N -> String -> Prop

--   |- DivBy15 n [ev]
--   -------------------------------- FB-FizzBuzz
--   |- FizzBuzz n "FizzBuzz" [FB-FZ n ev]

--   |- DivBy3 n [ev3]   |- NotDivBy5 n [ev5]
--   ----------------------------------------- FB-Fizz
--   |- FizzBuzz n "Fizz" [FB-F n ev3 ev5]

--   |- DivBy5 n [ev5]   |- NotDivBy3 n [ev3]
--   ----------------------------------------- FB-Buzz
--   |- FizzBuzz n "Buzz" [FB-B n ev5 ev3]

--   |- NotDivBy3 n [ev3]   |- NotDivBy5 n [ev5]
--   --------------------------------------------- FB-Num
--   |- FizzBuzz n (show n) [FB-N n ev3 ev5]
```

**Assessment:** Case analysis in inference rule style requires making the cases mutually exclusive via negative premises (`NotDivBy3`, `NotDivBy5`), which is both logically rigorous and visually heavy compared to `if/else`. The four rules align neatly with the four logical cases, and the exclusivity is *explicit* in the proof structure rather than relying on rule ordering — a genuine advantage over Prolog-style cut. The cost is significant boilerplate: a simple FizzBuzz requires eight auxiliary rules before reaching the four main cases.

---

## Overall Assessment

The mathematical/type-theoretic syntax achieves the highest level of logical precision of any notation considered for Evident. Every derivation corresponds directly to a proof object; every claim has a type; every rule has an explicit name. This makes the system ideal for domains where proofs are themselves outputs — certified compilation, proof-carrying code, auditable validation pipelines.

The tradeoffs are steep. Horizontal bar notation is two-dimensional and fights line-oriented tooling (editors, diffs, terminals). Unicode symbols require deliberate input methods. Negative premises (`NotDivBy3`) must be stated explicitly, unlike in Prolog where cut provides a pragmatic shortcut. Auxiliary judgments proliferate: a factorial program needs `Mult`; FizzBuzz needs eight divisibility rules. The evidence term syntax in `[·]` is novel and unambiguous but unfamiliar.

For Evident's target use cases, this syntax is best suited as a *specification language* or *type-checker output format* rather than what programmers write day-to-day. It could serve as the formal semantics that other, friendlier syntaxes compile into or are explained by.
