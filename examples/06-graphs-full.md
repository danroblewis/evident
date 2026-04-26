# Example 6: Graph Theory — Full Program

```evident
type Graph = {
    nodes ∈ Set Nat
    edges ∈ Set (Nat, Nat)
}


claim adjacent g ∈ Graph, a, b ∈ Nat : Prop
    (a, b) ∈ g.edges ∨ (b, a) ∈ g.edges


claim reachable g ∈ Graph, a, b ∈ Nat : Prop

a ∈ g.nodes                       ⇒ reachable g a a
reachable g a b, adjacent g b c   ⇒ reachable g a c


claim path_between g ∈ Graph, a, c ∈ Nat, path ∈ List Nat : Prop
    first_of path = a
    last_of  path = c
    ∀ (p, q) ∈ each_consecutive path : adjacent g p q


claim path_length path ∈ List Nat : Nat det
    _len = length path
    _len - 1


claim shortest_path_between g ∈ Graph, a, b ∈ Nat, path ∈ List Nat : Prop
    path_between g a b path
    _len = path_length path
    ∀ alt ∈ { p | path_between g a b p } : path_length alt ≥ _len

claim distance g ∈ Graph, a, b ∈ Nat : Nat det
    _path ∈ { p | shortest_path_between g a b p }
    path_length _path


claim in_cycle g ∈ Graph, n ∈ Nat : Prop
    adjacent g n _next
    reachable g _next n

claim acyclic g ∈ Graph : Prop
    ∀ n ∈ g.nodes : ¬ in_cycle g n


claim strongly_connected g ∈ Graph : Prop
    ∀ a ∈ g.nodes, ∀ b ∈ g.nodes : reachable g a b

claim weakly_connected g ∈ Graph : Prop
    ∀ a ∈ g.nodes, ∀ b ∈ g.nodes : reachable g a b ∨ reachable g b a


claim tree g ∈ Graph : Prop
    weakly_connected g
    acyclic g


-- example instantiation

assert g = {
    nodes = { 1, 2, 3, 4, 5 }
    edges = { (1,2), (1,3), (2,4), (3,4), (4,5) }
}

? reachable g 1 5
? reachable g 5 1
? reachable g 3 2

? ∃ p : path_between g 1 5 p

? d = distance g 1 5
? d = distance g 1 4
? d = distance g 5 1

? acyclic g
? strongly_connected g
? weakly_connected g
? tree g

? ∃ a, b ∈ g.nodes : reachable g a b, ¬ reachable g b a
```
