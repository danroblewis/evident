# Syntax: Functional / ML

## Philosophy

This syntax makes the Curry-Howard isomorphism the centerpiece: every claim family is a type, and every piece of evidence is a value of that type. The design borrows Haskell's `data` declarations for evidence constructors, ML-style pattern matching for clause selection, and `where` for local sub-claims. The `prove` keyword introduces evidence-construction cases analogous to function clauses. `infer` asks the runtime to search for evidence automatically. Because evidence terms are algebraic data types, they can be inspected, deconstructed, and passed to other proofs — the derivation tree is always a first-class citizen.

---

## 1. Factorial

```evident
module Factorial where

-- Claim family: Factorial n f means f is the factorial of n
evidence Factorial : Nat -> Nat -> Type where
  FactZ : Factorial Z (S Z)
  FactS : Factorial n f -> Multiply (S n) f sf -> Factorial (S n) sf

-- Proving Factorial 3 6 step by step:
prove factorial : (n : Nat) -> (f : Nat) -> Factorial n f
prove factorial Z     (S Z) = FactZ
prove factorial (S n) sf    =
  let inner = factorial n f
      mul   = infer (Multiply (S n) f sf)
  in  FactS inner mul

-- Evidence is a tree you can inspect
showFactTree : Factorial n f -> String
showFactTree FactZ          = "Factorial(0) = 1 [axiom]"
showFactTree (FactS inner mul) =
  "Factorial(S n) via:\n  " ++ showFactTree inner
                             ++ "\n  " ++ showMul mul
```

**What works well:** The `evidence ... where` block mirrors Haskell `data` perfectly, and the constructors read like typed proof terms. The `let`-binding style for chaining sub-evidences is natural to anyone who knows Haskell do-notation. **What feels awkward:** The `Multiply` helper claim must be defined separately; there is no inline arithmetic, so the factorial rule grows boilerplate. The implicit variable `f` inside `prove factorial (S n) sf` requires care — the surface syntax needs rules about which variables are universally quantified vs. locally bound.

---

## 2. Sorted List

```evident
module SortedList where

-- Ordering evidence (primitive claim, self-evident via axioms)
evidence Leq : Nat -> Nat -> Type where
  LeqZ  : Leq Z m
  LeqS  : Leq n m -> Leq (S n) (S m)

-- Claim family: Sorted xs means xs is sorted in ascending order
evidence Sorted : List Nat -> Type where
  SortedNil  : Sorted []
  SortedOne  : (n : Nat) -> Sorted [n]
  SortedCons : Leq a b -> Sorted (b :: rest) -> Sorted (a :: b :: rest)

-- Proving a concrete list is sorted
prove sortedExample : Sorted [1, 2, 3]
prove sortedExample =
  SortedCons (infer (Leq 1 2))
    (SortedCons (infer (Leq 2 3))
      (SortedOne 3))

-- Pattern-matching on sorted evidence to extract head bound
headBound : Sorted (a :: b :: rest) -> Leq a b
headBound (SortedCons leq _) = leq

-- Prove that appending a smaller element preserves sorting
prependSorted : Leq x a -> Sorted (a :: rest) -> Sorted (x :: a :: rest)
prependSorted leq sorted = SortedCons leq sorted
```

**What works well:** The three constructors map cleanly to the three structural cases of sorted lists, and the Curry-Howard reading is immediate — `SortedCons` is literally a product of a `Leq` proof and a recursive `Sorted` proof. Pattern-matching in `headBound` to extract sub-evidence is concise and satisfying. **What feels awkward:** The nested `SortedCons` applications in `sortedExample` create a pyramid of parentheses; a layout-sensitive `do`-style combinator for sequential evidence might read better for longer lists.

---

## 3. HTTP Request Validation

