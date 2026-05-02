# Monads and Constraint Systems: A Research Guide

## Introduction

This document explores monadic theory and its surprising relevance to constraint programming. Evident uses Z3 to find satisfying assignments for constraint systems, but the way constraints thread through the computation—carrying state (variable bindings), threading context (query scope), and sequencing effects (solving)—mirrors monad theory in ways that illuminate Evident's architecture.

Rather than imposing monad theory onto Evident, this document examines what monads *are*, which problems they *solve*, and where the analogy to constraint programming *works or breaks down*.

---

## Part 1: What is a Monad?

### Core Idea

A **monad** is a design pattern for sequencing computations where each step carries extra structure or context. That structure might be:

- **Failure** (Maybe monad): a computation might return nothing
- **Multiple results** (List monad): a computation might return many values
- **State change** (State monad): a computation threads state through steps
- **Side effects** (IO monad): a computation performs I/O
- **Read-only context** (Reader monad): a computation reads from shared context
- **Accumulated output** (Writer monad): a computation accumulates logs or results

Instead of threading this structure manually through every function call, a monad abstracts the threading—you compose computations and the monad handles the plumbing.

### The Three Components

Every monad requires three parts:

1. **Type constructor** `M`: A wrapper that adds structure to values
   - `Maybe a` wraps a value with the possibility of "nothing"
   - `State s a` wraps a value with a state of type `s` carried alongside
   - `IO a` wraps an I/O side effect returning `a`

2. **`return` (or `pure`)**: Lifts a plain value into the monad
   ```haskell
   return :: a -> M a
   ```
   - `return x` = "wrap this value with minimal/identity structure"
   - In Maybe: `return x = Just x`
   - In State: `return x = \s -> (x, s)` (carry state unchanged)
   - In IO: `return x = IO { perform no side effects, then return x }`

3. **`bind` (written as `>>=`)**: Chains computations while threading structure
   ```haskell
   (>>=) :: M a -> (a -> M b) -> M b
   ```
   - Read as: "execute the first computation, unwrap its result, feed it to the second computation, re-wrap the result"
   - In Maybe: if the first returns `Nothing`, short-circuit and return `Nothing`; otherwise unwrap `Just x` and apply the function
   - In State: execute the first computation with the input state, take the returned state and value, pass both forward to the second computation
   - In IO: sequence the side effects—execute the first I/O, take its result, execute the second I/O

### Operational Intuition

Think of `>>=` as "then" with automatic context threading:

```
// Without monads (manual threading):
result1, state1 = computation1(state0)
result2, state2 = computation2(result1, state1)  // manually pass state1
result3, state3 = computation3(result2, state2)  // manually pass state2

// With State monad (automatic threading):
result = 
  computation1 >>= \r1 ->
  computation2 r1 >>= \r2 ->
  computation3 r2
// The state threading happens invisibly inside >>=
```

In Haskell's `do`-notation (syntactic sugar for `>>=`):
```haskell
computation = do
  r1 <- computation1      -- unwrap result, threads state automatically
  r2 <- computation2 r1   -- unwrap result, threads state automatically
  r3 <- computation3 r2   -- unwrap result, threads state automatically
  return r3
```

This is **the core appeal**: you write sequential logic without manually threading structure.

---

## The Three Monad Laws

For a type to be a valid monad, it must satisfy three laws. These aren't arbitrary—they ensure that `>>=` behaves like a sensible composition operator.

### Law 1: Left Identity
```haskell
return a >>= f  ≡  f a
```

**Meaning**: Wrapping a value with `return` and immediately unwrapping it should be the same as using the value directly.

**Example (Maybe)**:
```haskell
return 5 >>= (\x -> if x > 0 then Just (x * 2) else Nothing)
  ≡
(\x -> if x > 0 then Just (x * 2) else Nothing) 5
  ≡  Just 10
```

### Law 2: Right Identity
```haskell
m >>= return  ≡  m
```

**Meaning**: Binding to `return` at the end does nothing—it's the identity operation.

**Example (Maybe)**:
```haskell
(Just 5 >>= return)  ≡  Just 5
```

**Intuition**: If you thread context and then immediately "wrap it back unchanged," you haven't changed anything.

### Law 3: Associativity
```haskell
(m >>= f) >>= g  ≡  m >>= (\x -> f x >>= g)
```

**Meaning**: The order of grouping successive binds doesn't matter. Whether you bind `f` first then `g`, or `f` chained into `g`, the result is identical.

**Example (State)**:
```
// Grouping 1: bind f, then bind g
(computation1 >>= f) >>= g

// Grouping 2: bind (f chained into g)
computation1 >>= (\x -> f x >>= g)

// Both produce the same state threading and result
```

**Why this matters**: Without associativity, you'd have to parenthesize every bind sequence carefully. With associativity, binds compose naturally, and `>>=` forms an associative operation—the essence of a category in abstract algebra.

