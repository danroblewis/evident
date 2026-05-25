# A `__pycache__`-style disk cache for compiled functions

> Status: design. No code yet. This doc resolves the one question that
> makes a build worth starting: **what, exactly, is cacheable?** — because
> the obvious answers ("the compiled function", "the `Z3Program`") don't
> serialize, and a naive build would discover that on day one.

## The ask

From the AOT-over-runtime guidance (paraphrased):
optimize per-tick steady-state, not load/setup — AOT is *allowed* to take
longer. As a future feature, **a disk cache for compiled functions, like
Python's `__pycache__`/`.pyc`**: persist the result of the expensive
ahead-of-time work keyed by a source hash, so re-running an unchanged
program skips recompilation and amortizes the AOT cost across runs.

The user accepts a slow first run. The goal is that the *second* run of an
unchanged program pays ≈0 setup.

## The honest finding (read this before the rest)

**"Compiled functions" cannot be persisted naively.** The two artifacts a
reader would reach for first are both bound to in-process, non-serializable
state:

1. **Native code is not trivially persistable.** The Cranelift JIT
   (`functionize/cranelift.rs`) emits into a `JITModule` that owns the
   executable pages; `JitProgram` holds a raw
   `unsafe extern "C" fn(...)` pointer plus `_string_pool` / `value_pool`
   buffers the code holds raw pointers *into*. None of that survives a
   process exit. Persisting it means emitting relocatable object code and
   writing a loader — a real project, not a cache.

2. **`Z3Program` holds `Dynamic<'ctx>` Z3 handles** (`core/z3_program.rs`).
   Every `Z3Step::Scalar { expr: Dynamic<'ctx> }`, every `checks` pair,
   every `predicates` entry is a Z3 AST node owned by a live Z3 `Context`
   (`EvidentRuntime::z3_ctx`, a leaked `'static`). It derives `Debug, Clone`
   but **not** `Serialize`, and it couldn't meaningfully — a Z3 handle is a
   pointer into a context's term bank. Cloning works only inside the same
   context.

So the `__pycache__` analogy has to be taken at its actual meaning: Python
doesn't cache *machine code* either — `.pyc` caches the **bytecode**, the
serializable intermediate that's expensive to *produce* (parse + compile)
and cheap to *reload*. Our analog is the same: **cache the serializable
RESULT of the expensive AOT stages, not the native code.** The native code
is re-JITted from the cached result — and the JIT is the cheap stage
(§1).

This doc identifies which intermediate to cache, how to key and invalidate
it, where it lives, and how a hit is guaranteed identical to a recompute.

---

## §1 What's expensive — profile the AOT path conceptually

The AOT pipeline for a functionized claim
(`runtime/query.rs::functionize_z3_uncached`):

```
source text
  → translate        AST → Z3 ASTs                     [Z3 term construction]
  → build_cache      schema → solver + env             [Z3 term construction]
  → get_assertions   pull the body's Bool assertions
  → simplify         simplify + propagate-values       ← EXPENSIVE (Z3 tactics)
  → extract_program  per-output Z3Program (the IR)      [AST walk, cheap-ish]
  → decompose        connected components               [union-find, cheap]
  → Cranelift JIT    Z3Program → native fn              ← CHEAP (µs-scale)
```

Where the cost actually sits, from the session history this work builds on:

- **The self-hosted tree-walk passes are the dominant setup cost.** Session
  XX cut `subscriptions` over to an Evident-only pass that runs the whole
  walk as an FSM-with-stack through the tier-3 blocking interpreter
  (`portable/subscriptions.rs`). XX measured the self-hosted walk at
  **~10,000× the deleted Rust walk per claim** — Mario load +0.87s
  (~+90%). YY clawed most of that back by killing per-tick `Value` clones
  (game load 184→85ms), and ZZ proved the residual is *setup-only* (per-tick
  runtime unchanged, +0.18s one-time). But the point stands: **the
  tier-3-interpreted passes are slow, run at setup, and produce a small
  serializable result.** That is the textbook thing to cache.

