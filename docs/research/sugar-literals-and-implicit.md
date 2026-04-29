# Syntactic Sugar, Collection Literals, and Implicit Operations

Research for the Evident constraint programming language design project. Goal: survey how languages use shorthand notation, implicit operations, and literal syntax to make collection-oriented code concise and readable — and derive lessons for a set-centric constraint language.

---

## 1. Collection Literal Syntax Across Languages

### Why literal syntax matters

Literal syntax is not mere convenience. It signals what the language considers *primitive*. A language with `[1, 2, 3]` for arrays is saying "sequences are central." A language with `{1, 2, 3}` for sets says "membership without order is central." The absence of a literal forces the programmer to write construction code, which creates visual weight that signals "this is unusual."

### Python

```python
lst    = [1, 2, 3]          # list — ordered, mutable, allows duplicates
tup    = (1, 2, 3)          # tuple — ordered, immutable, allows duplicates
dct    = {"a": 1, "b": 2}   # dict — key-value, insertion-ordered (3.7+)
s      = {1, 2, 3}          # set — unordered, no duplicates
empty  = set()              # NOT {} — that's an empty dict
fzn    = frozenset({1, 2})  # immutable set

# Set comprehension
evens  = {x for x in range(10) if x % 2 == 0}

# Dict comprehension
sq_map = {x: x**2 for x in range(5)}
```

**Observation:** Python's `{}` is overloaded between dict (empty or with `key: value`) and set (with bare elements). The `set()` constructor is required for the empty set. This is a genuine wart — the empty set has no natural literal.

**Why Python has set literals (added in 3.0):** The argument was that sets are used frequently enough in modern Python (membership testing, deduplication) that the visual overhead of `set([...])` is unjustified. The 3.x transition window allowed breaking the `{}` convention. Older Pythons never added set literals precisely because `{}` was already taken.

### Clojure

```clojure
(def v    [1 2 3])          ; vector — ordered, indexed, the "default sequence"
(def l    '(1 2 3))         ; list — linked, used as code structure
(def m    {:a 1 :b 2})      ; map — hash map
(def s    #{1 2 3})         ; set — hash set, the # prefix distinguishes from map

(conj s 4)                  ; => #{1 2 3 4}
(disj s 2)                  ; => #{1 3}
(s 2)                       ; => 2   (sets are functions of their elements)
(contains? s 2)             ; => true
```

**Why Clojure gets this right:** Every collection type has a *distinct literal prefix*. There is no overloading: `[]` is always a vector, `{}` is always a map, `#{}` is always a set, `'()` is always a list. The `#` prefix for sets is mnemonic for "number of elements" — it signals cardinality/distinctness. This means readers can always identify the collection type from the first character.

**Sets as functions:** In Clojure, a set acts as its own membership predicate: `(s element)` returns the element if it is a member, or `nil` otherwise. This makes sets composable with higher-order functions — `(filter my-set coll)` filters `coll` to elements present in `my-set`. No separate `member?` call.

### JavaScript

```javascript
const arr = [1, 2, 3];              // array
const obj = {a: 1, b: 2};          // object/dict
const s   = new Set([1, 2, 3]);    // set — no literal syntax

// Set operations require construction
const union = new Set([...a, ...b]);
const inter = new Set([...a].filter(x => b.has(x)));
const diff  = new Set([...a].filter(x => !b.has(x)));

// ES2025: native set methods
a.union(b)
a.intersection(b)
a.difference(b)
a.symmetricDifference(b)
a.isSubsetOf(b)
a.isDisjointFrom(b)
```

**Why JavaScript has no set literal:** JavaScript's `{}` syntax was fixed for objects before sets became standard (ES6). Adding a set literal would require either a new prefix sigil (like Clojure's `#{}`) or breaking `{}` semantics — neither was acceptable given JavaScript's compatibility constraints. The result is `new Set([...])`, which has three levels of syntactic overhead: `new`, the constructor name, and the array wrapping.

**Lesson:** The absence of a set literal in JavaScript is a design debt from early commitment to `{}` for objects. It makes sets feel second-class even though they are semantically the right data structure for many problems. Evident, which is *about* sets, must have a literal form. The cost of having no literal is felt every time a programmer writes a set.

### Haskell

```haskell
-- Lists (the primary sequence type)
lst      = [1, 2, 3]          -- list literal
empty    = []                 -- empty list
range1   = [1..10]            -- [1,2,3,4,5,6,7,8,9,10]
range2   = [1,3..10]          -- [1,3,5,7,9]  (step inferred)
range3   = [1..]              -- infinite: 1,2,3,...
chars    = ['a'..'z']         -- works on any Enum type

-- No set literal; sets require import and construction
import Data.Set (Set)
import qualified Data.Set as Set
s = Set.fromList [1, 2, 3]

-- List comprehension (closest to set-builder notation)
evens  = [x | x <- [1..20], even x]
pyth   = [(a,b,c) | c <- [1..100], b <- [1..c], a <- [1..b], a^2 + b^2 == c^2]
```

**Why Haskell has no set literal:** Haskell's type class system means that `[1, 2, 3]` desugars to list construction uniformly. Sets require `Ord` on elements, which cannot be inferred from syntax alone. Adding a set literal would require either a dedicated syntax that hard-codes the `Ord` constraint or compiler magic. The language chose to keep list syntax simple and use library construction for sets.

**Range syntax in Haskell:** `[1..10]` desugars to `enumFromTo 1 10`. Any type implementing `Enum` can be ranged over: integers, characters, even custom enumerations. The step form `[1,3..10]` desugars to `enumFromThenTo 1 3 10`, inferring step from the first two elements. This is elegant — it works without special syntax rules, just typeclass dispatch.

### Ruby