### Kleisli Composition

When thinking about composition itself, the three laws align with the monad composition operator `(>=>)`:

```haskell
(>=>) :: (a -> M b) -> (b -> M c) -> (a -> M c)
f >=> g = \a -> f a >>= g

-- The three laws rewritten:
return >=> h  ≡  h                    -- left identity
f >=> return  ≡  f                    -- right identity
(f >=> g) >=> h  ≡  f >=> (g >=> h)  -- associativity
```

This shows that **monad composition is just function composition** for Kleisli arrows, and it forms a valid category.

---

## Part 2: Key Monads and Their Effects

### Maybe Monad (Option in Rust)

**Structure**: `Maybe a = Nothing | Just a`

**Problem solved**: Represent computations that might fail without throwing exceptions

**Examples**:
```haskell
-- Find the first positive number in a list
findPositive :: [Int] -> Maybe Int
findPositive []     = Nothing
findPositive (x:xs) = if x > 0 then Just x else findPositive xs

-- Chain computations that might fail:
safe_divide :: Float -> Float -> Maybe Float
safe_divide x 0 = Nothing
safe_divide x y = Just (x / y)

computation = do
  r1 <- findPositive [1, 2, 3]      -- r1 = 1, or computation short-circuits
  r2 <- safe_divide 10.0 (fromIntegral r1)  -- r2 = 10.0, or fails
  return r2

-- If any step returns Nothing, the whole computation returns Nothing
```

**Rust equivalent**:
```rust
let result: Option<i32> = Some(5)
  .and_then(|x| if x > 0 { Some(x * 2) } else { None })
  .and_then(|x| Some(x + 1));
// result = Some(11), or None if any step failed
```

### Either Monad (Result in Rust)

**Structure**: `Either e a = Left e | Right a`

**Problem solved**: Like Maybe, but carry error information

**Examples**:
```haskell
data Error = DivideByZero | OutOfRange deriving Show

safe_divide :: Float -> Float -> Either Error Float
safe_divide _ 0 = Left DivideByZero
safe_divide x y = Right (x / y)

validate_positive :: Int -> Either Error Int
validate_positive x
  | x > 0 = Right x
  | otherwise = Left OutOfRange

computation = do
  x <- validate_positive 5        -- Right 5
  y <- validate_positive (-2)     -- Left OutOfRange, short-circuit
  return (x + y)                  -- never executes

-- computation = Left OutOfRange
```

**Rust equivalent**:
```rust
fn safe_divide(x: f64, y: f64) -> Result<f64, String> {
  if y == 0.0 {
    Err("DivideByZero".to_string())
  } else {
    Ok(x / y)
  }
}

let result: Result<f64, String> = safe_divide(10.0, 2.0)
  .and_then(|r| safe_divide(r, 0.0))  // Err("DivideByZero")
  .and_then(|r| safe_divide(r, 2.0)); // never executes
```

### State Monad

**Structure**: `State s a = \s -> (a, s')`

**Problem solved**: Thread mutable state through pure computations without side effects

**Operational**: A State computation is a function from an input state to a result-state pair.

**Examples**:
```haskell
-- Stateful counter
increment :: State Int Int
increment = State (\s -> (s, s + 1))

get_count :: State Int Int
get_count = State (\s -> (s, s))

computation = do
  c1 <- get_count         -- c1 = 0, state still 0
  _  <- increment         -- execute increment, state becomes 1
  c2 <- get_count         -- c2 = 1, state is 1
  _  <- increment         -- state becomes 2
  return (c1, c2)         -- returns (0, 1)
-- Final state: 2
```

**Key insight**: State is a monad over `s -> (a, s)`. The bind operator sequences these functions, automatically threading the state:

```haskell
(State f) >>= g = State (\s0 ->
  let (a, s1) = f s0                -- execute f, get result a and new state s1
      (State h) = g a               -- apply g to a, get new State computation
      (b, s2) = h s1                -- execute h with new state, get b and s2
  in (b, s2)
)
```

### Reader Monad

**Structure**: `Reader r a = \r -> a`

**Problem solved**: Thread read-only context/configuration through computations

**Examples**:
```haskell
-- Thread configuration without passing it explicitly
data Config = Config { apiUrl :: String, maxRetries :: Int }

get_api_url :: Reader Config String
get_api_url = Reader (\cfg -> apiUrl cfg)

get_max_retries :: Reader Config Int
get_max_retries = Reader (\cfg -> maxRetries cfg)

fetch_data :: Reader Config String
fetch_data = do
  url <- get_api_url       -- access config without explicit parameter
  retries <- get_max_retries
  return $ "Fetching from " ++ url ++ " (max " ++ show retries ++ " retries)"

-- Run with specific config:
result = runReader fetch_data (Config "https://api.example.com" 3)
-- result = "Fetching from https://api.example.com (max 3 retries)"
```