- **Z3 `simplify` + `propagate-values` is the expensive Z3 stage.**
  `extract_program` is an AST walk; the JIT is microseconds. The Z3 tactic
  application over a claim's full body is where the wall-clock goes in the
  translate→IR half.

- **The JIT is cheap.** This is the load-bearing observation. It means we
  do **not** need to persist native code to win — we persist the *input* to
  the JIT (or to the extractor) and re-run the cheap tail.

**Conclusion:** caching should target (a) the tier-3-interpreted pass
results and (b) the Z3 simplify/extract output — *not* the native code.

---

## §2 The cacheable-artifact options

Three candidate boundaries, ordered by serialization difficulty.

### Option 1 — Cache the pass / extraction RESULT (the most `__pycache__`-like)

Cache the *output* of an analysis pass, keyed by a source hash. The clearest
instance:

- **`subscriptions` access-sets.** `portable/subscriptions.rs::access_sets`
  returns `AccessSets` — a `(reads, writes)` pair of dotted-identifier
  string sets. Plain strings; serializes to JSON in one line. And it is a
  **pure function of one claim's body** — the walk classifies identifiers
  reachable in that body, with no cross-schema resolution (the prefix-split
  `classify` is the only Rust part). So it keys cleanly on the claim body's
  hash, independent of givens, independent of other schemas.

- Other tier-3-interpreted / setup-time pass outputs of the same shape: a
  desugar / inject rewritten-AST output (`runtime/desugar.rs`,
  `runtime/inject.rs`), generic monomorphization results
  (`runtime/generics.rs`). These produce ASTs or string-keyed maps.

| | |
|---|---|
| **Serialize** | Trivial for string-set outputs (serde_json — already a dep, see `commands/profile.rs`). Rewritten-AST outputs need either serde derives on `core/ast.rs` (today it derives only `Debug, Clone`) or a re-print/re-parse round-trip through `pretty.rs` + the parser. |
| **Invalidate** | Source hash of the one claim body (subscriptions) or whole file (desugar). Plus the format version tag (§3). |
| **Rebuild on miss** | Re-run the pass. For subscriptions that's the ~10,000×-slower-than-Rust walk we're trying to skip — so the hit is a big win. |
| **Risk** | Lowest. The cached value is *data the runtime already trusts*; a hit feeds the same `AccessSets`/AST the pass would have produced. No Z3, no native code, no context binding. |

This is the truest `__pycache__` analog: cache the expensive *output*, not
the machine code.

### Option 2 — Cache the simplified Z3 form as SMT-LIB text

Cache the post-`simplify`+`propagate-values` assertion set as **SMT-LIB
text**, produced by Z3's own printer. On a hit, re-parse the text into a
*fresh* Z3 context (`Context::from_smtlib` / `Solver::from_string`), then
re-run the cheap tail: `extract_program` → decompose → JIT.

This skips the two expensive Z3 stages — translate (AST → Z3 terms) and the
simplify tactic chain — and re-does only microsecond-scale work.

Crucially, the expensive stages are **given-independent**:
`functionize_z3_uncached` calls `build_cache(..., &empty_given, ...)` and
simplifies *before* any given values are pinned (the extracted program is
deliberately generic over input values; only the cheap `decompose` step
reads given *keys*, for its `broadcast` set). So the SMT-LIB cache keys on
the claim's translation inputs alone — no given-keys in the key.

| | |
|---|---|
| **Serialize** | Z3 prints assertions to SMT-LIB; re-parse into a fresh context on load. Text is portable across runs and (modulo §5) across machines. |
| **Invalidate** | Hash of (claim body **+ every schema/enum/stdlib file it transitively references**, because translation pulls those in) + Z3 version + format version. Per-file (whole-program) hashing is the safe default; per-claim needs dependency tracking (§3). |
| **Rebuild on miss** | Re-translate + re-simplify (the expensive half), then continue as normal. |
| **Risk** | Medium. SMT-LIB round-trip must reproduce the *same* assertion set the extractor expects (name preservation, datatype/enum declarations, no tactic re-normalization on parse). Validated by the §5 gate. |

