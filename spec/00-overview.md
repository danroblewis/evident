# Evident Language Specification — Overview

## Core concept

Every `claim` in Evident names a **set**. Every constraint in a claim body is a
**membership condition** that narrows that set. The solver finds elements of the
resulting intersection.

```
claim sorted list ∈ List T, T ∈ Ordered
    ∀ (a, b) ∈ each_consecutive list : a ≤ b
```

`sorted` names the set of non-decreasing lists. `list` is a variable. The body
condition defines membership. The solver finds lists satisfying it.

## What Evident is

Evident is a **constraint programming language** in the relational tradition.
Programs describe what is true, not how to compute it. The solver finds witnesses.

- **Relational**: claims are relations (sets of tuples), not functions
- **Bidirectional**: the same claim can check, generate, or enumerate depending on which variables are bound
- **Order-independent**: body constraints are simultaneous, not sequential
- **Evidence-first**: when a claim is established, the derivation is a first-class value

## What Evident is not

- Not functional programming. Claims do not return values.
- Not object-oriented. There are no methods, no mutation, no objects.
- Not Prolog. Clause ordering is irrelevant. There is no cut.

## The five primitives

Everything in Evident builds from five things:

1. **Set membership**: `x ∈ S` — x is a member of S
2. **Universal quantification**: `∀ x ∈ S : P(x)` — P holds for every element of S
3. **Existential quantification**: `∃ x ∈ S : P(x)` — P holds for some element of S
4. **Set operations**: `∪`, `∩`, `\`, `×`, `|S|` — standard set algebra
5. **Arithmetic**: `+`, `-`, `*`, `≤`, `≥`, `=`, `≠` — on numeric types

Types, claims, composition, and all sugar desugar to combinations of these.

## The execution model

1. Ground facts are asserted into the evidence base
2. Claim constraints fire to derive new facts (fixpoint computation)
3. Unbound variables are determined by the constraint solver
4. The evidence term (derivation tree) is available as a value

The solver uses constraint propagation first, then search for any remaining
free variables. Tightly-bound variables (defined by `=`) are eliminated by
substitution before search.

## Terminology

| Term | Meaning |
|---|---|
| **Claim** | A named constraint system — defines a set by membership conditions |
| **Variable** | A named unknown in a constraint system |
| **Body** | The list of constraints that define a claim's membership conditions |
| **Ground fact** | A concrete asserted value (`assert x = 5`) |
| **Unbound variable** | A variable whose value the solver must determine |
| **Evidence** | The derivation tree produced when a claim is established |
| **Names-match** | Variables with the same name across two systems are automatically identified |
| **Pass-through** (`..`) | Lift all variables of a sub-claim into the current scope |
