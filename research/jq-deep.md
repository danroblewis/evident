# jq Deep Analysis for Evident Language Design

Research for the Evident constraint programming language. jq is the closest existing language to what Evident needs for querying and transforming sets of structured data. This document provides a thorough analysis: design philosophy, every major operator, the generator model, relational algebra mapping, and concrete design lessons.

All examples are tested against jq 1.7.1.

---

## 1. Design Philosophy

### The Problem jq Was Designed to Solve

jq was created by Stephen Dolan in 2012. The motivating problem was simple and pervasive: JSON is everywhere, but Unix tools — `grep`, `awk`, `sed`, `cut` — are line-oriented and know nothing about structure. If a REST API returns:

```json
{"users": [{"name": "alice", "active": true}, {"name": "bob", "active": false}]}
```

there was no good way to extract "the names of all active users" from the shell. You could write a Python one-liner or a small script, but you couldn't do it *in a pipeline* as a single composable expression.

jq's answer: a language where the entire program is a *filter* — an expression that takes a JSON value as input and produces JSON values as output — designed to be embedded in Unix pipelines.

```sh
cat users.json | jq '.users[] | select(.active) | .name'
```

This is the archetype: `.users[]` iterates, `select(.active)` filters, `.name` projects. Three transformations composed with `|`. The Unix pipe at the start feeds in the JSON; jq handles everything after.

### Design Principles

**Everything is a filter.** A jq program is not a sequence of statements. It is a single expression — a filter — that transforms its input. Even constants are filters: `42` is a filter that ignores its input and outputs `42`. Even field access is a filter: `.name` is a filter that takes an object and outputs its `.name` field. There is no distinction between "operators" and "functions" — everything obeys the same contract.

**The pipe is composition.** `f | g` means "run f on the input, then run g on every output of f." This is not a data-flow pipe between processes — it is function composition made concrete. Because both sides of `|` are filters, composition is total: any filter can follow any other filter.

**The stream model is fundamental.** jq does not have "functions that return a value." It has "filters that produce a stream of values." A filter can produce zero values (filtering out), one value (the common case), or many values (iteration, combinations). This single design decision explains why `select`, `.[]`, and `group_by` all work uniformly with `|`.

**`.` is always the current value.** There is no explicit argument to jq filters. The input is always implicit, always called `.`. Every filter receives `.` and produces outputs. This is the point-free style taken to its logical extreme.

**Construction is as first-class as extraction.** `{name: .name, age: .age}` builds a new object from parts of the current value. `[.[] | select(. > 0)]` builds an array from a filtered stream. Construction syntax is symmetric with access syntax.

**Type-based dispatch.** jq operators are polymorphic. `+` adds numbers, concatenates strings, merges arrays, and merges objects. `.[]` iterates arrays and objects. `length` works on strings, arrays, objects, and null. This polymorphism keeps the operator set small.

### Why It Looks the Way It Does

jq's syntax is heavily influenced by its intended use: embedded in shell one-liners. Every character counts. `.` instead of `self` or `it`. `|` instead of `pipe` or `>>`. `[]` instead of `.iter()`. The terseness is a feature, not an oversight.

The language is also heavily influenced by Haskell and functional programming. The generator model is the list monad. `reduce` and `foreach` are fold and scan. `def` allows recursive named functions. But these ideas are expressed in a dense, symbol-heavy notation designed for interactive use rather than software engineering.

---

## 2. The Generator Model

### What a Generator Is

In jq, every expression is a **generator**: it produces a *stream* of values — zero, one, or many. This is fundamentally different from how functions work in most languages (which produce exactly one value) and different from SQL (where each expression in a row context produces exactly one value for that row).

The generator model means:
- You cannot tell from an expression's syntax how many values it will produce.
- All operators are implicitly "lifted" over the stream: if `f` produces three values and `g` produces two values, then `f, g` produces five values.
- Pipes compose generators: `f | g` sends every output of `f` through `g`, collecting all results.

### Zero Values: `select`

`select(cond)` produces its input if `cond` is true, or produces nothing at all if `cond` is false. This is filtering expressed as a generator.

```sh
echo '[1,2,3,4,5]' | jq '.[] | select(. > 10)'
# (no output — the stream is empty)

echo '[1,2,3,4,5]' | jq '[.[] | select(. > 10)]'
# []
```

The second form wraps the stream in an array. An empty stream becomes an empty array. The `select` generator "swallows" inputs that don't pass the test, producing nothing.

`empty` is the primitive zero-value generator — it always produces nothing. It is the identity element for stream concatenation, just as `0` is the identity element for addition.

### One Value: Field Access

