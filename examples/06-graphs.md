# Example 6: Graph Theory — Relations, Paths, and Structure

Graphs are naturally relational. Nodes and edges are facts in the evidence base.
Properties like reachability, connectivity, and acyclicity are claims over those facts.
No algorithms — just sets and membership conditions.

---

## The graph as asserted facts

```evident
assert node 1
assert node 2
assert node 3
assert node 4
assert node 5

assert edge 1 2
assert edge 1 3
assert edge 2 4
assert edge 3 4
assert edge 4 5
```

A directed graph: `1 → 2 → 4 → 5` and `1 → 3 → 4 → 5`.

---

## Adjacency — the base relation

```evident
claim adjacent : Nat → Nat → semidet

evident adjacent a b
    edge a b
```

`adjacent` names the set of pairs directly connected by an edge.
Solution space: exactly the asserted edges — 5 pairs.

For an undirected graph, add:

```evident
evident adjacent a b
    edge b a    -- symmetric: edges work in both directions
```

---

## Reachability — the transitive closure

`reachable a b` is established when there is any path from a to b.

```evident
claim reachable : Nat → Nat → semidet

evident reachable a a              -- every node reaches itself

evident reachable a c
    adjacent a b
    reachable b c
```

Solution space: all pairs `(a, b)` connected by a path of any length.
With the graph above:

```evident
? reachable 1 5     -- Yes ✓  (1→2→4→5 or 1→3→4→5)
? reachable 5 1     -- No   (no edges go backwards)
? reachable 3 2     -- No   (3→4→5, never reaches 2)
? reachable 1 1     -- Yes ✓ (reflexive)
```

The solution space of `reachable a b` is strictly larger than `adjacent a b` —
it includes all transitive connections.

---

## Paths — explicit sequences

`path_between a b p` is established when `p` is a list of nodes forming a valid
path from a to b.

```evident
claim path_between : Nat → Nat → List Nat → semidet

evident path_between a a [a]       -- trivial path: just the node itself

evident path_between a c [a | rest]
    adjacent a b
    path_between b c rest
```

```evident
? ∃ p : path_between 1 5 p
-- p = [1, 2, 4, 5]   (one valid path)
-- p = [1, 3, 4, 5]   (another — multiple solutions)
```

`path_between` is `nondet` — multiple paths may exist between two nodes.

---

## Path length — a `det` claim

The length of a path is uniquely determined by the path. Use `= claim` form:

```evident
claim path_length : List Nat → Nat → det

evident path_length [_] 0           -- single node: length 0 (no edges traversed)

evident path_length [_ | rest] n
    _n0 = path_length rest
    n   = _n0 + 1
```

`_n0` is body-internal scaffolding — the length of the tail. No domain meaning,
just the recursive subresult. The solver finds both simultaneously.

```evident
? n = path_length [1, 2, 4, 5]    -- n = 3  ✓
? n = path_length [1, 3, 4, 5]    -- n = 3  ✓
? n = path_length [1]              -- n = 0  ✓
```

---

## Shortest path — intersection of path and length constraints

`shortest_path_between a b p` names the set of shortest paths from a to b.

```evident
claim shortest_path_between : Nat → Nat → List Nat → semidet

evident shortest_path_between a b path
    path_between a b path
    _len = path_length path
    ∀ alt ∈ paths_between a b : path_length alt ≥ _len
```

Where `paths_between` collects all paths:

```evident
claim paths_between : Nat → Nat → Set (List Nat) → det

evident paths_between a b ps
    ps = { p | path_between a b p }
```

```evident
? ∃ p : shortest_path_between 1 5 p
-- p = [1, 2, 4, 5]  or  [1, 3, 4, 5]  (both length 3, both shortest)
```

The solution space of `shortest_path_between` is a subset of `path_between` —
the paths of minimal length. If there are multiple shortest paths, all are in the set.

---

## Distance — the length of the shortest path

Since there is a unique shortest distance (even if multiple shortest paths exist),
`distance` is `det`:

```evident
claim distance : Nat → Nat → Nat → det

evident distance a b d
    ∃ p : shortest_path_between a b p
    d = path_length p
```

```evident
? d = distance 1 5    -- d = 3  ✓
? d = distance 1 4    -- d = 2  ✓
? d = distance 1 1    -- d = 0  ✓
? d = distance 5 1    -- not evident (5 cannot reach 1)
```

---

## Connectivity — a property of the whole graph

```evident
claim strongly_connected : Prop

evident strongly_connected
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b
```

```evident
? strongly_connected
-- Not evident — 5 cannot reach 1, 2, 3, or 4.
```

The solution space of `strongly_connected` contains either the whole graph (if
every node reaches every other) or nothing. It is a 0-arity proposition — its
solution space has cardinality 0 or 1.

---

## Cycle detection — absence of self-reachability via edges

A node is in a cycle if it can reach itself via at least one edge.

```evident
claim in_cycle : Nat → semidet

evident in_cycle n
    adjacent n _next
    reachable _next n
```

`_next` is a body-internal witness — some neighbor of n from which n is reachable.

```evident
claim acyclic : Prop

evident acyclic
    ∀ n ∈ nodes : ¬ in_cycle n
```

```evident
? in_cycle 1    -- Not evident (no path from any neighbor back to 1)
? acyclic       -- Evident ✓  (the graph above is a DAG)
```

---

## Trees — connected acyclic graphs

A tree is a graph that is both connected (in the undirected sense) and acyclic,
with exactly n-1 edges for n nodes.

```evident
claim tree : Prop

evident tree
    _n      = node_count
    _e      = edge_count
    _e      = _n - 1          -- necessary condition: n-1 edges
    acyclic
    weakly_connected           -- connected ignoring edge direction
```

```evident
claim weakly_connected : Prop

evident weakly_connected
    ∀ a ∈ nodes, ∀ b ∈ nodes : undirected_reachable a b

claim undirected_reachable : Nat → Nat → semidet

evident undirected_reachable a a
evident undirected_reachable a c
    adjacent a b ∨ adjacent b a    -- ignore direction
    undirected_reachable b c
```

---

## Solution space summary

| Claim | Space |
|---|---|
| `adjacent a b` | The asserted edges — 5 pairs |
| `reachable a b` | All connected pairs — 11 pairs (with reflexive) |
| `path_between 1 5 p` | All paths from 1 to 5 — 2 paths |
| `shortest_path_between 1 5 p` | The 2 shortest paths |
| `d = distance 1 5` | The single value 3 |
| `in_cycle n` | Empty (this graph is acyclic) |
| `strongly_connected` | Empty (this graph is not strongly connected) |

Each claim is a slice of the product space of its argument types. The solver finds
members. The structure of the graph shapes the solution space without any graph
algorithm being written.