**Key insight**: Reader lifts configuration access into a monad, avoiding the "context parameter threading" problem. Every computation in the Reader monad automatically has access to the shared config.

### Writer Monad

**Structure**: `Writer w a = (a, w)`

**Problem solved**: Accumulate output (logs, events, traces) alongside computation results

**Examples**:
```haskell
import Data.Monoid (Sum, getSum)

-- Log computations
computation :: Writer (Sum Int) Int
computation = do
  tell (Sum 5)    -- accumulate 5
  result <- return 10
  tell (Sum 3)    -- accumulate 3
  return result

runWriter computation
-- (10, Sum 8)  -- result and accumulated log

-- More complex example: function tracing
factorial_traced :: Int -> Writer String Int
factorial_traced 0 = do
  tell "Base case: factorial(0) = 1\n"
  return 1
factorial_traced n = do
  tell $ "Computing factorial(" ++ show n ++ ")\n"
  prev <- factorial_traced (n - 1)
  tell $ "Returning factorial(" ++ show n ++ ") = " ++ show (n * prev) ++ "\n"
  return (n * prev)
```

**Key insight**: Writer separates the main computation (type `a`) from accumulated side information (type `w`). The monad automatically combines the side information when chaining operations (using monoid operations on `w`).

### List Monad (Nondeterminism)

**Structure**: `[a]` where a list is a monad

**Problem solved**: Represent nondeterministic computations that return multiple results

**Examples**:
```haskell
-- Generate all combinations
pairs :: [Int]
pairs = do
  x <- [1, 2, 3]      -- choose x from [1, 2, 3]
  y <- [4, 5]         -- choose y from [4, 5]
  return (x, y)       -- return each (x, y) pair

-- pairs = [(1,4), (1,5), (2,4), (2,5), (3,4), (3,5)]

-- Filter nondeterministically
filtered :: [(Int, Int)]
filtered = do
  x <- [1, 2, 3, 4, 5]
  y <- [1, 2, 3, 4, 5]
  if x < y && (x + y) `mod` 2 == 0   -- guard (automatic filter)
    then return (x, y)
    else []  -- empty list = "failure" = don't include this pair

-- filtered = [(1,3), (1,5), (2,4), (3,5)]
```

**Key insight**: The monad bind for lists is `flatMap` / `concatMap`. When you bind over `[a]`, you get multiple values, and each feeds into the next computation. The monad automatically collects all results.

### IO Monad

**Structure**: `IO a = "an action that performs I/O and returns a"`

**Problem solved**: Sequence side effects (file I/O, network requests, user input) in pure functional code without losing referential transparency.

**The Problem**: In a pure language like Haskell, you can't just call `readFile` and get a string—that's a side effect. Instead, `readFile :: FilePath -> IO String` returns an **IO action** (a description of what to do), not the actual file contents.

**Examples**:
```haskell
-- Each of these is an IO action (not executed until run):
action1 :: IO String
action1 = readFile "data.txt"

action2 :: IO ()
action2 = putStrLn "Hello, world!"

-- Chain them with do-notation (which becomes >>= under the hood):
main :: IO ()
main = do
  contents <- readFile "input.txt"    -- read file (action executed here)
  let processed = process contents    -- pure computation
  writeFile "output.txt" processed    -- write file (action executed here)
  putStrLn "Done!"                    -- print message (action executed here)

-- Haskell's runtime executes the IO actions in the order specified by >>=
```

**Key insight**: IO monad separates the **description** of effects (IO actions as values) from their **execution** (performed by the Haskell runtime). This preserves purity: `main` itself is a pure value describing what I/O to perform; the effects only happen when the runtime executes it.

---

## Part 3: Monad Transformers – Stacking Effects

### The Problem

What if you need multiple effects at once? For example:

- You want **error handling** (Maybe/Either) AND **state threading** (State)
- You want **logging** (Writer) AND **configuration access** (Reader)
- You want **asynchronous I/O** (IO) AND **error handling** (Either)

You can't just use one monad—you need both effects in the same computation.

### The Solution: Transformer Stacks

A **monad transformer** is a variant of a monad that can **wrap another monad**, adding new effects to its outer layer.

**Example transformer signatures**:
```haskell
-- StateT adds State to any monad m
StateT s m a = \s -> m (a, s)

-- ReaderT adds Reader to any monad m
ReaderT r m a = \r -> m a

-- WriterT adds Writer to any monad m
WriterT w m a = m (a, w)

-- ExceptT adds Either/Error to any monad m
ExceptT e m a = m (Either e a)
```

**Key insight**: Each transformer T takes the base monad `m` as a parameter and **produces a new monad** `T s m`, which is a monad with both T's effects *and* m's effects.

### Stacking Transformers

You stack them by nesting:

```haskell
type AppM = ReaderT Config (StateT AppState (ExceptT Error IO))

-- This monad stack has, from innermost to outermost:
-- 1. IO (actual side effects)
-- 2. ExceptT Error (error handling)
-- 3. StateT AppState (mutable state)
-- 4. ReaderT Config (read-only config)

-- So a computation in AppM can:
-- - Perform I/O (innermost)
-- - Fail with an Error (layer 2)
-- - Thread mutable state (layer 3)
-- - Access immutable config (outermost)
```

### How Transformers Compose

When you use `>>=` in a transformer stack, **all layers compose automatically**:

```haskell
computation :: ReaderT Config (StateT Int IO) String
computation = do
  cfg <- ask              -- ReaderT: access config
  count <- get            -- StateT: access state
  lift (putStrLn "...")   -- IO: print (lift brings it up through the stack)
  put (count + 1)         -- StateT: modify state (threading automatic)
  return "result"
```

The magic is that `>>=` in `ReaderT Config (StateT Int IO)` automatically:
1. Threads the config through (Reader effect)
2. Threads the state through (State effect)
3. Sequences the IO actions (IO effect)

### Common Transformer Stack: RWS (Reader-Writer-State)

A frequent pattern is combining Reader, Writer, and State. Haskell provides a single `RWS` monad that's isomorphic to `ReaderT r (WriterT w (State s))`:

```haskell
type RWS r w s a = r -> s -> (a, s, w)

computation :: RWS Config String AppState Int
computation = do
  cfg <- ask                    -- read config
  st <- get                     -- read state
  tell "Starting computation"   -- accumulate log
  put (st { count = count st + 1 })  -- update state
  tell "Finished"
  return 42
```

This single stack combines:
- **R**eader: immutable config
- **W**riter: accumulated output (logs)
- **S**tate: mutable application state

---

## Part 4: Brief Category Theory Connection

### The Mathematical View

Monads are a formalization from abstract algebra. Without diving deep, the key insight is:

**A monad is a monoid in the category of endofunctors.**

This sentence, attributed to James Ery, is notoriously cryptic. Here's what it means operationally:

1. **Endofunctor**: A function from types to types that preserves structure (`Maybe`, `State s`, etc.)
2. **Monoid**: A set with an associative binary operation and identity element
3. **Category of endofunctors**: We're treating functors themselves as objects and function composition as the operation

**The Monad Laws map to Monoid laws**:
- **Left identity** (`return >=> f = f`) = left identity of the monoid
- **Right identity** (`f >=> return = f`) = right identity of the monoid
- **Associativity** (`(f >=> g) >=> h = f >=> (g >=> h)`) = associativity of the monoid operation

### Why Category Theory Matters Here

The mathematical structure ensures that monads compose and reason about predictably. If you prove a monad satisfies the three laws, you *automatically* know that code using it will behave as expected—the laws guarantee nice properties.

For constraint systems, this suggests that **if we formalize constraint composition as a monad-like structure, we gain algebraic guarantees about constraint solving**.

---

## Part 5: Monads in Other Languages

### Rust: Option and Result

Rust implements monadic patterns without the full abstraction:

```rust
// Option<T> is the Maybe monad
let result: Option<i32> = Some(5)
  .and_then(|x| if x > 0 { Some(x * 2) } else { None })
  .map(|x| x + 1);
// result = Some(11) or None if any step failed

// Result<T, E> is the Either monad
fn safe_divide(x: f64, y: f64) -> Result<f64, String> {
  if y == 0.0 { Err("division by zero".into()) } else { Ok(x / y) }
}

let r: Result<f64, String> = safe_divide(10.0, 2.0)
  .and_then(|x| safe_divide(x, 0.0))  // Err(...), short-circuits
  .and_then(|x| safe_divide(x, 2.0)); // never executes
```

**Key methods**:
- `.map()` = functor map (transform inner value, keep structure)
- `.and_then()` = monadic bind (chain computations that return Option/Result)
- `.or_else()` = handle errors

### Kotlin: Coroutines and Async/Await

Kotlin coroutines implement an effect system similar to IO and State monads:

```kotlin
// async returns a Deferred<T>, which is awaitable
val task1: Deferred<Int> = async { fetchData1() }
val task2: Deferred<Int> = async { fetchData2() }

// await unwraps the Deferred (similar to monadic bind)
val result: Int = coroutineScope {
  val r1 = task1.await()  // wait for task1
  val r2 = task2.await()  // wait for task2
  r1 + r2
}

// Arrow library adds explicit monadic effects (like Haskell)
// Type: suspend fun foo(): Either<Error, Int>
```

### JavaScript: Promises and Async/Await

Promises are the asynchronous monad:

```javascript
// Promise<T> wraps an asynchronous value
const p1 = fetch('/api/data1').then(r => r.json());  // returns Promise
const p2 = fetch('/api/data2').then(r => r.json());  // returns Promise

// .then() is the bind operator
const result = p1
  .then(data1 => {
    // unwrap p1's value
    return p2.then(data2 => ({
      // unwrap p2's value
      combined: data1.value + data2.value
    }));
  })
  .catch(error => console.error(error));  // error handling (Either-like)

// async/await is do-notation for the Promise monad
async function fetchAll() {
  try {
    const data1 = await fetch('/api/data1').then(r => r.json());  // unwrap
    const data2 = await fetch('/api/data2').then(r => r.json());  // unwrap
    return data1.value + data2.value;  // wrap result back in Promise
  } catch (error) {
    console.error(error);
  }
}
```

**Key insight**: `async/await` is syntactic sugar for Promise chaining. `await` is like the `<-` in Haskell `do`-notation—it unwraps the monad, and the function itself is wrapped in a Promise.

### Python: Async/Await and Generators

Python's async/await is similar to JavaScript and Kotlin:

```python
import asyncio

async def fetch_data(url: str) -> dict:
    # This returns a coroutine (an awaitable)
    response = await fetch(url)  # unwrap the Future/Awaitable
    return await response.json()  # unwrap again

async def main():
    # async functions return coroutines
    # await unwraps them (monadic bind)
    data1 = await fetch_data('/api/data1')
    data2 = await fetch_data('/api/data2')
    return data1['value'] + data2['value']

# Run the async monad chain:
result = asyncio.run(main())
```

**Python's take**: Coroutines (created with `async def`) are awaitable monads. `await` is bind, and `asyncio.run()` is the runtime that executes the monadic chain.

---

## Part 6: Applicative Functors and Arrows

### Applicative Functors: Weaker Than Monads

The hierarchy: **Functor < Applicative < Monad**

```haskell
-- Functor: transform inner value
class Functor f where
  fmap :: (a -> b) -> f a -> f b

-- Applicative: apply wrapped function to wrapped value
class Applicative f where
  pure :: a -> f a
  (<*>) :: f (a -> b) -> f a -> f b

-- Monad: bind computations
class Monad f where
  return :: a -> f a
  (>>=) :: f a -> (a -> f b) -> f b
```

**Example**: Applicative doesn't allow the **output of one computation to depend on the result of a previous computation**.

```haskell
-- Applicative: dependencies are fixed statically
let x = [1, 2, 3]
let y = [4, 5]
let result = (\a b -> a + b) <$> x <*> y
-- result = [5, 6, 6, 7, 7, 8]  -- all combinations

-- Monad: next computation depends on previous result
let result = do
  a <- [1, 2, 3]
  b <- if a == 1 then [10, 20] else [100, 200]  -- b depends on a!
  return (a + b)
-- result = [11, 21, 102, 202, 103, 203]  -- different structure
```

**Why this matters**: Applicatives are simpler (weaker) and sometimes more efficient. Many computations don't need full monad power, so using Applicative where possible is better.

### Arrows: Explicit Input/Output Flow

Arrows formalize computation as **functions from input to output** with effects:

```haskell
-- Arrow a b c means: "a computation with input b, output c"
-- arr wraps a pure function
-- (>>>) composes arrows

process :: Arrow a => a Input Output
process = getInput >>> validate >>> transform >>> formatOutput
```

Arrows are useful for **dataflow** and **signal processing** but are more complex than monads.

---

## Part 7: How Monads Solve the Composition Problem

### The Core Problem: Plumbing Code

Without abstractions, threading effects requires manual "plumbing":

```python
# Manual state threading (no monad):
def computation(state0):
    result1, state1 = step1(state0)           # explicit threading
    result2, state2 = step2(result1, state1)  # explicit threading
    result3, state3 = step3(result2, state2)  # explicit threading
    return result3, state3

# The business logic (step1, step2, step3) is entangled with plumbing
```

With a monad, the plumbing is **hidden**:

```haskell
-- Monadic state threading (State monad):
computation = do
  result1 <- step1              -- unwrap, state threads automatically
  result2 <- step2 result1      -- unwrap, state threads automatically
  result3 <- step3 result2      -- unwrap, state threads automatically
  return result3
-- The bind operator handles all state threading
```

### Composition Without the Plumbing

Monads enable **clean composition** of effects:

**Problem**: I want to compose functions that have effects:
- Function A: `Int -> Maybe String` (might fail)
- Function B: `String -> Maybe Int` (might fail)
- Compose them: what should happen if A returns Nothing?

```haskell
-- Without monads (manual handling):
let compose_manual f g x =
  match f x with
  | Nothing -> Nothing
  | Just y -> g y

-- With monads (automatic):
let compose_monadic f g x = (return x) >>= f >>= g
-- Or using Kleisli composition (>=>):
let composed = f >=> g

-- Use it:
result = composed 5
-- If f 5 = Nothing, result = Nothing (short-circuit)
-- If f 5 = Just y, result = g y
```

The monad handles the "what if this fails?" logic automatically.

