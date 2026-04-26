# Multisets and Sequences in Evident

Evident's primary data structure is the set: unordered, no duplicates, membership-based. But real programs frequently need more. A bag of groceries can contain two identical apples. A DNA strand has an order that matters. A sorting problem's output must contain the same elements as its input, counted with multiplicity.

This document covers when sets are not enough, what replaces them, and how the three structures — sets, multisets, and sequences — relate to each other and to Evident's constraint model.

---

## 1. What is a Multiset (Bag)?

A **multiset** (also called a **bag**) is a collection where elements can appear more than once. The set `{1, 2, 3}` contains three distinct elements. The multiset `{1, 1, 2, 3}` contains four elements, where `1` has **multiplicity 2**.

Formally, a multiset over a type `T` is a function from elements to natural numbers:

```
M : T → ℕ
```

where `M(x)` is the number of times `x` appears. An ordinary set is the special case where every element has multiplicity 0 or 1.

### When does a program need a multiset rather than a set?

A set suffices when you only care about **membership** — whether something is present or absent. A multiset is needed when you care about **how many times** something is present.

| Situation | Structure | Why |
|---|---|---|
| Unique user IDs in a system | Set | Each ID appears exactly once by definition |
| Items in a shopping cart | Multiset | "3 copies of Book A" is meaningful |
| Words in a document | Multiset | Word frequency matters for search |
| Conference session assignments | Set | Each talk is assigned to exactly one slot |
| Dice rolled in a board game | Multiset | Rolling two 3s is different from rolling a 3 and a 4 |
| Poker hand | Multiset | "Three of a kind" requires multiplicity |
| Input to a sort | Multiset | The output must contain the same elements with the same counts |
| DNA sequence | Sequence | Order is the biological information |

The boundary between "set" and "multiset" is whether counting copies is meaningful. If you would never ask "how many times does X appear?", you want a set.

---

## 2. Multiset Operations

### Multiplicity function

The fundamental operation on a multiset is asking how many times an element appears:

```
mult(x, M)  →  ℕ
```

For a set, `mult(x, S)` is always 0 or 1. For a multiset, it can be any natural number.

In Evident, this is the `occurrences` claim from the sorting example:

```evident
claim occurrences[T ∈ Eq] : T → List T → Nat → det

evident occurrences x [] 0

evident occurrences x [x | rest] n
    ∃ n0 : occurrences x rest n0
    n = n0 + 1

evident occurrences x [y | rest] n when x ≠ y
    occurrences x rest n
```

`occurrences x M n` is established when `x` appears exactly `n` times in `M`.

### Multiset union (sum of multiplicities)

`M ⊎ N` adds multiplicities:

```
mult(x, M ⊎ N) = mult(x, M) + mult(x, N)
```

Merging two shopping carts: if Alice has 2 apples and Bob has 3 apples, the merged cart has 5 apples. This differs from set union, which would give {apple} — just one apple, membership only.

### Multiset intersection (minimum multiplicities)

`M ∩ N` takes the minimum multiplicity at each element:

```
mult(x, M ∩ N) = min(mult(x, M), mult(x, N))
```

"What do both orders have in common, counting copies?" If order A has 3 apples and order B has 2 apples, the intersection has 2 apples.

### Multiset difference (subtract multiplicities, floor at 0)

`M ∖ N` subtracts multiplicities but does not go negative:

```
mult(x, M ∖ N) = max(0, mult(x, M) − mult(x, N))
```

"What's left in the bag after removing these items?" Remove 3 apples from a bag with 2 apples: 0 apples remain (not −1).

### Size of a multiset

The **total size** (cardinality counting multiplicity) is the sum of all multiplicities:

```
|M| = Σ_{x ∈ support(M)} mult(x, M)
```

The **support** is the underlying set: elements with multiplicity ≥ 1.

### Converting multiset to set

Drop multiplicities — keep each element at most once:

```
set_of(M) = { x | mult(x, M) ≥ 1 }
```

