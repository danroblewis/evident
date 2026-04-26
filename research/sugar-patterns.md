# Pattern Matching and Destructuring Sugar Across Languages

*Research for the Evident language design project. Focus: how languages syntax-sugar
"bind specific fields from a collection element" and "match on structure," and what
the best ideas are for a constraint language that works with sets of records.*

---

## 1. ML/Haskell Pattern Matching — The Gold Standard

Pattern matching in ML and Haskell descends from the algebraic type systems of the
1970s. A type is described by its *constructors* — the ways to build values of that
type — and pattern matching is the canonical way to *consume* those values by case-
splitting on which constructor was used. The compiler checks exhaustiveness (all
constructors handled) and irredundancy (no unreachable cases), turning structural
inspection into a verified, compile-time-complete operation.

### Basic constructor patterns

```haskell
-- List is defined as:  data [a] = [] | (a : [a])
-- Pattern match on constructors:
describe :: [Int] -> String
describe []     = "empty"
describe [x]    = "singleton: " ++ show x
describe (x:xs) = "head " ++ show x ++ ", tail of length " ++ show (length xs)
```

The patterns `[]` and `(x:xs)` are constructor patterns. `x` and `xs` are binders
introduced by the match — they are in scope in the right-hand side. No binding keyword;
the position in the pattern determines the name.

### Wildcard `_`

```haskell
firstTwo :: [a] -> (a, a)
firstTwo (x:y:_) = (x, y)   -- _ discards the rest of the list
firstTwo _        = error "need at least two elements"
```

`_` is the anonymous binder — matches anything, binds nothing. Used when the structure
matters (you need to confirm a constructor was used) but the value doesn't.

### As-patterns `@`

```haskell
-- Bind the whole value AND destructure it at the same time:
firstAndAll :: [a] -> (a, [a])
firstAndAll all@(x:_) = (x, all)   -- 'all' is the whole list; 'x' is the head
```

`all@(x:_)` matches a non-empty list, binds the entire list to `all`, and simultaneously
binds the head to `x`. This avoids having to reconstruct `x:rest` just to get back the
original value.

### Nested patterns

```haskell
data Tree a = Leaf | Node (Tree a) a (Tree a)

-- Pattern match multiple levels deep in one expression:
leftChild :: Tree a -> Maybe a
leftChild (Node (Node _ x _) _ _) = Just x   -- left child exists and has a value
leftChild _                        = Nothing
```

Patterns compose arbitrarily deep. The compiler generates a single efficient decision
tree from the nested structure.

### Case expressions

```haskell
classify :: [Int] -> String
classify xs = case xs of
  []        -> "empty"
  [_]       -> "singleton"
  (x:y:_)
    | x < y    -> "ascending start"
    | x > y    -> "descending start"
    | otherwise -> "equal start"
```

`case` matches on an expression rather than at the function head. Guards (`| condition`)
add conditional dispatch within a case arm without needing a nested `case`.

### Why ML pattern matching became the gold standard

1. **Exhaustiveness checking**: the compiler verifies all constructors are covered.
   Adding a new constructor to a type produces compile errors at every non-exhaustive
   match — a structural refactoring aid that beats text search.

2. **Irredundancy**: unreachable patterns are flagged, preventing dead-code silently
   accumulating.

3. **Binders are structural**: variable names are introduced by their position in a
   pattern, not by a `let` or assignment statement. This makes the relationship between
   the structure matched and the names bound visually obvious.

4. **Composability**: patterns nest without limit. Any sub-term can be further matched.

5. **Decision tree compilation**: a set of patterns over the same data is compiled into
   a single, efficient decision tree — not a sequence of if/else tests. The generated
   code tests each discriminant at most once.

---

## 2. Rust Patterns — Exhaustive Matching for Systems Code

Rust took ML-style pattern matching and extended it to cover the concerns of a systems
language: ranges of integers, slices with unknown lengths, struct fields, and the
combination of patterns with reference semantics. Rust patterns are pervasive — they
appear in `match`, `if let`, `while let`, function arguments, and `let` bindings.

### `match`

```rust
enum Shape { Circle(f64), Rectangle(f64, f64), Triangle(f64, f64, f64) }

fn area(s: Shape) -> f64 {
    match s {
        Shape::Circle(r)          => std::f64::consts::PI * r * r,
        Shape::Rectangle(w, h)    => w * h,
        Shape::Triangle(a, b, c) => {
            let s = (a + b + c) / 2.0;
            (s * (s-a) * (s-b) * (s-c)).sqrt()
        }
    }
}
```

### `if let` — single-arm match without exhaustiveness requirement

```rust
// Instead of a full match when only one constructor is interesting:
if let Some(user) = find_user(id) {
    println!("Found: {}", user.name);
}

// With else:
if let Ok(data) = parse_json(input) {
    process(data)
} else {
    eprintln!("parse failed");
}
```

