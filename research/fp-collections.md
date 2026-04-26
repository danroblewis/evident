# Set and Collection Operations in Functional Programming Languages

Research for the Evident constraint programming language design project. Goal: understand what set and collection operations programmers reach for most often in practice.

---

## 1. Haskell `Data.Set`

`Data.Set` is a purely functional, ordered set backed by a balanced binary search tree (AVL/weight-balanced). All elements must be `Ord`.

### Full API (selected)

| Operation | Type signature | Notes |
|---|---|---|
| `member` | `a -> Set a -> Bool` | Most-used query |
| `notMember` | `a -> Set a -> Bool` | Negation of `member` |
| `insert` | `a -> Set a -> Set a` | O(log n) |
| `delete` | `a -> Set a -> Set a` | O(log n) |
| `union` | `Set a -> Set a -> Set a` | O(m log(n/m+1)) |
| `unions` | `[Set a] -> Set a` | Fold over `union` |
| `intersection` | `Set a -> Set a -> Set a` | |
| `difference` | `Set a -> Set a -> Set a` | Set subtraction |
| `isSubsetOf` | `Set a -> Set a -> Bool` | |
| `isProperSubsetOf` | `Set a -> Set a -> Bool` | Strict subset |
| `disjoint` | `Set a -> Set a -> Bool` | Added in containers-0.6.2 |
| `filter` | `(a -> Bool) -> Set a -> Set a` | |
| `partition` | `(a -> Bool) -> Set a -> (Set a, Set a)` | Splits into matching / non-matching |
| `map` | `Ord b => (a -> b) -> Set a -> Set b` | Result re-sorted; not a functor |
| `foldr` | `(a -> b -> b) -> b -> Set a -> b` | Fold in ascending order |
| `foldl'` | `(a -> b -> a) -> a -> Set b -> a` | Strict fold |
| `toList` | `Set a -> [a]` | Ascending order |
| `toAscList` | `Set a -> [a]` | Same as `toList` |
| `toDescList` | `Set a -> [a]` | Descending order |
| `fromList` | `Ord a => [a] -> Set a` | Deduplicates |
| `singleton` | `a -> Set a` | |
| `empty` | `Set a` | |
| `null` | `Set a -> Bool` | |
| `size` | `Set a -> Int` | |
| `findMin` | `Set a -> a` | Partial; throws on empty |
| `findMax` | `Set a -> a` | Partial; throws on empty |
| `deleteMin` | `Set a -> Set a` | |
| `deleteMax` | `Set a -> Set a` | |
| `lookupMin` | `Set a -> Maybe a` | Safe version |
| `lookupMax` | `Set a -> Maybe a` | Safe version |
| `splitMember` | `a -> Set a -> (Set a, Bool, Set a)` | Below, member?, above |
| `split` | `a -> Set a -> (Set a, Set a)` | Without membership test |
| `elems` | `Set a -> [a]` | Alias for `toAscList` |
| `cartesianProduct` | `Set a -> Set b -> Set (a,b)` | Added in containers-0.6.4 |
| `powerSet` | `Set a -> Set (Set a)` | All subsets |

### Most-used in practice

Based on Haskell codebase surveys and library usage patterns:

1. `member` — by far the most common; "is this element in the set?"
2. `fromList` / `toList` — conversion to/from lists is ubiquitous
3. `union` — combining sets
4. `insert` / `delete` — incremental construction
5. `filter` — subsetting
6. `intersection` / `difference` — set-theoretic queries
7. `null` / `size` — cardinality checks
8. `foldr` — traversal/aggregation
9. `partition` — split-by-predicate
10. `isSubsetOf` — containment queries

`splitMember`, `powerSet`, `cartesianProduct` are niche. `findMin`/`findMax` are used in priority-queue-like patterns.

---

## 2. Haskell `Data.Map` (and `Data.Map.Strict`)

Maps as sets of key-value pairs: `Map k v ≅ Set (k, v)` where keys are unique. Backed by the same weight-balanced tree.

### Full API (selected)

