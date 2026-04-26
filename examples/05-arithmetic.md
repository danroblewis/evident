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
? ∃ a, b ∈ Nat
```

Solution space: all pairs `(a, b)`. A two-dimensional infinite grid.

```evident
? ∃ a, b, c ∈ Nat
```

Solution space: all triples. Three-dimensional.

We will carve structure out of this space using claims.

---

## Addition — a 2D surface in 3-space

`sum` names the set of triples where the third is the sum of the first two.
It is `det`: any two values uniquely determine the third.

```evident
claim sum : Nat → Nat → Nat → det

evident sum 0 b b                   -- 0 + b = b
evident sum (succ a) b (succ c)     -- (a+1) + b = (a+b)+1
    _c = sum a b
    c = _c
```

The solution space of `sum a b c` with all three free is the set:

```
{ (0,0,0), (0,1,1), (0,2,2), ...,
  (1,0,1), (1,1,2), (1,2,3), ...,
  (2,0,2), (2,1,3), (2,2,4), ... }
```

A **2D surface** in Nat³ — one degree of freedom removed. Any two values determine
the third. Since `sum` is `det`, calling it binds a result:

```evident
-- Fix both inputs: collapses to a point
? c = sum 3 4
-- c = 7  ✓

-- Fix only the result: a 1D line (all pairs summing to 10)
? ∃ a, b ∈ Nat : sum a b = 10
-- (0,10), (1,9), (2,8), (3,7), (4,6), (5,5), (6,4), (7,3), (8,2), (9,1), (10,0)

-- Compose: find pairs that sum to a prime
? ∃ a, b, p ∈ Nat : p = sum a b, prime p
-- (0,2), (0,3), (1,2), (0,5), (2,3), ...
```

---

## Multiplication — repeated addition

`product` is also `det`. Its body uses the `= claim args` form for both the recursive
call and the final sum, with `_` names for body-internal scaffolding:

```evident
claim product : Nat → Nat → Nat → det

evident product 0 b 0               -- 0 × b = 0
evident product (succ a) b c        -- (a+1) × b = (a × b) + b
    _partial = product a b
    c        = sum _partial b
```

`_partial` is body-internal — a name for the partial product `a × b`. It has no
meaning outside this body. The solver finds its value as part of satisfying the
constraints, just like every other name in the body.

```evident
-- What pairs multiply to 12?
? ∃ a, b ∈ Nat : product a b = 12
-- (1,12), (2,6), (3,4), (4,3), (6,2), (12,1)

-- Compose multiplication with addition
? c = sum (product 3 4) 2
-- Not valid — no nested calls. Bind each step:
? _m = product 3 4, c = sum _m 2
-- c = 14  ✓
```

---

## Divisibility — a relation, not a function

`divides a b` is established when a divides b evenly — when b/a is a whole number.
Unlike `sum` and `product`, it is not `det`: for given a, many b values qualify.

```evident
claim divides : Nat → Nat → semidet

evident divides a b
    _k ∈ Nat
    product a _k = b        -- b = a × k for some natural number k
```

`_k` is the quotient — body-internal, found by the solver. `divides` names the set
of pairs `(a, b)` where such a `_k` exists.

```evident
-- What are the divisors of 12?
? ∃ d ∈ Nat : divides d 12
-- d ∈ {1, 2, 3, 4, 6, 12}

-- What does 7 divide?
? ∃ n ∈ Nat : divides 7 n
-- n ∈ {0, 7, 14, 21, 28, ...}

-- Membership check
? divides 5 30
-- Yes ✓  (solver finds _k = 6)
```

---

## Primality — the set of primes

A prime is a number greater than 1 whose only divisors are 1 and itself.

```evident
claim prime : Nat → semidet

evident prime n
    n > 1
    ∀ d ∈ Nat : divides d n ⇒ d = 1 ∨ d = n
```

`prime` names a subset of Nat.

### Monte Carlo interpretation

The `prime` claim defines a set with a measurable shape:

```
Primes under 10:   {2, 3, 5, 7}        — 4/10   (40%)
Primes under 100:  {2, 3, ..., 97}     — 25/100 (25%)
Primes under 1000:                      — 168/1000 (16.8%)
```

Sample random integers, check the `prime` claim, count hits — the density follows
1/ln(n). This is the prime number theorem, readable empirically from the claim's
solution space.

```evident
-- Twin primes: pairs differing by 2
? ∃ p ∈ Nat : prime p, _q = p + 2, prime _q
-- (3,5), (5,7), (11,13), (17,19), (29,31), ...

-- Goldbach (empirical): every even n > 2 is a sum of two primes
? ∀ n ∈ Nat : n > 2, even n ⇒ ∃ p, q ∈ Nat : prime p, prime q, sum p q = n
```

---

## GCD — the greatest common divisor

The GCD of a and b is uniquely determined — `det`.

```evident
claim gcd : Nat → Nat → Nat → det

evident gcd a b g
    divides g a
    divides g b
    ∀ d ∈ Nat : divides d a, divides d b ⇒ d ≤ g
```

No Euclidean algorithm. Just three simultaneous conditions: g divides both,
nothing larger divides both.

```evident
? g = gcd 12 8      -- g = 4  ✓
? g = gcd 7 13      -- g = 1  ✓ (coprime)
```

### Coprimality

```evident
claim coprime : Nat → Nat → semidet

evident coprime a b
    gcd a b = 1
```

Solution space of `coprime a b`: pairs whose GCD is 1. Density ≈ 6/π² ≈ 60.8%
of random integer pairs. Again readable by Monte Carlo from the claim definition.

---

## Composing the space reductions

Each claim is a set. Querying with combinations of bound and free variables
slices the solution space differently:

| Query | What you get |
|---|---|
| `? ∃ a, b, c ∈ Nat` | All triples — 3D space |
| `? ∃ a, b ∈ Nat : sum a b = 10` | The line a+b=10 — 1D |
| `? c = sum 3 4` | A single point — 0D |
| `? ∃ d ∈ Nat : divides d 12` | The 6 divisors of 12 |
| `? ∃ p ∈ Nat : prime p` | The infinite set of primes |
| `? ∃ a, b ∈ Nat : coprime a b` | ~60.8% of integer pairs |

The programmer writes claims. The solver navigates the resulting solution space.
Monte Carlo estimates the shape. No algorithms required.
