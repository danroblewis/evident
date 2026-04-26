# Example 5: Arithmetic — Solution Spaces and Number Theory

Every claim defines a set. Every body condition intersects that set with a smaller one.
This example makes that geometric: we write arithmetic from scratch, watch the solution
space shrink with each constraint, and build up to number theory.

---

## The starting space

```evident
? ∃ n ∈ Nat
```

Solution space: all natural numbers. Infinite. Uniform.

```evident
? ∃ a ∈ Nat, ∃ b ∈ Nat
```

Solution space: all pairs `(a, b)`. A two-dimensional infinite grid.

```evident
? ∃ a ∈ Nat, ∃ b ∈ Nat, ∃ c ∈ Nat
```

Solution space: all triples. Three-dimensional.

We will carve structure out of this space using claims.

---

## Addition — a 2D surface in 3-space

`sum a b c` is established when `a + b = c`. It names a set of triples.

```evident
claim sum : Nat → Nat → Nat → det

evident sum 0 b b               -- 0 + b = b  (base)
evident sum (succ a) b (succ c) -- (a+1) + b = (a+b)+1
    sum a b c
```

The solution space of `sum a b c` (all three free) is:

```
{ (0,0,0), (0,1,1), (0,2,2), ...,
  (1,0,1), (1,1,2), (1,2,3), ...,
  (2,0,2), (2,1,3), (2,2,4), ..., }
```

This is a **2D surface** in Nat³ — one degree of freedom removed. Any two values
determine the third. The solver finds it by propagating the arithmetic constraint.

### Slicing the space

```evident
-- Fix a and b: the space collapses to a single point
? ∃ c : sum 3 4 c
-- c = 7  ✓ (0 degrees of freedom remain)

-- Fix only c: the space is a 1D line
? ∃ a ∈ Nat, ∃ b ∈ Nat : sum a b 10
-- (0,10), (1,9), (2,8), (3,7), (4,6), (5,5), (6,4), (7,3), (8,2), (9,1), (10,0)
-- 11 solutions — the line a + b = 10

-- Run backwards: what pairs sum to a prime?
? ∃ a ∈ Nat, ∃ b ∈ Nat, ∃ p ∈ Nat : sum a b p, prime p
-- (0,2,2), (0,3,3), (1,2,3), (0,5,5), (2,3,5), ...
```

---

## Multiplication — repeated addition

`product a b c` is established when `a × b = c`.

```evident
claim product : Nat → Nat → Nat → det

evident product 0 b 0               -- 0 × b = 0
evident product (succ a) b c        -- (a+1) × b = (a × b) + b
    ∃ c0 : product a b c0
    sum c0 b c
```

The intermediate `c0` is the partial product — introduced with `∃`, not as a bare name.

### Solution space of `product a b c`

Sparse: only multiplicative triples exist.

```
(0,0,0), (0,1,0), (0,2,0), ...  -- anything × 0 = 0
(1,0,0), (1,1,1), (1,2,2), ...  -- 1 × b = b
(2,0,0), (2,1,2), (2,2,4), ...  -- 2 × b = 2b
(3,0,0), (3,1,3), (3,2,6), ...
```

Still a 2D surface, but sparsely distributed through Nat³.

```evident
-- Factorisation: what pairs multiply to 12?
? ∃ a ∈ Nat, ∃ b ∈ Nat : product a b 12
-- (1,12), (2,6), (3,4), (4,3), (6,2), (12,1)
-- Also: (0, anything) if we allow 0, but 0 × b = 0 ≠ 12 so no.
```

---

## Divisibility — a relation, not a function

`divides a b` is established when `a` is a divisor of `b` — when `b/a` is a whole number.

```evident
claim divides : Nat → Nat → Prop

evident divides a b
    ∃ k ∈ Nat : product a k b    -- b = a × k for some natural number k
```

`divides` names the set of pairs `(a, b)` where a divides b evenly.
The witness `k` is the quotient — it must exist and be a natural number.

### Solution space of `divides a b`

```
(1, 0), (1, 1), (1, 2), (1, 3), ...  -- 1 divides everything
(2, 0), (2, 2), (2, 4), (2, 6), ...  -- 2 divides even numbers
(3, 0), (3, 3), (3, 6), (3, 9), ...  -- multiples of 3
(4, 0), (4, 4), (4, 8), ...
```

```evident
-- What are the divisors of 12?
? ∃ d ∈ Nat : divides d 12
-- d ∈ {1, 2, 3, 4, 6, 12}

-- What does 7 divide?
? ∃ n ∈ Nat : divides 7 n
-- n ∈ {0, 7, 14, 21, 28, ...}  (multiples of 7)

-- Is 5 a divisor of 30?
? divides 5 30
-- Yes ✓  (k = 6)
```