| Operation | Notes |
|---|---|
| `lookup` | `k -> Map k v -> Maybe v` — most-used query |
| `member` / `notMember` | Key membership |
| `findWithDefault` | `v -> k -> Map k v -> v` |
| `insert` | Overwrites existing key |
| `insertWith` | `(v -> v -> v) -> k -> v -> Map k v -> Map k v` — merge on collision |
| `insertWithKey` | Like `insertWith` but also passes key to function |
| `insertLookupWithKey` | Returns old value alongside new map |
| `delete` | Remove by key |
| `adjust` | Modify value at key |
| `adjustWithKey` | Modify with key available |
| `update` | `(v -> Maybe v) -> k -> Map k v -> Map k v` — modify or delete |
| `alter` | `(Maybe v -> Maybe v) -> k -> Map k v -> Map k v` — most general single-key op |
| `unionWith` | `(v -> v -> v) -> Map k v -> Map k v -> Map k v` |
| `unionWithKey` | Merge function receives key |
| `unionsWith` | Fold over `unionWith` |
| `intersectionWith` | Keys present in both; values merged |
| `intersectionWithKey` | |
| `differenceWith` | Keys only in left; values optionally merged |
| `mergeWithKey` | Full three-way merge (most general) |
| `mapWithKey` | `(k -> v -> w) -> Map k v -> Map k w` |
| `mapKeys` | Change keys; may collapse duplicates |
| `mapKeysWith` | Merge on key collision |
| `filterWithKey` | `(k -> v -> Bool) -> Map k v -> Map k v` |
| `partitionWithKey` | Two maps from predicate |
| `foldlWithKey'` | Strict left fold over (k, v) pairs |
| `foldrWithKey` | Right fold over (k, v) pairs |
| `traverseWithKey` | Applicative traversal |
| `toAscList` | `[(k,v)]` in ascending key order |
| `toDescList` | `[(k,v)]` in descending key order |
| `fromList` | Last value wins on duplicate keys |
| `fromListWith` | Merge duplicates with function |
| `fromListWithKey` | |
| `fromSet` | `(k -> v) -> Set k -> Map k v` — build from key set |
| `keysSet` | `Map k v -> Set k` |
| `elems` / `keys` | Lists of values / keys |
| `singleton` / `empty` / `null` / `size` | Standard |
| `findMin` / `findMax` | `Map k v -> (k, v)` |
| `deleteMin` / `deleteMax` | |
| `updateMin` / `updateMax` | Modify extremal pair |
| `splitLookup` | Three-way split like `Data.Set.splitMember` |
| `isSubmapOf` | Key and value containment |
| `isSubmapOfBy` | Custom value comparison |

### Key design observations

- `alter` is the most general single-key operation: it unifies `insert`, `delete`, `adjust`, and `update`.
- `mergeWithKey` (and the newer `Merge` module with `merge`/`mergeA`) is the most general binary operation.
- The `Strict` variant (`Data.Map.Strict`) is preferred in practice to avoid space leaks.
- `fromListWith (+)` is the idiomatic histogram/frequency-count pattern.

---

## 3. Scala Collections

Scala's collection library is one of the most comprehensive in any language. It distinguishes `immutable` (default) from `mutable` collections and provides a rich hierarchy.

### `Set` operations

```scala
val s = Set(1, 2, 3)
s + 4              // insert
s - 2              // remove
s ++ Set(4, 5)     // union
s & Set(2, 3, 4)   // intersection (alias: intersect)
s | Set(4, 5)      // union (alias: union)
s &~ Set(2)        // difference (alias: diff)
s.subsetOf(other)
s.contains(x)
s.size / s.isEmpty
s.filter(predicate)
s.partition(predicate)   // (matching, non-matching)
s.map(f)
s.flatMap(f)
s.fold(z)(op)
s.foldLeft(z)(op)
s.foldRight(z)(op)
s.forall(p) / s.exists(p) / s.count(p)
s.find(p)
s.toList / s.toSeq / s.toVector / s.toArray
s.min / s.max
s.minBy(f) / s.maxBy(f)
s.sum / s.product
s.mkString(sep)
```

