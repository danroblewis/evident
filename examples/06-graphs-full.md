# Example 6: Graph Theory — Full Program

```evident
-- ─────────────────────────────────────────
-- The graph (asserted facts)
-- ─────────────────────────────────────────

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


-- ─────────────────────────────────────────
-- Adjacency
-- ─────────────────────────────────────────

claim adjacent : Nat → Nat → semidet

evident adjacent a b
    edge a b ∨ edge b a


-- ─────────────────────────────────────────
-- Reachability — transitive closure
-- ─────────────────────────────────────────

claim reachable : Nat → Nat → semidet

evident reachable a a

evident reachable a c
    adjacent a _b
    reachable _b c


-- ─────────────────────────────────────────
-- Paths — explicit node sequences
-- ─────────────────────────────────────────

claim path_between : Nat → Nat → List Nat → semidet

evident path_between a a [a]

evident path_between a c [a | rest]
    adjacent a _b
    path_between _b c rest


-- ─────────────────────────────────────────
-- Path length
-- ─────────────────────────────────────────

claim path_length : List Nat → Nat → det

evident path_length [_] 0

evident path_length [_ | rest] n
    _n0 = path_length rest
    n   = _n0 + 1


-- ─────────────────────────────────────────
-- Shortest path and distance
-- ─────────────────────────────────────────

claim shortest_path_between : Nat → Nat → List Nat → semidet

evident shortest_path_between a b path
    path_between a b path
    _len = path_length path
    ∀ alt ∈ { p | path_between a b p } : path_length alt ≥ _len

claim distance : Nat → Nat → Nat → det

evident distance a b d
    _path ∈ { p | shortest_path_between a b p }
    d = path_length _path


-- ─────────────────────────────────────────
-- Cycle detection
-- ─────────────────────────────────────────

claim in_cycle : Nat → semidet

evident in_cycle n
    adjacent n _next
    reachable _next n

claim acyclic : Prop

evident acyclic
    ∀ n ∈ nodes : ¬ in_cycle n


-- ─────────────────────────────────────────
-- Connectivity
-- ─────────────────────────────────────────

claim strongly_connected : Prop

evident strongly_connected
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b

claim weakly_connected : Prop

evident weakly_connected
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b ∨ reachable b a


-- ─────────────────────────────────────────
-- Trees
-- ─────────────────────────────────────────

claim tree : Prop

evident tree
    weakly_connected
    acyclic


-- ─────────────────────────────────────────
-- Queries
-- ─────────────────────────────────────────

? reachable 1 5         -- Yes ✓
? reachable 5 1         -- No
? reachable 3 2         -- No

? ∃ p : path_between 1 5 p
-- [1, 2, 4, 5]
-- [1, 3, 4, 5]

? d = distance 1 5      -- d = 3
? d = distance 1 4      -- d = 2
? d = distance 5 1      -- not evident

? acyclic               -- Yes ✓
? strongly_connected    -- No
? weakly_connected      -- Yes ✓
? tree                  -- No  (not a tree — node 4 has two parents)

? ∃ a, b ∈ nodes : reachable a b, ¬ reachable b a
-- asymmetric pairs: (1,2), (1,3), (1,4), (1,5), (2,4), (2,5), (3,4), (3,5), (4,5)
```