---

## Primality — the set of primes

A prime is a number greater than 1 with no divisors other than 1 and itself.

```evident
claim prime : Nat → semidet

evident prime n
    n > 1
    ∀ d ∈ Nat : divides d n ⇒ d = 1 ∨ d = n
```

`prime` names a subset of Nat. The body says: n is in the set of primes when
it is greater than 1 AND the only pairs `(d, n)` in `divides` that have n on
the right are the trivial ones.

### Monte Carlo interpretation

The `prime` claim defines a set. We can ask about its shape:

```
Primes under 10:  {2, 3, 5, 7}             — 4 out of 10    (40%)
Primes under 100: {2, 3, 5, ..., 97}       — 25 out of 100  (25%)
Primes under 1000:                          — 168 out of 1000 (16.8%)
```

Each row is asking: if you sample uniformly from Nat up to N, what fraction
land in the `prime` set? The density decreases — the solution space of `prime`
thins out as N grows. This is the prime number theorem, readable directly from
the claim's solution space.

If you Monte Carlo sampled random integers and checked the `prime` claim, you
would empirically rediscover the prime number theorem from the claim definition.

```evident
-- The primes: enumerate the solution space
? ∀ p ∈ Nat : prime p
-- 2, 3, 5, 7, 11, 13, 17, 19, 23, ...

-- Twin primes: pairs of primes differing by 2
? ∃ p ∈ Nat : prime p, prime (p + 2)
-- (3,5), (5,7), (11,13), (17,19), (29,31), ...

-- Goldbach (empirically): every even number > 2 is the sum of two primes
? ∀ n ∈ Nat : n > 2, even n ⇒ ∃ p ∈ Nat, ∃ q ∈ Nat : prime p, prime q, sum p q n
-- (checked for small n)
```

---

## GCD — the greatest common divisor

`gcd a b g` is established when g is the greatest common divisor of a and b.

```evident
claim gcd : Nat → Nat → Nat → semidet

evident gcd a b g
    divides g a                -- g divides a
    divides g b                -- g divides b
    ∀ d ∈ Nat :                -- and no larger divisor divides both
        divides d a, divides d b ⇒ d ≤ g
```

This is a purely declarative characterisation — no Euclidean algorithm, no recursion.
The three conditions define exactly the greatest common divisor:
- g divides both (g is a common divisor)
- no d divides both and exceeds g (g is greatest)

```evident
? ∃ g : gcd 12 8 g
-- g = 4  ✓

? ∃ g : gcd 7 13 g
-- g = 1  ✓  (7 and 13 are coprime)
```

### Coprimality — solution space of gcd = 1

```evident
claim coprime : Nat → Nat → semidet

evident coprime a b
    gcd a b 1
```

The solution space of `coprime a b` is all pairs with gcd 1. About 60.8% of
random pairs of integers are coprime (the probability is 6/π²). Again readable
from the claim by Monte Carlo.

---

## Composing the space reductions

Now compose: what pairs of primes are coprime to each other except when equal?

```evident
? ∃ p ∈ Nat, ∃ q ∈ Nat : prime p, prime q, p ≠ q
```

```
(2,3), (2,5), (2,7), (3,5), (3,7), (5,7), ...
```

All distinct primes are coprime — because any prime p's only divisors are 1 and p,
so the only common divisor of two distinct primes is 1. This is derivable from
the definitions, not an additional constraint.

```evident
-- Fermat numbers: 2^(2^n) + 1
-- The first five are prime (Fermat's conjecture — later disproved)
? ∃ n ∈ Nat : prime (fermat n)
-- 3, 5, 17, 257, 65537  ✓
-- 4294967297 = 641 × 6700417  ✗  (not prime — Euler, 1732)
```

---

## What this shows about the language

Every claim is a set. Every body condition is a set intersection.

| Claim | Solution space |
|---|---|
| `∃ n ∈ Nat` | All of Nat (infinite, 1D) |
| `sum a b c` | A 2D surface in Nat³ (one constraint) |
| `product a b c` | A sparse 2D surface in Nat³ |
| `divides a b` | A subset of Nat² — pairs where a\|b |
| `prime n` | A subset of Nat — density ~ 1/ln(n) |
| `gcd a b 1` | A subset of Nat² — density ~ 6/π² |
| `twin_prime p` | A subset of Nat — density unknown (open problem) |

The programmer's job is to write claims whose solution space is exactly the set of
interesting things. The solver finds members. Monte Carlo estimates the shape.

The solution space of any claim can be explored without writing a single algorithm —
just by querying with different combinations of bound and free variables.