### `Map` operations

```scala
val m = Map("a" -> 1)
m + ("b" -> 2)                     // insert/update
m - "a"                            // remove
m.get("a")                         // Option[V]
m("a")                             // V or NoSuchElementException
m.getOrElse("a", default)
m.updatedWith("a")(f)              // alter equivalent
m ++ other                         // merge, right wins
m.map { case (k, v) => ... }
m.mapValues(f)                     // lazy; use .view.mapValues(f).toMap for strict
m.filterKeys(p)
m.filter { case (k, v) => ... }
m.groupBy(f)                       // Map[K, Iterable[V]]
m.toList / m.toSeq / m.keys / m.values
m.keySet
m.withDefaultValue(v)
m.foldLeft(z) { case (acc, (k, v)) => ... }
```

### Sequence operations (applicable to `List`, `Vector`, `Seq`)

| Operation | Description |
|---|---|
| `groupBy(f)` | `Map[K, List[A]]` — partition by key function |
| `partition(p)` | `(List[A], List[A])` — by boolean predicate |
| `span(p)` | `(List[A], List[A])` — prefix satisfying p, rest |
| `splitAt(n)` | `(List[A], List[A])` — at index |
| `zip(other)` | `List[(A, B)]` |
| `zipWithIndex` | `List[(A, Int)]` |
| `unzip` | `(List[A], List[B])` from `List[(A,B)]` |
| `collect(pf)` | Partial function filter+map |
| `flatMap(f)` | Map then flatten one level |
| `exists(p)` / `forall(p)` | Existential / universal quantification |
| `count(p)` | Number of elements satisfying predicate |
| `sum` / `product` | Numeric aggregation |
| `min` / `max` | By natural ordering |
| `minBy(f)` / `maxBy(f)` | By key function |
| `sortBy(f)` / `sortWith(cmp)` | Sorting |
| `distinct` / `distinctBy(f)` | Deduplication |
| `sliding(n)` / `grouped(n)` | Windowed / chunked iteration |
| `takeWhile(p)` / `dropWhile(p)` | Prefix/suffix by predicate |
| `scanLeft(z)(op)` / `scanRight` | Running fold |
| `tails` / `inits` | All suffixes / prefixes |
| `combinations(n)` / `permutations` | Combinatorics |
| `corresponds(other)(p)` | Pairwise predicate |
| `diff(other)` | Multiset difference (list version) |
| `intersect(other)` | Multiset intersection |

---

## 4. Python Sets and `itertools`

### `set` built-in

Python's `set` is a hash set (unordered). `frozenset` is the immutable variant.

```python
s = {1, 2, 3}
s | t             # union
s & t             # intersection
s - t             # difference
s ^ t             # symmetric_difference
s <= t            # issubset
s >= t            # issuperset
s < t             # proper subset
s.isdisjoint(t)
s.add(x)
s.discard(x)      # remove if present, no error
s.remove(x)       # remove; KeyError if absent
s.pop()           # arbitrary element
s.update(iterable)
s.intersection_update(t)
s.difference_update(t)
len(s) / x in s
```

Python sets are heavily used for membership testing (`x in s`) and deduplication (`set(list)`). The set operators (`|`, `&`, `-`, `^`) work only on sets; the methods (`.union()`, etc.) accept any iterable.

### `itertools` module

| Function | Description |
|---|---|
| `chain(*iterables)` | Concatenate iterables |
| `chain.from_iterable(it)` | Flatten one level |
| `product(*its, repeat=n)` | Cartesian product |
| `permutations(it, r)` | r-length permutations |
| `combinations(it, r)` | r-length combinations, no repeat |
| `combinations_with_replacement` | With repetition |
| `groupby(it, key)` | Consecutive-group iterator (requires sorted input) |
| `islice(it, stop)` | Lazy slice |
| `compress(it, selectors)` | Filter by boolean mask |
| `filterfalse(p, it)` | Elements where p is false |
| `takewhile(p, it)` | Prefix satisfying p |
| `dropwhile(p, it)` | After prefix satisfying p |
| `starmap(f, it)` | Map with argument unpacking |
| `accumulate(it, func)` | Running reduction (like `scanLeft`) |
| `pairwise(it)` | Consecutive pairs (Python 3.10+) |
| `batched(it, n)` | Chunks of size n (Python 3.12+) |
| `zip_longest(*its)` | Zip with fill value |
| `cycle(it)` | Infinite repetition |
| `repeat(x, n)` | x repeated n times |
| `count(start, step)` | Infinite counter |