```ruby
arr   = [1, 2, 3]            # Array
hash  = {a: 1, b: 2}         # Hash (symbol keys with shorthand)
range = (1..10)              # Range, inclusive — 1 through 10
excl  = (1...10)             # Range, exclusive — 1 through 9
chars = ('a'..'z')           # Character range

# Word array shorthand
words  = %w[apple banana cherry]    # => ["apple", "banana", "cherry"]
syms   = %i[apple banana cherry]    # => [:apple, :banana, :cherry]

# No set literal; Ruby's Set is a library class
require 'set'
s = Set.new([1, 2, 3])
s = Set[1, 2, 3]             # shorter construction form

# Ranges are lazy; convert to array with .to_a
(1..5).to_a   # => [1, 2, 3, 4, 5]
```

**Ruby's range as a DSL pattern:** Ruby's `..` and `...` appear in many contexts: array slicing, `case`/`when` guards, `Comparable#between?`, and custom DSLs. The `...` exclusive form is particularly useful for array slicing where you want to exclude the endpoint. ActiveRecord uses Range in queries: `where(age: 18..30)` generates `WHERE age BETWEEN 18 AND 30`.

**`%w[]` and `%i[]`:** These are special literal forms for arrays of strings and symbols without needing to quote each element or separate with commas. They read like whitespace-delimited data, similar to shell word-splitting. For Evident, this pattern (a literal form optimized for collections of symbolic names) is interesting — enumerating a set of allowed values should not require syntactic overhead per element.

### Swift

```swift
var arr: [Int]   = [1, 2, 3]        // Array literal
var dict: [String: Int] = ["a": 1]  // Dictionary literal
var set: Set<Int> = [1, 2, 3]       // Set — same bracket syntax as Array!
var emptyArr: [Int] = []            // Empty array
var emptyDict: [String: Int] = [:]  // Empty dict (distinguishes from array)
var emptySet: Set<Int> = []         // Empty set (type annotation required)
```

**Swift's type-context-dependent literals:** In Swift, `[1, 2, 3]` can be an `Array`, a `Set`, or anything implementing `ExpressibleByArrayLiteral`. The type is resolved from context. This eliminates the need for distinct sigils, but requires type annotations in ambiguous cases. The empty set `[]` is identical to the empty array `[]` — the type annotation `Set<Int>` is what disambiguates. This is clever but can be confusing: the same literal means different things in different contexts.

### Why some languages have set literals and others don't

The pattern is clear:

| Condition | Example | Set literal? |
|---|---|---|
| `{}` already taken for maps/objects | JavaScript, Python (partly) | No |
| Language is order-neutral by design | Clojure | Yes (`#{}`) |
| Sets added after `{}` was committed | Python, JS | Partial (Python yes, JS no) |
| Type system requires type annotation | Swift, Haskell | No (contextual or library) |
| Language is math/logic oriented | Evident (proposed) | Must have one |

The core tension is always `{}` vs. a distinct sigil. Languages that committed `{}` to dict/object syntax before sets became first-class either have no set literal (JavaScript), a compromised literal (Python's `{1,2,3}` with the empty-set hole), or a prefix sigil (Clojure's `#{}`).

---

## 2. Range Syntax

### Inclusive vs. exclusive bounds

| Syntax | Language | Meaning |
|---|---|---|
| `1..10` | Ruby, Rust | 1 through 10 *inclusive* |
| `1...10` | Ruby | 1 through 9 *exclusive* of end |
| `1..=10` | Rust | 1 through 10 *inclusive* (explicit) |
| `1..<10` | Swift, Kotlin | 1 through 9 *exclusive* |
| `1..10` | Haskell `[1..10]` | 1 through 10 *inclusive* |
| `range(1, 10)` | Python | 1 through 9 *exclusive* of end |
| `1:10` | MATLAB, Julia | 1 through 10 *inclusive* |
| `1:3:10` | MATLAB | 1, 4, 7, 10 (start:step:end) |

**The inclusive/exclusive confusion is real.** Ruby's `..` vs. `...` is mnemonically weak — three dots means one fewer element. Rust's `..=` is explicit at the cost of a third character. Swift's `..<` encodes the direction of exclusion (less-than at the end). The cleanest solution found in practice is Python's `range(start, stop)` convention where `stop` is always exclusive, making `range(0, n)` have `n` elements — but this requires a function call rather than syntax.

### Range as lazy sequence vs. set

A range can be:
- A **lazy sequence**: evaluated element by element on demand. Haskell's `[1..]` is infinite; it is only safe because Haskell is lazy.
- A **closed interval**: a set-like object that knows its bounds but does not enumerate. Ruby's `Range` has `#include?` without enumerating.
- An **actual collection**: fully realized. Python's `list(range(1, 10))` materializes all elements.

For Evident, the distinction matters: a constraint `x in 1..100` does not require enumerating 100 values — it is a membership test against an interval predicate. A range used in set comprehension (`{x | x in 1..100, prime(x)}`) also does not enumerate all 100 values if the solver is smart enough to propagate the constraint.

**Key insight:** Ranges are most useful as *constraint expressions* — specifying the domain of a variable — not as sequences to iterate. `age in 18..65` reads clearly and does not need to create a 48-element set. The range syntax should be a first-class constraint form, not just sugar for a collection.

### When ranges are most useful

1. **Domain specification:** `x in 0..255` (byte values), `n in 1..7` (days of week)
2. **Slicing:** `xs[2..5]` — a subsequence without full enumeration
3. **Case guards:** `when score in 90..100` — readable case coverage
4. **Generating test cases:** when you need a sequence for enumeration
5. **Cardinality constraints:** `|results| in 1..10` — count must be in range

---

## 3. Implicit Iteration and Broadcasting

### NumPy broadcasting

