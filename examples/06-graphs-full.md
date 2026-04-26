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

claim adjacent a, b ∈ Nat : Prop
    edge a b ∨ edge b a


-- ─────────────────────────────────────────
-- Reachability — reflexive transitive closure
-- Defined by forward implication rules, not a body.
-- ─────────────────────────────────────────

claim reachable a, b ∈ Nat : Prop

node n                        ⇒ reachable n n
reachable a b, adjacent b c   ⇒ reachable a c


-- ─────────────────────────────────────────
-- Paths
-- ─────────────────────────────────────────

claim path_between a, c ∈ Nat, path ∈ List Nat : Prop
    first_of path = a
    last_of  path = c
    ∀ (p, q) ∈ each_consecutive path : adjacent p q


-- ─────────────────────────────────────────
-- Path length — edges traversed = nodes - 1
-- ─────────────────────────────────────────

claim path_length path ∈ List Nat : Nat det
    _len = length path
    _len - 1


-- ─────────────────────────────────────────
-- Shortest path and distance
-- ─────────────────────────────────────────

claim shortest_path_between a, b ∈ Nat, path ∈ List Nat : Prop
    path_between a b path
    _len = path_length path
    ∀ alt ∈ { p | path_between a b p } : path_length alt ≥ _len

claim distance a, b ∈ Nat : Nat det
    _path ∈ { p | shortest_path_between a b p }
    path_length _path


-- ─────────────────────────────────────────
-- Cycle detection
-- ─────────────────────────────────────────

claim in_cycle n ∈ Nat : Prop
    adjacent n _next
    reachable _next n

claim acyclic : Prop
    ∀ n ∈ nodes : ¬ in_cycle n


-- ─────────────────────────────────────────
-- Connectivity
-- ─────────────────────────────────────────

claim strongly_connected : Prop
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b

claim weakly_connected : Prop
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b ∨ reachable b a


-- ─────────────────────────────────────────
-- Trees
-- ─────────────────────────────────────────

claim tree : Prop
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
-- (1,2), (1,3), (1,4), (1,5), (2,4), (2,5), (3,4), (3,5), (4,5)
```