### `functools` additions

```python
functools.reduce(f, it, initializer)   # fold
functools.partial(f, *args)
```

---

## 5. Ruby `Enumerable`

`Enumerable` is mixed into any class that implements `each`. It provides ~60 methods, making it one of the richest collection interfaces in any mainstream language.

### Full method list (grouped)

**Transformation**
- `map` / `collect` — transform each element
- `flat_map` / `collect_concat` — map then flatten one level
- `zip(*others)` — pair elements from multiple enumerables
- `each_with_object(obj)` — fold into a mutable accumulator

**Filtering / selection**
- `select` / `filter` / `find_all` — keep matching
- `reject` — keep non-matching
- `find` / `detect` — first match
- `filter_map` — map + filter in one pass (Ruby 2.7+)
- `take(n)` / `drop(n)` — head/tail by count
- `take_while(p)` / `drop_while(p)` — head/tail by predicate
- `first(n)` / `min(n)` / `max(n)` — top-n elements

**Aggregation / reduction**
- `reduce` / `inject` — general fold
- `sum` — numeric (accepts optional initial + block)
- `count` — cardinality (with optional block = count matching)
- `tally` — frequency map: `["a","b","a"].tally # => {"a"=>2,"b"=>1}`
- `min` / `max` — by natural order
- `min_by(f)` / `max_by(f)` — by key function
- `minmax` / `minmax_by` — both extremes in one pass

**Testing**
- `any?(p)` / `all?(p)` / `none?(p)` / `one?(p)` — quantifiers
- `include?` / `member?` — membership
- `empty?` (via `none?` or direct)

**Sorting**
- `sort` / `sort_by(f)` — stable sort (via Schwartzian transform for `sort_by`)

**Grouping / partitioning**
- `group_by(f)` — `Hash[K, Array[V]]`
- `partition(p)` — `[matching, non_matching]`
- `chunk(f)` — consecutive runs by key
- `chunk_while(p)` — consecutive runs while predicate holds on adjacent pair
- `slice_when(p)` — slice at boundary where predicate true
- `slice_before(p)` / `slice_after(p)` — slice relative to matching element
- `each_slice(n)` — non-overlapping chunks of size n
- `each_cons(n)` — sliding window of size n

**Indexing / search**
- `flat_map` — flatten after map
- `zip` — parallel iteration

**Conversion**
- `to_a` / `entries` — materialize to array
- `to_h` — to hash (requires pairs)
- `uniq` / `uniq_by` — deduplication (Ruby 2.4+)

### Ruby `Enumerable` to Set Theory Mapping

