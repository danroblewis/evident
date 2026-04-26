# How Claims and Evidence Become a Constraint System

Using `sorted` and `sort` as the working example.

---

## 1. What a `claim` declaration is

```evident
claim sorted[T : Ordered] : List T -> Prop
```

A `claim` is a **relation schema**, not a function. It declares that `sorted` is a name
that can be *established* or *not established* for a given argument. `Prop` means it
produces a truth value, not a computed value. There is no "return."

```mermaid
graph LR
    A["name: sorted"] -->|"applied to"| B["List T\ne.g. the list 1,2,3"]
    B -->|"yields"| C["Prop\n(established or not)"]
    note["NOT a function.\nNo return value.\nNo computation.\nJust: does this hold?"]
    style note fill:#fef3c7,stroke:#d97706
```

Compare to a function: `sorted [1,2,3]` would *return* `true`. In Evident, `sorted [1,2,3]`
is either *derivable from the rules* or it isn't. The distinction matters because the solver
can work backwards — asking "what list would make `sorted ?xs` hold?"

---

## 2. What an `evident` block is

Each `evident` block is one **conditional rule**: if the body sub-claims are all established,
the head becomes established.

```evident
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```

```mermaid
graph TB
    subgraph RULE["One 'evident' rule"]
        HEAD["HEAD\n'sorted a b rest'\nwhat this rule establishes"]
        GUARD["GUARD\na ≤ b\nmust hold for rule to fire"]
        BODY["BODY\nmust all be established first"]
        SUB["sorted b rest\n(recursive sub-claim)"]

        GUARD -->|fires only when| HEAD
        BODY  -->|establishes| HEAD
        SUB   --> BODY
    end
    style HEAD fill:#dbeafe,stroke:#2563eb
    style GUARD fill:#fef9c3,stroke:#ca8a04
    style BODY fill:#dcfce7,stroke:#16a34a
```

Reading it as a **constraint on the solver**: to establish `sorted [a, b | rest]`, the
solver must find values for a, b, rest such that `a <= b` holds AND `sorted [b | rest]`
is established. If no such values exist, the rule cannot fire.

---

## 3. All rules for `sorted` together — a decision procedure

```evident
evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```

The three rules together form a complete decision procedure. The solver tries each:

```mermaid
flowchart TD
    Q["? sorted xs"]
    E1{"xs = empty list?"}
    E2{"xs = one element?"}
    E3{"xs = a,b followed by rest\nAND a ≤ b?"}
    FAIL["Not established\n(no rule matched)"]
    R1["✓ Established\n(base case: empty)"]
    R2["✓ Established\n(base case: singleton)"]
    R3["? sorted b,rest\n(recurse — same decision tree)"]

    Q  --> E1
    E1 -->|yes| R1
    E1 -->|no| E2
    E2 -->|yes| R2
    E2 -->|no| E3
    E3 -->|no| FAIL
    E3 -->|yes| R3
    R3 -->|established| R2
    R3 -->|not established| FAIL

    style R1 fill:#dcfce7,stroke:#16a34a
    style R2 fill:#dcfce7,stroke:#16a34a
    style FAIL fill:#fee2e2,stroke:#dc2626
```

No ordering between the three rules — the solver can try them in any order. Only one
can succeed for any given list. (The guards and patterns make them mutually exclusive.)

---

## 4. `sort` as a constraint conjunction

```evident
claim sort[T : Ordered] : List T -> List T -> Prop

evident sort xs ys
    length ys = length xs
    sorted ys
    permutation xs ys
```

The body is a **simultaneous conjunction of constraints**. To establish `sort xs ys`,
ALL THREE must hold at the same time. The solver must find values for any unbound
variables (like `ys` in a query) that satisfy all three together.

```mermaid
graph TD
    GOAL["GOAL: sort xs ys"]
    C1["length ys = length xs\narithmetic constraint"]
    C2["sorted ys\nstructural constraint"]
    C3["permutation xs ys\nrelational constraint"]

    C1 -->|AND| GOAL
    C2 -->|AND| GOAL
    C3 -->|AND| GOAL

    style GOAL fill:#dbeafe,stroke:#2563eb
    style C1   fill:#fef3c7,stroke:#d97706
    style C2   fill:#fef3c7,stroke:#d97706
    style C3   fill:#fef3c7,stroke:#d97706
```

