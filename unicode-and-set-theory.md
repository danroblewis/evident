# Unicode and Set Theory in Evident

## The colon is already ∈

In type theory, `a : T` ("a has type T") and `a ∈ T` ("a is an element of T") are
the same judgment. Evident inherits this — the colon in type annotations IS the set
membership relation. Both forms are valid and mean exactly the same thing:

```evident
type Task = { id : Nat,  duration : Nat,  deadline : Nat }
type Task = { id ∈ Nat,  duration ∈ Nat,  deadline ∈ Nat }
```

A record type is a set-builder expression restricted to named fields. This equivalence
runs all the way through the language:

| Evident form | Set theory reading |
|---|---|
| `claim sorted : List Nat -> Prop` | sorted is a relation whose domain is the set of all `List Nat` values |
| `evident sorted []` | `[] ∈ sorted` — the empty list is a member of the sorted relation |
| `claim member : Nat -> List Nat -> Prop` | member is a binary relation between Nat and List Nat |
| `some w in workers : w.id = a.worker_id` | `∃ w ∈ workers : w.id = a.worker_id` |

The whole language is set theory. The syntax now admits this openly.

---

## Full vocabulary: ASCII and Unicode aliases

Both forms parse identically. The lexer treats them as synonyms. You write ASCII;
the editor displays Unicode. The source file can store either.

| ASCII | Unicode | Meaning | Context |
|---|---|---|---|
| `x : T` | `x ∈ T` | x is a member of / has type T | type annotations, record fields, claim params |
| `->` | `→` | maps to / function type | claim type signatures |
| `=>` | `⇒` | implies / forward implication | forward implication rules |
| `some x in S :` | `∃ x ∈ S :` | there exists x in S such that | body existential |
| `all x in S :` | `∀ x ∈ S :` | for all x in S | body universal |
| `one x in S :` | `∃! x ∈ S :` | there exists exactly one x in S | unique existential |
| `none x in S :` | `¬∃ x ∈ S :` | there is no x in S | negated existential |
| `in` | `∈` | set membership test | body constraints |
| `not in` | `∉` | non-membership | body constraints |
| `<=` | `≤` | less than or equal | arithmetic |
| `>=` | `≥` | greater than or equal | arithmetic |
| `!=` | `≠` | not equal | constraints |
| `and` | `∧` | conjunction | combining conditions |
| `or` | `∨` | disjunction | alternatives within a line |
| `not` | `¬` | negation | negating a condition |
| `subset of` | `⊆` | subset relation | set comparisons |
| `[T : Ordered]` | `[T ∈ Ordered]` | constrained type parameter | claim declarations |

---

## Editor auto-replace

Type the ASCII shorthand; the editor inserts the Unicode symbol.
This is the same model used by Agda, Lean, and Coq.

| You type | Editor inserts |
|---|---|
| `\in` | `∈` |
| `\notin` | `∉` |
| `\->` or `\to` | `→` |
| `\=>` or `\Rightarrow` | `⇒` |
| `\exists` or `\ex` | `∃` |
| `\forall` or `\all` | `∀` |
| `\exists!` | `∃!` |
| `\neg` or `\not` | `¬` |
| `\and` or `\wedge` | `∧` |
| `\or` or `\vee` | `∨` |
| `\<=` or `\leq` | `≤` |
| `\>=` or `\geq` | `≥` |
| `\!=` or `\neq` | `≠` |
| `\subset` | `⊆` |

---

## The same program, both ways

### ASCII

```evident
type Task = {
    id       : Nat
    duration : Nat
    deadline : Nat
}

type Worker = {
    id              : Nat
    available_from  : Nat
    available_until : Nat
}

claim sorted[T : Ordered] : List T -> Prop

evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]

claim assignment_fits : List Worker -> List Task -> Assignment -> Prop

evident assignment_fits workers tasks a
    some w in workers : w.id = a.worker_id
    some t in tasks   : t.id = a.task_id
    a.start >= w.available_from
    a.start + t.duration <= w.available_until

claim valid_schedule : List Task -> List Worker -> Schedule -> Prop

evident valid_schedule tasks workers schedule
    all t in tasks :
        some a in schedule : a.task_id = t.id
    all a in schedule :
        assignment_fits workers tasks a
    all a in schedule, all b in schedule :
        a != b, a.worker_id = b.worker_id =>
            no_overlap a b tasks
    all t in tasks, all a in schedule :
        a.task_id = t.id =>
            a.start + t.duration <= t.deadline
```

### Unicode

```evident
type Task = {
    id       ∈ Nat
    duration ∈ Nat
    deadline ∈ Nat
}

type Worker = {
    id              ∈ Nat
    available_from  ∈ Nat
    available_until ∈ Nat
}

claim sorted[T ∈ Ordered] : List T → Prop

evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]

claim assignment_fits : List Worker → List Task → Assignment → Prop

evident assignment_fits workers tasks a
    ∃ w ∈ workers : w.id = a.worker_id
    ∃ t ∈ tasks   : t.id = a.task_id
    a.start ≥ w.available_from
    a.start + t.duration ≤ w.available_until

claim valid_schedule : List Task → List Worker → Schedule → Prop

evident valid_schedule tasks workers schedule
    ∀ t ∈ tasks :
        ∃ a ∈ schedule : a.task_id = t.id
    ∀ a ∈ schedule :
        assignment_fits workers tasks a
    ∀ a ∈ schedule, ∀ b ∈ schedule :
        a ≠ b, a.worker_id = b.worker_id ⇒
            no_overlap a b tasks
    ∀ t ∈ tasks, ∀ a ∈ schedule :
        a.task_id = t.id ⇒
            a.start + t.duration ≤ t.deadline
```

The Unicode version reads like a textbook definition of a valid schedule. That is the goal.

---

## Scoping of existential witnesses

When you write `∃ w ∈ workers : w.id = a.worker_id`, the name `w` is introduced as
the **witness** to the existential claim. It is available for the remainder of the
body block:

```evident
evident assignment_fits workers tasks a
    ∃ w ∈ workers : w.id = a.worker_id   -- w introduced here
    ∃ t ∈ tasks   : t.id = a.task_id     -- t introduced here
    a.start ≥ w.available_from            -- w used here
    a.start + t.duration ≤ w.available_until  -- both used here
```

`w` is not an assigned variable. It is the witness the solver must produce to establish
the existential. If no such witness exists in `workers`, the whole claim fails to be
established. If multiple witnesses exist, the claim is established by any of them
(the solver may produce one or enumerate all, depending on the determinism annotation).

This is the constructive reading of `∃`: to establish `∃ x ∈ S : P(x)`, the solver
must produce a concrete `x` from `S` satisfying `P`. That concrete value is the evidence
term for the existential — it is part of the derivation tree.

---

## Why both forms and not just Unicode

Unicode is the right display format. ASCII is the right input format. The two should
not be in conflict:

- **Unicode for reading**: code shared, reviewed, documented, or presented looks like
  mathematics. The claim declarations read as formal specifications.

- **ASCII for writing**: a standard keyboard can produce all ASCII forms without special
  input methods. No one should be blocked from writing Evident because they lack a
  Unicode input method.

- **Auto-replace bridges them**: as you type `\in`, the editor immediately replaces it
  with `∈`. The file saves Unicode. The programmer never has to think about which form
  to use — they type the ASCII shortcut and see the Unicode result.

This is already the standard workflow in Agda, Lean, and Coq. Evident inherits it.