`.name` produces exactly one value (or zero if the field doesn't exist and you use `.name?`).

```sh
echo '{"name":"alice","age":30}' | jq '.name'
# "alice"
```

Field access is the normal case: one input produces one output.

### Many Values: Iteration

`.[]` (array/object iteration) is the fundamental many-value generator. It turns a container into a stream of its elements.

```sh
echo '[1,2,3]' | jq '.[]'
# 1
# 2
# 3
```

Each element is a separate value in the output stream. jq prints each one on its own line because the stream has three elements.

### Composing Generators with Pipes

When generators are composed with `|`, the right side runs once per output of the left side. This is the key mechanic:

```sh
# Start with an array, iterate it, filter, project
echo '[{"name":"alice","age":30},{"name":"bob","age":17},{"name":"carol","age":25}]' | \
  jq '.[] | select(.age >= 18) | .name'
# "alice"
# "carol"
```

Step by step:
1. `.[]` turns the array into a stream of three objects.
2. `select(.age >= 18)` passes alice (30) and carol (25), swallows bob (17). Stream is now two objects.
3. `.name` extracts the name field from each. Stream is now two strings.

### The Comma Operator: Multiple Outputs Without Iteration

`,` joins two generators into one. `f, g` produces all outputs of `f` followed by all outputs of `g`.

```sh
echo '{"x":1,"y":2}' | jq '.x, .y'
# 1
# 2

echo '5' | jq '. * 2, . + 10'
# 10
# 15
```

This is how jq achieves "multiple return values" without arrays: you just produce multiple outputs.

### Collecting a Stream into an Array

`[expr]` evaluates `expr` and collects all its outputs into a single array. This is the bridge between the stream world and the array world.

```sh
echo '[1,2,3,4,5]' | jq '[.[] | select(. > 2)]'
# [3, 4, 5]

echo '[1,2,3]' | jq '[.[] * 2]'
# [2, 4, 6]
```

`map(f)` is exactly `[.[] | f]`. It's not a special form — it's defined in terms of the generator model.

### Why This Model Matters for Evident

The generator model is the key thing that makes jq feel like a query language rather than a scripting language. Selection is zero-value output; iteration is many-value output; transformation is one-to-one output. These are exactly the operations that relational algebra needs: selection (σ), unnesting, and projection (π). Expressing all three through a single "outputs a stream" abstraction is elegant.

Evident's constraint-based model achieves a similar effect: a claim either holds (one "row" in the result) or it doesn't (zero rows). Iteration over a set is like `.[]` — each element enters the solver separately.

---

## 3. The Implicit Identity `.`

### What `.` Is

In jq, `.` is the *identity filter*: it takes its input and produces it unchanged. But more importantly, `.` represents "the current value" — the thing being processed right now.

```sh
echo '42' | jq '.'
# 42

echo '{"a":1}' | jq '.'
# { "a": 1 }
```

Every filter receives `.` as its input. `.name` means "the `.name` field of `.`". `.[] | .age` means "iterate `.`, and for each element, access `.age`".

### How `.` Shapes the Language

Because `.` is implicit everywhere, jq code is written as a description of *what to extract* rather than as instructions about *where to look*. Compare:

```python
# Python: explicit argument everywhere
def get_active_names(data):
    return [user['name'] for user in data['users'] if user['active']]
```

```sh
# jq: implicit current value, no argument naming
'.users[] | select(.active) | .name'
```

The jq version describes a traversal path without ever naming the collection being traversed. This is point-free style: the argument to the filter is not named, only its structure is described.

### Context Shifting

When you enter a `map(f)` or a `reduce ... as $x`, the `.` inside the body refers to a different thing than `.` outside.

```sh
echo '[1,2,3]' | jq 'map(. * .)'
# [1, 4, 9]
# Here, . inside map refers to each element, not the array
```

```sh
echo '[1,2,3,4,5]' | jq 'reduce .[] as $x (0; . + $x)'
# 15
# Here, . in the body refers to the accumulator, not the array element (which is $x)
```

This context-shifting is jq's main source of confusion. When filters are nested, you need to track which `.` refers to which scope. jq's solution is named variables: `as $var` binds the current value to a name, preserving it across context shifts.

### Advantages of Implicit `.`

**Conciseness.** `.users[] | select(.active) | .name` is shorter and less noisy than any explicit-argument equivalent.

**Composability.** Because every filter has the same implicit contract (receive `.`, produce outputs), any filter can compose with any other filter using `|`. There are no argument-compatibility issues.

**Point-free pipeline style.** Chains of transformations read as a description of what you want, not how to compute it. This aligns with the declarative style.

**Disadvantages.** In deeply nested contexts, it becomes hard to know which "level" `.` refers to. jq addresses this with `as $var` for explicit naming, but the discipline of using it must be learned.

---

## 4. The Full Operator Set

### 4.1 Identity: `.`

Takes input, produces it unchanged. The starting point for all field access.

```sh
echo '{"a":1}' | jq '.'
# { "a": 1 }
```

### 4.2 Field Access: `.field`, `.["field"]`

Project a named field from the current object.

```sh
echo '{"name":"alice","age":30}' | jq '.name'
# "alice"

echo '{"name":"alice","age":30}' | jq '.["age"]'
# 30

# Optional operator ? suppresses errors on wrong types
echo '"hello"' | jq '.foo?'
# (no output, no error)

# Without ?, accessing a field on a non-object is an error
echo '"hello"' | jq '.foo'
# error
```

`.["field"]` is equivalent to `.field` but allows computed field names:

```sh
echo '{"key":"name","data":{"name":"alice"}}' | jq '.data[.key]'
# "alice"
```

### 4.3 Array Iteration and Indexing: `.[]`, `.[n]`, `.[n:m]`

```sh
# Iterate all elements (many-value generator)
echo '[10,20,30]' | jq '.[]'
# 10
# 20
# 30

# Index (zero-based)
echo '[10,20,30,40,50]' | jq '.[2]'
# 30

# Negative index (from end)
echo '[10,20,30,40,50]' | jq '.[-1]'
# 50

# Slice [inclusive:exclusive]
echo '[10,20,30,40,50]' | jq '.[2:4]'
# [30, 40]

# Slice from index to end
echo '[10,20,30,40,50]' | jq '.[2:]'
# [30, 40, 50]
```

Object iteration produces all values (not keys):

```sh
echo '{"a":1,"b":2,"c":3}' | jq '.[]'
# 1
# 2
# 3
```

### 4.4 Pipe: `|`

Composition. Feeds every output of the left side into the right side.

```sh
echo '[{"name":"alice","age":30},{"name":"bob","age":17}]' | \
  jq '.[] | select(.age >= 18) | .name'
# "alice"
```

Pipe is the central operator. Everything else is defined in terms of it.

### 4.5 Filter: `select(cond)`

Produces its input if `cond` evaluates to a truthy value; produces nothing otherwise.

```sh
echo '[1,2,3,4,5]' | jq '[.[] | select(. > 3)]'
# [4, 5]

echo '[1,2,3,4,5]' | jq '[.[] | select(. > 10)]'
# []
```

`select` is how jq expresses SQL's `WHERE`. It is a zero-or-one generator.

### 4.6 Construction: `{key: expr}`, `[expr]`

Build new objects and arrays.

```sh
# Object construction
echo '{"name":"alice","age":30,"city":"nyc"}' | jq '{person: .name, years: .age}'
# { "person": "alice", "years": 30 }

# If key and value field have the same name, shorthand works
echo '{"name":"alice","age":30}' | jq '{name, age}'
# { "name": "alice", "age": 30 }

# Computed keys
echo '{"key":"name","val":"alice"}' | jq '{(.key): .val}'
# { "name": "alice" }

# Array construction: collect a stream
echo '[1,2,3,4,5]' | jq '[.[] | select(. > 2)]'
# [3, 4, 5]
```

Array construction `[expr]` evaluates `expr` and collects all outputs into an array. Object construction `{k: v}` evaluates `v` once per key-value pair.

When an object construction contains an expression that produces multiple values for a key, jq generates multiple objects — one per output. This is an important generator interaction.

### 4.7 Arithmetic: `+`, `-`, `*`, `/`, `%`

```sh
echo '{"a":10,"b":3}' | jq '.a + .b, .a - .b, .a * .b, .a / .b, .a % .b'
# 13
# 7
# 30
# 3.3333333333333335
# 1
```

`+` is heavily overloaded:

```sh
# String concatenation
echo '"hello"' | jq '. + " world"'
# "hello world"

# Array concatenation
echo '[1,2]' | jq '. + [3,4]'
# [1, 2, 3, 4]

# Object merge (right wins on conflicts)
echo '{"a":1}' | jq '. + {"b":2}'
# { "a": 1, "b": 2 }
```

`*` also has non-numeric meanings:

```sh
# String repetition
echo '"ha"' | jq '. * 3'
# "hahaha"

# Object recursive merge
echo '{"a":{"x":1}}' | jq '. * {"a":{"y":2}}'
# { "a": { "x": 1, "y": 2 } }
```

`/` on strings splits on the delimiter:

```sh
echo '"a,b,c"' | jq '. / ","'
# ["a", "b", "c"]
```

### 4.8 Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`

Produce `true` or `false`. Work across types (numbers compare numerically; strings compare lexicographically; type order is: null < false < true < numbers < strings < arrays < objects).

```sh
echo '5' | jq '. > 3, . == 5, . != 4, . <= 5'
# true
# true
# true
# true

echo '"abc"' | jq '. < "abd"'
# true
```

### 4.9 Boolean: `not`, `and`, `or`

```sh
echo 'null' | jq 'null | not'
# true

echo 'true' | jq '. and false'
# false

echo 'false' | jq '. or true'
# true
```

In jq, `false` and `null` are falsy; everything else (including `0` and `""`) is truthy.

```sh
echo '0' | jq '. | not'
# false   (0 is truthy in jq!)
```

### 4.10 Null Coalescing: `//`

Produces the left value unless it is `false` or `null`, in which case it produces the right value.

```sh
echo 'null' | jq '. // "default"'
# "default"

echo '{"x":null}' | jq '.x // "fallback"'
# "fallback"

echo '{"x":42}' | jq '.x // "fallback"'
# 42
```

Note: `//` triggers on `false` too, not just `null`. This differs from SQL's `COALESCE`.

### 4.11 String Formatting: `@base64`, `@uri`, `@csv`, `@tsv`, `@json`, `@html`, `@sh`

Format operators transform the current value into a specific string encoding.

```sh
echo '"hello world"' | jq '@base64'
# "aGVsbG8gd29ybGQ="

echo '"hello world"' | jq '@uri'
# "hello%20world"

echo '["a","b","c"]' | jq '@csv'
# "\"a\",\"b\",\"c\""

echo '[["a","b"],["c","d"]]' | jq '.[] | @tsv'
# "a\tb"
# "c\td"

echo '{"a":1}' | jq '@json'
# "{\"a\":1}"

echo '"<script>alert(1)</script>"' | jq '@html'
# "&lt;script&gt;alert(1)&lt;/script&gt;"

echo '"hello world"' | jq '@sh'
# "'hello world'"
```

### 4.12 String Interpolation: `"\(.expr)"`

Embed jq expressions inside string literals.

```sh
echo '{"name":"alice","age":30}' | jq '"Name: \(.name), Age: \(.age)"'
# "Name: alice, Age: 30"

# Can be combined with format operators
echo '{"url":"hello world"}' | jq '"Encoded: \(.url | @uri)"'
# "Encoded: hello%20world"
```

### 4.13 Types: `type`, `arrays`, `objects`, `iterables`, `booleans`, `numbers`, `strings`, `nulls`, `values`, `scalars`

`type` returns a string name for the type:

```sh
echo '[1,"two",null,true,[],{}]' | jq '.[] | type'
# "number"
# "string"
# "null"
# "boolean"
# "array"
# "object"
```

Type-selector filters pass through values of the matching type and produce nothing for others. They are generators:

```sh
echo '[1,"two",null,true,[],{}]' | jq '[.[] | numbers]'
# [1]

echo '[1,"two",null,true,[],{}]' | jq '[.[] | strings]'
# ["two"]

echo '[1,"two",null,true,[],{}]' | jq '[.[] | arrays]'
# [[]]

echo '[1,"two",null,true,[],{}]' | jq '[.[] | objects]'
# [{}]

# values = everything that is not null
echo '[1,null,2,null,3]' | jq '[.[] | values]'
# [1, 2, 3]

# scalars = non-iterable (numbers, strings, booleans, null)
echo '[1,"two",null,true,[],{}]' | jq '[.[] | scalars]'
# [1, "two", null, true]

# iterables = arrays and objects
echo '[1,"two",null,true,[],{}]' | jq '[.[] | iterables]'
# [[], {}]
```

### 4.14 Length and Keys: `length`, `keys`, `keys_unsorted`, `values`, `has(key)`, `in`

```sh
echo '[10,20,30]' | jq 'length'
# 3

echo '"hello"' | jq 'length'
# 5

echo '{"a":1,"b":2,"c":3}' | jq 'length'
# 3

echo 'null' | jq 'length'
# 0

# keys returns sorted array of keys
echo '{"c":3,"a":1,"b":2}' | jq 'keys'
# ["a", "b", "c"]

# keys_unsorted returns keys in insertion order
echo '{"c":3,"a":1,"b":2}' | jq 'keys_unsorted'
# ["c", "a", "b"]

# has(key) checks field existence
echo '{"a":1,"b":2}' | jq 'has("a"), has("z")'
# true
# false

# in(object) checks if . is a key of the argument
echo '"a"' | jq 'in({"a":1,"b":2})'
# true
```

### 4.15 Containment: `contains`, `inside`, `indices`, `index`, `rindex`

```sh
# contains: does . contain the argument (deep structural containment)?
echo '[1,2,3,2,1]' | jq 'contains([2,3])'
# true

echo '{"a":1,"b":2}' | jq 'contains({"a":1})'
# true

# inside: is . contained in the argument?
echo '[2,0]' | jq 'inside([0,1,2])'
# true

# indices: positions of all occurrences of a value
echo '[1,2,3,2,1]' | jq 'indices(2)'
# [1, 3]

# index: first occurrence
echo '[1,2,3,2,1]' | jq 'index(2)'
# 1

# rindex: last occurrence
echo '[1,2,3,2,1]' | jq 'rindex(2)'
# 3
```

### 4.16 String Testing: `test`, `match`, `capture`, `scan`, `splits`

```sh
# test: regex match boolean
echo '"foo bar baz"' | jq 'test("bar")'
# true

echo '"foo bar baz"' | jq 'test("^foo")'
# true

# match: returns match object with offset, length, string, captures
echo '"foo bar baz"' | jq 'match("b[a-z]+")'
# { "offset": 4, "length": 3, "string": "bar", "captures": [] }

# capture: named capture groups as an object
echo '"2024-01-15"' | jq 'capture("(?<year>[0-9]+)-(?<month>[0-9]+)-(?<day>[0-9]+)")'
# { "year": "2024", "month": "01", "day": "15" }

# scan: all matches as a generator
echo '"test foo bar foo"' | jq '[scan("foo")]'
# ["foo", "foo"]

# splits: split on regex, includes empty strings
echo '"a,b,,c"' | jq '[splits(",")]'
# ["a", "b", "", "c"]
```

### 4.17 String Utilities: `split`, `join`, `ltrimstr`, `rtrimstr`, `startswith`, `endswith`, `ascii_downcase`, `ascii_upcase`, `tostring`, `tonumber`, `explode`, `implode`

```sh
echo '"a,b,c"' | jq 'split(",")'
# ["a", "b", "c"]

echo '["a","b","c"]' | jq 'join(",")'
# "a,b,c"

echo '"Hello World"' | jq 'ascii_downcase, ascii_upcase'
# "hello world"
# "HELLO WORLD"

echo '"Hello World"' | jq 'ltrimstr("Hello ")'
# "World"

echo '"Hello World"' | jq 'rtrimstr(" World")'
# "Hello"

echo '"Hello"' | jq 'startswith("He"), endswith("lo")'
# true
# true

echo '42' | jq 'tostring'
# "42"

echo '"42"' | jq 'tonumber'
# 42

# explode: string to Unicode codepoint array
echo '"A"' | jq 'explode'
# [65]

# implode: codepoint array to string
echo '[65,66,67]' | jq 'implode'
# "ABC"
```

### 4.18 Array Transformations: `map(f)`, `map_values(f)`, `add`, `any`, `all`, `flatten`, `range`

```sh
# map: apply f to each element, collect results
echo '[1,2,3,4,5]' | jq 'map(. * 2)'
# [2, 4, 6, 8, 10]

# map is sugar for [.[] | f]
echo '[1,2,3,4,5]' | jq '[.[] | . * 2]'
# [2, 4, 6, 8, 10]

# map(select(...)): filter
echo '[1,2,3,4,5]' | jq 'map(select(. > 2))'
# [3, 4, 5]

# map_values: apply to values of object or array
echo '{"a":1,"b":2,"c":3}' | jq 'map_values(. * 10)'
# { "a": 10, "b": 20, "c": 30 }

# add: sum an array (works for numbers, strings, arrays, objects)
echo '[1,2,3,4,5]' | jq 'add'
# 15

echo '["a","b","c"]' | jq 'add'
# "abc"

echo '[[1,2],[3,4],[5]]' | jq 'add'
# [1, 2, 3, 4, 5]

# any/all: existential/universal quantifiers
echo '[1,2,3,4,5]' | jq 'any(. > 4)'
# true

echo '[1,2,3,4,5]' | jq 'all(. > 0)'
# true

# any and all also take a generator and condition
echo 'null' | jq 'any(range(5); . > 3)'
# true

# flatten: remove nesting (optionally to depth)
echo '[[1,[2]],[[3],4]]' | jq 'flatten'
# [1, 2, 3, 4]

echo '[[1,[2]],[[3],4]]' | jq 'flatten(1)'
# [1, [2], [3], 4]

# range: integer generator
echo 'null' | jq '[range(5)]'
# [0, 1, 2, 3, 4]

echo 'null' | jq '[range(2;7)]'
# [2, 3, 4, 5, 6]

echo 'null' | jq '[range(0;10;3)]'
# [0, 3, 6, 9]
```

### 4.19 Math: `floor`, `ceil`, `round`, `sqrt`, `pow`, `fabs`, `nan`, `infinite`, `isnormal`, `isnan`, `isinfinite`, `isfinite`, `log`, `exp`

```sh
echo '16' | jq 'sqrt'
# 4

echo '2.7' | jq 'floor, ceil, round'
# 2
# 3
# 3

echo '-3.14' | jq 'fabs'
# 3.14

echo 'null' | jq 'pow(2;10)'
# 1024

echo 'null' | jq 'nan | isnan'
# true

echo 'null' | jq 'infinite | isinfinite'
# true

echo '1' | jq 'isnormal'
# true
```

### 4.20 Grouping: `group_by`, `unique`, `unique_by`, `sort`, `sort_by`, `min`, `max`, `min_by`, `max_by`, `reverse`

These are the operators most relevant to Evident.

```sh
# group_by: group array elements by a key expression
# Input must be an array. Output is an array of arrays, sorted by key.
echo '[{"name":"alice","dept":"eng"},{"name":"bob","dept":"hr"},
       {"name":"carol","dept":"eng"},{"name":"dave","dept":"hr"},
       {"name":"eve","dept":"eng"}]' | jq 'group_by(.dept)'
# [
#   [{"name":"alice","dept":"eng"},{"name":"carol","dept":"eng"},{"name":"eve","dept":"eng"}],
#   [{"name":"bob","dept":"hr"},{"name":"dave","dept":"hr"}]
# ]
```

The output format is crucial: `group_by(.f)` returns an array of arrays, where each inner array is a group sharing the same value of `.f`. The groups are sorted by the grouping key. The original objects are preserved in full.

```sh
# sort_by
echo '[{"name":"carol","age":25},{"name":"alice","age":30},{"name":"bob","age":17}]' | \
  jq 'sort_by(.name)'
# [{"name":"alice","age":30}, {"name":"bob","age":17}, {"name":"carol","age":25}]

# sort (natural ordering)
echo '[3,1,4,1,5,9,2,6]' | jq 'sort'
# [1, 1, 2, 3, 4, 5, 6, 9]

# unique: deduplicate (implies sort)
echo '[3,1,4,1,5,9,2,6]' | jq 'unique'
# [1, 2, 3, 4, 5, 6, 9]

# unique_by: deduplicate keeping first occurrence of each key value
echo '[{"name":"alice","dept":"eng"},{"name":"bob","dept":"hr"},{"name":"carol","dept":"eng"}]' | \
  jq 'unique_by(.dept)'
# [{"name":"alice","dept":"eng"}, {"name":"bob","dept":"hr"}]

# min, max
echo '[3,1,4,1,5,9]' | jq 'min, max'
# 1
# 9

# min_by, max_by
echo '[{"name":"alice","age":30},{"name":"bob","age":17},{"name":"carol","age":25}]' | \
  jq 'min_by(.age), max_by(.age)'
# {"name":"bob","age":17}
# {"name":"alice","age":30}

# reverse
echo '[1,2,3,4,5]' | jq 'reverse'
# [5, 4, 3, 2, 1]
```

### 4.21 Object Manipulation: `to_entries`, `from_entries`, `with_entries`, `del`

```sh
# to_entries: object to array of {key, value} pairs
echo '{"a":1,"b":2,"c":3}' | jq 'to_entries'
# [{"key":"a","value":1}, {"key":"b","value":2}, {"key":"c","value":3}]

# from_entries: array of {key,value} pairs to object
echo '[{"key":"a","value":1},{"key":"b","value":2}]' | jq 'from_entries'
# {"a":1, "b":2}

# with_entries(f): apply f to each {key,value} pair, then reconstruct
echo '{"a":1,"b":2,"c":3}' | jq 'with_entries(.value += 10)'
# {"a":11, "b":12, "c":13}

# with_entries for selective projection:
echo '{"a":1,"b":2,"c":3}' | jq 'with_entries(select(.key == "a" or .key == "c"))'
# {"a":1, "c":3}

# del: remove a field
echo '{"a":1,"b":2,"c":3}' | jq 'del(.b)'
# {"a":1, "c":3}

# del with array index
echo '[1,2,3,4,5]' | jq 'del(.[2])'
# [1, 2, 4, 5]
```

### 4.22 Control Flow: `if-then-else`, `try-catch`, `?//` (alternative operator)

```sh
# if-then-else (elif supported)
echo '5' | jq 'if . > 3 then "big" elif . > 1 then "medium" else "small" end'
# "big"

# if works as a generator: if condition produces multiple values, multiple branches execute
echo '[1,2,3]' | jq '.[] | if . > 2 then "big" else "small" end'
# "small"
# "small"
# "big"

# try-catch: catch errors
echo '"not a number"' | jq 'try (. / 2) catch "error: \(.)"'
# "error: string (\"not a number\") and number (2) cannot be divided"

# try without catch: suppress errors (zero outputs on error)
echo '"not a number"' | jq 'try (. / 2)'
# (no output)

# ? suffix: shorthand for try (suppress errors)
echo '"hello"' | jq '.foo?'
# (no output, not an error)

# ?// alternative operator: try left, if error use right
# (jq 1.6+)
echo '"hello"' | jq '.foo? // "missing"'
# "missing"
```

### 4.23 Reduction: `reduce`, `foreach`, `limit`, `first`, `last`, `nth`

```sh
# reduce: fold over a generator
echo '[1,2,3,4,5]' | jq 'reduce .[] as $x (0; . + $x)'
# 15
# Syntax: reduce <generator> as $var (<initial>; <body>)
# In the body, . is the accumulator, $x is the current generator value

# foreach: like reduce but emits intermediate states
echo 'null' | jq '[foreach range(5) as $x (0; . + $x)]'
# [0, 1, 3, 6, 10]
# Each intermediate accumulator value is output

# limit: take first n outputs from a generator
echo 'null' | jq '[limit(5; range(100))]'
# [0, 1, 2, 3, 4]

# first, last
echo '[1,2,3,4,5]' | jq 'first(.[] | select(. > 3))'
# 4

echo '[1,2,3,4,5]' | jq 'last(.[])'
# 5

# nth: zero-indexed nth output
echo 'null' | jq 'nth(2; range(10))'
# 2
```

### 4.24 Recursion: `recurse`, `recurse_down`

```sh
# recurse: repeatedly apply f until it produces no output
echo '{"a":{"b":{"c":1}}}' | jq '[recurse | .b? // empty]'
# [{"c":1}]

# recurse with a generator body: walk the tree
echo '{"a":{"b":{"c":42}}}' | jq '.. | numbers'
# 42
# (.. is shorthand for recurse)

# recurse to find all leaf values
echo '{"a":1,"b":{"c":2,"d":{"e":3}}}' | jq '[.. | numbers]'
# [1, 2, 3]
```

`..` is jq's "recursive descent" operator. It generates every value in the JSON structure, breadth-or-depth-first. Combined with type filters, it's a powerful way to extract deeply nested values.

### 4.25 Path Operations: `path`, `getpath`, `setpath`, `delpaths`, `paths`

Paths in jq are arrays of keys/indices that describe a location in a JSON structure.

```sh
# path: get the path expression as an array
echo '{"a":{"b":{"c":42}}}' | jq 'path(.a.b.c)'
# ["a", "b", "c"]

# getpath: access value at path
echo '{"a":{"b":{"c":42}}}' | jq 'getpath(["a","b","c"])'
# 42

# setpath: create a modified copy with value at path set
echo '{"a":{"b":1}}' | jq 'setpath(["a","c"]; 99)'
# {"a":{"b":1,"c":99}}

# delpaths: delete multiple paths at once
echo '{"a":{"b":1,"c":2},"d":3}' | jq 'delpaths([["a","b"],["d"]])'
# {"a":{"c":2}}

# paths: enumerate all paths in the structure
echo '{"a":{"b":{"c":42}}}' | jq '[paths]'
# [["a"], ["a","b"], ["a","b","c"]]

# paths(f): enumerate paths to values matching f
echo '{"a":{"b":{"c":42}}}' | jq '[paths(scalars)]'
# [["a","b","c"]]
```

Path operations enable a style of programming where you build up a transformation as a set of path-value modifications rather than rebuilding the whole structure.

### 4.26 Variables: `as $var`, and Definitions: `def name: body;`

```sh
# as $var: bind current value to a name
echo '5' | jq '. as $n | ($n * $n)'
# 25

# $var persists across context shifts
echo '[1,2,3]' | jq '. as $arr | $arr | length'
# 3

# def: define a reusable named filter
echo '[1,2,3,4,5]' | jq 'def double: . * 2; map(double)'
# [2, 4, 6, 8, 10]

# def with arguments (which are filters, not values)
echo '[1,2,3,4,5]' | jq 'def thresh(n): select(. > n); [.[] | thresh(3)]'
# [4, 5]

# Recursive defs are allowed
echo '5' | jq 'def fact: if . <= 1 then 1 else . * ((. - 1) | fact) end; fact'
# 120
```

### 4.27 Label-Break: `label $out | ... | break $out`

An advanced control flow construct for early exit from a generator loop.

```sh
echo 'null' | jq 'label $out | foreach range(10) as $x (0; . + $x; if . > 10 then ., break $out else . end)'
# 0
# 1
# 3
# 6
# 10
# 15
```

`label $out` sets up a label; `break $out` immediately terminates the innermost label, like a `break` in an imperative loop. Used for implementing early termination in recursive generators.

### 4.28 toJSON / fromJSON

```sh
echo '{"a":1}' | jq 'tojson'
# "{\"a\":1}"

echo '"{\"a\":1}"' | jq 'fromjson'
# {"a":1}
```

Useful for treating JSON as a string (for embedding in another format) and back.

---

## 5. `group_by` and `unique_by` in Depth

### `group_by(.field)` Exactly

`group_by(f)` takes an array as input. It applies `f` to each element, sorts the elements by `f`'s output, and then groups consecutive elements with the same `f`-value together.

**Input:** an array of objects (or any values).
**Output:** an array of arrays, where each inner array is a group.

```sh
echo '[
  {"name":"alice","dept":"eng"},
  {"name":"bob","dept":"hr"},
  {"name":"carol","dept":"eng"},
  {"name":"dave","dept":"hr"},
  {"name":"eve","dept":"eng"}
]' | jq 'group_by(.dept)'
```

Output:
```json
[
  [
    {"name":"alice","dept":"eng"},
    {"name":"carol","dept":"eng"},
    {"name":"eve","dept":"eng"}
  ],
  [
    {"name":"bob","dept":"hr"},
    {"name":"dave","dept":"hr"}
  ]
]
```

Key properties:
1. **The original objects are preserved in full** — not just the grouped field.
2. **Groups are sorted by the grouping key** — "eng" before "hr" alphabetically.
3. **The output is an array of arrays** — you then use `map(...)` to process each group.

### Aggregating After `group_by`

The typical pattern is `group_by(.field) | map({key: .[0].field, agg: ...})`:

```sh
echo '[
  {"name":"alice","dept":"eng"},
  {"name":"bob","dept":"hr"},
  {"name":"carol","dept":"eng"},
  {"name":"dave","dept":"hr"},
  {"name":"eve","dept":"eng"}
]' | jq 'group_by(.dept) | map({dept: .[0].dept, count: length, members: map(.name)})'
```

Output:
```json
[
  {"dept":"eng","count":3,"members":["alice","carol","eve"]},
  {"dept":"hr","count":2,"members":["bob","dave"]}
]
```

This is the full GROUP BY + aggregate pattern: group, then transform each group into a summary object.

### `unique_by(.field)` Exactly

`unique_by(f)` keeps only the **first** element of each group (as defined by `f`). It is equivalent to `group_by(f) | map(.[0])`.

```sh
echo '[
  {"name":"alice","dept":"eng"},
  {"name":"bob","dept":"hr"},
  {"name":"carol","dept":"eng"}
]' | jq 'unique_by(.dept)'
```

Output:
```json
[
  {"name":"alice","dept":"eng"},
  {"name":"bob","dept":"hr"}
]
```

Alice is kept (first in "eng"), Carol is dropped (also "eng", but later). Bob is kept (first in "hr").

### Mapping to Evident's `grouped_by`

In Evident, the proposed syntax `grouped_by .field` would work differently from jq's `group_by`. The key differences:

| Feature | jq `group_by(.field)` | Evident `grouped_by .field` (proposed) |
|---|---|---|
| Input | An array (must be explicit) | A set (implicit — the claim's domain) |
| Output | Array of arrays | A relation with a group key and a set-valued column |
| Accessing groups | `.[0].field` for the key; `map(.name)` for members | `.field` for key; `.members` as a set |
| Group key in output | Must extract from first element: `.[0].field` | Available directly as `.field` |
| Subsequent aggregation | `map({key: .[0].key, count: length})` | Declarative: `count .members`, `sum .members.salary` |

jq's design is honest about what `group_by` is: it returns the raw data partitioned into sub-arrays. You must then extract the key and apply aggregates yourself. This is flexible but verbose.

A better Evident design would be a grouped relation where the grouping key is a first-class field and the group members are a set-valued field. Something like:

```evident
-- Proposed Evident syntax for the same operation
{dept: .dept, members: {.name | . in employees, .dept == dept}} 
  grouped_by .dept
```

Or more naturally expressed as a claim that produces grouped records:

```evident
evident eng_summary dept
    dept in departments
    members = { e.name | e in employees, e.dept == dept.name }
    count = |members|
```

---

## 6. jq vs. Relational Algebra

### Selection (σ): `select(cond)`

**Relational algebra:** σ_P(R) — keep only tuples satisfying P.

**jq:** `.[] | select(cond)` — iterate the array, keep only elements satisfying `cond`.

```sh
echo '[{"id":1,"age":25},{"id":2,"age":17},{"id":3,"age":30}]' | \
  jq '[.[] | select(.age >= 18)]'
# [{"id":1,"age":25}, {"id":3,"age":30}]
```

This maps cleanly. The `.[]` is needed because jq works on JSON, not sets directly — you must first iterate to get individual tuples. In relational algebra the set is implicit.

**jq is clean here.**

### Projection (π): Object Construction

**Relational algebra:** π_{A,B}(R) — keep only attributes A and B, deduplicate.

**jq:** `.[] | {a: .a, b: .b}` — extract named fields.

```sh
echo '[{"id":1,"name":"alice","age":25,"city":"nyc"},{"id":2,"name":"bob","age":30,"city":"la"}]' | \
  jq '[.[] | {id, name}]'
# [{"id":1,"name":"alice"}, {"id":2,"name":"bob"}]
```

Note: jq does **not** deduplicate. If two rows project to the same result, you get two copies. This is bag semantics, not set semantics. To deduplicate, use `unique` or `unique_by`.

**jq is approximately clean. No automatic deduplication is a semantic gap with true RA.**

### Join (⋈): Variable Binding

**Relational algebra:** R ⋈_θ S — all pairs from R and S satisfying θ.

**jq:** No native join operator. Must be expressed through variable binding:

```sh
echo '{
  "users":[{"id":1,"name":"alice"},{"id":2,"name":"bob"}],
  "orders":[{"user_id":1,"item":"widget"},{"user_id":1,"item":"gadget"},{"user_id":2,"item":"doohickey"}]
}' | jq '.users as $users | .orders | map(. as $o | {
    item: $o.item, 
    name: ($users[] | select(.id == $o.user_id) | .name)
  })'
# [{"item":"widget","name":"alice"}, {"item":"gadget","name":"alice"}, {"item":"doohickey","name":"bob"}]
```

This works but is verbose and O(n*m) by default — there is no query optimizer to turn it into a hash join. For small data this is fine. For large data, it's a significant problem.

**jq is awkward for joins. This is a major limitation.**

### Group-By (γ): `group_by`

Already covered in depth above. jq's `group_by` maps directly to SQL's `GROUP BY` but with manual aggregation syntax.

```sh
# jq GROUP BY + COUNT + collect names
echo '[...]' | jq 'group_by(.dept) | map({dept: .[0].dept, count: length, members: map(.name)})'

# SQL equivalent
# SELECT dept, COUNT(*) as count, ARRAY_AGG(name) as members
# FROM employees
# GROUP BY dept
```

**jq is functional but verbose. The key must be extracted from the first group element.**

### Aggregation: `add`, `length`, `min`, `max`

**Relational algebra:** γ_{G; F(A)→B}(R).

jq has per-array aggregates that correspond to SQL aggregate functions:

| SQL | jq | Notes |
|---|---|---|
| `COUNT(*)` | `length` | On the group array |
| `SUM(a)` | `map(.a) \| add` | Project then add |
| `AVG(a)` | `map(.a) \| add / length` | No native AVG |
| `MIN(a)` | `min_by(.a).a` | Or `map(.a) \| min` |
| `MAX(a)` | `max_by(.a).a` | Or `map(.a) \| max` |
| `STRING_AGG(a, ',')` | `map(.a) \| join(",")` | |
| `ARRAY_AGG(a)` | `map(.a)` | |

jq has no `SUM` or `AVG` directly — you must decompose them. This is a meaningful gap for data analysis work.

### Union (∪): `add` or Concatenation

**Relational algebra:** R ∪ S.

**jq:** `[expr1, expr2]` or `(arr1 + arr2) | unique`.

```sh
# Bag union (no deduplication)
echo 'null' | jq '([1,2,3] + [2,3,4])'
# [1, 2, 3, 2, 3, 4]

# Set union (deduplicated)
echo 'null' | jq '([1,2,3] + [2,3,4]) | unique'
# [1, 2, 3, 4]
```

**jq handles union cleanly, but bag vs. set semantics requires explicit `unique`.**

### Difference (−): No Direct Operator

**Relational algebra:** R − S.

**jq:** Must be expressed through `select(. | in(...) | not)` or similar.

```sh
# "Elements of A not in B"
echo '{"a":[1,2,3,4,5],"b":[2,4]}' | jq \
  '[.a[] | . as $x | select((.b | contains([$x])) | not)]'
# [1, 3, 5]
```

This is not efficient (O(n*m) containment check). jq has no `except` or `minus` operator.

**jq is awkward for set difference.**

### Transitive Closure / Recursion: `recurse`

**Relational algebra:** Not expressible. Requires Datalog / WITH RECURSIVE.

**jq:**

```sh
echo '{"graph":{"a":["b","c"],"b":["d"],"c":[],"d":[]}}' | \
  jq '.graph as $g | "a" | [limit(100; recurse(($g[.]? // [])[]?))]'
# ["a", "b", "c", "d"]
```

jq's `recurse` enables transitive closure, making it more expressive than classical relational algebra. However, the recursion depth is limited and cycle detection is not built in.

**jq can express recursion, awkwardly. No cycle protection by default.**

### Summary: jq vs. Relational Algebra

| RA Operation | jq Support | Ergonomics |
|---|---|---|
| Selection (σ) | `select(cond)` | Excellent |
| Projection (π) | Object construction | Good (no auto-dedup) |
| Cartesian product (×) | Variable binding loops | Verbose |
| Join (⋈) | Variable binding + select | Verbose, O(n*m) |
| Union (∪) | `+` then `unique` | Good |
| Difference (−) | No direct operator | Awkward |
| Intersection (∩) | No direct operator | Awkward |
| Group-by (γ) | `group_by` + `map` | Good structure, verbose aggregates |
| Aggregation | `length`, `add`, `min`, `max` | Missing `sum`, `avg` |
| Rename (ρ) | Object construction | Good |
| Recursion | `recurse` | Functional but verbose |
| Ordering | `sort_by` | Good |
| Limit/offset | `limit`, `.[n:m]` | Good |
| Window functions | No direct support | Very awkward |
| Division (÷) | No direct operator | Very awkward |

---

## 7. What Evident Should Adopt from jq

### 7.1 The Generator/Stream Model

**What jq does:** Every expression produces a stream of zero, one, or many values. `select` produces zero or one; `.[]` produces many; field access produces one. All composable through `|`.

**Why Evident should adopt it:** Evident's constraint model already implements this: a claim either has solutions (one or many) or no solutions (zero). But the query layer — when you want to extract values from established claims — should explicitly embrace the generator model. A query `? e in employees, e.dept == "eng"` should stream results, not require specifying an accumulator.

**Proposed Evident form:**
```evident
-- Stream of all matching employees
{ e | e in employees, e.dept == "eng" }

-- Stream of names (projection)
{ e.name | e in employees, e.dept == "eng" }
```

The set comprehension notation `{ expr | conditions }` is Evident's native equivalent of jq's generator pipeline.

### 7.2 The Pipe Operator for Query Pipelines

**What jq does:** `f | g` composes filters. The output of `f` feeds into `g`.

**Why Evident should adopt it (partially):** For ad-hoc query pipelines — things like "take this set, sort it, take the top 10, format it" — a pipe syntax is cleaner than nested function calls. jq demonstrates that `| sort_by(.salary) | reverse | limit(10; .[])` reads naturally.

**Proposed Evident form (query context only):**
```evident
? { e | e in employees, e.dept == "eng" }
    | sort_by(.salary)
    | reverse
    | limit 10
```

This should be syntactic sugar in the query layer, not a core claim construct.

### 7.3 Field Access with `.field` Syntax

**What jq does:** `.name` means "project the `name` field of the current value."

**Why Evident should adopt it:** This is already natural mathematical notation for record fields. Evident's existing examples use `e.department`, `req.status`, etc. This is the right choice and jq confirms it works well at scale.

**Status:** Already in Evident. Keep it.

### 7.4 The `select` Name and Filter Pattern

**What jq does:** `select(cond)` is the canonical filter-pass-or-suppress operation.

**Why Evident should adopt it:** The name `select` is now standard in data query languages (SQL, LINQ, dplyr, jq, Elixir Enum). Evident's constraint-based filtering is even more powerful — you don't need a `select` because constraints in the claim body are already filters. But for query-layer pipelines:

**Proposed Evident form:**
```evident
-- In a query pipeline
? employees | select(.dept == "eng")

-- More naturally as a comprehension
{ e | e in employees, e.dept == "eng" }
```

### 7.5 Construction Syntax `{key: expr}`

**What jq does:** `{name: .name, dept: .dept}` builds a new record from parts of the current value.

**Why Evident should adopt it:** Projection and reshaping are common operations. The object literal syntax `{field: expr}` is already standard in JSON-family languages and immediately readable.

**Proposed Evident form:**
```evident
-- Reshape records in a query
{ e | e in employees, e.dept == "eng" }
    | map { name: .name, salary: .salary }
```

Or in comprehension form:
```evident
{ {name: e.name, salary: e.salary} | e in employees, e.dept == "eng" }
```

### 7.6 `group_by` as an Explicit Operation

**What jq does:** `group_by(.field)` partitions an array into sub-arrays by a key.

**Why Evident should adopt it (with improvements):** Group-by is the central operation for aggregating data. jq's version works well but requires the awkward `.[0].field` to extract the key.

**Proposed Evident form:**
```evident
-- Group employees by department, with cleaner key access
employees
    | grouped_by .dept
    | map { dept: .key, count: .members | count, names: .members | map .name }
```

Here `.key` is the grouping key (not `.[0].dept`), and `.members` is the group as a set.

### 7.7 `sort_by`, `min_by`, `max_by` as Named Operations

**What jq does:** `sort_by(.field)`, `min_by(.field)`, `max_by(.field)` are obvious and widely used.

**Why Evident should adopt them:** The `_by(expr)` naming convention is clear and generalizable. The operations themselves (sort on a key, find extremum by key) are universally needed.

**Proposed Evident form:**
```evident
employees | sort_by .salary
employees | max_by .salary
employees | min_by .age
```

### 7.8 `map(f)` / `filter(f)` for Set Transformations

**What jq does:** `map(f)` applies `f` to each element and collects results. Equivalent to `[.[] | f]`.

**Why Evident should adopt it:** These are the canonical array/set higher-order operations. `map` and `filter` are universal. jq's `map(f)` with `f` being an arbitrary generator is particularly powerful — `map(select(. > 0))` as a filter pattern is elegant.

**Proposed Evident form:**
```evident
employees | map { name: .name, salary: .salary }
employees | filter { .salary > 100000 }
```

### 7.9 `reduce` for Aggregation

**What jq does:** `reduce .[] as $x (init; body)` is a left fold.

**Why Evident should adopt it:** When standard aggregate functions (`count`, `sum`, `max`) are not enough, `reduce` provides a general escape hatch. Evident needs `count`, `sum`, `min`, `max` as built-ins, but `reduce` as the general form.

**Proposed Evident form:**
```evident
-- Built-in aggregates
count { e | e in employees }
sum { e.salary | e in employees }

-- General reduce
reduce employees as $e from 0 with .total + $e.salary
```

### 7.10 `to_entries` / `from_entries` / `with_entries`

**What jq does:** Converts objects to and from `[{key, value}]` format, enabling map-over-object-entries.

**Why Evident should adopt it:** Working with dynamic objects (where the keys are data, not schema) requires this pattern. It is not common in the core language, but in the query layer for reshaping outputs it is frequently needed.

### 7.11 `flatten`, `unique`, `unique_by`

**What jq does:** Standard set operations on arrays — flatten nested arrays, deduplicate.

**Why Evident should adopt it:** `unique` is the enforcement of set semantics (Evident works with sets, so uniqueness is usually automatic). `flatten` is useful for nested set structures. `unique_by(.field)` is useful for deduplication by key.

### 7.12 String Interpolation `"\(.expr)"`

**What jq does:** Embed jq expressions in string literals.

**Why Evident should adopt it:** This is readable and widely understood. String construction is common in output formatting. The `\(.expr)` syntax is cleaner than `%s` formatting or concatenation.

**Proposed Evident form:**
```evident
"Employee \(.name) earns \(.salary)"
```

---

## 8. What Evident Should NOT Copy from jq

### 8.1 The Implicit `.` as the Sole Context Variable

**What jq does:** `.` is always the current value. There are no other implicit inputs.

**Why Evident should not copy it uncritically:** jq's implicit `.` is elegant in simple cases but becomes treacherous in nested contexts. Inside `map(f)`, `.` refers to each element, not the outer array. Inside `reduce .[] as $x (...; body)`, `.` inside the body refers to the accumulator, not the element (which is `$x`). Inside `group_by(.field)`, `.` inside the key expression refers to each element.

These context shifts are the largest source of bugs and confusion in jq programs. The solution in jq is `as $var` to capture the outer `.` before entering a context that shifts it — but this requires discipline and is easy to forget.

**What Evident should do instead:** Use explicit named variables for all bindings. Evident's `as $e` pattern (or `e in set`) makes every variable explicit. This is more verbose but far clearer. The cost is worth it for a language intended for non-jq-experts.

```evident
-- jq: relies on context to know which . is which
.employees[] | . as $e | {name: $e.name, dept: .dept}  -- bug: .dept is not in scope

-- Evident: every variable is named explicitly
{ e.name, e.dept | e in .employees }  -- clear: e is the employee
```

### 8.2 Bag Semantics as the Default

**What jq does:** Arrays can have duplicates. `+` on arrays concatenates, not unions. `unique` is an explicit deduplication step.

**Why Evident should not copy it:** Evident is explicitly a *set* language. Sets do not have duplicates. Using bag semantics as the default (and requiring explicit deduplication) means bugs where duplicates creep in silently. Evident should use set semantics by default, with explicit opt-in to bag semantics where order and duplicates matter.

**What Evident should do instead:** All collections are sets by default. Duplicates are impossible (the constraint solver prevents them). Sequences (ordered with duplicates) are an explicit type when needed.

### 8.3 No Native Join Operator

**What jq does:** Joins must be expressed by hand using variable binding and nested `select`. This is verbose, error-prone, and O(n*m) without a query optimizer.

**Why Evident should not copy it:** Join is one of the most fundamental operations in data processing. jq's lack of a native join operator is a significant limitation — you can see it clearly in the join example earlier in this document. Evident should have a native join syntax, especially for the natural join case (equijoin on shared field name).

**What Evident should do instead:**
```evident
-- Natural join (equijoin on shared fields)
employees join departments on .dept_id == .id

-- Or expressed as a claim (which is already a join)
evident employee_with_dept e d
    e in employees
    d in departments
    e.dept_id == d.id
```

### 8.4 No Type Safety / Schema

**What jq does:** jq is dynamically typed. Accessing `.name` on a number produces an error (or nothing with `?`). There is no schema enforcement.

**Why Evident should not copy it:** For a language designed around constraint satisfaction and correctness, type safety is important. Evident should know the schema of its records and enforce it. Accessing `.salary` on an employee record that has no salary field should be a type error, not a silent null.

### 8.5 The `reduce` / `foreach` Syntax is Awkward

**What jq does:**
```sh
reduce .[] as $x (0; . + $x)
foreach range(5) as $x (0; . + $x)
```

The syntax `(init; body)` with `.` as accumulator and `$x` as the element is unusual and hard to read. The parenthesized pair is not consistent with any other jq syntax.

**Why Evident should not copy it:** The `reduce` pattern is fundamental and should have readable syntax. `reduce X as $x from init with body` or `fold X starting from init accumulating + $x` or any clearer form is better.

**What Evident should do instead:**
```evident
-- Clearer reduce
fold employees with .total + .salary starting from { total: 0 }

-- Or using standard aggregate functions
sum { e.salary | e in employees }
```

### 8.6 The `label-break` Mechanism

**What jq does:** `label $out | ... | break $out` allows early exit from a generator loop.

**Why Evident should not copy it:** This is an imperative escape hatch bolted onto a functional model. It arises because jq has no native `limit` with early termination at the language level. Evident should instead provide `limit N` and `first` as primitives that naturally stop after N results.

**What Evident should do instead:**
```evident
-- Take first 10 results
{ e | e in employees, e.dept == "eng" } | limit 10

-- Take the first match
first { e | e in employees, e.salary > 100000 }
```

### 8.7 String-Split Overloading of `/`

**What jq does:** `"a,b,c" / ","` splits a string, using `/` as the split operator.

**Why Evident should not copy it:** This is clever but confusing. `/` looks like division. Most readers will not immediately recognize `str / delim` as "split string on delimiter." Explicit function call (`split(str, delim)`) is clearer.

### 8.8 Global State (`$ENV`, `env`)

**What jq does:** `env` and `$ENV` access environment variables. `$__loc__` gives the source location.

**Why Evident should not copy it:** These are jq conveniences for shell integration. Evident is not a shell scripting language. Environmental state should be passed explicitly as input, not accessed as implicit global state. Implicit globals undermine Evident's commitment to making all dependencies explicit.

### 8.9 Path-Based Mutation (`setpath`, `delpaths`)

**What jq does:** `setpath(["a","b"]; 42)` produces a modified copy with a value set at a path. This is a functional update but expressed through runtime path arrays.

**Why Evident should not copy it:** Path-based mutation is a runtime escape hatch for situations where you don't know the schema at compile time. Evident, being a typed system, should express updates through field assignment syntax on typed records, not through runtime path arrays.

---

## 9. Summary Table: jq Features and Evident Stance

| jq Feature | Adopt? | Notes |
|---|---|---|
| Generator/stream model | Yes | Core to Evident's query semantics |
| `|` pipe composition | Yes, in query layer | Not in claim bodies (unordered there) |
| `.` identity | Partial | Use `.field` syntax; avoid context-shifting implicit `.` |
| `select(cond)` | Yes | Rename to `filter` or `where` in Evident |
| `map(f)` | Yes | Standard set transformation |
| `{key: expr}` construction | Yes | Standard record construction |
| `[expr]` array construction | Yes | For ordered sequences |
| `group_by(.field)` | Yes, with improvements | `.key` instead of `.[0].field` |
| `sort_by(.field)` | Yes | Standard |
| `unique_by(.field)` | Yes | Deduplication by key |
| `min_by`, `max_by` | Yes | Standard extrema |
| `reduce` | Yes, cleaner syntax | Fold with cleaner notation |
| `limit`, `first`, `last` | Yes | Essential stream control |
| `flatten` | Yes | Nested set/array flattening |
| `to_entries` / `from_entries` | Yes | For dynamic key manipulation |
| String interpolation `"\(.)"` | Yes | Readable string construction |
| `test`, `match`, `capture` | Yes | Standard regex support |
| `split`, `join` | Yes | Standard |
| `any`, `all` | Yes, rename | `exists` and `forall` align with logic vocabulary |
| `recurse` / `..` | Yes | For tree traversal |
| Implicit `.` context shifting | No | Explicit named variables instead |
| Bag semantics as default | No | Set semantics by default |
| No native join | Improve | Evident needs native join syntax |
| No type schema | Improve | Evident is typed |
| `reduce (.[] as $x (init; body))` syntax | No | Too syntactically odd; use clearer form |
| `label-break` | No | Use `limit` / `first` instead |
| `str / delim` split | No | Use `split(str, delim)` |
| `env` / global state | No | Explicit inputs only |
| `setpath` / `delpaths` | No | Use typed field updates |
| `@base64`, `@uri`, etc. | Maybe | Useful in output formatting; not core |

---

## 10. Core Lessons from jq for Evident

**The stream model is the right abstraction for queries.** jq proves that expressing selection, iteration, and transformation through a single "stream of values" model is coherent and powerful. Evident's set comprehension syntax is the natural set-semantic equivalent.

**Implicit context is a double-edged sword.** `.` makes simple cases very concise. Context shifts make complex cases confusing. Evident should prefer explicit variable naming (`e in employees`) over implicit context (`./the current element`).

**Construction and extraction should be symmetric.** jq's `{key: .field}` for both extracting (`.field`) and constructing (`{key: ...}`) is elegant. Evident's record syntax should similarly let you read and write records using the same field-access notation.

**The `group_by` → `map` pattern is the GROUP BY analogue.** jq establishes the idiom: group an array, then map over the groups to produce summaries. Evident should provide this at the claim level, where a grouped relation has the grouping key as a first-class attribute and the group members as a set-valued attribute.

**A pipe in the query layer does not imply a pipe in the claim layer.** jq is uniformly pipe-based. Evident's claim bodies are constraint sets (unordered). The pipe is appropriate for the query/output layer but should not imply ordering in the constraint layer. These two layers should be syntactically distinct.

**Missing: native join, set difference, and typed aggregates.** These are jq's three biggest gaps relative to SQL and relational algebra. Evident should not inherit these gaps. Native join syntax, set difference (`except`), and named aggregate functions (`count`, `sum`, `avg`, `min`, `max`) should all be first-class.
