# The Python tiny-runtime branch — FTI + minimal-runtime techniques

Read-only exploration of the pre-Rust Python runtime. No code was
copied or merged; this note captures the techniques.

## Branch provenance

- Branch: **`tiny-runtime`**, tip `418eaf5` (2026-06-01, one day before
  this exploration — it is a *current* parallel track, not an old
  artifact).
- Tip commit: *"docs: rust-shrink-experiment report — partial success,
  recommendation to stay Python."*
- Size: **5 `.py` files, 2,712 LOC total.** The load-bearing runtime is
  tiny: `runtime.py` (177) + `ffi.py` (168) = ~345 LOC trampoline+FFI.
  The bulk is `parser.py` (1,370) + `transpile.py` (906), and they
  implement a *much smaller language* than current Evident (no enums,
  no generics, no multi-FSM scheduler, a 2-variant `FFIArg`, a 1-effect
  `Effect`).

## Correcting the coordinator's mental model of FTI

The briefing's model — *"the Z3 value is a Seq holding only the new
tail; the kernel detects an FTI queue and applies the op to an
external backing structure it maintains"* — is **half right, and the
wrong half matters.** Three corrections:

1. **The runtime knows nothing about FTIs.** `runtime.py` is a generic
   trampoline that understands exactly one effect, `LibCall`. There is
   no queue/stack recognition in the runtime. "This is an FTI queue" is
   resolved **entirely at compile time** by the transpiler, which
   *inlines* the FTI's body into the host FSM under a namespace
   (`s.contents` → `s__contents`). See `transpile_fti_instance`.

2. **The Z3 model holds the *full* sequence, not the tail.** The FTI's
   `contents ∈ Seq(T)` carries the whole logical stack/queue across
   ticks via the `_contents`/`contents` state pair. This is the
   "growing data in the FSM body" cost that current Evident's CLAUDE.md
   warns against — the Python branch hits it **knowingly** (its
   `docs/fti-composition.md` calls it "Option A", accepted for v1). So
   the per-tick Z3 solve sees the entire sequence pinned as `_contents`.

3. **What *is* tail-only is the external write, and it's driven by the
   FTI body, not the runtime.** The detection of "a push happened" is a
   `match` *inside the FTI's Evident body* on the length delta of the
   state pair; on a push it emits a single `mem_store_long` of the new
   last element. The runtime just fires whatever `LibCall` the solved
   model produced.

The tail-only / minimal-Z3-footprint mechanism the coordinator is
thinking of **does exist in the branch — but only as an unimplemented
design** (`docs/fti-z3.md`): the *Z3-via-libcall* FTI, which builds Z3
ASTs by calling libz3 through `LibCall`, so Z3 sees nothing in the
SMT-LIB body. That is a different and more powerful mechanism than the
shipped Stack/Queue. See §"The Z3 FTI (designed, not built)" below.

## FTI primitive #1 — Stack (shipped: `prelude/stack.ev`)

**Evident-side encoding.** The host FSM declares `s ∈ Stack(Int)` and
constrains `s.contents` *relationally*. There is no `s.push(42)`:

```evident
s ∈ Stack(Int)
s.contents = match phase
    0 ⇒ _s.contents ++ ⟨10⟩     -- push
    3 ⇒ init(_s.contents)        -- pop (drop last)
    _ ⇒ _s.contents              -- no-op
```

**FTI body (the whole thing).** It asserts which transitions are legal,
then a `match` maps the detected transition to libcalls:

```evident
fti Stack(T)
    base ∈ Int                -- malloc'd region start
    contents ∈ Seq(T)         -- full logical contents (Z3-side)
    effects ∈ Seq(Effect)     -- this FTI's OWN effects channel

    (contents = _contents
       ∨ contents = init(_contents)
       ∨ len(contents) = len(_contents) + 1 ∧ init(contents) = _contents)

    effects = match is_init
        true ⇒ ⟨LibCall("__mem__","mem_alloc","l(l)",⟨ArgInt(8192)⟩,"base","")⟩
        false ⇒ match len(contents) = len(_contents) + 1
            true  ⇒ ⟨LibCall("__mem__","mem_store_long","v(ll)",
                       ⟨ArgInt(base + len(_contents) * 8), ArgInt(last(contents))⟩,
                       "","")⟩
            false ⇒ ⟨⟩
```