```python
import numpy as np

a = np.array([1, 2, 3])
b = np.array([10, 20, 30])
c = a + b              # [11, 22, 33] — element-wise, no loop
d = a * 2              # [2, 4, 6] — scalar broadcasts to all elements
e = a > 1              # [False, True, True] — predicate broadcasts

# Multi-dimensional broadcasting
m = np.array([[1,2,3],[4,5,6]])  # shape (2, 3)
v = np.array([10, 20, 30])      # shape (3,) — broadcasts along rows
m + v   # [[11,22,33],[14,25,36]]
```

Broadcasting is a *default behavior*: operations on arrays implicitly iterate. There is no `map` keyword; `a + b` means "add element-wise." This works because the language makes a commitment: scalar operations on arrays are always lifted.

**Readability impact:** NumPy code reads like mathematical notation on scalars even when operating on arrays. `norm = np.sqrt(np.sum(x**2))` is conceptually `√(Σ xᵢ²)`. The tradeoff is that you must understand broadcasting rules to know when `a + b` operates element-wise vs. when it is a scalar add — shape errors happen at runtime, not at a glance.

### APL's array-at-a-time operations

APL was the original language to make implicit iteration the design center:

```apl
+/ 1 2 3 4 5       ⍝ 15 — reduce with addition
×/ 1 2 3 4 5       ⍝ 120 — reduce with multiplication
1 2 3 + 4 5 6      ⍝ 5 7 9 — element-wise add
2 × 1 2 3          ⍝ 2 4 6 — scalar broadcast
1 2 3 < 2 2 2      ⍝ 1 1 0 — element-wise comparison
```

APL's key design principle: there are no loops in the language. Everything is an array operation. The discipline forces programmers to think in transformations on collections rather than element-wise procedures. The payoff is extreme density — programs that would be 10 lines in Python fit on one line. The cost is that reading APL requires fluency in the entire operator vocabulary.

### R's vectorized operations

```r
x <- c(1, 2, 3, 4, 5)
x * 2          # c(2, 4, 6, 8, 10)
x > 3          # c(FALSE, FALSE, FALSE, TRUE, TRUE)
x[x > 3]      # c(4, 5) — subset by boolean mask

# Apply family
sapply(x, function(v) v^2)    # c(1, 4, 9, 16, 25)
lapply(lst, f)                # list of results
tapply(values, groups, mean)  # split-apply-combine
```

R's vectorization is default for primitive operations but not for user-defined functions (those require `sapply`/`vapply`). This inconsistency is a source of confusion — `x * 2` works implicitly but `f(x)` does not vectorize unless `f` was written with vector inputs in mind.

### Lesson for Evident