`set_of({1, 1, 2, 3})` = `{1, 2, 3}`. Information is lost in this direction.

### Multiset equality

Two multisets are equal when they have the same multiplicity for every element:

```
M = N  ⟺  ∀ x : mult(x, M) = mult(x, N)
```

`{1, 2, 2}` = `{2, 1, 2}` as multisets (order is irrelevant). `{1, 2, 2}` ≠ `{1, 2}` as multisets (different counts of 2).

---

## 3. What is a Sequence?

A **sequence** is an ordered collection where position matters and repetition is allowed. It is indexed by natural numbers: `xs[0]`, `xs[1]`, ..., `xs[n-1]`.

The sequence `[1, 2, 2]` is not the same as `[2, 1, 2]` — the same elements in a different order are different sequences. This is the key distinction from multisets.

Sequences are sometimes called **lists** (when the focus is on recursive structure) or **tuples** (when the length is fixed and elements may have different types). In Evident, `List T` is a sequence of elements of type `T`.

### When is order the information?

| Situation | Structure | Why |
|---|---|---|
| Steps in a recipe | Sequence | "Mix, then bake" is not "Bake, then mix" |
| DNA strand | Sequence | ATCG ≠ CGTA |
| HTTP request log | Sequence | Request ordering reveals usage patterns |
| The output of a sort | Sequence | Sorted means elements are in a specific order |
| A queue of waiting tasks | Sequence | First-in-first-out depends on position |
| Program bytecode | Sequence | Instruction order is the computation |
| Keyboard input | Sequence | Letters typed in order form words |

A sequence is needed whenever the identity of a collection is determined by what appears and where, not merely by what appears.

---

## 4. Sequence Operations

### Head and tail (recursive decomposition)

```evident
head [x | _] = x      -- first element
tail [_ | xs] = xs    -- all but first
```

In pattern matching:

```evident
evident first_element [x | _] x
evident last_element  [x] x
evident last_element  [_ | rest] x
    last_element rest x
```

`init` is all but the last element; `last` is the final element.

### Concatenation

```
xs ++ ys
```

Joins two sequences end-to-end. The result has length `|xs| + |ys|`.

```evident
claim concat[T] : List T → List T → List T → det

evident concat [] ys ys
evident concat [x | xs] ys [x | zs]
    concat xs ys zs
```

### Indexing

```
xs[i]
```

The element at position `i` (0-indexed). Only defined for `0 ≤ i < length xs`. Unlike set membership, indexing gives you a specific element at a specific position.

### Slicing

```
xs[i..j]
```

The subsequence from position `i` up to (but not including) `j`. This is a sequence in its own right.

### Consecutive pairs and windows

```evident
-- All consecutive pairs in a sequence
consecutive_pairs xs = [(xs[i], xs[i+1]) | i ∈ 0..length xs - 2]
```

This pattern appears naturally in `sorted`: a sorted sequence is one where every consecutive pair is in non-decreasing order.

```evident
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

The `[a, b | rest]` pattern matches a sequence whose first two elements are `a` and `b` — exactly the consecutive-pair check.

### Prefix, suffix, infix

A sequence `ys` is a **prefix** of `xs` when `xs = ys ++ zs` for some `zs`.
A sequence `ys` is a **suffix** of `xs` when `xs = zs ++ ys` for some `zs`.
A sequence `ys` is an **infix** (contiguous subsequence) of `xs` when `xs = pre ++ ys ++ suf`.

These are naturally expressible as existential claims:

```evident
claim prefix_of[T] : List T → List T → Prop

evident prefix_of [] _xs
evident prefix_of [x | ys] [x | xs]
    prefix_of ys xs
```

### Sorting a sequence

Sorting produces a new sequence from the same multiset, in non-decreasing order. The key claim is `permutation`: the output is a rearrangement of the input with the same element counts.

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | sorted ys, permutation xs ys }
```

### Zip and unzip

`zip` pairs corresponding elements from two sequences:

```
zip [1, 2, 3] ["a", "b", "c"] = [(1,"a"), (2,"b"), (3,"c")]
```