- **Kernel-side recognition:** none. The transpiler inlines this body
  namespaced; the length-delta `match` (Evident code, solved by Z3)
  *is* the recognition.
- **Kernel-side storage:** a `malloc`'d region, reached through a
  *synthetic library* `__mem__` intercepted in `ffi.py`
  (`mem_alloc`/`mem_load_long`/`mem_store_long`/`mem_free` over
  `ctypes`). The address is an `Int` carried in `base`.
- **Read-back on later ticks:** `_s.contents` is the **full** sequence
  (state-pair carryover), *not* the head/tail. A popped value is read
  by naming `last(_s.contents)` *in the same tick as the pop*, before
  the carry overwrites it. The external memory is essentially a
  write-only mirror in v1 — the examples never read it back (they
  `puts "stack ok"`).
- **Unsupported transitions** (e.g. `contents = reverse(_contents)`) are
  rejected by the legal-transition disjunction going UNSAT → the
  trampoline halts. "The FTI honestly declares what it supports."

## FTI primitive #2 — Queue (shipped: `prelude/queue.ev`)

Structurally identical to Stack; only the legal-transition set differs:
dequeue is `tail(_contents)` (drop head) instead of `init` (drop last),
giving FIFO. Same `__mem__` backing, same single `mem_store_long` per
enqueue at `base + len(_contents)*8`. `examples/queue_basic.ev` is a
byte-for-byte twin of `stack_basic.ev` with `tail` for `init`.

## The single-writer interaction — solved by namespaced channels

Current Evident has one `effects` channel with a single-writer rule and
`++` composition. The Python branch **sidesteps single-writer entirely**:
each FTI gets its *own* effects channel, namespaced (`s__effects`,
`q__effects`). The runtime discovers them by convention:

```python
self.effects_vars = sorted(n for n in self.sorts
                           if n == "effects" or n.endswith("_effects"))
```

Each channel is dispatched independently every tick. So the host FSM's
`effects` and each FTI's `*_effects` coexist with no `++` merge and no
single-writer conflict. **This is a runtime feature the current Rust
kernel lacks** (see Caveats).

## The Z3 FTI (designed in detail, NOT built)

`docs/fti-z3.md` + `docs/fti-z3-m6-extensions.md` design a `Z3` FTI that
**is** the minimal-Z3-footprint mechanism: the host asserts a
`Formula` tree (a 14–26 constructor datatype), and the FTI *materializes*
it into real libz3 AST nodes via a post-order sequence of `LibCall`s to
`libz3` (`Z3_mk_int`, `Z3_mk_eq`, `Z3_solver_assert`, …). Z3 the solver
is then reached **as a library through FFI**, and the SMT-LIB body the
trampoline solves stays tiny — only the Formula being pushed this tick.

This requires three runtime extensions that **were never implemented**
(no `prelude/z3.ev` exists; `FFIArg` in `transpile.py` has only
`ArgInt`/`ArgStr`; `runtime.py`/`ffi.py` have no scratchpad):

- **`ArgRef` + tick-local scratchpad** — lets one `LibCall`'s result be
  an argument to a later `LibCall` *within the same tick* (handle
  threading). Current cross-tick threading uses `ok_dest` → next tick's
  `given`; intra-tick chaining needs the scratchpad.
- **`@push` / `@pop:N:@push` stack scratchpad** — RPN-style post-order
  evaluation of the Formula tree without needing unique names per
  subtree.
- **`def` / `define-fun-rec`** — a recursive `materialize` over the
  Formula datatype.

This is the genuinely novel idea on the branch and the one most
relevant to a minimal kernel: **"Z3 is a library, not a runtime"** —
the same FFI bridge that calls libc can build Z3 ASTs, so the kernel
needs no Z3 module at all. It is, however, a design sketch with
validated sub-pieces (Q2 const-dedup tested via ctypes), not running
code.