```evident
module RequestValidation where

-- Sub-claim families
evidence ValidMethod  : Method  -> Type where
  IsGet    : ValidMethod GET
  IsPost   : ValidMethod POST
  IsPut    : ValidMethod PUT
  IsDelete : ValidMethod DELETE

evidence ValidPath    : String  -> Type where
  WellFormedPath : StartsWith "/" path -> ValidPath path

evidence ValidHeaders : Headers -> Type where
  HasContentType : LookupKey "Content-Type" headers v
                -> ValidContentType v
                -> ValidHeaders headers

evidence AuthOk       : Request -> Type where
  BearerAuth : LookupKey "Authorization" req.headers tok
             -> ValidToken tok
             -> AuthOk req

-- Top-level claim: a request is valid
evidence ValidRequest : Request -> Type where
  MkValidRequest
    :  { method  : ValidMethod  req.method  }
    -> { path    : ValidPath    req.path    }
    -> { headers : ValidHeaders req.headers }
    -> { auth    : AuthOk       req         }
    -> ValidRequest req

-- Construct evidence for a specific request
prove validateRequest : (req : Request) -> Maybe (ValidRequest req)
prove validateRequest req = do
  method  <- infer (ValidMethod  req.method)
  path    <- infer (ValidPath    req.path)
  headers <- infer (ValidHeaders req.headers)
  auth    <- infer (AuthOk       req)
  pure (MkValidRequest { method, path, headers, auth })

-- Extract sub-evidence by field name
extractAuth : ValidRequest req -> AuthOk req
extractAuth (MkValidRequest { auth, .. }) = auth

-- Re-use the auth evidence downstream
logAuthToken : ValidRequest req -> IO ()
logAuthToken vr =
  let BearerAuth _ tok = extractAuth vr
  in  putStrLn ("Authenticated with token: " ++ showToken tok)
```

**What works well:** Record-style named fields in `MkValidRequest` make it trivial to extract individual sub-evidences, and `do`-notation for `Maybe` gives a natural short-circuit semantics when any sub-claim fails. The field punning (`{ method, path, headers, auth }`) keeps construction sites readable. **What feels awkward:** Mixing record field access (`req.method`) with positional type arguments feels inconsistent; the language needs a clear convention for whether requests are record-typed or index-typed. The `Maybe` wrapper also papers over the question of *why* validation failed — a richer `Either ValidRequest ValidationError` type would carry rejection evidence.

---

## 4. Graph Reachability

```evident
module GraphReachability where

-- Nodes and edges as axioms (self-evident claims)
evidence Node : Type where
  A | B | C | D | E : Node

evidence Edge : Node -> Node -> Type where
  AB : Edge A B
  BC : Edge B C
  CD : Edge C D
  BD : Edge B D

-- Reachability: the transitive closure of Edge
evidence Reachable : Node -> Node -> Type where
  Direct : Edge u v -> Reachable u v
  Step   : Edge u v -> Reachable v w -> Reachable u w

-- Prove A can reach D two ways
prove reachAD_via_C : Reachable A D
prove reachAD_via_C = Step AB (Step BC (Direct CD))

prove reachAD_via_BD : Reachable A D
prove reachAD_via_BD = Step AB (Direct BD)

-- The runtime can search for any reachability proof
prove reachEC : Reachable E C
prove reachEC = infer (Reachable E C)   -- fails: no path from E

-- Compute the length of a reachability path
pathLength : Reachable u v -> Nat
pathLength (Direct _)     = 1
pathLength (Step _ rest)  = 1 + pathLength rest

-- All paths from a node (non-deterministic, returns a list of evidences)
allPaths : (u : Node) -> [Exists v. Reachable u v]
allPaths u = infer* (Exists v. Reachable u v)
```

**What works well:** The `Direct`/`Step` constructors precisely mirror the two-case inductive definition of transitive closure, and having two distinct proofs of `Reachable A D` illustrates that evidence terms carry the specific path taken — not just the Boolean fact. `pathLength` as a function over evidence trees is elegant. **What feels awkward:** `infer*` for enumerating all solutions is ad hoc; the language needs a principled story for how backtracking and multiple derivations surface at the type level, perhaps via a `Search` monad or a `List`-valued `infer`.