### Option 3 — Cache Cranelift object code

Emit relocatable object code (`cranelift-object` instead of pure
`cranelift-jit`), write the `.o` to disk, and relocate/load it on startup.
Closest to a literal "compiled function on disk."

| | |
|---|---|
| **Serialize** | Object emission is supported by Cranelift, but you must also persist the *interface* `JitProgram` carries (`input_offsets`, `output_kinds`, `enum_tags`, the `_string_pool` / `value_pool` the code points into) and write a loader that relocates symbols and re-binds those pools. |
| **Invalidate** | Same key as Option 2 plus the target triple / ABI / Cranelift version. |
| **Rebuild on miss** | Full pipeline. |
| **Risk** | Highest, and the payoff is smallest: per §1 the JIT is the *cheap* stage. We'd take on object-loader complexity to save microseconds. |

### v1 recommendation

**Ship Option 1 first (subscriptions access-sets), then Option 2 (simplified
SMT-LIB). Defer Option 3 indefinitely.**

Rationale, against the AOT-can-take-longer priority:

- Option 1 attacks the **largest, best-understood setup cost** (the
  tier-3-interpreted subscriptions walk, XX's ~10,000× regression), with the
  **lowest serialization and correctness risk** (plain string sets, pure
  per-claim function, no Z3/native binding). It is the smallest useful slice
  and the cleanest `__pycache__` analog.
- Option 2 generalizes the win to the rest of the AOT path (translate +
  simplify), reusing the same cache-dir / key / version machinery, and still
  never touches native code.
- Option 3 is explicitly a non-goal for v1: it's the heaviest lift to save
  the stage that's already cheap. Revisit only if profiling ever shows JIT
  time dominating — which today it does not.

---

## §3 Cache key + invalidation

Mirror `__pycache__`'s two-part validity check: a **magic number** (here, a
format/runtime version tag — bump it and every prior entry is ignored) plus
a **source fingerprint** (Python uses mtime or a source hash; we use a hash,
which is robust to checkouts / clock skew / `touch`).

### Key components

A cache entry's key is the hash of:

1. **The source fingerprint.** SHA-256 (or BLAKE3) of the relevant source
   text. Granularity:
   - **Per-claim (Option 1, subscriptions):** hash the claim's body. Sound
     because `access_sets` is a pure function of that body alone.
   - **Per-file / transitive (Option 2, SMT-LIB):** hash the claim body
     **plus every schema, enum, and stdlib file it transitively references**
     — because translation resolves `ClaimCall`s, passthroughs, types, and
     enums across files. The safe default is to hash the whole loaded
     program (all source files concatenated in load order). Per-claim with a
     precise dependency set is the optimization, not the starting point.
2. **A format/runtime version tag** — a single integer constant in the
   runtime, bumped by hand whenever any of these change in a
   result-affecting way: the AST shape, the translator, the simplify tactic
   chain, the extractor, the `AccessSets` semantics, or the serialization
   format itself. A bump silently invalidates the whole cache (entries with
   a stale tag are never read). This is the `.pyc` magic number.
