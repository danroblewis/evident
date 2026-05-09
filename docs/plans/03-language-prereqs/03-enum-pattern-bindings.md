# Phase 3.3: Enum-typed pattern bindings

## Goal

Lift the v1 `match` restriction that pattern bindings must be primitive
(Int/Bool/String/Real). Allow:

```
match list
    Cons(head, tail) ⇒ ...   -- where tail is enum-typed (List)
```

## Today's limitation

`runtime-rust/src/translate/exprs.rs::translate_match_arms` rejects
enum-typed bindings:

```rust
let var = if let Some(i) = raw.as_int() { Var::IntVar(i) }
    else if let Some(b) = raw.as_bool() { Var::BoolVar(b) }
    else if let Some(s) = raw.as_string() { Var::StrVar(s) }
    else if let Some(r) = raw.as_real() { Var::RealVar(r) }
    else { return None; };
```

The `else { return None; }` rejects enum-typed payloads. We need to
recognize the field's declared type, look up its `(DatatypeSort,
fields)` in the EnumRegistry, and create a `Var::EnumVar` from the
accessor's output.

## What to build

The translator's match helper needs access to the EnumRegistry. Today
it doesn't get one. Either:

1. Pass `enums: &EnumRegistry` through the translate_* signatures.
2. Cache the field-type → enum_name mapping in the
   `Var::EnumVar`'s declaration so we can look it up post-hoc.

(1) is cleaner but threads through 5+ translate functions.

For each binding whose field type is an enum (look up via
`EnumRegistry.by_name[scrutinee_enum].variants[var_idx].fields[j]`),
construct `Var::EnumVar { ast: raw.as_datatype()?, enum_name, dt }`
where `dt` is the field-type's DatatypeSort (also from the registry).

For SELF-RECURSIVE fields (the common case — Cons(Int, List) where
List is the scrutinee's own type), the dt is the SAME as the
scrutinee's. Easy.

## Files touched

- `runtime-rust/src/translate/exprs.rs::translate_match_arms`
- Possibly threading `enums` through translate_* signatures

## Test it

A claim that walks a recursive list using a `match` with enum
binding:

```evident
enum List = Nil | Cons(Int, List)

claim head_or(list ∈ List, fallback ∈ Int, out ∈ Int)
    out = match list
        Cons(h, t) ⇒ h          -- t is List-typed; not used here but legally bound
        Nil        ⇒ fallback

claim sat_picks_head
    list ∈ List
    out ∈ Int
    list = Cons(7, Nil)
    head_or(list, 99, out)
    out = 7
```

Plus a recursion-using example (combines with Phase 3.1):

```evident
claim sum(list ∈ List, total ∈ Int)
    total = match list
        Nil        ⇒ 0
        Cons(h, t) ⇒
            ∃ rest ∈ Int :
                sum(t, rest)        -- uses bound `t` of enum type
                h + rest
```

## Acceptance

- [ ] Basic enum-binding match works
- [ ] Combined with recursive claims, full AST walk works
- [ ] All existing match tests still pass
- [ ] LOC: +~80 Rust

## Notes

Once this lands, all the necessary language features for Phase 4
exist. The codegen migrations can begin.