Implicit iteration is powerful but requires commitment: either everything lifts (APL, NumPy for primitives), or nothing does, or you provide explicit lift operators. The danger of partial implicit iteration (some things lift, some don't) is confusion about when lifting happens.

For Evident, implicit iteration over sets could be the right default for constraint propagation: `age > 18` where `age` is a domain variable implicitly filters the domain. This maps well to constraint propagation semantics — a constraint on a variable implicitly filters the set of values that variable can take. The *set* is not enumerated; the constraint is applied.

---

## 4. The Dot (`.`) Shorthand

### jq's `.field` on arrays

```jq
# Input: [{"name": "alice", "age": 30}, {"name": "bob", "age": 25}]
.[].name          # "alice", "bob" — iterate array, project field
[.[].name]        # ["alice", "bob"] — collect into array
map(.name)        # ["alice", "bob"] — equivalent, more explicit
.[0].name         # "alice" — index then project
.[] | select(.age > 27) | .name    # "alice" — filter then project
```

In jq, `.field` on an *array* of objects automatically iterates the array and projects the field on each element. The iteration is implicit — `.` means "current input," and if the current input is an array, `.[]` explodes it. This makes pipelines on arrays of records feel like operating on a single record.

### Swift's `$0.field` in closures

```swift
let names = people.map { $0.name }
let adults = people.filter { $0.age >= 18 }
let total = orders.reduce(0) { $0 + $1.amount }
```

`$0` is the implicit first parameter of a closure when no parameter list is given. For single-argument closures operating on a field, this eliminates the `person in person.name` boilerplate. The trade is that `$0` loses the meaningful name `person` — for simple projections this is fine; for complex logic, `{ person in ... }` is clearer.

### Kotlin's `it`

```kotlin
val names = people.map { it.name }
val adults = people.filter { it.age >= 18 }
val sorted = people.sortedBy { it.lastName }
```

`it` is Kotlin's implicit single-parameter name in lambdas. It behaves identically to `$0` but is a word, which some find more readable. The tradeoff is the same: meaningful in simple projections, confusing in nested lambdas where `it` can shadow an outer `it`.

### JavaScript optional chaining `?.`

```javascript
user?.address?.city           // undefined if user or address is null
arr?.[0]?.name               // safe array index + field access
obj?.method?.()              // safe method call
people.map(p => p?.name)     // field access on possibly-null elements
```

`?.` short-circuits to `undefined` if the left side is `null` or `undefined`. It is a null-propagation operator, not strictly a collection operation, but it composes with iteration: `people.map(p => p?.contact?.email)` handles missing contact records without explicit null checks.

### XPath's `//element`

```xpath
//book              # all book elements anywhere in document
//book/title        # title elements inside any book
//book[@price<10]   # books with price attribute < 10
//@price            # all price attributes
.//book             # books anywhere under current node
```

XPath treats the entire document as a navigable tree. `//` means "recursive descent" — find this element at any depth. `.` means the current context node. Operations on paths implicitly iterate over all matching nodes: `//book/price` returns all price elements in all books, not just the first.

**Lesson:** XPath shows that implicit recursive iteration over a collection structure can be incredibly terse. For tree-shaped data, `//field` reads like "give me all `field` values" — the collection structure is implicit. For Evident's constraint claims over structured data, a similar recursive projection could be powerful: given a graph of claims, traversing the evidence tree to find all instances of a sub-claim is the natural operation.

---

## 5. Anonymous Function Shorthand

### Scala's `_` (wildcard parameter)

```scala
val evens = nums.filter(_ % 2 == 0)           // x => x % 2 == 0
val doubled = nums.map(_ * 2)                  // x => x * 2
val sum = nums.reduce(_ + _)                   // (a, b) => a + b
val names = people.map(_.name)                 // p => p.name
val sorted = people.sortBy(_.age)              // p => p.age
```

Scala's `_` in a lambda context means "the next unbound argument in order." Each `_` in `_ + _` binds to a different parameter. This is extremely terse but can be confusing: `_ => _ > 0` means something different from `_ > 0` (the first is a function that ignores its argument; the second is a function that tests its argument). The `_` is position-sensitive and single-use per parameter position.

### Kotlin's `it`

Already covered in section 4. Key difference from Scala: `it` is a single named implicit parameter. You cannot write `it + it` to mean "add first and second arguments." For multi-argument lambdas, named parameters are required.

### Ruby's `(&:method)` symbol-to-proc

```ruby
["hello", "world"].map(&:upcase)       # ["HELLO", "WORLD"]
[1, -2, 3, -4].select(&:positive?)    # [1, 3]
[1, 2, 3].map(&:to_s)                 # ["1", "2", "3"]
[words].sort_by(&:length)             # sorted by string length
```

`&:method_name` converts a Symbol to a Proc that calls that method on its first argument. This is the most common anonymous function shorthand in idiomatic Ruby because most collection operations are "call this method on each element." It eliminates `{ |x| x.upcase }` entirely when the operation is a single method call.

**Key design insight:** In most `map` calls, you are projecting a field or calling a method. A shorthand that captures this pattern — "apply this named operation to each element" — eliminates most anonymous function boilerplate in practice.

### Haskell sections

```haskell
filter (> 0) xs          -- filter xs to positive elements
map (* 2) xs             -- double all elements
map (+ 1) xs             -- increment all
filter (== 'a') str      -- keep only 'a' characters
zipWith (+) xs ys        -- element-wise addition
filter (`elem` valid) xs -- filter to elements in valid set
```

An operator section `(> 0)` is a partially applied binary operator with one argument missing. `(> 0)` means `\x -> x > 0`; `(2 *)` means `\x -> 2 * x`. This works because in Haskell, all binary operators are functions of two arguments, and partial application is the default. Sections are arguably the cleanest shorthand across all languages: they look like the operator with a hole, which is exactly what they mean.

### Swift's `$0`, `$1`, `$2`...

```swift
let sorted = people.sorted { $0.age < $1.age }
let pairs = zip(a, b).map { ($0, $1) }
let product = nums.reduce(1, *)           // operator reference
```

Swift uses `$0`, `$1` for anonymous closure parameters. The numbered form makes it explicit which parameter you mean, avoiding the Scala ambiguity where `_` is positional but implicit. The trailing-closure syntax `sorted { ... }` is also notable — when the last argument is a closure, it can move outside the parentheses, reducing nesting.

**Swift operator references:** `reduce(1, *)` passes the `*` operator directly as a function. This is similar to Haskell's operator sections but without partial application — the operator must have the right arity.

### The universal pattern

Across these languages, anonymous function shorthand appears in three patterns:

| Pattern | Example | What it solves |
|---|---|---|
| **Implicit single parameter** | `it.name`, `$0.name` | `{ p -> p.name }` |
| **Method reference** | `&:upcase`, `String::toUpperCase` | `{ s -> s.upcase }` |
| **Partial operator application** | `(> 0)`, `(_ > 0)` | `{ x -> x > 0 }` |

All three reduce "one thing applied to each element" to its essence. The ideal shorthand for Evident would cover all three cases: field projection, named constraint, and inline comparison.

---

## 6. String Interpolation as a Model

### How interpolation works

```ruby
"Hello, #{name}! You are #{age} years old."         # Ruby
f"Hello, {name}! You are {age} years old."          # Python f-string
`Hello, ${name}! You are ${age} years old.`         # JavaScript template literal
s"Hello, $name! You are ${age + 1} years old."     # Scala
"Hello, \(name)! You are \(age) years old."         # Swift
```

String interpolation embeds an *expression* inside a *literal context*. The string is still a string, but it now contains live code. The parser must recognize the interpolation delimiters and switch modes: literal text until `#{`, then expression, then back to literal text.

### What interpolation teaches us

**Embedding expressions in literals makes the template vs. computation distinction visible.** In `"Total: #{items.sum}"`, the string is the template and `items.sum` is the computation. Without interpolation, you write `"Total: " + items.sum.to_s`, which obscures the template structure.

**The same principle applies to set builder notation.** `{ x | x in students, gpa(x) > 3.5 }` has a similar structure: the set literal `{ ... }` is the context, and `gpa(x) > 3.5` is the condition embedded inside it. The readability of set-builder notation comes from the same effect as string interpolation: the literal container makes the "shape" obvious, and the conditions are visually embedded within it.

**Could set operations benefit from inline notation?** Consider:

```evident
-- Interpolation-style set refinement (hypothetical)
valid_users = {u | u in all_users, active(u), verified(u)}

-- Embedding a constraint inline in a claim (analogous to interpolation)
evident valid_request(req) when req.method in {GET, POST, PUT, DELETE}
```

The `{GET, POST, PUT, DELETE}` reads as a set literal embedded in a constraint expression — the same visual effect as interpolation. The programmer sees "this thing is drawn from a fixed set of values" at a glance.

### Lessons for Evident

String interpolation works because the template and the embedded expression have *different visual weight*: the template is plain text, the expression is inside a delimiter. For Evident:
- Set literals `{...}` visually signal "this is a fixed collection of values"
- Constraint expressions embedded in claims should have similar visual framing
- The reader should be able to see at a glance: "this is the collection, and this is the constraint on membership"

---

## 7. Spread and Splat Operators

### Python `*args` and `**kwargs`

```python
def f(*args, **kwargs): ...        # collect remaining positional / keyword args

# Spread in function call
items = [1, 2, 3]
print(*items)                      # same as print(1, 2, 3)

# Spread in list/set construction
combined = [*list1, *list2]        # concatenation via spread
merged   = {**dict1, **dict2}      # dict merge via spread (later wins)
union    = {*set1, *set2}          # set union via spread
```

The `*` operator in Python serves double duty: **collect** (in function definition) and **spread** (in calls and literals). The collect form captures remaining arguments into a tuple; the spread form expands an iterable into a context expecting multiple values.

### JavaScript `...spread`

```javascript
// Spread in array
const combined = [...arr1, ...arr2, extraItem];

// Spread in object
const merged = {...obj1, ...obj2, override: value};

// Spread in function call
Math.max(...nums);

// Rest in destructuring
const [first, ...rest] = arr;
const {a, b, ...others} = obj;
```

JavaScript's `...` is syntactically unified across collect and spread uses, and works in object literals (dict merge), array literals (concatenation), and set construction (`new Set([...a, ...b])`).

### Ruby `*splat`

```ruby
def f(first, *rest)              # collect remaining into array
  [first, rest]
end

a, *b, c = [1, 2, 3, 4]        # a=1, b=[2,3], c=4
arr = [*arr1, *arr2]            # array concatenation via splat
[1, *middle, 5]                 # array with spread middle

# Double splat for hash
merged = {**hash1, **hash2}     # Ruby 2.7+
```

Ruby's `*` in destructuring is particularly expressive: `a, *b, c = array` assigns the first element to `a`, the last to `c`, and everything in between to `b`. This is "collect the middle" destructuring, which most languages cannot express so concisely.

### Spread as a set operation

Spread operators are syntactic sugar for collection union (for sets) or concatenation (for sequences). The important insight is that spread makes *construction from parts* as readable as constructing from scratch. Without spread:

```python
# Without spread — must name the intermediate
combined_items = list(set1) + list(set2) + [extra_item]
# With spread — the structure is visible
combined_items = [*set1, *set2, extra_item]
```

For Evident, the analog is composing sets of valid values: `{*valid_statuses, provisional}` meaning "the set of valid statuses plus the provisional value." Or spreading one claim's domain into another constraint.

---

## 8. Index and Slice Notation

### Python slicing

```python
xs = [0, 1, 2, 3, 4, 5]
xs[1:4]        # [1, 2, 3] — from index 1 up to (not including) 4
xs[::2]        # [0, 2, 4] — every other element
xs[::-1]       # [5, 4, 3, 2, 1, 0] — reverse
xs[-1]         # 5 — last element
xs[-3:]        # [3, 4, 5] — last three elements
xs[1:4:2]      # [1, 3] — start:stop:step
```

Python's slice syntax `[start:stop:step]` with optional components is the most expressive slice notation in common use. All three parts are optional; omitting `start` means from the beginning, omitting `stop` means to the end, omitting `step` means step of 1. Negative indices count from the end.

NumPy extends this to multiple dimensions: `matrix[0:3, 1:4]` selects rows 0-2, columns 1-3.

### Haskell: slice as function composition

```haskell
-- No slice syntax; composed from primitives
take 3 . drop 1 $ xs    -- [1, 2, 3] from [0, 1, 2, 3, 4, 5]
take 3 $ drop 1 xs      -- equivalent
init xs                  -- all but last
tail xs                  -- all but first
last xs                  -- last element
head xs                  -- first element
splitAt 3 xs             -- ([0,1,2], [3,4,5])
```

Haskell has no slice syntax — slicing is function composition. `take 3 . drop 1` reads as "take 3 after dropping 1." This is composable but verbose compared to `[1:4]`.

### MATLAB and Julia

```matlab
xs(2:4)          % MATLAB — indices 2, 3, 4 (1-indexed, inclusive)
xs(end-2:end)    % last three elements
xs(1:2:end)      % every other element
matrix(2:4, 1:3) % sub-matrix
```

```julia
xs[2:4]          # Julia — indices 2, 3, 4 (1-indexed, inclusive)
xs[end-2:end]    # last three elements
xs[1:2:end]      # every other element
```

MATLAB and Julia use 1-based indexing with inclusive ranges, which makes `1:n` have exactly `n` elements. The `end` keyword refers to the last index, making `end-k` patterns readable.

### Slicing as a set operation on ordered collections

From a set-theory perspective, slicing an ordered collection is selecting a subset by *position* rather than by *predicate*. It is a special case of `filter` where the predicate is based on index: `{ xs[i] | i in start..stop }`.

For Evident, positional slicing is a sequence operation, not a set operation in the strict sense. Sets are unordered — there is no "first three elements of a set." However, if Evident supports ordered sequences as a derived type (a set with a total order), slicing becomes meaningful. The syntax decision: use the established `[start:stop]` convention or express it as a constraint on position.

---

## 9. Implicit `self` / `this` in Collection Operations

### ActiveRecord's implicit receiver

```ruby
# ActiveRecord — no explicit receiver needed
User.where(age: 18..30)          # WHERE age BETWEEN 18 AND 30
User.where(active: true)
     .order(:created_at)
     .limit(10)

# Inside a model scope
scope :adults, -> { where(age: 18..) }
scope :active, -> { where(active: true) }

# Chained without explicit variable
User.adults.active.order(:name)
```

ActiveRecord's DSL works by having each method return the same query object (a "scope"), making `where`, `order`, `limit` chainable without naming an intermediate variable. The "implicit receiver" is the query being built — each method extends the same query.

### Smalltalk's cascades

```smalltalk
OrderedCollection new
    add: 1;
    add: 2;
    add: 3;
    yourself.
```

The `;` cascade operator sends multiple messages to the same receiver without repeating the receiver name. `add: 1; add: 2; add: 3` sends all three `add:` messages to the same collection.

### Builder patterns in modern languages

```kotlin
// Kotlin DSL
buildList {
    add(1)
    add(2)
    addAll(listOf(3, 4, 5))
}

buildMap {
    put("a", 1)
    put("b", 2)
}

// apply block — implicit `this` in scope
person.apply {
    name = "Alice"
    age = 30
}
```

Kotlin's `apply`, `also`, `run`, and `with` blocks all establish an implicit receiver. Inside `apply { ... }`, `this` is the object being configured. Field access and method calls on that object do not require an explicit receiver — it is "as if you were inside the object."

### Implications for Evident

In constraint programming, the "implicit receiver" is often the variable being constrained. When writing `valid_request(req)`, every sub-claim has `req` as an implicit context — `method_allowed(req)`, `auth_valid(req)`, etc. A syntax that makes this implicit could eliminate significant repetition:

```evident
-- Explicit (current approach)
evident valid_request(req)
    method_allowed(req)
    auth_valid(req)
    content_type_valid(req)

-- With implicit receiver (hypothetical)
evident valid_request(req)
    method_allowed     -- req is implicit
    auth_valid         -- req is implicit
    content_type_valid -- req is implicit
```

This is the ActiveRecord pattern applied to claim decomposition. The benefit is that when all sub-claims share the same primary argument, it need not be repeated. The cost is that the implicit receiver must be unambiguous — it fails when sub-claims have different primary arguments or when the primary argument is not the first parameter.

---

## 10. Symbol-to-Proc and Method References

### Ruby's `&:method`

```ruby
["hello", "world"].map(&:upcase)
# equivalent to: .map { |s| s.upcase }

[1, -2, 3].select(&:positive?)
# equivalent to: .select { |n| n.positive? }

people.sort_by(&:last_name)
# equivalent to: .sort_by { |p| p.last_name }

# Explicit proc from symbol
upcase_proc = :upcase.to_proc        # => #<Proc>
upcase_proc.call("hello")            # => "HELLO"
```

Ruby's `&:method` converts a symbol to a one-argument proc that calls the named method on its argument. This works for any method with no arguments. It is the most-used shorthand in idiomatic Ruby because "apply this method to each element" is the dominant pattern in collection processing.

### Java's method references (`::`)

```java
List<String> names = people.stream()
    .map(Person::getName)          // method reference
    .collect(Collectors.toList());

// Types of method references
String::toUpperCase     // instance method reference (unbound)
System.out::println     // instance method reference (bound to System.out)
String::new             // constructor reference
Arrays::sort            // static method reference

// Used with streams
people.stream()
    .filter(Person::isActive)
    .map(Person::getName)
    .sorted(String::compareTo)
```

Java's `::` is more flexible than Ruby's `&:`: it works for static methods, instance methods (bound or unbound), and constructors. Unbound instance method references (`Person::getName`) function as functions `Person -> String` — the first argument is the receiver.

### Kotlin's `::method`

```kotlin
val names = people.map(Person::name)        // property reference
val lengths = words.map(String::length)     // property reference
val parsed = strings.map(String::toInt)     // method reference
val predicate = ::isValid                   // reference to top-level function
val bound = person::name                    // bound reference to specific instance
```

Kotlin's `::` works on properties as well as methods, which is important in Kotlin where `person.name` is a property, not a method call. `Person::name` is a function `Person -> String` that accesses the `name` property.

### Python's `operator` module and `attrgetter`/`itemgetter`

```python
from operator import attrgetter, itemgetter, methodcaller

people.sort(key=attrgetter('last_name'))        # p.last_name
data.sort(key=itemgetter('price'))              # d['price']
results = map(methodcaller('upper'), words)     # w.upper()
results = map(methodcaller('strip', '.,'), words)  # w.strip('.,')

# attrgetter supports nested access
key = attrgetter('address.city')                # p.address.city
```

Python lacks a built-in shorthand, so the `operator` module provides factory functions. `attrgetter('name')` returns a function that accesses the `name` attribute. `itemgetter(0)` returns a function that accesses index 0. These are composable — `attrgetter('address.city')` does a two-level access.

### What these patterns share

All method/field reference shorthand addresses the same pattern: *project a named field or apply a named method to each element of a collection*. This is a set projection in mathematical terms: given a set of `Person` objects, `map(Person::name)` is the image of the set under the `name` projection function.

For Evident, the equivalent is projecting a claim over a collection of values: if `valid_user(u)` holds for each `u` in `users`, that is a universal quantification. A shorthand for "this constraint applies to every element of this set" would directly express that pattern.

---

## 11. The `@` Binding in Patterns

### Haskell's `@` pattern

```haskell
-- Without @: you must decompose and can't refer to whole
process (x:rest) = doSomething rest

-- With @: name the whole, also decompose
process all@(x:rest) = doSomething all rest    -- all is the full list
process all@[]       = handleEmpty all

-- Practical uses
firstAndAll xs@(x:_) = (x, xs)    -- access head without losing list
summarize list@(first:_:_) = 
    "List of " ++ show (length list) ++ 
    " starting with " ++ show first
```

The `@` pattern (pronounced "as") simultaneously names the whole value and pattern-matches on its structure. `all@(x:rest)` means "match this value as both a list with head `x` and tail `rest`, AND bind the whole thing to `all`." This eliminates the need to reconstruct the whole from its parts.

### Rust's `ref @` binding

```rust
match value {
    ref x @ Some(n) if n > 10 => {
        println!("Large: {:?}", x);  // x is &Some(n), n is i32
    }
    _ => {}
}

// In let bindings
let ref x @ Some(n) = some_option;

// Nested @ bindings
let nested @ (a @ 1..=5, b) = (3, "hello");
// nested = (3, "hello"), a = 3, b = "hello"
```

Rust's `@` works in pattern position and is commonly used with range patterns to capture both the value and verify it falls in a range: `n @ 1..=10` means "bind the value to `n`, and also verify it is between 1 and 10."

### Erlang/Elixir's `=` in patterns

```elixir
# Elixir — the = in pattern context is match + bind
case value do
  [head | tail] = list ->      # head, tail, and list all bound
    IO.puts("List: #{inspect list}, head: #{head}")
end

# Practical: avoid reconstructing the list from head + tail
def process([first | rest] = full_list) do
  {first, rest, length(full_list)}
end
```

Elixir uses `= pattern` inside a pattern to bind the whole while also decomposing. This is equivalent to Haskell's `@` but uses the `=` sign.

### Why `@` matters for Evident

In constraint decomposition, you often need to refer to both a structured value and its components:

```evident
-- Without @: must re-reference sub-components of req
evident valid_request(req)
    method_allowed(req.method)
    auth_valid(req.auth)

-- With @: the whole and its parts can be named simultaneously
evident valid_request(req @ {method, auth, body})
    method_allowed(method)     -- using destructured part
    auth_valid(auth)           -- using destructured part
    logged(req)                -- using whole
```

The `@` pattern, or a destructuring syntax in claim parameters, lets you simultaneously name the compound value and introduce names for its components without repeated access. This is particularly useful when some sub-claims need the whole and others need only parts.

---

## 12. What Evident Could Learn

### Which shorthand notations matter most for a constraint language

The table below ranks shorthand patterns by relevance to Evident's domain:

| Pattern | Priority | Reason |
|---|---|---|
| Set literal `{...}` | Critical | Sets are the primary data structure; must have a literal |
| Range `a..b` | High | Domain specification is fundamental to constraint programming |
| Field projection shorthand | High | `.field` access on constrained records appears everywhere |
| Universal/existential quantifiers | High | `∀ x ∈ S: P(x)` vs `∃ x ∈ S: P(x)` are constraint primitives |
| Implicit single parameter (`it`, `$0`) | Medium | Useful in claim bodies; named parameters may be clearer |
| Method/field reference (`::field`) | Medium | Projection over sets of structured values |
| `@` destructuring binding | Medium | Needed when claim arguments are compound structures |
| Spread operator | Low-Medium | Composing sets of allowed values |
| String-style interpolation inline | Low | Less relevant than structured set expressions |
| Implicit receiver | Low | Useful but risks ambiguity in multi-argument claims |

### What makes code "obviously correct" at a glance

**Structural mapping to the domain model.** The most legible code is code where the syntax mirrors the structure of the problem. For constraints: a set literal for a fixed domain, a range for an interval, a universal quantifier for "all elements must satisfy," an existential for "at least one must satisfy." When the code *looks like* the mathematical specification, checking correctness is visual comparison.

**Visual distinctiveness between collection types.** A set `{1,2,3}` should look different from a sequence `[1,2,3]` and from a tuple `(1,2,3)`. When all three look the same, the reader must track types mentally. When they look different, the eye catches mismatches.

**Named sub-claims as cognitive anchors.** Deep nesting without names forces token-by-token reading. Named intermediate results let the reader check each piece independently. The `where` pattern and Evident's `because` block both provide this — but shorthand that makes sub-claims anonymous (implicit iteration, `.` chaining) removes those anchors. Evident should be conservative about implicit iteration: make the structure visible.

**Constraints that look like constraints.** `x in 1..100` reads as the constraint it is. `x >= 1 && x <= 100` reads as two comparisons that happen to be constraints. The set-membership form is one syntactic unit encoding one concept; the boolean form is two units encoding two concepts. For a constraint language, the single-unit form is always better.

---

## Recommendations for Evident

### 1. Set literal: adopt a distinct sigil

**Recommendation:** Use `{1, 2, 3}` for set literals, with `{}` for the empty set (since Evident has no dict literal that would conflict). The Python wart — `{}` being an empty dict rather than an empty set — does not apply because Evident does not have dict literals in the same way.

If Evident does need to distinguish sets from maps (sets of key-value pairs), follow Clojure's lead: `#{1, 2, 3}` for sets, `{a: 1, b: 2}` for maps. The `#` prefix is mnemonic ("#" as in "count of distinct elements") and does not conflict with any established syntax in Evident.

```evident
-- Set literal
allowed_methods = {GET, POST, PUT, DELETE}

-- Range as set expression
valid_age = 18..65

-- Empty set
no_exceptions = {}
```

### 2. Range syntax: use `a..b` inclusive, `a..<b` exclusive

**Recommendation:** Adopt `..` for inclusive ranges (matching Ruby, Rust, common mathematical convention) and `..<` for exclusive upper bound (matching Swift, Kotlin, and being visually distinct from `..`).

Avoid `...` (Ruby's exclusive range) — the visual difference between `..` and `...` is too subtle. Avoid `1..=10` (Rust's explicit inclusive) — unnecessary when `..` can be the default.

```evident
-- Inclusive range
valid_score = 0..100        -- scores from 0 to 100, both inclusive

-- Exclusive upper bound
byte_values = 0..<256       -- 0 through 255

-- Open-ended range (lower bound only)
adult_age = 18..            -- 18 and above

-- Character ranges
lowercase = 'a'..'z'
```

### 3. Set comprehension: use mathematical set-builder notation

**Recommendation:** Adopt `{ expr | var in collection, condition }` as the primary set-comprehension syntax. This directly mirrors mathematical notation `{ x ∈ S | P(x) }` while being ASCII-compatible.

```evident
-- Set comprehension
adult_users     = { u | u in users, age(u) >= 18 }
prime_squares   = { n*n | n in 1..100, prime(n) }
active_sessions = { s.id | s in sessions, s.expires_at > now }

-- With multiple generators (Cartesian product filtered)
pythagorean = { (a,b,c) | a in 1..100, b in a..100, c in b..100,
                           a*a + b*b == c*c }
```

### 4. Field projection shorthand: adopt `.field` for record sets

**Recommendation:** Allow `.field` as a shorthand for projecting a field from each element of a set. In a context where the current element is implicit (inside a `map` or comprehension), `.field` accesses the field of the current element.

```evident
-- Field projection over set
user_names = { .name | u in users }     -- project name field
user_emails = users.map(.email)          -- alternative form

-- Inside a claim, .field refers to the claim's primary argument
evident valid_user(u)
    .name != ""             -- u.name (implicit receiver)
    .email contains "@"     -- u.email
    .age >= 0               -- u.age
```

This follows jq's `.field` convention and Kotlin's `it.field` pattern but makes the implicit receiver the primary argument of the surrounding claim.

### 5. Universal and existential quantifiers: make them first-class syntax

**Recommendation:** Provide `forall` and `exists` as first-class constraint keywords, not just library functions. These are the two most fundamental quantifiers in constraint programming.

```evident
-- Universal quantification
forall u in users: valid_email(u.email)
forall item in cart: item.quantity > 0

-- Existential quantification
exists admin in users: u.role == "admin"
exists seat in seats: seat.available

-- Quantifiers in claim bodies
evident team_valid(team)
    forall member in team.members: active(member)
    exists leader in team.members: leader.is_lead
```

The `forall` / `exists` syntax mirrors predicate logic notation and reads as English prose while remaining unambiguous.

### 6. Destructuring binding with `@`

**Recommendation:** Support `@` for simultaneous whole-binding and destructuring in claim parameters, following Haskell's convention.

```evident
evident valid_order(order @ {items, customer, total})
    items != {}
    valid_customer(customer)
    total == sum({ i.price | i in items })
    logged(order)              -- uses whole order
```

### 7. Anonymous function shorthand: use `_` for the implicit argument

**Recommendation:** Support `_` as the implicit argument in inline predicates, following Scala's convention. For field access, `_.field` projects a field from the argument.

```evident
-- Filter with implicit argument
active_users = filter(users, _.active)            -- equivalent to { u | u in users, u.active }
adult_users  = filter(users, _.age >= 18)
names        = map(users, _.name)

-- Sorting
sorted_users = sort_by(users, _.last_name)
```

Note: use `_` sparingly. Named sub-claims and set comprehensions are almost always clearer than `_`-based inline expressions in the context of a constraint language. The `_` shorthand is most appropriate in `sort_by`, `group_by`, and `map` where the transformation is a single field access.

### 8. Method-reference shorthand: adopt `::field` for projection

**Recommendation:** Support `::field` as a shorthand for a projection function, following Kotlin and Java conventions.

```evident
-- Method reference in higher-order contexts
sorted_by_name = sort_by(users, ::name)
all_emails     = map(users, ::email)
active         = filter(users, ::active)   -- where active is a boolean field
```

### 9. Spread for set union in literals: use `...`

**Recommendation:** Support `...collection` inside set literals to spread the elements of one collection into another, following JavaScript convention.

```evident
-- Spread in set literal
admin_methods   = {GET, POST, PUT, DELETE, PATCH}
readonly_methods = {GET}
extra_allowed   = {...readonly_methods, OPTIONS}   -- {GET, OPTIONS}

-- Spread in claim bodies
evident allowed_actions(user)
    user.permissions in {...base_permissions, ...role_permissions(user.role)}
```

### 10. Implicit iteration: be conservative

**Recommendation:** Do *not* make implicit iteration the default. NumPy and APL's implicit iteration is powerful but makes it ambiguous whether an operation applies to the collection as a whole or to each element.

For Evident, the constraint interpretation is clear: `age > 18` where `age` is a domain variable constrains the variable. But `users.age > 18` — does this mean "all users are over 18" or "each user's age is individually constrained to be > 18"? The distinction matters. Explicit quantifiers (`forall`, `exists`, comprehension syntax) are safer here because they make the quantification structure visible.

**Exception:** Within a comprehension body `{ expr | var in collection, condition }`, operations on `var` are implicitly per-element. This is already the standard mathematical convention.

---

### Summary Table: Syntax Decisions for Evident

| Feature | Recommended Syntax | Rationale |
|---|---|---|
| Set literal | `{1, 2, 3}` or `#{1, 2, 3}` | Distinct from sequence `[1,2,3]`; `#{}` avoids conflict if map literal needed |
| Empty set | `{}` or `#{}` | Matches set literal prefix |
| Sequence literal | `[1, 2, 3]` | Established, universal |
| Inclusive range | `a..b` | Common in Ruby, Rust, math; both bounds included |
| Exclusive upper range | `a..<b` | Swift/Kotlin convention; visually distinct |
| Set comprehension | `{ expr \| var in S, cond }` | Mathematical set-builder notation |
| Universal quantifier | `forall x in S: P(x)` | Predicate logic, readable as English |
| Existential quantifier | `exists x in S: P(x)` | Predicate logic, readable as English |
| Field projection | `.field` (implicit element) | jq convention; minimal noise |
| Anonymous argument | `_` or `_.field` | Scala convention; use sparingly |
| Collection method ref | `::field` | Kotlin/Java convention |
| Destructuring binding | `val @ {a, b, c}` | Haskell `@`; names whole and parts |
| Spread into literal | `{...coll, extra}` | JavaScript convention |
| Implicit iteration | Avoid; use explicit quantifiers | Prevents ambiguity in constraint context |

The guiding principle throughout: *the syntax should make the set-theoretic structure visible*. When a programmer reads a constraint claim in Evident, they should see at a glance whether it is a universal or existential statement, what the domain is, and what the condition is. Shorthand that hides this structure trades momentary brevity for lasting comprehension cost.