3. **The Z3 version** (Option 2 only) — Z3's simplifier output is not
   guaranteed stable across Z3 releases, so the simplified-SMT-LIB cache
   must key on the linked Z3 version string. (Option 1 doesn't touch Z3.)
4. **Environment that changes the result** — any env var that alters what
   the pass produces. `EVIDENT_TACTICS` and `EVIDENT_Z3_ARITH_SOLVER` change
   the simplified form, so they belong in the Option 2 key. (`EVIDENT_LENIENT`
   is toggled internally during `build_cache` and is constant per run.)

The on-disk filename is the hex of the combined hash, under a per-kind
subdirectory (e.g. `subscriptions/`, `smtlib/`); the file's header re-states
the version tag and the source fingerprint so a hit is re-verified before
use (collision defense, §5), exactly as the in-memory `value_cache` re-checks
its stored `input` on a hash hit (`query.rs::value_cache_get`).

### Granularity trade

Per-file keying is simpler and always correct; one claim's edit invalidates
the file's entries, which is fine (re-running the passes for an unchanged
file's other claims is the cost we pay for not tracking dependencies). Per-
claim keying (Option 1) is free here because the pass is genuinely per-claim.
Per-claim keying for Option 2 requires a real dependency graph and is a v2
concern.

---

## §4 Cache location

Resolve the cache directory with the same discipline as WW's stdlib
resolver (`stdlib_path.rs`): one resolver function, an explicit override
that's authoritative, then standard locations, then a clear fallback.

Resolution order:

1. **`EVIDENT_CACHE`** — explicit override (the `PYTHONPYCACHEPREFIX`
   analog). If set, it's authoritative; the runtime creates it if absent.
   An empty/whitespace value is treated as unset.
2. **XDG cache dir** — `$XDG_CACHE_HOME/evident`, falling back to
   `$HOME/.cache/evident`. This is the default for an installed binary.
3. **Project-local fallback** — `.evident-cache/` in the current working
   directory (the closest analog to `__pycache__` sitting next to the
   source). Used when no HOME/XDG is available, and convenient for
   project-scoped caching in a dev tree.

Layout under the resolved root:

```
<cache-root>/
  CACHEDIR.TAG                       # marks the dir as a cache (so backup
                                     # tools skip it; standard convention)
  v<TAG>/                            # version-tag namespace: a tag bump
                                     # writes a new dir, old ones are dead
    subscriptions/<hash>.json        # Option 1: AccessSets, serde_json
    smtlib/<hash>.smt2               # Option 2: simplified assertions
```

Namespacing entries under `v<TAG>/` means a version bump never reads stale
entries *and* leaves the old generation on disk for a `clear` to reclaim
(rather than silently mixing generations).

Controls:

- **`--no-cache`** (CLI flag) / **`EVIDENT_CACHE=0`** (or a dedicated
  `EVIDENT_NO_CACHE=1`) — bypass entirely: never read, never write. For A/B
  measurement and as a safety valve, mirroring the existing
  `EVIDENT_VALUE_CACHE=0` switch on the in-memory value cache.
- **`evident cache clear`** (CLI subcommand, under `commands/`) — remove the
  cache root. Optionally `clear --stale` to drop only non-current
  version-tag generations.
- A cache that can't be read/written (permissions, full disk) must
  **degrade to recompute**, never error — the cache is an accelerator, not a
  dependency.

---

## §5 Correctness — a hit must be identical to a recompute

The cache must change only *speed*, never *results*. This is the section
that earns the right to ship.

**The guarantee:** for any program P, running with the cache cold, then warm,
then cold again, must produce byte-identical query results / effects /
exit codes. The cache is a memoization of pure functions; a hit substitutes
a stored output for a recomputation that is defined to produce the same
output.

Mechanisms, layered:

1. **Version tag (the magic number).** Any change to the AST, translator,
   simplifier, extractor, pass semantics, or serialization format is
   accompanied by a manual tag bump. Stale-tag entries are unreachable
   (namespaced under `v<TAG>/`, §4). This is the primary defense and the one
   that must be respected by every future change — the same discipline
   `.pyc`'s magic number imposes. A CI check can assert "if these files
   changed in a PR, the tag constant changed too."

2. **Source-fingerprint re-verification on hit.** The entry's header repeats
   the source hash; on a hit it's re-compared against the freshly computed
   hash before the payload is trusted. A hash collision (astronomically
   unlikely with SHA-256, but the discipline is cheap) falls through to
   recompute rather than serving the wrong entry — exactly how
   `value_cache_get` re-checks its stored `input`.

3. **Round-trip equivalence gate (Option 2 specifically).** SMT-LIB
   re-parse is the one place where "the text reloaded equals what the
   extractor expects" is non-obvious. Guard it two ways:
   - A **build-time differential test**: for every `examples/test_*.ev`,
     extract the program both directly and via SMT-LIB round-trip; assert
     the resulting `Z3Program`s (hence `QueryResult`s) match. This belongs in
     the existing `./test.sh` Rust phase.
   - A **runtime self-check mode** (`EVIDENT_CACHE_VERIFY=1`): on a hit, also
     recompute and assert equality, logging any divergence. Off by default
     (it defeats the speedup); on in CI and when debugging a suspected stale
     hit.

4. **No partial writes.** Write to a temp file and atomically rename into
   place, so a crash mid-write never leaves a truncated entry a later run
   would read as valid. (`__pycache__` does the same.)

5. **The cache never caches uncertainty.** Mirror the existing in-memory
   discipline: `try_functionize_z3` only memoizes when the fast path
   *produced* a result; a `None` (fall-through-to-Z3) is never cached as a
   positive. The disk cache inherits this — we cache pass outputs and
   simplified forms that are *defined*, never "this didn't work."

---

## §6 What it amortizes

The cache makes the **second run of an unchanged program pay ≈0** for the
stages it covers. Tied to the session history this builds on:

- **Subscriptions inference (Option 1).** ZZ measured the Evident-only
  subscriptions cutover as **+0.18s one-time setup** (Mario), with per-tick
  runtime unchanged — XX's self-hosted walk is ~10,000×/claim slower than
  the deleted Rust walk but runs only at setup. A disk-cached `AccessSets`
  turns that +0.18s into a sub-millisecond file read on every subsequent
  run of the unchanged program. **This is the single biggest, cleanest
  amortization available**, and it's why Option 1 is v1.

- **Translate + simplify (Option 2).** The desugar / generics / inject ports
  and the Z3 simplify pass are all setup-time. Caching the simplified
  SMT-LIB collapses the translate+simplify half of the AOT path to a
  text-parse + cheap re-extract + re-JIT. The re-JIT cost stays (it's
  microseconds, §1), so the warm-run AOT cost drops to roughly "parse the
  cached forms + JIT" — the expensive Z3 work is gone.

- **It compounds with the in-memory caches, doesn't replace them.** The
  in-process `fn_cache` / `slow_path_cache` / `value_cache` (all keyed
  `(name, given-keys)`, all cleared on reload — `load.rs`) accelerate
  *within* one run. The disk cache accelerates *across* runs by skipping the
  work that populates them. A warm disk cache means the first tick of run #2
  reaches a hot in-memory plan far faster than run #1 did.

Expected win, qualitatively: for a program whose source is unchanged between
runs (the common dev inner-loop and any production redeploy of the same
artifact), the **setup cost the user feels on run #2+ drops to the I/O of
reading a handful of small files** — exactly the `__pycache__` experience.
First run is unchanged (or marginally slower, by the cost of writing the
entries — acceptable per the AOT-can-take-longer priority).

---

## §7 Implementation plan

### v1 slice — cache the subscriptions pass result (smallest useful)

1. **Cache-dir resolver.** A `cache_dir() -> Result<PathBuf, String>` in a
   new `runtime/src/cache_path.rs`, modeled directly on `stdlib_path.rs`:
   `EVIDENT_CACHE` override → XDG `~/.cache/evident` → `.evident-cache/`.
   Honors `--no-cache` / `EVIDENT_NO_CACHE`. Writes `CACHEDIR.TAG` and the
   `v<TAG>/` namespace on first use. Unit-test the pure resolution core
   (override-wins, default-finds-XDG, unwritable-degrades) the way
   `stdlib_path` tests do.
2. **Version tag constant.** One `const CACHE_FORMAT_VERSION: u32` in the
   runtime, plus a CI guard that flags PRs touching the relevant modules
   without bumping it.
3. **Serialize `AccessSets`.** Add serde to the `(reads, writes)` string-set
   output (serde_json is already a dependency). Define the on-disk JSON:
   header (`{version, source_hash}`) + payload.
4. **Wire it into `portable/subscriptions.rs::access_sets`.** Before running
   the FSM walk for a claim, compute the claim-body hash, look up
   `subscriptions/<hash>.json`; on a verified hit, decode and return; on a
   miss, run the walk as today and write the result atomically. Bypass under
   `--no-cache`.
5. **Tests.** Cold/warm/cold equivalence over `examples/`; a `--no-cache`
   path that matches the warm path's results; a version-bump-invalidates
   test; an unwritable-dir-degrades test. All inside `./test.sh`.

### Generalize — Option 2 (simplified SMT-LIB)

6. Reuse the resolver + version tag. Add a `smtlib/` kind. After
   `simplify_assertions` in `functionize_z3_uncached`, on a miss write the
   simplified assertions as SMT-LIB keyed by the transitive source hash + Z3
   version + tactic env; on a hit, re-parse into the runtime context and
   skip translate+simplify, continuing at `extract_program`.
7. Add the §5 differential test (direct vs round-trip extraction) and the
   `EVIDENT_CACHE_VERIFY=1` self-check mode.
8. Land `evident cache clear` under `commands/`.

### Later

9. Per-claim dependency tracking for Option 2 (so one claim's edit doesn't
   invalidate a whole file's entries). Object-code persistence (Option 3)
   only if JIT time ever becomes the bottleneck.

---

## §8 Open questions / non-goals

- **Native-code persistence is deferred** (Option 3). The JIT is the cheap
  stage; persisting `JITModule` output is the heaviest lift for the smallest
  win. Out of scope until profiling says otherwise.

- **Interaction with the in-memory caches.** The disk cache sits *upstream*
  of `fn_cache` / `slow_path_cache` / `value_cache` and the
  `functionize_z3_cache` — it accelerates producing the inputs those caches
  hold, and is consulted at the same setup points. The in-memory caches keep
  their current keying (`(name, given-keys)`) and their reload-clears-all
  invariant (`load.rs`); the disk cache adds a persistent layer behind them.
  Open: should a disk-cache hit warm the in-memory caches eagerly, or only
  on first demand? (Lazy is simpler and matches today's flow.)

- **Multi-process cache safety.** Two `evident` processes sharing one cache
  dir can race on writes. Atomic temp-file+rename (§5) makes a *read* always
  see a complete file, but two writers can still duplicate work. Acceptable
  for v1 (worst case: redundant compute, never corruption). A per-entry lock
  file is a v2 nicety, not a correctness requirement, given atomic rename.

- **Cache eviction / size bounds.** `__pycache__` never evicts; entries are
  tiny (string sets, SMT-LIB text). v1 follows suit — `cache clear` is the
  only reclamation. Revisit a size cap only if the `smtlib/` corpus grows
  large for big programs.

- **AST serialization for desugar/inject result caching.** `core/ast.rs`
  derives only `Debug, Clone` today. Caching a rewritten-AST pass output
  needs either serde derives on the AST (mechanical, but touches `core/`) or
  a re-print/re-parse round-trip through `pretty.rs` + the parser (no `core/`
  change, but adds a parse on every hit and a round-trip-fidelity obligation
  like Option 2's). Decide per-pass when those passes are cached; not needed
  for the v1 subscriptions slice (string-set output) or Option 2 (SMT-LIB).

- **Cross-machine portability.** Option 1's string sets are portable. Option
  2's SMT-LIB is portable *as text* but keyed on the local Z3 version; a
  shared cache across heterogeneous machines must include enough of the
  target identity (Z3 version is in the key; add OS/arch only if a divergence
  is ever observed). Option 3 would be inherently target-specific. v1 treats
  the cache as machine-local.