### Stacking Solutions

Monad transformers let you **compose multiple effects**:

```haskell
-- Without transformers (manual threading of multiple effects):
computation :: State s (Either e a)
computation = State (\s0 ->
  case step1 s0 of
    (Left err, s1) -> (Left err, s1)           -- error + state
    (Right r1, s1) ->
      case step2 r1 s1 of
        (Left err, s2) -> (Left err, s2)       -- error + state
        (Right r2, s2) ->
          case step3 r2 s2 of
            (Left err, s3) -> (Left err, s3)   -- manual error handling
            (Right r3, s3) -> (Right r3, s3)   -- state + result
)

-- With ExceptT transformer (automatic threading of both):
computation :: ExceptT e (State s) a
computation = do
  r1 <- lift step1        -- step1 :: State s a (or ExceptT e (State s) a)
  r2 <- lift step2 r1     -- step2 result depends on r1, automatic error + state
  r3 <- lift step3 r2     -- step3 result depends on r2, automatic error + state
  return r3               -- wraps in ExceptT (Right r3)

-- ExceptT automatically threads both state and error handling
```

**The insight**: Monad transformers compose effects **algebraically**. You don't duplicate error-handling logic; the transformer handles it once and for all.

---

## Part 8: Relating Monads to Constraint Programming

### The Analogy

Evident's constraint system has monad-like structure:

| Monad Aspect | Constraint System Analogy |
|---|---|
| **Type constructor** `M a` | Schema and its variable bindings |
| **`return` (lift value)** | Wrap a constant in a schema context |
| **`bind` (thread and compose)** | Chain constraints, threading variable bindings and state (Z3 context) |
| **Carried state** | Variable assignments, constraint store, Z3 solver state |
| **Context** | Query scope, field accessors (task.duration), type environment |
| **"failure"** | Unsatisfiable constraint (Nothing in Maybe) |
| **"multiple results"** | Multiple satisfying assignments (List monad) |

### What Evident Carries

When you execute a query in Evident, several things thread through:

1. **Variable environment** (Reader-like): Type names, schema names, field declarations
2. **Constraint accumulation** (Writer-like): The growing set of assertions (∃x ∈ Nat, x > 5)
3. **Solver state** (State-like): Z3 context, push/pop stack, model
4. **Potential failure** (Maybe-like): The constraint might be unsatisfiable

### "Constraint Monad" Structure

A monadic view of Evident's `evaluate` function:

```python
# Current implementation (threads manually):
def evaluate(expr, env, sorts, z3_context):
    """
    Recursively evaluate a constraint expression.
    Threads env (variable bindings) and z3_context through.
    """
    # ... manual threading ...

# Monadic view (conceptual):
# evaluate :: Expr -> ConstraintM Value
# where ConstraintM threads:
# - Variable environment (Reader)
# - Z3 context and constraints (State)
# - Satisfiability (Maybe/Either for failure)

# In do-notation (pseudo-code):
# evaluate_expr = do
#   env <- ask_environment           # Reader: variable bindings
#   ctx <- get_z3_context            # State: Z3 state
#   case expr of
#     Var x -> return env[x]
#     BinaryExpr op left right -> do
#       l_val <- evaluate left       # chain: threads env and ctx
#       r_val <- evaluate right      # chain: threads env and ctx
#       result <- apply_op op l_val r_val (in ctx)  # State: create Z3 expr
#       return result
```

### Where the Analogy Works

1. **Constraint composition**: When you chain constraints (A, B ⇒ C), you're composing effects. Monad theory predicts that proper associativity and identity should hold.

2. **Variable scoping**: When you compose schemas, variables need to be scoped and prefixed. This is like Reader monad environment extension.

3. **Error handling**: Unsatisfiability is like the Maybe monad's "Nothing"—a constraint might fail to be satisfiable.

4. **State threading**: Z3's context (push/pop, assertions, solver state) threads through like a State monad.

### Where the Analogy Breaks Down

1. **Non-sequential composition**: Constraints don't require a strict execution order like monadic bind does. Constraint "A and B" is commutative and associative in ways that bind is not. Monad bind is inherently ordered (`m >>= f` always does `m` first); constraints are more declarative.

2. **Non-determinism isn't captured**: A satisfying assignment is either found or not. The List monad's multiple results don't have a direct analogue in Evident's constraint solving (Evident returns *one* satisfying assignment, not all).

3. **Satisfiability is global**: The Maybe monad short-circuits on failure locally (one failed computation). Unsatisfiability in constraints is a global property—it's not about failing early, but about the whole system being inconsistent.

4. **Implicit commutativity**: Constraints like `x ∈ Nat, x > 5, x < 10` can be reordered without changing meaning. Monadic computations cannot (order matters for side effects).

5. **Z3's push/pop model**: Z3 uses a stack of assertion contexts. While this looks like State monad state, it's not—push/pop allows **backtracking to a previous state**, which isn't standard monadic threading (where state is monotonic).