| Ruby method | Set-theory equivalent | Notes |
|---|---|---|
| `select` / `filter` | Set comprehension: `{ x ∈ S \| P(x) }` | Subset by predicate |
| `reject` | Complement subset: `{ x ∈ S \| ¬P(x) }` | Inverse of `select` |
| `map` | Image: `f(S) = { f(x) \| x ∈ S }` | May not preserve cardinality without `uniq` |
| `flat_map` | `⋃ { f(x) \| x ∈ S }` | Union of images |
| `include?` | `x ∈ S` | Membership |
| `any?` | `∃ x ∈ S: P(x)` | Existential quantification |
| `all?` | `∀ x ∈ S: P(x)` | Universal quantification |
| `none?` | `¬∃ x ∈ S: P(x)` | No witness |
| `one?` | `\|{ x ∈ S \| P(x) }\| = 1` | Unique witness |
| `count` | `\|S\|` or `\|{ x ∈ S \| P(x) }\|` | Cardinality |
| `sum` | `∑_{x ∈ S} f(x)` | Sum over set |
| `reduce` / `inject` | `⊕_{x ∈ S} x` (arbitrary monoid) | Fold |
| `min` / `max` | `min(S)` / `max(S)` | Extremal elements |
| `min_by(f)` | `argmin_{x ∈ S} f(x)` | Argmin |
| `max_by(f)` | `argmax_{x ∈ S} f(x)` | Argmax |
| `sort_by(f)` | Ordering by key: `≤_f` on S | Total order by key |
| `group_by(f)` | Partition by equivalence: `S / ∼_f` | Quotient set |
| `partition(p)` | `{ x \| P(x) }, { x \| ¬P(x) }` | Binary partition |
| `chunk(f)` | Consecutive run decomposition | Ordered partition (weaker) |
| `uniq` | Set identity: `S` without duplicates | Deduplication |
| `zip(T)` | Cartesian pairing: `S × T` (positional) | Not full product |
| `tally` | Multiplicity function: `S → ℕ` | Multiset → histogram |
| `flat_map { [x, x] }` | Multiset expansion | |
| `each_cons(n)` | Sequence of n-tuples from ordered set | n-grams |
| `each_slice(n)` | Partition into equal-sized chunks | |
| `take_while(p)` | Initial segment: `{ x_{1..k} \| P(x_i) ∀ i ≤ k }` | Prefix |
| `drop_while(p)` | Complement of initial segment | Suffix |
| `find` / `detect` | `min { x ∈ S \| P(x) }` (by order) | First witness |
| `to_h` | Function as set of pairs: `{ (k,v) }` | Map construction |
| `minmax` | `(min S, max S)` | Both bounds |

---

## 6. JavaScript Array and `Set`

### `Array` methods

| Method | Description |
|---|---|
| `filter(p)` | Subset by predicate |
| `map(f)` | Transform elements |
| `reduce(f, init)` | Left fold |
| `reduceRight(f, init)` | Right fold |
| `find(p)` | First match (or `undefined`) |
| `findIndex(p)` | Index of first match |
| `findLast(p)` / `findLastIndex(p)` | From end (ES2023) |
| `every(p)` | Universal quantification |
| `some(p)` | Existential quantification |
| `includes(x)` | Membership (uses `===`) |
| `flat(depth)` | Flatten nested arrays |
| `flatMap(f)` | Map then flatten one level |
| `indexOf(x)` / `lastIndexOf(x)` | Linear search |
| `sort(cmp)` | In-place sort (stable since ES2019) |
| `reverse()` | In-place reverse |
| `slice(start, end)` | Non-mutating subarray |
| `splice(start, n, ...items)` | Mutating insert/remove |
| `concat(...others)` | Concatenation |
| `join(sep)` | To string |
| `fill(v, start, end)` | Fill range |
| `at(i)` | Negative indexing (ES2022) |
| `with(i, v)` | Non-mutating replace at index (ES2023) |
| `toSorted()` / `toReversed()` / `toSpliced()` | Non-mutating variants (ES2023) |
| `keys()` / `values()` / `entries()` | Iterators |
| `from(iterable, mapFn)` | Construct from iterable |
| `of(...items)` | Construct from arguments |
| `Array.isArray(x)` | Type check |

### `Set` (ES6+)

```js
const s = new Set([1, 2, 3]);
s.add(x)
s.delete(x)        // returns boolean
s.has(x)           // membership
s.size
s.clear()
s.forEach(f)
[...s]             // to array
// Set operations (ES2025 proposal / now shipping):
s.union(t)
s.intersection(t)
s.difference(t)
s.symmetricDifference(t)
s.isSubsetOf(t)
s.isSupersetOf(t)
s.isDisjointFrom(t)
```

Note: set operations on `Set` were only standardized in ES2025. Before that, programmers used `new Set([...a].filter(x => b.has(x)))` patterns.

### `Map` (ES6+)

```js
const m = new Map();
m.set(k, v)
m.get(k)           // undefined if missing
m.has(k)
m.delete(k)
m.size
m.keys() / m.values() / m.entries()
m.forEach((v, k) => ...)
```

