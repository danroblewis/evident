# Seq idioms (M7)

These are **language idioms**, not library code. The transpiler
recognizes them directly and lowers each to the corresponding SMT-LIB
`seq.*` operation. There is no `.ev` source for any of them — they
exist only in the transpiler's built-in identifier list.

Each name is a **value expression** denoting a relation about a Z3
Seq value — used inside constraints, not "called" in the procedural
sense.

## Identifiers

| Evident | SMT-LIB lowering |
|---|---|
| `head(s)` | `(seq.nth s 0)` |
| `last(s)` | `(seq.nth s (- (seq.len s) 1))` |
| `len(s)` | `(seq.len s)` |
| `init(s)` | `(seq.extract s 0 (- (seq.len s) 1))` |
| `tail(s)` | `(seq.extract s 1 (- (seq.len s) 1))` |
| `unit(x)` | `(seq.unit x)` |
| `empty(T)` | `(as seq.empty (Seq T))` |

## Binary operator

`++` is sequence concatenation at the same precedence as `+`:

```
a ++ b   →   (seq.++ a b)
```

## Notes

- The argument to `empty` is a bare sort name (an `IDENT`). Generic
  element sorts are not yet expressible at the call site — use the
  bare-IDENT form like `empty(Int)` until the language gains a richer
  sort literal grammar.
- `head(s)` and `last(s)` on an empty seq are undefined per the
  SMT-LIB Seq theory; constrain `len(s) ≥ 1` if your relation needs
  them defined.
- `init` and `tail` are relational: `t = init(s)` says "t equals s
  with its last element dropped." There is no mutation.

## Example

```
claim seq_three()
    s ∈ Seq(Int)
    len(s) = 3
    head(s) = 1
    last(s) = 3
```

Z3 finds a 3-element Seq whose first element is 1 and last is 3.