### A More Precise View: Declarative vs. Imperative

**Monads are fundamentally imperative**: They sequence effects in a strict order.

**Constraint systems are fundamentally declarative**: They describe relationships without specifying execution order.

A closer analogy might be **applicative functors** (which don't require ordering) rather than monads. Or perhaps **constraint domains** themselves (from domain theory or constraint logic programming) are the right abstraction.

---

## Part 9: Design Implications for Evident

### Question 1: Does Evident's Evaluation Algorithm Implement a Monad?

Not strictly, because:
- Constraints are declarative, not imperative
- The order of constraint composition doesn't match monadic bind semantics
- Multiple solutions aren't explored (unlike List monad)

However, **the evaluation pipeline** (`normalizer → parser → transformer → translate → evaluate`) does implement a monad-like structure in parts:
- **`translate.py`** threads an environment (variable bindings) + a constraint accumulator (Writer-like)
- **`evaluate.py`** threads the Z3 solver state (State-like)

### Question 2: What Would an Explicit "Constraint Monad" Look Like?

If we wanted to formalize Evident using monad theory:

```python
# Pseudo-Haskell
type ConstraintM = ReaderT Env (WriterT Constraints (StateT Z3Context Maybe))

-- Carries:
-- - Env: variable environment (Reader)
-- - Constraints: accumulated assertions (Writer)
-- - Z3Context: solver state (State)
-- - Maybe: satisfiability (short-circuit on unsat)

evaluate :: Expr -> ConstraintM Value

-- Example:
evaluate (BinaryExpr And left right) = do
  l <- evaluate left                    -- threads Env, Constraints, Z3Context
  r <- evaluate right                   -- threads Env, Constraints, Z3Context
  constraint <- apply_and_to_z3 l r    -- create Z3 constraint
  tell [constraint]                     -- accumulate constraint (Writer)
  return constraint

-- The bind operator handles:
-- 1. Environment inheritance (Reader)
-- 2. Constraint accumulation (Writer)
-- 3. Z3 state threading (State)
-- 4. Short-circuit on unsatisfiability (Maybe)
```

**Benefits**:
- Composition becomes mathematically rigorous
- The three monad laws guarantee predictable rewriting
- Constraint combining follows algebraic laws

**Costs**:
- Adds abstraction overhead
- Doesn't reduce the actual solving complexity
- Constraint logic is fundamentally declarative; monads are imperative

### Question 3: Is There a Natural Monad for Constraint Solving?

The **closest fit** might be:

- **Constraint logic programming monads**: Used in languages like Mercury or Constraint Handling Rules (CHR)
- **Delayed constraints**: Monads that defer constraint resolution until variables are sufficiently bound
- **Proof/evidence monads**: Where the monad carries a derivation tree (which Evident does via `evidence.py`)

A **constraint monad** might look like:

```python
# Constraint monad: carries a proof/evidence tree
type ConstraintM a = { value: a, constraints: [Constraint], evidence: DerivationTree }

# Bind threads the evidence:
m >>= f:
  let (val, constrs, evid) = m
  let (val2, constrs2, evid2) = f val
  return { value: val2, constraints: constrs ++ constrs2, evidence: Branch(evid, evid2) }
```

This captures how Evident chains constraints while building evidence of satisfiability.

---

## Part 10: Conclusions and Recommendations

### What We Learned

1. **Monads solve the composition problem** by abstracting plumbing (threading state, context, effects, failure).

2. **Monad laws provide guarantees**: Left identity, right identity, and associativity ensure that composition behaves predictably.

3. **Monad transformers stack effects** without duplicating logic.

4. **Constraint systems are monad-adjacent**: They thread variable bindings (Reader), accumulate constraints (Writer), manage solver state (State), and handle satisfiability (Maybe).

5. **The constraint-monad analogy partially breaks down** because constraints are declarative, not imperative. Order matters in monadic bind but not (usually) in constraints.

### For Evident's Architecture

**Current approach** (pipeline of transformations):
- Works well: normalizer → parser → AST → translate → evaluate
- Clear separation of concerns
- Doesn't try to force monad abstraction

**Potential refactoring** (if we wanted to formalize with monad theory):
- Rewrite `translate.py` + `evaluate.py` as transformer stack (Reader + Writer + State + Maybe)
- Would make composition laws explicit
- Might help catch bugs related to environment/constraint threading
- Cost: more abstraction boilerplate, harder to reason about performance

**Recommended approach**:
- Keep the current pipeline—it works and is clear
- Document the monad-like threading in `env.py` and `translate.py` (Reader + Writer pattern)
- Use monad terminology in comments when relevant (e.g., "this function threads the variable environment like a Reader monad")
- If constraint composition becomes a bottleneck, consider formalizing it with monad transformers

### Where Monads Illuminate Evident

The monad lens is useful for:
- **Understanding variable scoping**: The `Env` threading is exactly Reader monad semantics
- **Understanding constraint accumulation**: The constraint set is exactly Writer monad semantics
- **Reasoning about solver state**: Z3 state management is exactly State monad semantics
- **Handling unsatisfiability**: The short-circuit on failure is exactly Maybe monad semantics

Understanding this helps us reason about:
- Why variable shadowing is safe (Reader monad handles it)
- Why constraint order doesn't matter (mostly—they're declarative)
- Why sub-schema field access works (Reader environment extension)
- Why Z3 isolation is necessary (State monad requires single-threaded Z3 access)