---

## 7. SQL Window Functions as Set Operations

Window functions operate on a *frame* — an ordered partition of rows — without collapsing the group. They are the SQL equivalent of operations on ordered, partitioned sets.

### Conceptual model

```
OVER (PARTITION BY <keys> ORDER BY <ordering> ROWS/RANGE BETWEEN <frame>)
```

- `PARTITION BY` = partition the relation into subsets (quotient by equivalence)
- `ORDER BY` = impose a total order on each partition
- Frame clause = define the sliding window within the ordered partition

### Window function catalog

| Function | Set-theory interpretation |
|---|---|
| `ROW_NUMBER()` | Assign rank in the total order (injective) |
| `RANK()` | Rank with ties (non-injective; gaps after ties) |
| `DENSE_RANK()` | Rank with ties, no gaps |
| `PERCENT_RANK()` | Fractional rank in [0,1] |
| `CUME_DIST()` | Cumulative distribution function |
| `NTILE(n)` | Quantile assignment |
| `LAG(x, offset)` | Predecessor in ordered sequence: `x_{i-k}` |
| `LEAD(x, offset)` | Successor in ordered sequence: `x_{i+k}` |
| `FIRST_VALUE(x)` | `min_{order}(frame)` projected onto x |
| `LAST_VALUE(x)` | `max_{order}(frame)` projected onto x |
| `NTH_VALUE(x, n)` | n-th element in ordered frame |
| `SUM(x)` over frame | Running sum: `∑_{j in frame} x_j` |
| `AVG(x)` over frame | Running mean |
| `COUNT(*)` over frame | Frame cardinality |
| `MIN(x)` / `MAX(x)` over frame | Running extrema |

### Key insight for Evident

Window functions decompose a collection operation into three orthogonal parts:
1. **Partitioning** (group membership): which subset does this element belong to?
2. **Ordering** (within partition): what is the linear order of the partition?
3. **Frame** (local window): which neighbors are visible to the computation?

This tri-part structure (`partition × order × frame`) is more expressive than standard set operations and allows expressing sliding aggregations, rankings, and lookback/lookahead as first-class operations.

---

## 8. `jq`

`jq` is a functional, filter-based language for JSON. Every expression is a filter `input → output`, and filters compose with `|`.

### Core filter operations

| Syntax / builtin | Description |
|---|---|
| `.field` | Field access (projection) |
| `.[n]` | Array index |
| `.[]` | Iterate all values (explode) |
| `.[start:end]` | Array/string slice |
| `select(cond)` | Filter: pass value through iff condition is truthy |
| `map(f)` | Apply f to each element; equivalent to `[.[] \| f]` |
| `map_values(f)` | Apply f to each value of object or array |
| `to_entries` / `from_entries` | `{k,v}` pair arrays ↔ object |
| `with_entries(f)` | `to_entries \| map(f) \| from_entries` |
| `keys` / `values` / `has(k)` | Object introspection |
| `in(obj)` | Membership test |
| `contains(x)` | Deep containment |
| `inside(x)` | Inverse of `contains` |
| `indices(x)` / `index(x)` / `rindex(x)` | Find positions |
| `group_by(f)` | `Array[Array[V]]` grouped by key — input must be sorted |
| `unique` / `unique_by(f)` | Deduplication |
| `sort` / `sort_by(f)` | Sort array |
| `min_by(f)` / `max_by(f)` | Extremal element by key |
| `flatten` / `flatten(depth)` | Recursive / depth-bounded flatten |
| `add` | Fold `+` over array (sum, concat, merge) |
| `any(gen; cond)` / `any` | Existential |
| `all(gen; cond)` / `all` | Universal |
| `limit(n; gen)` | Take first n outputs from generator |
| `first(gen)` / `last(gen)` | First or last output |
| `nth(n; gen)` | n-th output of generator |
| `range(from; to; step)` | Integer sequence generator |
| `recurse(f)` | Fixed-point recursion / DFS |
| `recurse` | Shorthand for deep traversal |
| `path(expr)` | Path expression |
| `getpath(p)` / `setpath(p;v)` / `delpaths(ps)` | Lens-like operations |
| `env` | Access environment variables |
| `input` / `inputs` | Read additional inputs |
| `debug` | Print to stderr (pass-through) |
| `error(msg)` | Raise error |
| `try-catch` | Error handling |
| `label-break` | Early exit from generators |
| `def f(args): body;` | Local function definition |
| `reduce expr as $x (init; update)` | Explicit fold |
| `foreach expr as $x (init; update; extract)` | Streaming fold |

