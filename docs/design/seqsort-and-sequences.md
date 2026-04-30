# Z3 SeqSort and Sequences in Evident

## What SeqSort Is

Z3's `SeqSort(T)` is the sort of finite sequences of elements of type T.
`String` is a special case: `String = Seq(Char)`.

The critical insight: **everything we've added for strings works for sequences
of any type**, because Z3's string operations are actually sequence operations
that Z3 specializes for character sequences.

```python
# In Z3:
IntSeq = z3.SeqSort(z3.IntSort())
s = z3.Const('s', IntSeq)
z3.PrefixOf(z3.Unit(z3.IntVal(1)), s)   # [1] is a prefix of s
z3.Contains(s, z3.Unit(z3.IntVal(42)))  # s contains [42]
z3.Length(s)                             # length of s
```

This means `⊑`, `⊒`, `∈` (contains), `++` could all work on sequences of
integers, enums, or any other Evident type — not just character strings.

---

## The Operations (String = Seq(Char))

| Z3 function | Meaning | Evident notation (planned) |
|---|---|---|
| `Concat(s1, s2)` | s1 ++ s2 | `s1 ++ s2` ✓ |
| `Length(s)` | number of elements | `\|s\|` (pending) |
| `PrefixOf(pre, s)` | pre is a prefix of s | `pre ⊑ s` (planned) |
| `SuffixOf(suf, s)` | suf is a suffix of s | `s ⊒ suf` (planned) |
| `Contains(s, sub)` | sub appears in s | `s ∋ sub` ✓ |
| `At(s, i)` | element at index i | `s[i]` (planned) |
| `Extract(s, i, n)` | n elements starting at i | `s[i..i+n]` (planned) |
| `IndexOf(s, sub, from)` | first occurrence of sub | (planned) |
| `InRe(s, re)` | s matches regex re | `s ∈ /re/` ✓ |
| `Replace(s, src, dst)` | replace first occurrence | (planned) |
| `StringLe(s1, s2)` | lexicographic ≤ | `s1 ≤_lex s2` (planned) |
| `StrToInt(s)` | parse string as integer | (planned) |
| `IntToStr(n)` | integer to string | (planned) |

---

## Sequences of Non-Character Types

This is the part most languages don't give you. In Z3, you can have:

```
Seq(Nat)      -- sequences of natural numbers
Seq(Color)    -- sequences of enum values
Seq(Point)    -- sequences of structured values
```

And ALL of the above operations apply. This means in Evident:

```
-- A DNA strand is a sequence of nucleotides
type Nucleotide = A | C | G | T

claim has_start_codon
    strand ∈ Seq(Nucleotide)
    strand ∋ [A, T, G]            -- contains the subsequence ATG

claim is_palindrome
    strand ∈ Seq(Nucleotide)
    reversed ∈ Seq(Nucleotide)
    -- strand[i] = reversed[length - 1 - i] for all i  (planned)

-- An HTTP request is a sequence of tokens
type Token = Method | Path | Header | Body

claim valid_request_sequence
    tokens ∈ Seq(Token)
    tokens ⊑ [Method, Path]       -- starts with method and path
```

---

## The Membership / Language Connection

The regex connection makes this all coherent:

- A **string** is an element of `Seq(Char)`
- A **regex** defines a **language**: a subset of `Seq(Char)`
- `s ∈ /regex/` is literally set membership: s is in the language

This generalises:
- A **grammar** (context-free) defines a language over any alphabet
- A **schema** in Evident already IS a language: the set of satisfying assignments

Possible future: `s ∈ SomeSchema` where SomeSchema defines a grammar over
a sequence type, and the solver checks membership.

---

## The `∈`/`∋` Overloading

With sequences and regexes, `∈` and `∋` do different things depending on
the type of the right-hand side:

| Expression | Right-side type | Meaning |
|---|---|---|
| `s ∈ /regex/` | RegexLiteral | regex language membership |
| `/regex/ ∋ s` | RegexLiteral | same (reverse notation) |
| `haystack ∋ needle` | String/Sequence | subsequence containment |
| `needle ∈ haystack` | String/Sequence | subsequence containment |
| `x ∈ {a, b, c}` | SetLiteral | existing set membership |
| `x ∈ TypeName` | Identifier (type) | existing type membership |

This is consistent: `∈` always means "is a member of / is contained in",
and the semantics follows from the type of the container.

---

## What Needs to Be Built

1. **Regex literals** `/pattern/` as a new literal kind ✓ (implemented)
2. **`∋` symbol** in normalizer and grammar ✓ (implemented)
3. **Type-directed dispatch** in translate_constraint for `∈`/`∋` ✓ (implemented)
4. **`Seq(T)` type syntax** — `x ∈ Seq(Nat)` declaring a sequence variable
5. **Sequence literals** — `[1, 2, 3]` as a Seq value (currently SetLiteral)
6. **`⊑`/`⊒`** prefix/suffix operators
7. **`|s|`** string/sequence length (blocked by grammar disambiguation)
8. **`s[i]`** element access
9. **`s[i..j]`** slice
10. **`StrToInt`/`IntToStr`** for HTTP status codes, etc.
