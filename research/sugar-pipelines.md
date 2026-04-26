# Pipeline and Collection Chaining Syntax Across Programming Languages

Research for the Evident constraint programming language design project. Goal: survey pipeline and chaining syntax idioms across languages, understand what makes them readable and composable, and extract principles for Evident's treatment of "take this set, filter it, project a field, assert something about the result."

---

## 1. Unix Pipes — The Founding Metaphor

Unix pipes, introduced in Version 3 Unix (1973) by Doug McIlroy, are the original pipeline. The `|` operator connects the standard output of one process to the standard input of the next:

```sh
cat access.log | grep "404" | awk '{print $7}' | sort | uniq -c | sort -rn | head -20
```

Each stage is a *filter*: it reads a stream, transforms it, and writes a stream. The data flows left to right. Stages are independent programs knowing nothing about each other. The shell composes them.

### What made Unix pipes so influential

**The identity of the medium.** Every stage speaks the same language: a stream of bytes, usually lines of text. This uniform medium means any program can connect to any other. There are no type mismatches, no adapter layers, no impedance.

**Independence and composability.** Each program in a pipeline does one thing. `sort` sorts. `uniq` deduplicates. `wc` counts. Because each is focused, you can compose them freely. McIlroy's design principle: "Write programs that do one thing and do it well. Write programs to work together."

**Discoverability through the single-stream convention.** Because the medium is always the same (a byte stream), you can try stages interactively. Stop the pipeline at any point and inspect the intermediate result. This is the REPL property of pipelines: every prefix is a valid program.

**Left-to-right temporal flow matches reading order.** The data moves in the direction that English reads. Early programming languages had nested function application that read right-to-left: `wc(uniq(sort(grep("404", cat(file)))))`. Pipes reversed this.

### What Unix pipes cannot do well

**Structured data.** Pipes work on byte streams. Connecting tools that output JSON to tools that expect CSV requires explicit conversion. Every field extraction requires a mini-parser (`awk`, `cut`, `jq`). The uniform medium is both the strength and the limitation.

**Multiple outputs.** A process has one stdout. Pipelines are linear; graphs require `tee` or named pipes. Multiple streams must be interleaved or serialized.

**Error handling.** A failing stage produces no output; subsequent stages see an empty or partial stream. Shell pipelines do not propagate errors by default. `set -o pipefail` helps, but the error model is weak.

**The Evident connection.** Evident operates on sets, which are structured and typed. The Unix insight — uniform medium, left-to-right flow, stages that transform and filter — is valuable, but the medium must be richer. A pipeline over a typed set is what every language after Unix has tried to build.

---

## 2. Elixir `|>` — The Pipe Operator That Spread Everywhere

Elixir's pipe operator `|>` rewrites `f(g(x))` as `x |> g() |> f()`. More precisely, `x |> f(a, b)` desugars to `f(x, a, b)` — the value on the left becomes the **first argument** of the function on the right.

```elixir
# Without pipe: inside-out reading, argument buried in nesting
result = Enum.sum(Enum.map(Enum.filter(list, fn x -> x > 0 end), fn x -> x * 2 end))

# With pipe: left-to-right, data flows through each stage
result =
  list
  |> Enum.filter(fn x -> x > 0 end)
  |> Enum.map(fn x -> x * 2 end)
  |> Enum.sum()
```

The typical Elixir data pipeline:

```elixir
"access.log"
|> File.read!()
|> String.split("\n")
|> Enum.filter(&String.contains?(&1, "404"))
|> Enum.map(&parse_log_line/1)
|> Enum.group_by(& &1.path)
|> Enum.map(fn {path, entries} -> {path, length(entries)} end)
|> Enum.sort_by(fn {_, count} -> count end, :desc)
|> Enum.take(20)
```

### Why Elixir's pipe was influential

**The first-argument convention makes it systematic.** The Elixir standard library is designed so that the "primary data" argument is always first. `Enum.filter(list, pred)` — list is first. `String.split(str, sep)` — string is first. This is not accidental; it is a language-level design constraint on all APIs. The convention makes `|>` work uniformly.

**The operator is syntactic sugar, not a runtime primitive.** The compiler rewrites pipe chains before evaluation. There is no runtime overhead. The code that runs is the same as manually nested function calls.

**It handles multi-argument functions naturally.** `list |> Enum.filter(fn x -> x > 0 end)` supplies the remaining argument explicitly. The pipe fills the first slot; the programmer supplies the rest. This is partial application without a currying requirement.

**It makes intermediate results inspectable.** In Elixir, inserting `|> IO.inspect()` anywhere in a chain prints the current value and passes it through unchanged. This is the same "stop the pipeline and look" property as Unix pipes.

### The "data-first" convention as a design discipline