`if let` is syntactic sugar for a `match` with one interesting arm and a wildcard. It
trades exhaustiveness for concision when you genuinely only care about one variant.

### `while let` — loop until pattern stops matching

```rust
let mut stack = vec![1, 2, 3];
while let Some(top) = stack.pop() {
    println!("{}", top);
}
// Loop exits when pop() returns None
```

### `@` bindings — bind and test simultaneously

```rust
match value {
    n @ 1..=12  => println!("month number {}", n),   // bind n if in range
    n @ 13..=19 => println!("teen: {}", n),
    n            => println!("other: {}", n),
}
```

`n @ pattern` checks that the value matches `pattern` and simultaneously binds it to `n`.
Equivalent to Haskell's `all@(x:rest)` as-patterns.

### Range patterns

```rust
match score {
    90..=100 => "A",
    80..=89  => "B",
    70..=79  => "C",
    _        => "F",
}
```

`1..=5` is an inclusive range pattern. Rust also supports `1..5` (exclusive upper bound).
These work on integers and characters. The exhaustiveness checker understands ranges.

### Slice patterns — matching sequences by structure

```rust
fn describe(slice: &[i32]) -> &str {
    match slice {
        []                   => "empty",
        [x]                  => "one element",
        [first, .., last]    => "multiple elements",   // .. = zero or more in middle
        [a, b]               => "exactly two",
    }
}

// Bind specific positions:
fn head_and_tail(s: &[i32]) -> Option<(i32, &[i32])> {
    match s {
        [head, tail @ ..] => Some((*head, tail)),
        []                => None,
    }
}
```

`..` in a slice pattern matches zero or more elements without binding them. `tail @ ..`
binds the matched-over portion to `tail`.

### Struct patterns — bind fields by name

```rust
struct Point { x: f64, y: f64 }
struct Person { name: String, age: u32, email: String }

fn is_origin(p: Point) -> bool {
    match p {
        Point { x: 0.0, y: 0.0 } => true,
        _                         => false,
    }
}

// Shorthand: when binding name matches field name
fn greet(person: Person) {
    let Person { name, age, .. } = person;   // .. ignores remaining fields
    println!("{} is {}", name, age);
}
```

`..` in a struct pattern discards all fields not explicitly named. Without `..`, the
pattern must name every field (exhaustiveness applies to fields too).

### Tuple struct patterns

```rust
// Newtype wrappers:
struct Celsius(f64);
struct Fahrenheit(f64);

fn to_fahrenheit(Celsius(c): Celsius) -> Fahrenheit {
    Fahrenheit(c * 9.0/5.0 + 32.0)
}
// The parameter is pattern-matched directly in the function signature
```

### Or-patterns — alternatives within a single arm

```rust
match x {
    1 | 2 | 3 => println!("small"),
    4 | 5 | 6 => println!("medium"),
    _          => println!("large"),
}

// Or-patterns compose with binding:
match opt {
    Some(0) | None => println!("zero or absent"),
    Some(n)        => println!("got {}", n),
}
```

Or-patterns (`|` inside a pattern) let multiple structural alternatives share one arm,
as long as all alternatives bind the same set of names with the same types.

---

## 3. Scala Pattern Matching — Extractors and Open Extension

Scala's pattern matching is distinguished by its *extractor* mechanism: any object can
define `unapply` to make itself usable in patterns, without being a case class. This
makes patterns user-definable and extensible.

### Case classes — automatic pattern matching

```scala
sealed trait Expr
case class Num(n: Int)          extends Expr
case class Add(a: Expr, b: Expr) extends Expr
case class Mul(a: Expr, b: Expr) extends Expr

def eval(e: Expr): Int = e match {
  case Num(n)    => n
  case Add(a, b) => eval(a) + eval(b)
  case Mul(a, b) => eval(a) * eval(b)
}
```

`case class` automatically generates `unapply`, making the class usable in patterns.
The sealed trait enables exhaustiveness checking.

### Extractors — `unapply`

```scala
object Even {
  def unapply(n: Int): Option[Int] =
    if (n % 2 == 0) Some(n / 2) else None
}

object Odd {
  def unapply(n: Int): Boolean = n % 2 != 0
}

// Now Even and Odd work in patterns:
42 match {
  case Even(half) => s"even, half is $half"
  case Odd()      => "odd"
}
```

`unapply` returns `Option[T]` (for binding one value), `Option[(T1, T2)]` (for binding
multiple), `Boolean` (for guards with no binding), or a sequence type for `unapplySeq`.
This is the formal mechanism underlying all pattern matching in Scala.

### Sequence patterns with `_*`