### Future Exploration

1. **Evidence as a monad**: Evident's `evidence.py` builds derivation trees. These could be formalized as a monad that carries proof terms alongside computations.

2. **Delayed constraints**: If Evident grows to support constraints that are resolved later (incremental solving), a monad would naturally express this.

3. **Constraint composition algebra**: Formalizing when constraints can be reordered (commutativity) and when they can't (dependencies). Monad laws might not apply, but **arrow laws** (which are more flexible) might.

4. **Multi-solution exploration**: If Evident grows to return all satisfying assignments (not just one), the List monad becomes directly relevant.

---

## References

### Monad Theory & Functional Programming

- [Monad laws (ploeh blog)](https://blog.ploeh.dk/2022/04/11/monad-laws/)
- [Monad laws (HaskellWiki)](https://wiki.haskell.org/Monad_laws)
- [Wikipedia: Monad (functional programming)](https://en.wikipedia.org/wiki/Monad_(functional_programming))
- [Real World Haskell: Chapter 18 - Monad transformers](https://book.realworldhaskell.org/read/monad-transformers.html)

### Monads in Practical Languages

- [Option Monads in Rust](https://hoverbear.org/blog/option-monads-in-rust/)
- [JavaScript Promises as Monads](https://swizec.com/blog/javascript-promises-are-just-like-monads-and-i-can-explain-both-in-less-than-2-minutes/)
- [Async/Await as Promise Monad](https://gist.github.com/VictorTaelin/bc0c02b6d1fbc7e3dbae838fb1376c80)

### Applicative Functors & Arrows

- [Haskell/Applicative functors (Wikibooks)](https://en.wikibooks.org/wiki/Haskell/Applicative_functors)
- [Haskell/Understanding arrows (Wikibooks)](https://en.wikibooks.org/wiki/Haskell/Understanding_arrows)
- [Applicative Programming with Effects (McBride & Paterson)](https://www.staff.city.ac.uk/~ross/papers/Applicative.pdf)

### Constraint & SMT Solving

- [Z3 Guide (Microsoft)](https://microsoft.github.io/z3guide/)
- [Programming Z3 (Stanford)](https://theory.stanford.edu/~nikolaj/programmingz3.html)

---

## Appendix: Code Examples in Evident

### Current Pattern: Reader + Writer Threading in `translate.py`

```python
def translate_expr(expr: Expr, env: Env, sorts: SortRegistry) -> z3.ExprRef:
    """
    Threads 'env' (variable bindings) through recursion.
    Similar to Reader monad: read from env without modifying it.
    """
    if isinstance(expr, Identifier):
        return env.lookup(expr.name)  # read from Reader context
    
    elif isinstance(expr, BinaryExpr):
        left = translate_expr(expr.left, env, sorts)   # thread env
        right = translate_expr(expr.right, env, sorts)  # thread env
        return translate_binop(expr.op, left, right)
    
    elif isinstance(expr, QuantifiedExpr):
        # Reader monad: extend environment with new bindings
        new_env = env.extend(expr.var.name, z3_var)
        inner = translate_expr(expr.body, new_env, sorts)  # thread extended env
        return translate_quantifier(expr.quantifier, z3_var, inner)
```

This is **Reader monad** in disguise: `env` is threaded through, functions have access to it without explicitly passing it back.

### Current Pattern: State Threading in `evaluate.py`

```python
def evaluate(query: Query, runtime: EvidentRuntime) -> Evidence:
    """
    Threads 'solver' (Z3 state) through the evaluation.
    Similar to State monad: pass state in, get modified state + result out.
    """
    solver = runtime.solver
    sorts = runtime.sorts
    
    # Translate constraints (builds Z3 expressions)
    for constraint in query.schema.constraints:
        z3_expr = translate_expr(constraint, env, sorts)
        solver.add(z3_expr)  # modify State (Z3 solver)
    
    # Check satisfiability
    result = solver.check()  # read State
    
    if result == sat:
        model = solver.model()
        # Extract solutions (State -> Value transformation)
        return Evidence(...)
    else:
        return Evidence(unsatisfiable=True)
```

This is **State monad** in disguise: `solver` is the state, modified by each operation.

---

**Document completed: 2026-04-30**