---

## 5. FizzBuzz

```evident
module FizzBuzz where

-- Divisibility as a claim
evidence DivisibleBy : Nat -> Nat -> Type where
  DivBy : Multiply k d n -> DivisibleBy n d

evidence NotDivisibleBy : Nat -> Nat -> Type where
  NotDivBy : ((k : Nat) -> Not (Multiply k d n)) -> NotDivisibleBy n d

-- The four FizzBuzz cases, each carrying divisibility evidence
evidence FizzBuzz : Nat -> String -> Type where
  IsFizzBuzz
    :  DivisibleBy n 3
    -> DivisibleBy n 5
    -> FizzBuzz n "FizzBuzz"

  IsFizz
    :  DivisibleBy    n 3
    -> NotDivisibleBy n 5
    -> FizzBuzz n "Fizz"

  IsBuzz
    :  NotDivisibleBy n 3
    -> DivisibleBy    n 5
    -> FizzBuzz n "Buzz"

  IsNum
    :  NotDivisibleBy n 3
    -> NotDivisibleBy n 5
    -> FizzBuzz n (showNat n)

-- Derive FizzBuzz label for any n
prove fizzBuzz : (n : Nat) -> (s : String ** FizzBuzz n s)
prove fizzBuzz n =
  case (infer? (DivisibleBy n 3), infer? (DivisibleBy n 5)) of
    (Just d3, Just d5) -> ("FizzBuzz", IsFizzBuzz d3 d5)
    (Just d3, Nothing) ->
      let nd5 = infer (NotDivisibleBy n 5)
      in  ("Fizz", IsFizz d3 nd5)
    (Nothing, Just d5) ->
      let nd3 = infer (NotDivisibleBy n 3)
      in  ("Buzz", IsBuzz nd3 d5)
    (Nothing, Nothing) ->
      let nd3 = infer (NotDivisibleBy n 3)
          nd5 = infer (NotDivisibleBy n 5)
      in  (showNat n, IsNum nd3 nd5)

-- Run it
main : IO ()
main = traverse_ (\n -> let (s, _) = fizzBuzz n in putStrLn s) [1..100]
```

**What works well:** Encoding each of the four cases as a distinct constructor forces the programmer to explicitly handle every combination of divisibility, and the evidence constructors carry the *reason* a number is Fizz or Buzz — not just the label. The dependent pair `(s : String ** FizzBuzz n s)` ties the output string to its proof. **What feels awkward:** `infer?` returning `Maybe` vs. `infer` returning the value (or failing at compile time) introduces an inconsistency; the language needs a uniform story for "soft" vs. "hard" inference. The `NotDivisibleBy` constructor wrapping a universally-quantified negation is verbose and may intimidate users expecting a simple modulo check.

---

## Overall Assessment

The Haskell/ML functional syntax makes Evident's Curry-Howard nature explicit and readable for type-theory practitioners. Evidence constructors as algebraic data types give derivation trees a concrete, inspectable representation, and `where`/`let`/`do` allow compositional proof building with minimal ceremony. Pattern matching on evidence to extract sub-derivations is the standout strength — it turns "querying the proof" into ordinary functional programming.

The main friction points are: (1) the `infer` / `infer?` / `infer*` family needs a unified design — mixing hard inference, optional inference, and search feels piecemeal; (2) negation and inequality claims require verbose wrapper constructors that break the otherwise clean algebraic style; (3) the fixpoint execution model is invisible in the syntax, which may confuse users who expect lazy or strict evaluation semantics. Overall this syntax rewards users with dependent-type or proof-assistant backgrounds and would feel at home alongside Agda or Idris, but carries a steeper learning curve than the logic-programming alternative.