```scala
def describe(list: List[Int]): String = list match {
  case Nil              => "empty"
  case x :: Nil         => s"singleton: $x"
  case first :: rest    => s"head $first, ${rest.length} more"
}

// Sequence patterns with rest binding:
Seq(1, 2, 3, 4) match {
  case Seq(first, second, rest @ _*) =>
    println(s"$first, $second, then ${rest.length} more")
}
```

`rest @ _*` binds the remainder of a sequence to `rest`. The `_*` pattern matches any
number of elements (zero or more).

### Guard conditions

```scala
def classify(n: Int): String = n match {
  case x if x < 0  => "negative"
  case 0            => "zero"
  case x if x < 10 => s"small positive: $x"
  case x            => s"large: $x"
}
```

Guards (`case x if condition`) allow arbitrary boolean conditions within a match arm.
They do not participate in exhaustiveness checking — the compiler does not reason about
guard conditions, so you may need a wildcard arm even when guards logically cover all cases.

### Pattern matching in for-comprehensions

```scala
case class Person(name: String, age: Int)
val people = List(Person("Alice", 30), Person("Bob", 17), Person("Carol", 25))

// Pattern binding in generator — destructure the element directly:
val adults = for (Person(name, age) <- people if age >= 18) yield name
// Result: List("Alice", "Carol")

// Pattern matching filters: non-matching elements are silently dropped
// (unlike let bindings in Haskell which would raise an exception)
```

Scala's for-comprehension generators allow pattern matching: elements that don't match
are silently dropped. This differs from Haskell's `let` in list comprehensions, where
a non-matching pattern raises an error.

---

## 4. Python Structural Pattern Matching (3.10+)

Python's `match`/`case` (PEP 634, released 3.10) introduced structural pattern matching
to a dynamically-typed language. Because Python has no algebraic data types, the patterns
are defined over the runtime structure of objects, not over compile-time constructors.
The result is more flexible (works with any object) but less safe (no exhaustiveness
checking, type errors at runtime).

### Class patterns

```python
from dataclasses import dataclass

@dataclass
class Point:
    x: float
    y: float

def classify_point(point):
    match point:
        case Point(x=0, y=0):
            return "origin"
        case Point(x=0, y=y):
            return f"on y-axis at {y}"
        case Point(x=x, y=0):
            return f"on x-axis at {x}"
        case Point(x=x, y=y) if x == y:
            return f"on diagonal at {x}"
        case Point(x=x, y=y):
            return f"at ({x}, {y})"
```

`Point(x=0, y=y)` matches an object of type `Point` where `x == 0` and binds the `y`
attribute to the name `y`. Named fields are matched by attribute name, not position.
`x=0` is a value pattern (must equal 0); `y=y` is a capture pattern (binds to `y`).

### Sequence patterns

```python
def describe_sequence(seq):
    match seq:
        case []:
            return "empty"
        case [x]:
            return f"one element: {x}"
        case [first, *rest]:
            return f"first: {first}, {len(rest)} more"
        case [first, *middle, last]:
            return f"from {first} to {last}"
```

`[first, *rest]` mirrors Python's unpacking syntax. `*rest` captures zero or more
elements into a list. Unlike regular unpacking, `*` can appear in non-final position:
`[first, *middle, last]` is valid.

### Mapping patterns

```python
def handle_command(command):
    match command:
        case {"action": "quit"}:
            return quit()
        case {"action": "move", "direction": direction}:
            return move(direction)
        case {"action": "attack", "weapon": weapon, **rest}:
            return attack(weapon, modifiers=rest)
```

`{"key": pattern}` matches a dict (or dict-like object) that contains the key, with
the value matching the sub-pattern. `**rest` captures remaining key-value pairs. Unlike
sequence patterns, mapping patterns are open — extra keys not mentioned are ignored
(unless `**rest` is absent, in which case extra keys are still permitted).

### OR patterns

```python
def is_small(n):
    match n:
        case 1 | 2 | 3 | 4 | 5:
            return True
        case _:
            return False

# OR patterns with binding — all alternatives must bind the same names:
match command:
    case "quit" | "exit" | "q":
        print("goodbye")
```

### Guard conditions

```python
match point:
    case Point(x=x, y=y) if x > 0 and y > 0:
        return "first quadrant"
    case Point(x=x, y=y) if x < 0 and y > 0:
        return "second quadrant"
```

Guards in Python's match work the same way as in Scala and Haskell: they are arbitrary
boolean conditions evaluated after the structural match. The compiler does not reason
about them for exhaustiveness.

---

## 5. JavaScript/TypeScript Destructuring

JavaScript destructuring (ES2015) is not pattern matching in the traditional sense —
there is no case analysis, no constructor inspection, no exhaustiveness check. It is
purely about *binding* names from structures. Despite this limitation, JavaScript's
destructuring syntax is remarkably concise for the common case of "pull these fields out."