This is where the constraint solver earns its keep. With `xs = [3, 1, 2]`:

- C1 alone says: ys has length 3
- C1 + C2 say: ys is a sorted list of length 3 (e.g. `[0,0,0]` would qualify)
- C1 + C2 + C3 say: ys is a sorted list of length 3 containing exactly {1, 2, 3}

Only one list satisfies all three. The solver finds it.

---

## 5. Solver trace: `? sort [3, 1, 2] ?ys`

```mermaid
sequenceDiagram
    participant Q  as Query
    participant S  as Solver
    participant DB as Evidence Base

    Q  ->> S:  ? sort [3,1,2] ?ys
    S  ->> S:  apply rule: evident sort xs ys
    Note over S: xs = [3,1,2], ys = unknown

    S  ->> S:  post constraint 1: length ys = length [3,1,2]
    S  ->> S:  length [3,1,2] = 3  (arithmetic)
    S  ->> S:  ∴ length ys = 3
    Note over S: ys is now constrained to 3-element lists

    S  ->> S:  post constraint 2: sorted ys
    S  ->> S:  ys must be non-decreasing
    Note over S: ys ∈ { [a,b,c] | a≤b, b≤c }

    S  ->> S:  post constraint 3: permutation [3,1,2] ys
    S  ->> S:  ys must contain exactly the elements {1, 2, 3}
    Note over S: length 3 + sorted + contains {1,2,3} → unique: [1,2,3]

    S  ->> DB: establish sort [3,1,2] [1,2,3]
    DB -->> Q: ys = [1,2,3]  ✓

    Note over Q,DB: Evidence term also returned — the full derivation tree
```

The key step: constraints 1, 2, and 3 **propagate** to narrow the space of possible `ys`
values until only one remains. The solver never tried any permutation explicitly — the
constraints ruled everything else out.

---

## 6. The full dependency graph

How `sort` depends on `sorted` and `permutation`, which depend on further sub-claims:

```mermaid
graph TD
    SORT["sort xs ys"]

    subgraph SORT_BODY["sort body — all must hold"]
        LEN["length ys = length xs"]
        SORTED["sorted ys"]
        PERM["permutation xs ys"]
    end

    subgraph SORTED_RULES["sorted rules"]
        SB1["sorted empty  ← axiom"]
        SB2["sorted one-element  ← axiom"]
        SRE["sorted a,b,rest\nrequires: a≤b AND sorted b,rest"]
    end

    subgraph PERM_RULES["permutation rules"]
        PB["permutation empty empty  ← axiom"]
        PRE["permutation x,xs ys\nrequires: member x ys\nAND permutation xs (ys minus x)"]
    end

    SORT --> LEN
    SORT --> SORTED
    SORT --> PERM

    SORTED --> SB1
    SORTED --> SB2
    SORTED --> SRE
    SRE    -->|recurses into| SORTED

    PERM --> PB
    PERM --> PRE
    PRE  -->|recurses into| PERM

    style SORT fill:#dbeafe,stroke:#2563eb
    style LEN  fill:#fef3c7,stroke:#d97706
    style SORTED fill:#fef3c7,stroke:#d97706
    style PERM fill:#fef3c7,stroke:#d97706
```

Each node is a claim. Each edge is "requires." The solver walks down from the query,
posting constraints at each level, propagating their consequences upward until the top-level
claim is established.

---

## What the programmer's job actually is

| | Conventional programming | Evident |
|---|---|---|
| You write | An algorithm (steps to execute) | A model (conditions that must hold) |
| Variables | Storage locations (assigned, mutated) | Unknowns (constrained, resolved) |
| A "call" | Execute this computation | Check / establish this claim |
| Body lines | Instructions in sequence | Constraints that must all hold simultaneously |
| Order matters? | Yes — sequence is the program | No — the solver finds any valid order |
| Output | Return value | Established fact + evidence term |

The `claim` line says: **this is the shape of a fact that can be established.**

The `evident` line(s) say: **here are the conditions under which that fact is established.**

The solver says: **given a query, find values that make all conditions true simultaneously.**

You are not telling the computer *how* to sort. You are telling it *what sorted means*,
and it figures out how to get there.