### Key `jq` design observations

- `.[]` (explode) and `|` (compose) are the two most fundamental operations.
- `select` is the universal filter — equivalent to `filter` in other languages.
- `group_by` requires pre-sorted input (unlike most languages), exposing the sort-group pattern.
- `reduce` and `foreach` make the fold structure explicit; most `jq` programs avoid them in favor of `map`/`select`/`add` pipelines.
- `to_entries`/`from_entries` are the idiomatic way to do `mapWithKey` / `filterWithKey` on objects.

---

## 9. Most Commonly Used Operations in Practice

Based on API usage surveys, GitHub code search patterns, Stack Overflow frequency, and library design rationale across the ecosystems above.

### Top 10 operations (ranked by prevalence)

| Rank | Operation | Cross-language names | Why it dominates |
|---|---|---|---|
| 1 | **Membership test** | `member`, `in`, `has`, `contains`, `include?`, `has()` | The single most common question about a set |
| 2 | **Filter / select** | `filter`, `select`, `where`, `SELECT … WHERE` | Subset by predicate; appears in virtually every data pipeline |
| 3 | **Map / transform** | `map`, `collect`, `fmap`, `SELECT f(x)` | Apply a function to each element |
| 4 | **Convert to/from list** | `toList`, `fromList`, `to_a`, `Array.from`, `[...s]` | Interop with other APIs |
| 5 | **Fold / reduce** | `fold`, `reduce`, `inject`, `accumulate` | General aggregation; specializations (`sum`, `count`, `max`) are more common individually |
| 6 | **Group by** | `groupBy`, `group_by`, `GROUP BY`, `group_by()` | Partition by key; the relational workhorse |
| 7 | **Union / merge** | `union`, `++`, `|`, `merge`, `UPDATE … SET` | Combining two collections |
| 8 | **Sort** | `sort`, `sortBy`, `sort_by`, `ORDER BY` | Ordering is required before many other operations |
| 9 | **Existential / universal test** | `any`, `all`, `exists`, `every`, `some`, `forall` | Boolean queries over a set |
| 10 | **Lookup / find** | `lookup`, `find`, `get`, `detect`, `find()` | Retrieve a specific element |

### Notable runners-up

- `partition` — split into two sets by predicate; used whenever branches are needed
- `flatMap` — map-then-flatten; essential for one-to-many transformations
- `count` — cardinality (with or without predicate)
- `intersection` / `difference` — set-theoretic; less common in non-set types
- `min` / `max` / `minBy` / `maxBy` — extrema queries
- `zip` — pairwise combination; common when relating two parallel sequences
- `unique` / `distinct` — deduplication; often the first step in set construction

### What this means for Evident

The frequency ranking reveals a hierarchy:

1. **Point queries** (membership, lookup) — most frequent; must be O(log n) or better
2. **Element-wise transformation** (filter, map) — near-universal; should be first-class syntax
3. **Aggregation** (fold, group-by, sum, count) — very common; often the end goal
4. **Set-theoretic** (union, intersection, difference) — important but less frequent than transformation
5. **Ordering and ranking** (sort, min, max, rank) — important when sequence matters
6. **Combinatorial** (zip, product, combinations) — niche; used in specific domains

For a constraint programming language, the most relevant primitives are: **membership**, **filter** (constraint propagation is filter on the domain), **group-by** (partition domains by equivalence), **union/intersection/difference** (set arithmetic for domains), and **existential/universal quantification** (which map directly to constraint satisfaction semantics).