### Object destructuring

```javascript
const { name, age, email } = person;

// Rename while destructuring:
const { firstName: name, age: years } = person;
// 'name' is bound to person.firstName; 'years' is bound to person.age

// Default values:
const { x = 0, y = 0 } = point;
// x is point.x if present, 0 otherwise

// Combine rename and default:
const { color: c = "red" } = style;
```

### Array destructuring

```javascript
const [head, ...tail] = array;
const [first, second, , fourth] = array;  // skip third element with empty slot
const [a, b, c = 0] = [1, 2];            // c defaults to 0
```

The `...rest` spread captures remaining elements. There is no equivalent to ML's
`[first, .., last]` — you cannot match a final element without capturing everything
in between.

### Nested destructuring

```javascript
const { address: { city, zipCode }, name } = person;
const [[a, b], [c, d]] = matrix;

// Function parameter destructuring:
function greet({ name, age = 0 }) {
  console.log(`${name} is ${age}`);
}

// Array parameter destructuring:
function sum([first, ...rest]) {
  return rest.reduce((acc, n) => acc + n, first ?? 0);
}
```

### TypeScript additions

TypeScript adds type narrowing that interacts with destructuring:

```typescript
type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "square"; side: number };

function area(shape: Shape): number {
  const { kind } = shape;
  if (kind === "circle") {
    // TypeScript narrows shape to { kind: "circle"; radius: number }
    const { radius } = shape;
    return Math.PI * radius ** 2;
  } else {
    const { side } = shape;
    return side ** 2;
  }
}
```

This is discriminated union narrowing — TypeScript's approximation of constructor-based
pattern matching. It lacks the syntactic integration of `match` but achieves the same
narrowing effect through if/else chains.

---

## 6. Elixir Pattern Matching — Everything Is Pattern Matching

Elixir's defining design decision is that the `=` sign is not assignment — it is the
*match operator*. This single choice makes pattern matching pervasive throughout the
language. Every binding is a match. Every function call is a match against the function
head. Every `case` arm is a match. The language has exactly one binding mechanism, and
that mechanism is structural matching.

### `=` as match

```elixir
# Binding a simple name:
x = 5          # matches 5, binds x to 5

# Matching a tuple:
{a, b} = {1, 2}       # a = 1, b = 2
{:ok, value} = result  # asserts result is {:ok, something}, binds value

# Matching fails if structure doesn't match:
{:ok, v} = {:error, "not found"}   # MatchError raised at runtime
```

### Pattern matching in function heads

```elixir
defmodule List do
  def length([]),        do: 0
  def length([_ | rest]), do: 1 + length(rest)
end

defmodule Http do
  def handle({:get, path}),    do: serve_file(path)
  def handle({:post, path, body}), do: create_resource(path, body)
  def handle({:delete, path}), do: delete_resource(path)
end
```

Function clauses are tried in order, and the first matching head wins. Pattern matching
in function heads is Elixir's primary dispatch mechanism — it replaces `if/else` chains
and `switch` statements in most cases.

### `case` and `cond`

```elixir
case result do
  {:ok, value}    -> process(value)
  {:error, reason} -> log_error(reason)
  nil              -> handle_nil()
end

# cond is for conditions, not patterns:
cond do
  x < 0  -> "negative"
  x == 0 -> "zero"
  true   -> "positive"   # true is the default arm
end
```

### The pin operator `^`

```elixir
expected = 42
^expected = 42   # OK: matches because expected == 42
^expected = 43   # MatchError: 43 != 42

# Without ^, the existing binding would be shadowed:
x = 1
x = 2   # OK in Elixir: this rebinds x, not a match against 1
^x = 2  # MatchError: ^ forces matching against the current value of x
```

`^` distinguishes "use this variable's current value as a pattern" from "bind this
variable to whatever matches." This is Elixir's resolution of the ambiguity that
Prolog resolves through variable-name capitalization conventions.

### How Elixir's philosophy differs

In most languages, pattern matching is an exceptional operation — a specialized syntax
for case analysis. In Elixir:

- Every variable binding is a pattern match
- Every function head is a pattern match
- Every `receive` block is a pattern match
- Failure to match is an explicit, handleable runtime event (MatchError)

This means Elixir programmers naturally think in terms of structure and shape.
A function that processes HTTP responses is not "a function that takes a response object
and checks fields" — it is "multiple clauses, each matching a different response shape."
The pattern-matching mindset is baked into how Elixir programs are structured.

---

## 7. Prolog Unification — Two-Way Pattern Matching

Prolog's underlying operation is *unification*, not pattern matching. The distinction
is crucial:

- **Pattern matching** (ML, Haskell, Rust): one side is a pattern (with holes),
  the other side is a value. The pattern is matched against the value. The direction
  is fixed: the value flows into the pattern.

