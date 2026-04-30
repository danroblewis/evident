# String Constraints in Evident

## The Key Insight

String parsing and string templating are the **same operation** with different
variables known and unknown.

In a functional language you write a parser (bytes → structure) and a serializer
(structure → bytes) as two separate functions. In Evident, you write one
constraint that relates the byte sequence to the structure, and the solver does
either direction depending on what you give it.

```
-- This schema relates a raw request line to its parsed fields.
-- Give it the raw bytes → solver finds method and path.
-- Give it method and path → solver finds a valid raw encoding.
-- They are the same schema.

schema ParsedRequestLine
    raw     ∈ String
    method  ∈ String
    path    ∈ String
    raw = method ++ " " ++ path ++ " HTTP/1.1"
```

This is not string concatenation as a function. It is a constraint on three
strings. Z3 has the theory of strings and can handle this.

---

## What Z3 Already Has

Z3 implements the theory of strings (SMTLIB `str.*`). We expose `StringSort`
already but have not exposed the operations. They exist in the Z3 Python API:

| Z3 operation | What it does |
|---|---|
| `z3.Concat(s1, s2)` | s1 ++ s2 (usable bidirectionally as a constraint) |
| `z3.Length(s)` | len(s) as an integer |
| `z3.PrefixOf(pre, s)` | s starts with pre |
| `z3.SuffixOf(suf, s)` | s ends with suf |
| `z3.Contains(s, sub)` | s contains sub |
| `z3.At(s, i)` | character at index i |
| `z3.SubString(s, start, len)` | slice |
| `z3.InRe(s, re)` | s matches regex re |
| `z3.Re(literal)`, `z3.Star(re)`, `z3.Union(re1, re2)` | regex combinators |
| `z3.Replace(s, src, dst)` | replace first occurrence |
| `z3.IndexOf(s, sub, offset)` | find substring |
| `z3.StrToInt(s)`, `z3.IntToStr(n)` | conversion |

All of these are constraints, not functions. They can appear on either side of
an equation. They compose. The solver handles them.

---

## What Needs to Be Added to Evident

1. **Syntax**: expose string operations in the grammar as operators or built-in
   function calls. Candidates:
   - `++` for concatenation (natural notation)
   - `|s|` for length
   - `s starts_with p`, `s ends_with p`, `s contains sub`
   - `s matches /regex/`

2. **Translation**: in `translate.py`, recognize these expressions and emit the
   corresponding Z3 calls.

3. **Type-checking** (optional but helpful): a warning when you do numeric
   operations on string-typed variables or vice versa.

That's it. The solver already handles the hard part.

---

## Parsing as Constraint Satisfaction

Traditional parser: procedural, left-to-right, one pass, constructive.

Evident parser: declare the grammar as constraints, give the solver the input
string, let it find the parse. No parser code to write. The grammar IS the
parser.

Example — HTTP request line:

```
schema RequestLine
    raw     ∈ String
    method  ∈ String
    path    ∈ String

    -- Grammar: method SP path SP "HTTP/1.1"
    raw = method ++ " " ++ path ++ " HTTP/1.1"

    -- Method must be a valid verb
    method ∈ {"GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"}

    -- Path must start with /
    path starts_with "/"
```

Query with `? RequestLine raw="GET /hello HTTP/1.1"` and the solver finds
`method="GET"`, `path="/hello"`. The same schema generates valid request lines
if you give it the method and path instead.

---

## Templating as Constraint Satisfaction

Response generation in a traditional server: string concatenation.

In Evident: constrain the response string to have the right structure and let
the solver fill it in.

```
schema HttpResponse
    status_code ∈ Nat
    body        ∈ String
    raw         ∈ String

    -- Status line
    status_line ∈ String
    status_code = 200 ⇒ status_line = "200 OK"
    status_code = 404 ⇒ status_line = "404 Not Found"
    status_code = 500 ⇒ status_line = "500 Internal Server Error"

    content_length ∈ Nat
    content_length = |body|

    raw = "HTTP/1.1 " ++ status_line ++ "\r\n"
       ++ "Content-Length: " ++ str(content_length) ++ "\r\n"
       ++ "\r\n"
       ++ body
```

Query with `? HttpResponse status_code=200 body="Hello!"` and the solver
constructs the raw response string. No template engine needed.

---

## The Motivating Example: Text Adventure

A text adventure is the perfect first program for this:

- The player types a command string
- Parsing the string into a structured command is a constraint problem
- Game world transitions are constraint implications
- Response text is a template filled by the solver

See `docs/design/text-adventure-plan.md`.

---

## On Highly Optimized Web Servers

nginx, H2O, Envoy do not use string concatenation for parsing. They use:
- Finite-state-machine parsers that scan byte-by-byte
- Zero-copy: return pointers into the original buffer
- SIMD instructions to find delimiters fast

This is actually MORE like the Evident model than functional string operations.
A zero-copy parser identifies *positions and lengths* within the input — exactly
what a constraint solver would do if you asked it "where in this string does the
method end and the path begin?" The solver doesn't copy anything; it identifies
structure.

The difference is that nginx's FSM is handwritten C while Evident's would be
declared as constraints. Both avoid string construction during parsing.
