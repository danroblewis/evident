# Example 14: Graph Hierarchy — Data Structures as Constrained Graphs

Every sequential data structure is a graph with progressively tighter constraints.
Each level adds exactly one new constraint to the previous.

```
graph → dag → tree → linked_list → sequence
```

No `Nat` required. No index arithmetic. Positions emerge from structure.

---

## Foundation: reachability

```evident
-- reachable: there is a directed path from a to b via at least one edge.
-- Self-referential body — the solver uses tabling to avoid infinite loops.

claim reachable
    edges ⊆ T × T
    a     ∈ T
    b     ∈ T
    (a, b) ∈ edges ∨ ∃ c ∈ T : (a, c) ∈ edges, reachable edges c b
```

---

## Level 1: Graph

A set of nodes and edges, where every edge connects two nodes.

```evident
claim graph
    nodes ⊆ T
    edges ⊆ T × T
    ∀ (x, y) ∈ edges : x ∈ nodes, y ∈ nodes
```

---

## Level 2: DAG — add acyclicity

```evident
claim dag
    ..graph
    ∀ x ∈ nodes : ¬ reachable edges x x
```

---

## Level 3: Tree — add single root and single parent

```evident
claim tree
    ..dag
    root ∈ nodes
    ∀ (x, y) ∈ edges : y ≠ root                                  -- root has no predecessors
    ∀ x ∈ nodes : x ≠ root ⇒ exactly 1 { (y, x) | (y, x) ∈ edges } -- one parent per non-root
    ∀ x ∈ nodes : reachable edges root x                          -- all reachable from root
```

---

## Level 4: Linked List — add at most one child

```evident
claim linked_list
    ..tree
    last ∈ nodes
    ∀ x ∈ nodes : at_most 1 { y ∈ nodes | (x, y) ∈ edges }      -- at most one child
    ∀ (x, y) ∈ edges : x ≠ last                                   -- last has no successors
```

Now the structure is linear: `root → n1 → n2 → ... → last`.
`edges` contains exactly the consecutive pairs.

---

## Consequences — everything falls out of the structure

```evident
-- consecutive pairs ARE the edges (no derivation needed)
claim in_order
    T ∈ Ordered
    ..linked_list
    ∀ (a, b) ∈ edges : a ≤ b

claim length_of
    ..linked_list
    n ∈ Nat
    n = |nodes|

-- first and last are the root and last of the linked list
-- (already named root and last in linked_list — they flow by names-match)
```

---

## Level 5: Sequence — positions emerge from distance

If you need explicit indices, they come from counting hops from root.
No `Nat` primitive required — Nat emerges from counting.

```evident
claim position_of
    ..linked_list
    x ∈ nodes
    i ∈ Nat
    i = |{ y ∈ nodes | reachable edges root y, ¬ reachable edges y x }|
```

`position_of` asks: how many nodes are strictly before x? That count is x's index.
Position 0 is the root. Position n-1 is the last.

---

## The full hierarchy as layered constraints

```
claim graph
    nodes, edges
    edges ⊆ nodes × nodes

        +  ∀ x : ¬ cycle through x
claim dag

        +  unique root
        +  every node has one parent
        +  all reachable from root
claim tree

        +  every node has at most one child
        +  unique last node
claim linked_list

        +  position = hop count from root
claim sequence
```

Each structure is the previous structure plus one line.
`Nat`, array indexing, and consecutive pairs are not primitives —
they are derived from graph structure when needed.

---

## Sorted list — composing the hierarchy

```evident
claim sorted_list
    T ∈ Ordered
    ..linked_list
    in_order
```

Two lines. `linked_list` gives the sequential structure.
`in_order` asserts the ordering. Both flow via names-match.
`edges` is simultaneously the graph structure and the consecutive pairs.