- **Unification** (Prolog): both sides are terms that may contain unbound variables.
  The unifier finds the most general substitution that makes both sides identical.
  There is no privileged "pattern" side. Either side can have variables.

### Unification examples

```prolog
% Simple binding:
X = foo.        % X is bound to foo

% Structural:
f(X, bar) = f(baz, Y).   % X = baz, Y = bar

% List unification:
[H | T] = [1, 2, 3].   % H = 1, T = [2, 3]

% Two-way: either side can be the "pattern"
[1, 2 | T] = L.          % L = [1, 2 | T], T still unbound — partial information
append([1,2], [3,4], Z). % Z = [1,2,3,4]  — forward mode
append([1,2], Y, [1,2,3,4]). % Y = [3,4]  — backward mode (solving for Y)
append(X, Y, [1,2,3]).   % generates all splits: X=[], Y=[1,2,3]; X=[1], Y=[2,3]; etc.
```

A predicate like `append/3` written with structural clauses is simultaneously:
- A function (given two lists, produce the concatenation)
- An inverse function (given the concatenation and one part, find the other)
- A relation (enumerate all ways to split a list)

The direction emerges from which arguments are ground at call time.

### Why this is powerful for constraint programming

Unification is the simplest possible constraint solver: it handles the theory of
equality over first-order terms (herbrand unification). Variables range over terms.
Constraints are equalities. The solver propagates bindings. When a contradiction
is found (two non-unifiable terms forced equal), the branch fails.

The connection to Evident: Evident's evidence base is a generalized constraint store,
and the solver it uses subsumes unification. When Evident writes `_w ∈ workers,
_w.id = a.worker_id`, the `_w.id = a.worker_id` constraint is handled by the
congruence closure solver — a strict generalization of unification. The "bind a name
to a record by matching a field" idiom descends directly from Prolog's structural
unification over lists.

---

## 8. Record Syntax and Field Access Sugar

Working with named-field records is one of the most common operations in data-heavy
programs. Languages have developed many different syntaxes for this.

### Haskell record syntax

```haskell
data Person = Person
  { name :: String
  , age  :: Int
  , email :: String
  }

-- Construction with named fields:
alice = Person { name = "Alice", age = 30, email = "alice@example.com" }

-- Pattern matching with named fields:
greet :: Person -> String
greet Person { name = n, age = a } = n ++ " is " ++ show a

-- Record update (create modified copy):
older = alice { age = alice.age + 1 }

-- Wildcard in record pattern:
nameOf :: Person -> String
nameOf Person { name = n } = n   -- other fields ignored implicitly
```

Haskell's record patterns use `FieldName = Binding` pairs. The order of fields in
the pattern need not match the order in the type definition. Unmentioned fields are
implicitly ignored.

### Rust struct patterns (revisited)

```rust
struct Config {
    host: String,
    port: u16,
    timeout: u64,
    max_retries: u32,
}

fn describe(Config { host, port, .. }: Config) -> String {
    format!("{}:{}", host, port)
    // timeout and max_retries ignored via ..
}

// Shorthand when binding name matches field name:
let Config { host, port, .. } = config;
// Binds 'host' to config.host, 'port' to config.port
```

### Kotlin data class destructuring

```kotlin
data class Point(val x: Double, val y: Double)
data class Person(val name: String, val age: Int)

// Destructuring declarations — positional, not named:
val (x, y) = Point(1.0, 2.0)
val (name, age) = Person("Alice", 30)

// In for loops:
for ((name, age) in people) {
    println("$name: $age")
}

// Component functions: data class auto-generates component1(), component2(), etc.
// Can define custom componentN() on any class to make it destructurable.
```

Kotlin's destructuring is positional (based on constructor parameter order), not named.
This is conciser but breaks when fields are reordered.

### C# positional patterns and property patterns

```csharp
// Property patterns (named field access):
bool IsAdult(Person person) => person is { Age: >= 18 };

// Pattern in switch:
string Describe(Shape shape) => shape switch {
    { Kind: "circle", Radius: var r }  => $"circle r={r}",
    { Kind: "square", Side: var s }    => $"square s={s}",
    _                                   => "unknown",
};

// Positional pattern (requires Deconstruct method):
var (x, y) = point;  // calls point.Deconstruct(out var x, out var y)

// Combined:
if (point is (0, var y)) {
    Console.WriteLine($"on y-axis at {y}");
}
```

C# property patterns (`{ Property: pattern }`) are the most ergonomic for records
since they use field names rather than positions. They are open by default — unmentioned
properties are not checked.

---

## 9. Binding in Quantifiers — Pattern Matching in Collection Operations

This is the most directly relevant section for Evident's design.

### Evident's current quantifier syntax

Evident's developing syntax for set quantification already involves a form of
binding through membership constraints:

```evident
-- Existential binding: introduce a name constrained to be an element of a set
_w ∈ workers, _w.id = a.worker_id

-- Universal quantification over set comprehension:
∀ { talk = t, room = r } ∈ schedule : no_overlap t r

-- Equivalent set-theoretic notation:
∀ a ∈ schedule : ∀ b ∈ schedule, a ≠ b : ¬ overlap a b
```

The phrase `{ talk = t, room = r } ∈ schedule` is already a pattern: it matches records
in `schedule` that have a `talk` field and a `room` field, binding those fields to `t`
and `r`. This is record destructuring inside a quantifier.

### Haskell list comprehension generators

```haskell
-- Generator with pattern matching — non-matching elements are silently skipped:
rights :: [Either a b] -> [b]
rights xs = [b | Right b <- xs]
-- Only Right-constructed values match; Lefts are discarded

-- Named field patterns in generators:
names = [name | Person { name, age } <- people, age >= 18]

-- Tuple patterns:
pairs = [(x, y) | (x, y) <- zip [1..] [10, 20, 30]]

-- Nested patterns:
nested = [x | Just (Just x) <- values]  -- only doubly-wrapped values
```

In Haskell list comprehensions, pattern matching in the generator `pat <- list` causes
non-matching elements to be silently filtered out. This is a significant semantic choice:
it makes pattern-filtering and pattern-binding the same operation.

### Erlang list comprehensions with pattern matching

```erlang
%% Pattern matching in generators filters non-matching elements:
Evens = [X || {ok, X} <- Results, X rem 2 == 0].
%% Only {ok, X} tuples are considered; {error, _} tuples are dropped

%% Record field binding in generators:
Names = [Name || #person{name = Name, age = Age} <- People, Age >= 18].
%% #person{name = Name, age = Age} is an Erlang record pattern
```

Erlang's list comprehension generators `Pattern <- List` filter by matching — elements
that don't match the pattern are silently discarded. This is the same behavior as
Haskell.

### How this applies to Evident

The `∀ { talk = t, room = r } ∈ schedule` construct in Evident is simultaneously:

1. A **quantifier**: range over elements of `schedule`
2. A **structural match**: confirm the element has `talk` and `room` fields
3. A **binding**: introduce `t` and `r` as names in the body

This tripling of roles is what makes the syntax powerful. The record pattern `{ talk = t,
room = r }` serves the same function as `Person { name = n, age = a }` in Haskell or
`Point { x, y }` in Rust — it binds named fields from a structured value.

The key design question for Evident: should non-matching elements be silently filtered
(Haskell/Erlang style) or produce a type error (Rust/Haskell function pattern style)?
For universal quantification, silently filtering non-matching elements seems wrong —
`∀ x ∈ S : P(x)` should range over all of S, not only the elements with a particular
shape. For existential (`some x ∈ S : ...`), filtering makes more sense.

---

## 10. View Patterns and Active Patterns

Standard patterns match on constructors — the static structure of a value. But
sometimes you want to match on a *computed* property, or match after applying a
normalization function. View patterns and active patterns are the mechanisms for this.

### Haskell view patterns

```haskell
{-# LANGUAGE ViewPatterns #-}

-- Syntax: (function -> pattern)
-- Apply 'function' to the scrutinee, then match the result against 'pattern'

describeMap :: Map String Int -> String
describeMap (Map.toList -> [])     = "empty"
describeMap (Map.toList -> [(k,v)]) = "singleton: " ++ k ++ " -> " ++ show v
describeMap (Map.size   -> n)      = "map with " ++ show n ++ " entries"

-- More useful: normalize before matching
describeWords :: String -> String
describeWords (words -> [])    = "no words"
describeWords (words -> [w])   = "one word: " ++ w
describeWords (words -> (w:_)) = "starts with: " ++ w
```

`(f -> pat)` applies `f` to the matched value and then matches the result against `pat`.
The function `f` can be any Haskell function — it is an ordinary function call, not a
special matching construct. This lets you match on computed views of a value without
restructuring your data or adding extra `let` bindings.

### F# Active Patterns

F# active patterns are the most developed version of this idea. They allow user-defined
constructors that behave like algebraic type constructors in patterns, but perform
arbitrary computation.

```fsharp
// Complete active pattern: partitions all inputs into named cases
let (|Even|Odd|) n =
    if n % 2 = 0 then Even (n / 2) else Odd

match 42 with
| Even half -> printfn "even, half = %d" half
| Odd       -> printfn "odd"

// Partial active pattern: matches some inputs, fails for others
// Useful for parsers:
let (|Int|_|) (s: string) =
    match System.Int32.TryParse(s) with
    | true, n -> Some n
    | _       -> None

match input with
| Int n  -> printfn "number: %d" n
| _      -> printfn "not a number"

// Parameterized active pattern:
let (|DivisibleBy|_|) divisor n =
    if n % divisor = 0 then Some (n / divisor) else None

match 15 with
| DivisibleBy 5 q -> printfn "divisible by 5, quotient = %d" q
| DivisibleBy 3 q -> printfn "divisible by 3, quotient = %d" q
| _               -> printfn "neither"
```