`unzip` is the inverse. Both require the sequences to have the same length.

```evident
claim zip[A, B] : List A → List B → List (A, B) → det

evident zip [] [] []
evident zip [a | as] [b | bs] [(a, b) | rest]
    zip as bs rest
```

### Take and drop

```
take n xs   -- the first n elements
drop n xs   -- everything after the first n elements
```

`take 3 [1, 2, 3, 4, 5]` = `[1, 2, 3]`.
`drop 3 [1, 2, 3, 4, 5]` = `[4, 5]`.

Together: `xs = take n xs ++ drop n xs`.

### takeWhile and dropWhile

```
takeWhile P xs  -- elements from the front while P holds
dropWhile P xs  -- elements after the initial P-satisfying prefix
```

`takeWhile (< 4) [1, 2, 3, 4, 5]` = `[1, 2, 3]`.

---

## 5. Converting Between Types

The three structures form a hierarchy of information:

```
Sequence  →  Multiset  →  Set
  (most info)           (least info)
```

Going right loses information. Going left requires choices.

### Set → Multiset

Trivially: every element has multiplicity 1. No information is gained; the multiset is just a set with explicit counts of 1.

```
multiset_of({a, b, c}) = {a:1, b:1, c:1}
```

### Multiset → Set

Drop multiplicities. Information lost: how many copies of each element existed.

```
set_of({1:2, 2:1, 3:1}) = {1, 2, 3}
```

### Sequence → Multiset

Forget the order; keep the counts.

```
multiset_of([1, 2, 1, 3]) = {1:2, 2:1, 3:1}
```

Information lost: which positions held which elements. Two sequences with the same multiset are permutations of each other.

### Sequence → Set

Forget both order and multiplicity.

```
set_of([1, 2, 1, 3]) = {1, 2, 3}
```

### Multiset → Sequence

Requires choosing an order. Two natural choices:
- **Sorted order**: the unique non-decreasing sequence from the multiset (requires `T ∈ Ordered`)
- **Arbitrary order**: any sequence from the multiset (nondet — many valid answers)

```evident
-- The sorted sequence for a given multiset
type SequenceOf M = { xs | permutation xs (to_list M), sorted xs }
```

### Set → Sequence

Requires choosing both an order and one representative per element (trivial since multiplicities are all 1). For ordered types, sorting is the canonical choice.

```evident
-- All elements of a set in sorted order
type SortedSequenceOf S = { xs | set_of xs = S, sorted xs }
```

---

## 6. The `permutation` Constraint

Two sequences are **permutations** of each other when they are the same multiset in possibly different orders:

```evident
claim permutation[T ∈ Eq] : List T → List T → Prop

evident permutation xs ys
    length xs = length ys
    ∀ x ∈ xs : occurrences x xs = occurrences x ys
```

`permutation xs ys` is the bridge between sequences and multisets. It says: "these two sequences, ignoring order, are the same collection."

This is the central constraint in sorting: the output of a sort must be a permutation of the input.

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | sorted ys, permutation xs ys }
```

`permutation` also appears in any problem where you need to reassign elements without losing or gaining any:
- Assigning workers to tasks (each worker used exactly once)
- Scheduling talks into slots (each talk scheduled exactly once)
- Seating guests at tables (each guest appears exactly once)

The `permutation` constraint is the formal statement that a rearrangement preserves all element counts. Without it, you might sort `[3, 1, 2]` into `[1, 1, 1]` — sorted, correct length, but wrong elements.

---

## 7. Multiset Equality vs. Sequence Equality

This is the most practically important distinction.

| Expression | As multisets | As sequences |
|---|---|---|
| `{1, 2, 2}` vs `{2, 1, 2}` | **Equal** (same counts) | N/A (multisets have no order) |
| `[1, 2, 2]` vs `[2, 1, 2]` | Represent equal multisets | **Not equal** (different order) |
| `[1, 2, 2]` vs `[1, 2]` | Not equal (different count of 2) | Not equal (different length) |

When you say "the input and output of a sort are the same," you mean they are equal as **multisets** — same elements, same counts, order does not matter. When you say "the output is the sorted version," you mean it is a specific **sequence** in non-decreasing order.

The `sorted` claim constrains the sequence structure. The `permutation` claim constrains the multiset identity. Both are necessary; neither alone is sufficient.

```evident
-- sorted [1, 1, 2, 3]: true (non-decreasing sequence)
-- sorted [1, 2, 1, 3]: false (not non-decreasing)

