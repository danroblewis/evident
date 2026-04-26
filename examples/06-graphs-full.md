# Example 6: Graph Theory — Full Program

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

claim nodes : Set Nat det
    { n | node n }


claim adjacent a, b ∈ Nat : Prop
    edge a b ∨ edge b a


claim reachable a, b ∈ Nat : Prop

node n                      ⇒ reachable n n
reachable a b, adjacent b c ⇒ reachable a c


claim path_between a, c ∈ Nat, path ∈ List Nat : Prop
    first_of path = a
    last_of  path = c
    ∀ (p, q) ∈ each_consecutive path : adjacent p q


claim path_length path ∈ List Nat : Nat det
    _len = length path
    _len - 1


claim shortest_path_between a, b ∈ Nat, path ∈ List Nat : Prop
    path_between a b path
    _len = path_length path
    ∀ alt ∈ { p | path_between a b p } : path_length alt ≥ _len

claim distance a, b ∈ Nat : Nat det
    _path ∈ { p | shortest_path_between a b p }
    path_length _path


claim in_cycle n ∈ Nat : Prop
    adjacent n _next
    reachable _next n

claim acyclic : Prop
    ∀ n ∈ nodes : ¬ in_cycle n


claim strongly_connected : Prop
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b

claim weakly_connected : Prop
    ∀ a ∈ nodes, ∀ b ∈ nodes : reachable a b ∨ reachable b a


claim tree : Prop
    weakly_connected
    acyclic


? reachable 1 5
? reachable 5 1
? reachable 3 2

? ∃ p : path_between 1 5 p

? d = distance 1 5
? d = distance 1 4
? d = distance 5 1

? acyclic
? strongly_connected
? weakly_connected
? tree

? ∃ a, b ∈ nodes : reachable a b, ¬ reachable b a
```