The `(|Name|)` syntax defines a new pattern that can appear in `match` and `function`
expressions. Partial patterns `(|Name|_|)` return `Option` — `None` means the pattern
doesn't match, causing the match to try the next arm.

### When view patterns are useful for Evident

Evident's field access (`a.slot`, `a.talk.speaker.track`) is already a form of view
pattern — it projects a sub-field of a record to match against. The current syntax
`a.talk.speaker.track = "ml"` is equivalent to `(a |> .talk.speaker.track -> "ml")`
in view-pattern notation.

A more explicit view-pattern extension could allow:

```evident
-- Computed property matching:
∀ a ∈ schedule, normalized_title a.talk.title = t : title_valid t

-- Matching on the result of a claim:
∀ a ∈ schedule, makespan_of a = d, d > max_duration : ...
```

The key question for Evident: because claims already encapsulate arbitrary computations,
`makespan_of a = d` in a body block is already a view-pattern-like construct. Explicit
view pattern syntax may not be needed if claims serve that role.

---

## Pattern Matching in Collection Operations: Readability

The most compelling application of pattern matching — and the one most relevant to
Evident — is in collection operations. When you iterate over a collection of structured
records, you need to (a) access individual elements, (b) extract their fields, and
(c) impose constraints on those fields. Pattern matching collapses these three operations
into one syntactic unit.

### Without pattern matching

```python
# Verbose: access, extract, constrain separately
for assignment in schedule:
    talk = assignment["talk"]
    room = assignment["room"]
    slot = assignment["slot"]
    if talk["duration"] <= slot["end"] - slot["start"]:
        if room["capacity"] >= talk["expected_audience"]:
            valid_assignments.append(assignment)
```

```sql
-- SQL requires explicit join syntax to access related fields:
SELECT a.* FROM assignments a
JOIN talks t ON a.talk_id = t.id
JOIN rooms r ON a.room_id = r.id
WHERE t.duration <= a.slot_end - a.slot_start
  AND r.capacity >= t.expected_audience
```

### With pattern matching in generators

```haskell
-- Haskell: the pattern in the generator simultaneously accesses and destructures
validAssignments =
  [ a
  | a@Assignment { talk = Talk { duration = d, audience = aud }
                 , room = Room { capacity = cap }
                 , slot = Slot { start = s, end = e }
                 } <- schedule
  , d <= e - s
  , cap >= aud
  ]
```

### With set comprehension and record patterns (Evident)

```evident
-- The record pattern in the quantifier collapses access + extraction + constraint:
valid_assignments =
  { a ∈ schedule |
    a.talk.duration ≤ a.slot.end - a.slot.start
    a.room.capacity ≥ a.talk.expected_audience
  }
```

Or with explicit field binding for use in multiple constraints:

```evident
∀ { talk = t, room = r, slot = s } ∈ schedule :
    t.duration ≤ s.end - s.start
    r.capacity ≥ t.expected_audience
    t.requires_av ⇒ r.has_av
```

The `{ talk = t, room = r, slot = s } ∈ schedule` pattern:
1. Ranges over `schedule` (quantification)
2. Confirms each element has `talk`, `room`, and `slot` fields (structural check)
3. Introduces `t`, `r`, `s` as short local names (binding)

Then the body uses `t`, `r`, `s` without further field access. The pattern did the
"zooming in" work; the body states the constraints. This is the maximum separation of
concerns.

### Why this matters for readability

**The cost of without-pattern-matching:** every constraint in the body must include
the full navigation path. `a.talk.duration ≤ a.slot.end - a.slot.start` repeats `a`
twice and `a.talk` / `a.slot` in each comparison. As constraints multiply, so does
the prefix noise.

**The benefit of with-pattern-binding:** once `t`, `r`, `s` are introduced by the
pattern, the constraints read as `t.duration ≤ s.end - s.start`, `r.capacity ≥ t.audience`.
The constraint content dominates; the navigation is factored out.

**The further benefit of deep patterns:** `∀ { talk = { duration = d, requires_av = av },
room = { capacity = cap, has_av = rav }, slot = { start = st, end = en } } ∈ schedule`
would introduce `d`, `av`, `cap`, `rav`, `st`, `en` directly — no dot access at all
in the body. This is most useful when the same field is accessed many times.