-- permutation [3, 1, 2] [1, 2, 3]: true (same multiset)
-- permutation [3, 1, 2] [1, 1, 2]: false (different count of 3)
```

---

## 8. Ordered Sets

An **ordered set** is a set equipped with a total ordering — no duplicates, but with a defined order for every pair of elements. It combines the no-duplication guarantee of a set with the ability to navigate by position.

Ordered sets support:
- `min`, `max`: the smallest and largest elements
- `predecessor(x)`: the largest element smaller than `x`
- `successor(x)`: the smallest element larger than `x`
- Range queries: `{ x ∈ S | a ≤ x ≤ b }`

Ordered sets are what you get when you sort a set: a sequence with no repeated elements. They are more structured than a plain set (you can navigate) but less expressive than a multiset (no repetition allowed) and less than a sequence (no notion of position beyond the ordering).

```
Plain set   →   Ordered set   →   Sorted sequence (no duplicates)
```

---

## 9. Practical Examples: Choosing the Right Structure

### Sorting

**Input**: a sequence (order matters — it's what the user gave you)
**Output**: a sequence (sorted means elements are in a specific order)
**Constraint linking them**: they are the same multiset (same elements, same counts)

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | sorted ys, permutation xs ys }
```

The input and output are both sequences. The connecting constraint (`permutation`) is multiset equality. The sorting property (`sorted`) is a sequence property.

If the output were a set, you could not preserve duplicates: `sort [3, 1, 1, 2]` must produce `[1, 1, 2, 3]`, not `{1, 2, 3}`.
If the output were a multiset, you could not define "sorted" — order is required.

### Scheduling

**Is a schedule a set or a sequence?**

A schedule like the one in Example 9 (`valid_conference`) is naturally a **set of assignments**:

```evident
schedule ∈ Set Assignment
```

Each assignment is a unique triple `(talk, room, slot)`. There is no inherent ordering among assignments — we don't care that talk A is listed before talk B in the schedule, only that both are present. Set semantics are correct here.

The schedule is not a multiset because each talk appears exactly once (the `all_talks_scheduled` constraint enforces this). It is not a sequence because the order in which assignments are listed is meaningless.

However, if the schedule had time-ordered events and you needed to reason about "what happens next?", you would want a sequence indexed by time. The right structure depends on what questions you ask.

**The N-Queens problem** is similar: the solution is most naturally represented as a sequence indexed by column, where `queens[col]` = the row of the queen in that column. This is a sequence of length N. Alternatively, it is a set of `(col, row)` pairs where columns are all different and rows are all different. Both work; the sequence is more natural because column is the natural index.

### DNA Sequences

**Structure**: Sequence over `{A, T, C, G}`
**Why not a set?**: `ATCG` and `CGTA` are completely different genetic sequences.
**Why not a multiset?**: The multiset `{A:1, T:1, C:1, G:1}` says nothing about the gene — order carries all the biological information.

The number of times each nucleotide appears (the multiset content) is relevant for some analyses (GC content), but the primary biological identity of a DNA strand is its sequence.

### A bag of items in a store inventory

**Structure**: Multiset over `Item`
**Why not a set?**: The inventory contains 47 copies of "Widget A". That count matters for fulfillment.
**Why not a sequence?**: The physical order of items in the warehouse is irrelevant to what items exist.

```evident
type Inventory = { item : Item, quantity : Nat }
```

Or, modeled directly as a multiset:

```evident
type Inventory = Multiset Item

-- Checking availability
claim can_fulfill : Order → Inventory → Prop

evident can_fulfill order inventory
    ∀ line ∈ order :
        occurrences line.item inventory ≥ line.quantity
```

### A queue of tasks

**Structure**: Sequence (with FIFO discipline)
**Why not a set?**: Order determines which task is processed next.
**Why not a multiset?**: The same task at position 3 vs. position 7 is a different situation.

Processing a queue consumes the head and returns the tail:

```evident
evident process_queue [] done
evident process_queue [task | rest] done
    execute task
    process_queue rest done
```

### Conference talks by track

A track's talks are a **set**: unordered, each talk distinct.
The full conference schedule is a **set of assignments**: each `(talk, room, slot)` triple appears once.
The sequence of slots in a day is a **sequence**: 9am, 10am, 11am... ordering determines what comes first.

This mixture — sets for groupings, sequences for time — is typical in real scheduling problems.

---

## 10. Summary: When to Use Each Structure

| Question to ask | If yes, use |
|---|---|
| Does order matter? | Sequence |
| Can elements repeat, and do copies need counting? | Multiset |
| Do you only care whether something is present or absent? | Set |
| Do you need min/max/predecessor/successor? | Ordered set |
| Do you need to relate "same elements, different order"? | `permutation` constraint |

More precisely:

**Use a set when**:
- Identity is determined by membership alone
- Duplicates cannot arise or are meaningless by construction
- You ask "is X in this collection?" not "how many Xs are in this collection?"
- Examples: conference rooms, registered users, valid HTTP methods

**Use a multiset when**:
- Elements can legitimately repeat
- You care about counts, not just presence
- You need to relate a sequence to its "bag content" (e.g., sorting)
- Examples: shopping cart, word frequencies, dice outcomes, poker hands

**Use a sequence when**:
- Position or order is part of the information
- You need indexing: "what is the third element?"
- You need prefix/suffix structure or consecutive-element relationships
- The problem involves "before" and "after"
- Examples: sorted output, DNA, event logs, queues, program instructions

**Use an ordered set when**:
- No duplicates (set property)
- But you need navigation: min, max, predecessor, successor, range queries
- Examples: timestamps of distinct events, sorted vocabulary of a language

---

## 11. Implications for Evident's Type System

Evident currently uses `List T` as its sequence type and `Set T` as its set type. The design questions this research surfaces:

**Should `Multiset T` be a built-in type?**

The multiset is currently simulated via `List T` with the `occurrences` claim as a bridge. This works but requires the programmer to manually manage the multiset-sequence distinction. A first-class `Multiset T` type would:
- Make the distinction syntactically clear
- Allow the solver to use efficient multiset propagation algorithms
- Allow `mult(x, M)` as a built-in operation rather than a derived claim
- Make `permutation` definable as `multiset_of xs = multiset_of ys`

**The `permutation` claim is foundational**

`permutation xs ys` is the constraint connecting sequences to multisets. It appears in sorting, assignment problems, and anywhere elements must be "the same but possibly rearranged." It deserves first-class treatment — potentially a built-in with an efficient propagation algorithm (similar to `alldifferent` in MiniZinc).

**`occurrences` generalizes to multiset membership**

Multiset membership is not a Boolean: it is a count. `x ∈ M` for a multiset should return a natural number (or perhaps `x ∈ M` checks membership and a separate `mult x M` gives the count). The design must decide whether `∈` for multisets means "count ≥ 1" (consistent with set intuition) or "this is not the right notation for multisets."

**Sequence slicing and window operations**

The consecutive-pairs pattern — used in `sorted` and in many sequence problems — suggests that `xs[i..j]` slicing and `zip xs (tail xs)` (consecutive pairs) should be expressible without defining helper claims each time.

**Type inference for the right collection kind**

When a programmer writes `{ a ∈ schedule | a.slot = slot }`, Evident should infer that this is a set comprehension (no duplicates possible if `schedule` is a set) rather than a multiset or sequence comprehension. The kind of the source collection should propagate to the kind of the result.
