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
    edge a b ∨ edge b a     -- either direction; genuine OR, not case analysis


-- ─────────────────────────────────────────
-- Reachability — reflexive transitive closure
--
-- No base case. No recursion. Two closure properties:
-- every node reaches itself; reachability extends along edges.
-- The solver derives the full transitive closure from these.
-- ─────────────────────────────────────────

claim reachable : Nat → Nat → semidet

node n         ⇒ reachable n n
reachable a b, adjacent b c ⇒ reachable a c


-- ─────────────────────────────────────────
-- Paths — a path is a non-empty list of nodes
-- where every consecutive pair is adjacent.
--
-- No base case needed: if path = [a], there are no consecutive
-- pairs, so the ∀ holds vacuously.
-- ─────────────────────────────────────────

claim path_between : Nat → Nat → List Nat → semidet

evident path_between a c path
    first_of path = a
    last_of  path = c
    ∀ (p, q) ∈ each_consecutive path : adjacent p q


-- ─────────────────────────────────────────
-- Path length — edges traversed = nodes - 1
-- ─────────────────────────────────────────

claim path_length : List Nat → Nat → det

evident path_length path n
    _len = length path
    n    = _len - 1


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
-- (1,2), (1,3), (1,4), (1,5), (2,4), (2,5), (3,4), (3,5), (4,5)
```