## Other minimal-runtime techniques (load-bearing ones)

From `docs/runtime-architecture.md` and the shipped code:

- **The whole runtime is trampoline + libcall.** Pin `_X`/`is_init`,
  solve, read `*_effects`, fire `LibCall`s, carry state, halt when
  nothing changed and no effects. ~345 LOC. Everything else is library.
- **State-pair convention with tick-1 defaults.** `_X` is the previous
  `X`; on tick 1 each `_X` is seeded with its sort's zero
  (`default_for`: `0`/`false`/`""`/empty-Seq) so FTI bodies need not
  spell out initial state.
- **`ok_dest`/`err_dest` cross-tick handle threading.** A `LibCall`'s C
  return is pinned into `given[ok_dest]` and visible next tick — the
  two-tick-latency idiom (`mem_raw.ev`: alloc tick N, use tick N+1).
- **`__mem__` synthetic library** as the escape hatch for "dereference
  a pointer," which the `i/l/d/s/v` sig grammar can't express.
- **`phase` state-pair driver.** A hand-rolled program counter
  (`phase ∈ {0..8}`, `match _phase`) sequences multi-tick work — the
  branch's stand-in for control flow.
- **No intra-tick iteration, by design.** Synchronous semantics: all
  constraints hold at once, each transition applied exactly once per
  tick. Recursion/iteration is either **compile-time unrolling**
  (bounded) or **multi-tick convergence** (unbounded). Continuations /
  coroutines / work-stacks are *explicitly rejected*. (Note: a
  cell-array PDA / LR-parser path is described in
  `runtime-architecture.md`, but those files do **not** exist at this
  branch tip — that doc is partly aspirational/historical.)
- **Composition over scheduling.** Tight coupling → compose FSM bodies
  at compile time (namespaced, AND'd, one trampoline). Loose coupling →
  separate trampolines + mailbox FTIs (actors). No runtime scheduler,
  no subscription mechanism — Z3 propagates shared variables.

## Caveat for transcribing to the *current* kernel

The shipped FTI mechanism assumes runtime features the current Rust
kernel (per `CLAUDE.md`) does **not** have: multiple namespaced
`*_effects` channels, the `__mem__` library, and (for the Z3 FTI) the
`ArgRef`/scratchpad chaining. The current kernel has one `effects`
channel, single-writer + `++` composition, and `LibCall`/`Exit`
natives. Porting Stack/Queue verbatim would need either kernel changes
(frozen) or re-expressing per-FTI effects as guarded writes into the
single channel composed with `++`. Worth a design note before anyone
treats `prelude/stack.ev` as directly transcribable.

## Recommendation: capture-only for the *code*, copy the *design docs*

**Do not import the Python tree.** The runtime/FTI *code* is tiny and
its techniques are fully captured by the excerpts above — `runtime.py`
and `ffi.py` are reproducible from this note, and the two prelude FTIs
appear here in full. The parser/transpiler (2,276 of the 2,712 lines)
implement a smaller, divergent language and duplicate reference value
the existing Rust `bootstrap/` already provides; copying them adds
maintenance surface for little gain.

**However, the four FTI/runtime *design docs* are the real intellectual
property and they are prose, not code** — `fti-composition.md`,
`fti-z3.md`, `fti-z3-m6-extensions.md`, `runtime-architecture.md`. The
most valuable single idea (the Z3-via-libcall Formula FTI that keeps
Z3's SMT-LIB body tiny) exists *only* in those docs and was never
implemented, so it cannot be recovered from code. If anything is
brought into this repo as reference, it should be those ~1,500 lines of
markdown under `legacy-python/docs/` (plus optionally the two ~50-line
prelude FTIs and the `examples/*stack*|*queue*|mem_raw*.ev` fixtures as
worked examples) — **not** the Python runtime. That preserves the
ideas at a fraction of the footprint and keeps the "no Python under the
repo" deletion target clean (markdown + `.ev`, no `.py`).