**The tradeoff:** deep patterns are more verbose at the binding site but more readable
at the use sites. Shallow patterns with dot access are more concise at the binding site
but noisier at the use sites. The optimal depth depends on how many times each field
is used in the body. One use: keep the dot access. Three or more uses: introduce a
binding.

---

## Synthesis: Design Recommendations for Evident

Based on this survey, several patterns stand out as most relevant to Evident's
"sets of records with quantifiers" model.

### 1. Record pattern binding in quantifiers

The most useful sugar is binding record fields directly in a quantifier/membership
constraint:

```evident
-- Bind fields in the quantifier, use short names in the body:
∀ { talk = t, room = r, slot = s } ∈ schedule :
    t.duration ≤ s.end - s.start

-- Equivalent but more verbose:
∀ a ∈ schedule :
    a.talk.duration ≤ a.slot.end - a.slot.start
```

This directly parallels Haskell's `Person { name = n, age = a } <- people` in
list comprehensions and Rust's `Point { x, y }` struct patterns.

The shorthand where field name equals binding name (no `=`):

```evident
-- When field name and binding name are the same:
∀ { talk, room, slot } ∈ schedule :
    talk.duration ≤ slot.end - slot.start
```

This mirrors Rust's `Point { x, y }` (shorthand for `Point { x: x, y: y }`) and
JavaScript's `const { name, age } = person`.

### 2. Nested field binding

For frequently-accessed nested fields:

```evident
∀ { talk = { duration = d, speaker = { track } }, slot } ∈ schedule :
    d ≤ slot.end - slot.start
    track ∈ allowed_tracks
```

This is Haskell-style nested record patterns, where each level introduces new names.

### 3. As-patterns for the whole element plus its fields

When you need both the whole record and its fields:

```evident
-- 'a' is the whole assignment; 't', 'r' are its fields:
∀ a @ { talk = t, room = r } ∈ schedule :
    assignment_valid a   -- use the whole record
    t.duration ≤ 60      -- use a field
```

This mirrors Haskell's `all@(x:rest)` and Rust's `n @ 1..=5`.

### 4. Wildcard for unused fields

When iterating over records but only accessing some fields:

```evident
∀ { talk = t, .. } ∈ schedule :   -- .. ignores room, slot
    t.speaker ∈ invited_speakers
```

This mirrors Rust's `Config { host, .. }` and Haskell's record patterns with implicit
field omission.

### 5. Pin operator for matching against an existing value

When you need to test equality against an already-bound name (not introduce a new binding):

```evident
-- target_room is already bound; filter assignments to that room only:
∀ { room = ^target_room, talk = t } ∈ schedule :
    t.duration ≤ 45
```

This mirrors Elixir's `^` pin operator, which distinguishes "match against current value"
from "introduce new binding."

### 6. View-claim patterns for computed properties

When matching on a derived property rather than a raw field:

```evident
-- Match on a claim result in the quantifier binding:
∀ a ∈ schedule, duration_minutes a = d, d > 60 :
    a.room.has_av
```

This is F# active patterns applied to Evident's claim system: `duration_minutes a = d`
calls the `duration_minutes` claim on `a` and binds the result to `d`, which can then
appear in constraints. Because claims in Evident already serve as computed properties,
this pattern emerges naturally.

---

## Quick Reference: Syntax Comparison

| Operation | Haskell | Rust | Python 3.10+ | Elixir | Evident (proposed) |
|---|---|---|---|---|---|
| Bind field by name | `Person { name = n }` | `Person { name }` | `Person(name=n)` | `%Person{name: n}` | `{ name = n }` |
| Bind and name whole | `all@(Person { name })` | `p @ Person { name }` | — | — | `p @ { name }` |
| Skip unused fields | implicit | `Point { x, .. }` | — | — | `{ x, .. }` |
| Sequence head+tail | `(x:xs)` | `[head, tail @ ..]` | `[first, *rest]` | `[h \| t]` | `[h \| t]` |
| Match on computed | view patterns | — | — | — | `claim a = v` in body |
| Pin existing value | N/A | — | — | `^x` | `^x` |
| OR alternatives | N/A in patterns | `1 \| 2 \| 3` | `x \| y` | — | — |
| Guard condition | `\| condition` | `if condition` | `if condition` | `when condition` | `when condition` |
| Universal over set | list comprehension | — | — | list comprehension | `∀ x ∈ S :` |
| Existential witness | `[x \| Just x <- xs]` | — | — | `[x \|\| {:ok, x} <- xs]` | `some x ∈ S :` |

The most important column for Evident is the last. The proposed syntax draws from
Haskell's record patterns (named field binding), Rust's `..` (unused field wildcard),
Elixir's `^` (pin operator), and Haskell/Erlang's generator patterns (binding-as-
quantification). The combination makes Evident's quantifier syntax do in one expression
what other languages need two or three separate constructs to achieve.