The Elixir community discovered that the first-argument convention requires discipline across the entire ecosystem. When a library puts data second (as some functional libraries do, e.g. `pred |> Enum.filter(list)` would be wrong), the pipe operator breaks. The convention must be consistent or the ergonomics collapse.

This is a concrete design lesson: a pipeline operator is only as good as the API discipline behind it.

---

## 3. F# `|>` — Point-Free Style and Functional Composition

F# introduced `|>` before Elixir (F# 2005, Elixir 2012). In F#, the operator has the type `'a -> ('a -> 'b) -> 'b`, making it the reverse function application operator:

```fsharp
// Definition
let (|>) x f = f x

// Usage: identical ergonomics to Elixir
[1..10]
|> List.filter (fun x -> x % 2 = 0)
|> List.map (fun x -> x * x)
|> List.sum
```

### Point-free style

Because F# is fully curried, pipeline stages can be written as partially applied functions rather than lambdas:

```fsharp
// With lambdas (point-ful)
[1..10]
|> List.filter (fun x -> x % 2 = 0)
|> List.map (fun x -> x * x)

// Point-free: the element variable is implicit
let pipeline = 
    List.filter isEven
    >> List.map square    // >> is left-to-right function composition
```

`>>` is F#'s function composition operator: `(f >> g) x = g(f(x))`. It composes functions without naming the argument that flows through.

```fsharp
// These three forms are equivalent in F#
let process1 = List.filter isEven >> List.map square >> List.sum
let process2 list = list |> List.filter isEven |> List.map square |> List.sum
let process3 list = List.sum (List.map square (List.filter isEven list))
```

### F# vs Elixir pipes

| Aspect | F# `\|>` | Elixir `\|>` |
|---|---|---|
| Currying | Full currying; partial application natural | No currying; lambdas or `&` capture syntax |
| Data position | First arg by convention | First arg by convention (enforced culturally) |
| Composition | `>>` (point-free) | No built-in composition operator |
| Ecosystem convention | Less uniform (some libraries inconsistent) | Very uniform (community enforced) |
| Interactive inspection | F# Interactive (FSI) | `IO.inspect/1` insertable in chain |

F# demonstrates that currying and a pipe operator are synergistic. In a curried language, every function is automatically a pipeline stage — `List.filter pred` is already a function from `'a list -> 'a list` that fits naturally into a pipeline. In a non-curried language like Elixir, you must explicitly close over the extra arguments each time.

---

## 4. R's `%>%` (magrittr) and Native `|>`

R's most influential pipeline innovation came from a third-party package. The magrittr package (2014) introduced `%>%`, which became the dominant idiom in R data analysis through the dplyr ecosystem:

```r
library(dplyr)
library(magrittr)

mtcars %>%
  filter(cyl > 4) %>%
  select(mpg, hp, wt) %>%
  group_by(cyl) %>%
  summarize(
    mean_mpg = mean(mpg),
    mean_hp  = mean(hp),
    n        = n()
  ) %>%
  arrange(desc(mean_mpg))
```

This reads exactly like a data analysis workflow: start with the dataset, narrow to relevant rows, select relevant columns, group, summarize, sort. Each step is a transformation on a data frame (R's table type).

### Why dplyr's pipe API became dominant

**The data frame as uniform medium.** Just as Unix pipes work over byte streams, dplyr's pipe works over data frames. Every dplyr verb takes a data frame as its first argument and returns a data frame. The medium is consistent. Any two verbs can be connected.

**Verb naming matches analyst vocabulary.** `filter`, `select`, `group_by`, `summarize`, `arrange`, `mutate` are words that data analysts already use to describe their workflows. The API is not "apply this function" but "do this operation." The verb-based naming matches the domain.

**The `%>%` syntax was adopted faster than any language feature.** It spread through the R community in 2014–2016 faster than any R language change ever had, because it solved a real pain point and the package was easy to install. This is evidence that programmer adoption tracks ergonomic improvement, not theoretical elegance.

### R's native `|>` (R 4.1+, 2021)

R added a native `|>` operator in version 4.1:

```r
mtcars |> subset(cyl > 4) |> nrow()
```

The native `|>` is simpler than `%>%`: it inserts the left side as the first argument and does nothing else. The magrittr `%>%` has additional features (placeholder `%>% f(arg1, ., arg2)`, right-hand side assignment, etc.) that the native operator omits.

**The key insight:** the native operator succeeded by being simpler, not more powerful. The magrittr extras are rarely needed in practice, and the simplified semantics make the native `|>` easier to reason about and faster to evaluate.

### What R teaches about pipe adoption

1. **Ecosystem buy-in matters more than syntax beauty.** dplyr's verbs and `%>%` are inseparable. The operator succeeded because the API was designed for it.
2. **A simpler operator is usually better.** The native `|>` discarded most of magrittr's features and is now preferred for new code.
3. **Domain vocabulary in the API makes pipelines readable.** `filter %>% select %>% summarize` reads as a description of intent, not as function application.

---

## 5. Ruby Method Chaining — The Fluent Interface

Ruby's `Enumerable` module (mixed into any class implementing `each`) provides a rich API designed for chaining. Every operation returns a new collection:

```ruby
# Filter positive numbers, double them, sum the result
[1, -2, 3, -4, 5]
  .select { |x| x > 0 }
  .map    { |x| x * 2 }
  .sum
# => 18

# More complex: find top-3 employees by salary in each department
employees
  .group_by { |e| e.department }
  .transform_values { |group|
    group
      .sort_by { |e| -e.salary }
      .first(3)
  }

# Database-like pipeline over objects
orders
  .select { |o| o.date >= 30.days.ago }
  .reject { |o| o.cancelled? }
  .map    { |o| o.total_value }
  .sum
```

### Ruby's block syntax as the key enabler

Ruby's `{ |x| ... }` block syntax is what makes chaining readable. Each stage's transformation is expressed as a self-contained block immediately following the method name. The block is syntactically attached to the method call, so visually each stage is a single unit.

Compare with a language that requires lambda syntax:

```python
# Python: each step needs an explicit lambda keyword
from functools import reduce
result = reduce(lambda acc, x: acc + x,
                map(lambda x: x * 2,
                    filter(lambda x: x > 0, lst)), 0)

# Even with comprehensions, the pipeline is not left-to-right
result = sum(x * 2 for x in lst if x > 0)
```

```ruby
# Ruby: each step reads naturally
result = lst.select { |x| x > 0 }.map { |x| x * 2 }.sum
```

The Python comprehension is more concise but reverses the order (`sum` is at the front, `filter` is at the end). Ruby's chain reads in evaluation order.

### The fluent interface pattern

"Fluent interface" (Fowler and Evans, 2005) is the design pattern where every method returns the receiver (or a modified copy) to allow chaining. In Ruby's collection API, this is achieved by always returning an `Enumerable` — the next method has something to chain on.

Ruby's lazy enumerables (`.lazy`) make chains efficient by not materializing intermediate collections:

```ruby
# Without lazy: creates three intermediate arrays
(1..Float::INFINITY).select { |x| x.odd? }.map { |x| x ** 2 }.first(5)
# => NoError (infinite, but first(5) without lazy would hang)

# With lazy: evaluates only as much as needed
(1..Float::INFINITY).lazy.select { |x| x.odd? }.map { |x| x ** 2 }.first(5)
# => [1, 9, 25, 49, 81]
```

Laziness turns chains over potentially infinite collections into efficient pipelines. Each element flows through all stages before the next element enters.

---

## 6. JavaScript Array Method Chaining and Functional Libraries

JavaScript arrays have chainable methods built into the language:

```javascript
const result = [1, -2, 3, -4, 5]
  .filter(x => x > 0)
  .map(x => x * 2)
  .reduce((acc, x) => acc + x, 0);
// => 18

// Real-world: process API response data
const topUsers = data.users
  .filter(u => u.active && u.signupDate > cutoff)
  .map(u => ({ ...u, score: computeScore(u) }))
  .sort((a, b) => b.score - a.score)
  .slice(0, 10);
```

**Optional chaining `?.`** extends the pipeline pattern to handle null/undefined safely:

```javascript
const city = user?.address?.city?.toLowerCase() ?? "unknown";
```

Each `?.` short-circuits to `undefined` if the value on the left is null. This is a pipeline that handles missing values without explicit null checks at each stage. The `??` at the end provides a default.

### Lodash/Ramda: functional approach vs. method chaining

Lodash provides method chaining via `_.chain()`:

```javascript
_.chain([1, -2, 3, -4, 5])
  .filter(x => x > 0)
  .map(x => x * 2)
  .sum()
  .value();   // .value() materializes the result
```

Ramda takes the opposite approach: composition over chaining. All Ramda functions are auto-curried and data-last (opposite of Elixir's data-first):

```javascript
const process = R.pipe(
  R.filter(x => x > 0),
  R.map(x => x * 2),
  R.sum
);

process([1, -2, 3, -4, 5]);  // => 18
```

`R.pipe` composes functions left-to-right (equivalent to F#'s `>>`). Because Ramda is data-last and auto-curried, each stage is a fully reified function — `R.filter(x => x > 0)` is already a function from array to array. The composed `process` function can be stored, passed, and reused without naming the input.

**Data-first (Elixir) vs. data-last (Ramda) is a critical design choice:**

| | Data-first | Data-last |
|---|---|---|
| Pipe operator | Natural: `data \|> f(args)` | Natural: `f(args)(data)` |
| Composition | Awkward without `>>` | Natural: partially applied function |
| Naming convention | `Enum.filter(list, pred)` | `R.filter(pred, list)` |
| Ecosystem | Elixir, F# stdlib, dplyr | Ramda, Haskell (mostly) |

---

## 7. Kotlin Scope Functions — Integrating Side Effects into Chains

Kotlin provides five "scope functions" — `let`, `run`, `with`, `apply`, `also` — that allow inserting arbitrary computations into method chains:

```kotlin
val result = listOf(1, -2, 3, -4, 5)
    .filter { it > 0 }
    .map { it * 2 }
    .also { println("Intermediate: $it") }   // log, then pass through
    .sum()
    .let { it * 100 }                         // transform the scalar result
```

The five scope functions differ in two dimensions:

| Function | Context object as | Returns |
|---|---|---|
| `let` | `it` (lambda parameter) | Lambda result |
| `run` | `this` (receiver) | Lambda result |
| `also` | `it` (lambda parameter) | Context object (unchanged) |
| `apply` | `this` (receiver) | Context object (unchanged) |
| `with` | `this` (receiver, not extension) | Lambda result |

The critical distinction is between `let`/`run` (which return the lambda's result, *transforming* the chain) and `also`/`apply` (which return the original object, allowing *side effects* without breaking the chain).

```kotlin
// also: insert a debug print without breaking the chain
users
    .filter { it.active }
    .also { logger.info("Found ${it.size} active users") }
    .map { it.id }

// let: transform the end result
val uppercase = "hello"
    .also { println("Original: $it") }
    .uppercase()
    .let { "<<$it>>" }   // returns String, changing the chain type
// => "<<HELLO>>"

// apply: configure an object and return it
val request = HttpRequest()
    .apply {
        method = "POST"
        url = "https://example.com/api"
        headers["Content-Type"] = "application/json"
    }
// request is the HttpRequest, fully configured
```

### What Kotlin's scope functions reveal

**`also` solves the logging problem.** In a pure chain, you cannot insert a side-effecting operation that does not change the value. `also` threads the current value through unchanged, executing a block for its side effects only. This is the tap or `IO.inspect` pattern made explicit.

**Different return semantics enable type-changing chains.** `let` lets you apply an arbitrary function to the current value and continue with the result. This allows chains that change the type at each stage — the chain does not have to be homogeneous.

**Named scope functions are clearer than an anonymous operator.** `let`, `run`, `also`, `apply` each communicate intent. `also` signals "I'm doing something here but the value doesn't change." `let` signals "I'm transforming this."

---

## 8. jq — Pipelines as the Entire Language Model

`jq` is a command-line JSON processor where every expression is a **filter** — a function from JSON value to JSON value (or a sequence of values). Filters compose with `|`:

```sh
# Get all failed requests from a log file
cat requests.json | jq '
  .[]
  | select(.status >= 400)
  | { path: .path, status: .status, error: .error.message }
'

# Group by status code, count each group
cat requests.json | jq '
  [.[] | select(.status >= 400)]
  | group_by(.status)
  | map({ status: .[0].status, count: length })
  | sort_by(.count)
  | reverse
'
```

### jq's core design innovations

**`.` as identity and focus.** In jq, `.` is the identity filter — it outputs its input unchanged. `.field` is "project field from the current value." `.[]` is "iterate all elements of the current value." The `.` is always the implicit current value, making every filter relative to context.

```jq
.users                    # project the "users" field
.users[]                  # iterate all user objects
.users[] | .name          # extract name from each user
.users[] | select(.active)  # filter to active users
.users[] | select(.active) | .name  # chain: active user names
```

**`[]` as iteration.** `.[]` is the "explosion" operator — it turns a JSON array into a sequence of values, one per output. This is the flatMap / monadic bind pattern: a filter that receives an array can output multiple values, and subsequent filters receive each value independently. `select` filters the stream; `map(f)` is sugar for `[.[] | f]`.

**The implicit input/output model.** Every jq filter has one implicit input and produces zero or more outputs. The `|` feeds the output of the left filter into the right filter. There are no named variables for the main data flow — the "current value" is always implicit.

**The generator pattern.** Because filters can output multiple values, `jq` filters are essentially generators. `range(5)` outputs 0, 1, 2, 3, 4 in sequence. `select(.)` is a filter that either outputs its input (if truthy) or nothing. `limit(n; generator)` takes the first n outputs of a generator.

```jq
# All combinations of two numbers from 0 to 4
[range(5) as $x | range(5) as $y | {x: $x, y: $y}]

# First path in a graph (via recursive descent)
def paths(adj):
  . as $start
  | recurse(
      .current as $c
      | adj[$c][]?
      | select(. != $start)
      | {path: ($start.path + [.]), current: .}
    )
  | .path;
```

### What jq teaches

**`.` as the current focus makes context explicit without naming it.** In most languages, "the current element" has different names in different lambda scopes (`x`, `it`, `self`). In jq, it is always `.`. This is consistent but can be confusing when filters are nested (the `.` inside a `map` refers to the inner value, not the outer one).

**The generator model is more powerful than filter-and-collect.** Most pipeline designs assume one-in, one-out. `jq`'s one-in, many-out (or zero-out) model handles explosion, flatMap, and filtering in a single unified primitive. This is the monad-list model expressed as syntax.

---

## 9. Haskell — Function Composition vs. Pipes

Haskell provides two operators for assembling pipelines: `.` (compose) and `&` (reverse application, the "pipe" operator). They enable two distinct styles:

```haskell
-- Function composition (right-to-left)
process :: [Int] -> Int
process = sum . map (*2) . filter (>0)

-- Reverse application / pipe (left-to-right)
result :: Int
result = [1, -2, 3, -4, 5] & filter (>0) & map (*2) & sum

-- Traditional function application (inside-out)
result = sum (map (*2) (filter (>0) [1, -2, 3, -4, 5]))
```

### `.` (compose): defining functions without naming the argument

`f . g` is "f after g": `(f . g) x = f(g(x))`. This is right-to-left composition.

Point-free style uses `.` to define functions as compositions without ever naming their argument:

```haskell
-- Point-ful: names the argument
processWords :: String -> Int
processWords text = length (filter isLong (words text))
  where isLong w = length w > 5

-- Point-free: the text argument is implicit
processWords :: String -> Int
processWords = length . filter isLong . words
  where isLong w = length w > 5
```

Point-free style is concise but can be hard to read when compositions are complex. The expert Haskell community uses it freely; learners find it opaque.

### `&` (reverse application): making data flow left-to-right

`x & f = f x`. This is just function application with arguments reversed, making left-to-right reading natural:

```haskell
[1..10] & filter even & map (^2) & sum
```

`&` is in `Data.Function` (base library). It is less idiomatic than `$` (which is right-to-left application) but growing in use because it aligns with how programmers think about data transformation.

### When composition is clearer than piping

Composition (`.`) is better when you are defining a *function*, not immediately applying it. It names the pipeline as a reusable thing:

```haskell
-- This is a reusable transformer, not an application
normalize :: String -> String
normalize = unwords . map capitalize . words . map toLower

-- These are immediate computations
result = inputStr & map toLower & words & map capitalize & unwords
```

The `.` version defines `normalize` without ever mentioning its argument. The `&` version is a sequence of steps applied to `inputStr` right now.

**The composability test:** if you want to pass the pipeline around as a value, use composition (`.`). If you want to express a sequence of steps applied to a specific dataset, use piping (`&` or `|>`).

---

## 10. The Method Cascade Pattern — Messaging the Same Receiver

Method chaining (`.filter().map().sum()`) is *not* the only way to chain operations. Smalltalk introduced the **cascade** operator `;`, which sends multiple messages to the *same receiver* without threading the return value:

```smalltalk
"Smalltalk cascade: all messages go to the same OrderedCollection"
| col |
col := OrderedCollection new.
col
  add: 'first';
  add: 'second';
  add: 'third';
  yourself.  "returns the receiver"
```

In a cascade, each message is sent to the original receiver, not to the result of the previous message. This is for *configuring* an object, not for *transforming* data through it.

### Dart's `..` cascade

Dart adopted this pattern with `..`:

```dart
// Without cascade: repetitive variable reference
var paint = Paint();
paint.color = Colors.blue;
paint.strokeWidth = 2.0;
paint.style = PaintingStyle.stroke;

// With cascade: configure one object
var paint = Paint()
  ..color = Colors.blue
  ..strokeWidth = 2.0
  ..style = PaintingStyle.stroke;
```

Each `..` sends a message to the same `Paint` object created on the first line. The result of the whole expression is the `Paint` object itself.

### Cascade vs. chaining: different use cases

| | Method chaining | Method cascade |
|---|---|---|
| Data flows... | Through each stage (transformed) | Stays on the same object |
| Each stage... | Receives previous output | Receives original receiver |
| Use for... | Data transformation pipelines | Object configuration / builder pattern |
| Return type changes? | Yes (filter returns subset, map returns different type) | No (always same receiver) |

The cascade pattern is the **builder pattern** expressed as syntax. It is useful when an object has mutable configuration and you want to set multiple properties in sequence. It is *not* a data pipeline.

**For Evident:** cascades are irrelevant — Evident deals with immutable sets, not mutable objects. But the conceptual distinction matters: pipeline syntax should imply *transformation*, not *mutation*. The data emerges different at the end of a pipeline.

---

## 11. SQL as a Pipeline — The Backwards-Written Query

SQL's `SELECT` statement encodes a logical pipeline, but writes it in an unusual order. The *logical* evaluation order is:

```
FROM → WHERE → GROUP BY → HAVING → SELECT → ORDER BY
```

But the *written* order is:

```sql
SELECT   columns              -- written first, evaluated fifth
FROM     table                -- written second, evaluated first
WHERE    row_condition         -- written third, evaluated second
GROUP BY grouping_columns     -- written fourth, evaluated third
HAVING   group_condition      -- written fifth, evaluated fourth
ORDER BY ordering_columns     -- written sixth, evaluated sixth
```

A SQL query expressed in evaluation order would be:

```sql
-- Hypothetical "logical-order SQL"
FROM employees
WHERE hire_year >= 2020
GROUP BY department
HAVING COUNT(*) > 5
SELECT department, AVG(salary) AS avg_salary
ORDER BY avg_salary DESC
```

This reads exactly like the pipeline it is: start with the data, filter it, group it, filter the groups, project the result, sort the output.

### Why SQL feels "backwards"

SQL was designed to read like an English sentence: "SELECT the salary FROM employees WHERE department = 'engineering'." English puts the verb (SELECT) before the noun (FROM employees). But the noun must be resolved before the verb can be applied — the column names in the SELECT clause refer to columns that don't exist until after the FROM is processed.

This creates the well-known problem that you cannot write:

```sql
-- This FAILS: alias not visible in WHERE
SELECT salary * 1.1 AS adjusted
FROM employees
WHERE adjusted > 50000   -- 'adjusted' doesn't exist yet
```

The alias `adjusted` is not defined until the SELECT is evaluated, which comes after WHERE. You must write it as:

```sql
SELECT salary * 1.1 AS adjusted
FROM employees
WHERE salary * 1.1 > 50000   -- repeat the expression
```

Or use a subquery. This is a direct consequence of writing the pipeline backwards.

### PRQL and attempts to fix SQL's order

PRQL (Pipelined Relational Query Language, 2022) rewrites SQL in evaluation order with an explicit pipe:

```prql
from employees
filter hire_year >= 2020
group department (
  aggregate { avg_salary = average salary, count = count }
)
filter count > 5
select { department, avg_salary }
sort { -avg_salary }
take 10
```

Each line is a stage in the pipeline. You read top-to-bottom to understand what happens. Aliases defined in one stage are immediately available in the next.

### What SQL teaches about pipeline order

1. **Logical evaluation order should match reading order.** When they differ (as in SQL), programmers must maintain a mental model of two orderings simultaneously.
2. **Pipeline syntax should let later stages reference results of earlier stages.** SQL's backwards order prevents this and forces expression repetition or subqueries.
3. **Named stages enable readable, referenceable intermediate results.** SQL's CTEs (`WITH` clauses) fix the alias problem by making intermediate results into named tables.

**The Evident connection.** Evident's `evident` declaration reads top-down: claim head first, then the sub-claims that establish it. The evaluation order within the body is unordered (the solver finds a valid order). But the *declaration* order — claim before justification — matches how humans explain things. This is the right convention: declare what you want, then describe what establishes it.

---

## 12. What Makes a Good Pipeline Syntax

Drawing from the survey above and from practitioner experience (blog posts, language design rationale, and adoption studies), the key properties are:

### 1. Left-to-right data flow matches reading order

Every successful pipeline syntax flows left-to-right (or top-to-bottom). This is not cultural accident — it is how humans read causation and time. "First this happens, then that, then this." When function application or composition reverses this order (as in traditional f(g(h(x)))), programmers must mentally invert the reading to understand execution order.

**Design rule:** the first thing in a pipeline expression should be the initial data; the last thing should be the final result.

### 2. The medium must be uniform

Unix pipes work because the medium is always a byte stream. dplyr's pipe works because the medium is always a data frame. Elixir's `|>` works because the Elixir standard library consistently puts data first. When the medium is inconsistent — different argument positions, different return types that don't chain — the pipeline breaks and the programmer falls back to nesting.

**Design rule:** pipeline stages should have a consistent contract about where the "current data" goes in and comes out.

### 3. Stages should be independently readable

Each stage in `list.filter { |x| x > 0 }.map { |x| x * 2 }.sum` can be read in isolation. `.filter { |x| x > 0 }` means "keep elements greater than zero." You don't need context to understand it.

This is the chunking property: expert programmers recognize pipeline stages as semantic units. If a stage requires understanding the surrounding stages to interpret, chunking fails and reading becomes sequential token-by-token parsing.

**Design rule:** each stage should be self-contained — its meaning should be apparent without reading the stages before or after it.

### 4. Named intermediate results aid comprehension

Long pipelines (more than 3–4 stages) become hard to read. Research on working memory limits (Miller's Law, chunking) suggests that 4–7 items is the maximum before errors increase. Naming intermediate results creates re-entry points:

```python
# Hard: six anonymous stages in one expression
result = sorted(set(e.dept for e in filter(lambda e: e.active, employees)))

# Better: named intermediates
active_employees = filter(lambda e: e.active, employees)
departments      = (e.dept for e in active_employees)
unique_depts     = set(departments)
result           = sorted(unique_depts)
```

The named version is longer but easier to verify, test, and modify. Each name is a semantic checkpoint.

**Design rule:** make it easy (low ceremony) to name intermediate results in a pipeline.

### 5. Inspectability and debugging

The best pipeline syntaxes make it easy to insert inspection without breaking the chain:

- Unix: insert `| tee /dev/stderr` or `| cat -A`  
- Elixir: insert `|> IO.inspect(label: "after filter")`  
- Kotlin: insert `.also { println(it) }`  
- Ruby: insert `.tap { |x| p x }`  

Languages without a good inspection primitive force the programmer to break the chain into separate variable assignments to inspect intermediate values — and then reconstruct the chain.

**Design rule:** provide a "tap" or "also" primitive that executes a side-effectful operation on the current value without changing it.

### 6. Composability: pipelines as values

The most powerful pipeline syntaxes allow the pipeline itself to be stored, passed, and composed:

- Haskell: `normalize = unwords . map capitalize . words` is a function value
- Ramda: `R.pipe(R.filter(pred), R.map(f), R.sum)` is a function value  
- jq: a jq filter is itself a composable unit

When a pipeline is just syntax and not a value, you cannot factor out common sub-pipelines or pass them as arguments.

**Design rule:** where possible, a pipeline expression should be usable as a composable function, not just a statement.

### 7. Adoption evidence: pipe operators spread rapidly

The empirical record shows that pipe operators spread faster than almost any other language feature through ecosystems:

- R's `%>%` went from package to universal idiom in ~18 months (2014–2016)
- Elixir's `|>` is now the dominant way to write Elixir; code without it is considered unidiomatic
- The JavaScript community adopted `.filter().map().reduce()` chains so thoroughly that the pattern is now built into ES6+ arrays
- Hack (Facebook's PHP dialect) added `|>` in 2021; PHP 8.0 `|>` proposals have strong community support

This adoption speed is evidence that these operators address a genuine pain point. Programmers adopt them not because they are theoretically elegant but because they make their code more readable to themselves and their colleagues.

---

## What Evident Should Learn from Pipeline Design

Evident's core operation is: "take this set, filter it, project a field, assert something about the result." This is a pipeline over sets, expressed declaratively. Here is what the survey recommends.

### Observation 1: Evident already has the right primitive — set comprehension via claims

A claim body is already a pipeline. Consider:

```evident
evident high_salary_engineer e
    e in employees
    e.department == "engineering"
    e.salary > 100000
```

This is: start with `employees` (the set), filter to those in engineering, filter to those with salary > 100000, establish `high_salary_engineer` for each such `e`. This is `FROM employees WHERE department = 'engineering' AND salary > 100000` expressed as a claim.

The pipeline is implicit in the claim structure. Each body constraint is a stage. The order of stages doesn't matter (the solver finds a valid evaluation order). The result is a set of records for which all constraints hold simultaneously.

**This is the right model.** Evident does not need a pipe operator for this use case. The claim body *is* the pipeline.

### Observation 2: Named intermediate results should be first-class

The survey shows that long anonymous pipelines hurt readability. In Evident, naming an intermediate set is declaring a new claim:

```evident
-- Without names: all constraints in one body (can become long)
evident report_entry e dept
    e in employees
    e.department == dept.name
    dept in departments
    dept.budget > 1000000
    e.salary > dept.avg_salary

-- With named intermediates: each claim is a checkpoint
evident report_entry e dept
    senior_employee e
    high_budget_department dept
    e.department == dept.name

evident senior_employee e
    e in employees
    e.salary > 80000

evident high_budget_department dept
    dept in departments
    dept.budget > 1000000
```

The named-intermediate form is more verbose but each claim is independently readable, testable, and reusable. This is exactly what the literature on pipeline readability recommends.

**Design implication:** discourage very long claim bodies (more than 5–6 constraints) in style guides. Encourage factoring out named sub-claims. The language supports this — every sub-claim can be a first-class named claim.

### Observation 3: Projection and aggregation need explicit syntax

The survey reveals that *projection* (selecting fields) and *aggregation* (count, sum, max) are common pipeline operations that Evident does not yet handle clearly.

Projection in Evident is implicit: if a claim head mentions only some fields of a record type, the missing fields are existentially quantified away. But there is no explicit "project these fields" syntax for constructing the output shape.

Aggregation is harder: `count`, `sum`, `max` over a set require a closed-world assumption. The pipeline `employees -> filter -> count` needs to enumerate all employees matching the filter before reporting a count.

**Design implication:** Evident needs a concrete answer to "how do I express count/sum over a filtered set?" The relational algebra document identifies this as requiring the closed-world assumption. A possible form:

```evident
-- Counting: aggregate over a named set
? count { e | e in employees, e.department == "engineering" }

-- Sum: aggregate over a projection
? sum { e.salary | e in employees, e.department == "engineering" }
```

The `{ expression | condition }` set comprehension notation encodes the full pipeline (filter + project + collect) as a single expression. The aggregate function wraps it. This is mathematically: `Σ { e.salary | e ∈ employees ∧ e.department = "engineering" }`.

### Observation 4: The `|>` idiom does not fit Evident's programming model

Elixir's `|>` works because programs are sequences of transformations applied to a single value. Evident's programming model is different: a claim is not a transformation applied in sequence — it is a set of constraints that must all hold simultaneously. There is no single "current value" being threaded through.

However, a limited pipe syntax *could* be useful for expressing common set operation pipelines in queries:

```evident
-- Hypothetical query pipeline syntax
? employees
    |> where { e | e.department == "engineering" }
    |> select { e.name, e.salary }
    |> sort_by { e.salary }
    |> take 10
```

This would be syntactic sugar for:

```evident
? result where
    result = take 10 (sort_by salary (select [name, salary] (where dept_is_eng employees)))
    dept_is_eng e = e.department == "engineering"
```

The pipeline form is more readable. But this is a query-time convenience, not a core claim construct. It would be a separate query language layered on top of claims.

**Design implication:** Consider whether Evident needs a separate query syntax for ad-hoc set pipelines, separate from the claim declaration syntax. The claim system handles the structural/reusable part; a query pipeline syntax handles the ad-hoc exploration part. These are different use cases.

### Observation 5: Tap / inspection should be built in for development

Every good pipeline syntax has a way to inspect intermediate values without breaking the chain. For Evident's development ergonomics, a built-in `trace` or `watch` primitive in claim bodies would be valuable:

```evident
evident high_salary_engineers result
    e in employees
    trace e              -- print each e being considered
    e.department == "engineering"
    e.salary > 100000
    result = collect e   -- gather all matching e into result
```

The `trace` here is a development tool that does not affect the logic. It is the Kotlin `also` or Elixir `IO.inspect` pattern.

### Observation 6: The set comprehension notation is Evident's native pipeline

The set comprehension `{ x | P(x) }` is the mathematical form of the pipeline "start with some domain, filter by P, collect into a set." It is standard mathematical notation, used in every textbook on set theory and logic.

Evident should adopt set comprehension notation as first-class query syntax:

```evident
-- Set comprehension: the mathematical form
{ e.name | e ∈ employees ∧ e.department = "engineering" }

-- This is equivalent to the pipeline:
--   FROM employees
--   WHERE department = 'engineering'
--   SELECT name

-- ASCII form
{ e.name | e in employees, e.department == "engineering" }
```

This is more aligned with Evident's mathematical foundations than a pipe operator. It is concise, standard, and maps directly to relational algebra (σ then π). Chaining set comprehensions by nesting or naming them gives the full pipeline:

```evident
-- Two-stage pipeline via comprehension composition
eng_salaries = { e.salary | e ∈ employees ∧ e.department = "engineering" }
high_earners = { s | s ∈ eng_salaries ∧ s > 100000 }
? count high_earners
```

Or in one expression:

```evident
? count { e.salary | e ∈ employees ∧ e.department = "engineering" ∧ e.salary > 100000 }
```

**Summary recommendation for Evident pipeline syntax:**

1. **Claim bodies are the primary pipeline** — each constraint is a stage. Use named sub-claims to handle complex pipelines.
2. **Set comprehension `{ x | P(x) }` is the native query pipeline** — for ad-hoc "filter and project" queries. This is standard notation and mathematically grounded.
3. **Aggregate functions wrap comprehensions** — `count { x | P(x) }`, `sum { f(x) | P(x) }` follow from set comprehension notation naturally.
4. **Named intermediate claims are the Evident version of named pipeline stages** — cheaper than a pipe operator and more reusable.
5. **A `trace` primitive for development** — threading inspection without breaking claim logic.
6. **A query pipeline syntax (`|>`) is worth exploring for the query/REPL context** — but it is a query convenience, not a core claim construct. Implement last, after the claim system is stable.
