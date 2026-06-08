# A Gang-of-Four for Constraint Programming

### Design patterns discovered in the Evident self-hosted compiler (`compiler2/`)

> Status: exploratory analysis. This document reads `compiler2/` as a
> *corpus* — one of the first large programs written in Evident — to
> discover the recurring design patterns of constraint programming. It
> changes no source. File:line references are to the tree at branch
> `cp-design-patterns`.

---

## 1. Thesis

The "Gang of Four" cataloged the recurring shapes of object-oriented
construction: not algorithms, but *the structural and behavioral
arrangements that keep showing up* once you build large systems out of
objects and messages. This document asks the analogous question for a
different substrate: **what are the recurring shapes once you build a
large system out of constraints and a solver?**

Evident is an unusually sharp lens for this question because of its
execution model, which is worth stating precisely:

- A program is a set of **constraints** over named variables.
- Execution is a sequence of **ticks**. Each tick, a Z3 SMT solver
  finds a satisfying assignment for the whole constraint set, the
  kernel reads back the `effects` Seq from the model and dispatches it
  (I/O, libffi calls, exit), and the results are fed back as
  `last_results` on the next tick.
- The only memory across ticks is the **carry**: a top-level variable
  `x` of primitive type is re-asserted next tick as `_x = <its model
  value>`. So `_x` is "x one tick ago."

This is **FSM-over-SMT**: a deterministic finite-state machine whose
transition function is expressed *entirely as constraints*, re-solved
from scratch every tick. There is no mutable store, no loop, no call
stack, no random-access array in the language — only constraints, the
one-tick carry, and pattern-match over recursive enums. And yet
`compiler2/` is a ~7,800-line program that lexes, Pratt-parses, and
emits SMT-LIB for a substantial subset of Evident, *by building Z3 ASTs
in memory via libffi*.

A program that ambitious, written under those constraints, is forced to
**reinvent — in constraint form — most of the machinery a normal
language gives you for free**: a stack, a heap, a symbol table, a
program counter, subroutine calls, an instruction interpreter, a
sliding parse window. The shapes it uses to do so are the patterns this
catalog names. They are *not* the patterns of the classical CP modeling
literature (see §2): those are about how to encode a combinatorial
*search* problem so a solver finds the answer efficiently. These are
about how to drive a *deterministic computation* over a solver that
re-solves every tick — closer in spirit to GoF (how to *construct*
software) than to MiniZinc's handbook (how to *model* a puzzle).

The catalog is organized into four GoF-style categories:

- **Structural** — how data and types are organized in/around the model.
- **Behavioral** — how constraints drive computation across ticks.
- **Creational** — how Z3 objects and effects get built.
- **Compositional** — how claims combine (Evident-specific; the closest
  analog to GoF's "object vs. class" axis).

---

## 2. Prior art (web survey)

There is a real, if young, literature on **constraint-programming
patterns**, but it sits at a different altitude than this catalog.

- **Domain-specific constraint patterns** — the most explicit "patterns
  for CP" effort. A live repository ([constraintpatterns.com]) and the
  backing paper (Kelareva et al., *Easy, adaptable and high-quality
  Modelling with domain-specific Constraint Patterns*, [arXiv:2206.02479])
  use a recognizably GoF-like template — *recurring problem → solution
  approach (expert/best-practice modelling) → consequences* — but each
  pattern is tied to a problem *domain* (scheduling, timetabling,
  TSP/permutation) and bundles the global constraints and search
  strategies known for that domain. These are patterns for *encoding a
  search problem*, not for *driving a computation*.
- **MiniZinc "Effective Modelling Practices"** ([MiniZinc Handbook §2.7])
  is the canonical idiom list: tight **variable bounds**, **dual /
  viewpoint models** with **channeling constraints**, **redundant
  (implied) constraints**, and **symmetry breaking**. All are about
  making *search* converge — again, a different concern.
- **SMT-LIB encoding idioms** — **reification** (associate a 0/1 var
  with a constraint's truth), the **element constraint** (array indexing
  via an index variable), and **constraint decomposition** into
  auxiliary variables ([Effective encodings of CP models to SMT],
  St Andrews; [An Encoding for CLP Problems in SMT-LIB], arXiv:2404.14924).
- **Answer Set Programming** contributes two micro-idioms that *do* show
  up here in spirit: the **fact** (rule with empty body) and the
  **integrity constraint** (rule with empty head — "this must not
  happen"), plus the pervasive **generate-and-test** structure.

**The gap.** None of this prior art catalogs patterns for using a
constraint solver as a *general-purpose computational substrate driven
by a state machine*. The classical work treats the solver as an oracle
you call once to solve a static puzzle; Evident calls it 100,000 times,
threading state through it to run an interpreter. The closest single
overlap is **reification** (Evident's whole "lower a boolean expression
to a Z3 handle" path is reification taken to its logical extreme — see
*Reify-to-Handle*, §5.5). The classic CP modeling patterns
(channeling, symmetry breaking, redundant constraints) are essentially
**absent** from compiler2, precisely because compiler2 does no search:
every tick's constraint set is engineered to have exactly one model.
That absence is itself the finding — **FSM-over-SMT is a genuinely
under-cataloged regime**, and the patterns below appear to be new
contributions rather than restatements.

Sources are listed in full in Appendix B.

---

## 3. Structural patterns

How data and types are organized in, and alongside, the constraint
model. The recurring tension: the language gives you only primitive
carried scalars and immutable recursive enums, but the program needs
structs, heaps, stacks, and arrays. These patterns bridge that gap.

### 3.1 Co-Traveling Handle Bank

- **Intent.** Keep a struct's worth of opaque foreign references (Z3
  context/solver/sort handles, buffer pointers) as a flat bank of named
  carried scalars that travel together through every tick.
- **Motivation.** Z3 and libc live *outside* the model. A `Z3_context`
  is just an `i64` pointer that libffi handed back. There is no struct
  type to hold the dozen long-lived handles, and they must survive every
  tick — so each becomes its own top-level `Int` state-field, latched
  once and read forever.
- **Structure.** One `name ∈ Int` per handle, all initialized on tick 0
  and otherwise carrying `_name`; written exactly once at a named setup
  step (see *Latch-Once Register*, §4.2).
- **Example.** `compiler2/driver_zinit.ev:33-115` — `z_cfg`, `z_ctx`,
  `z_sol`, `z_isort/bsort/ssort/rsort`, `z_arena`, `z_zero..z_four`,
  `tbase`, `st_base`, `ci_base` — ~40 handles in one bank.
- **Consequences.** (+) Dead simple, fully inspectable. (−) No
  encapsulation: any tick can read any handle, and the bank is a flat
  namespace that grows linearly with the foreign API surface. (−) Adding
  a handle touches three lines (decl, latch, the step number).
- **Related.** GoF *Facade* over a foreign library; the "god-struct of
  handles" idiom in FFI-heavy C. No CP-literature analog — the modeling
  literature never holds long-lived foreign state.

### 3.2 Externalized Heap (FTI Buffer)

- **Intent.** Store bulk/mutable data *outside* the constraint world in
  raw `calloc`'d memory, keeping only a base pointer and a cursor in the
  model.
- **Motivation.** The token stream, the symbol-table handles, and the
  claim index are all too large and too mutable to live as carried
  enums (a cons-list of N tokens would be re-asserted in full every
  tick — quadratic, and it blows the solver up). "FTI" (Foreign-Typed
  Interface) puts the bytes in libc memory; the model holds an `Int`
  base and an `Int` count, and reads/writes one slot via
  `__mem.read_long` / `write_long` effects.
- **Structure.** `base ∈ Int` (calloc'd region) + `count ∈ Int` cursor;
  fixed-stride records (`base + i*stride`); the region is zeroed so
  reads past the written end decode deterministically (e.g. to `EofTok`).
- **Example.** `compiler2/lex_fti.ev:1-72` (the token buffer: 32-byte /
  4-slot records, written in append order via `write_long` at
  `base + count*32`); `compiler2/driver_symtab.ev:21-50` (the symbol
  table's `st_base` handle array); the claim index `ci_base`
  (`driver_zinit.ev:114`).
- **Consequences.** (+) Constant per-tick model size regardless of data
  volume — the single most important scaling move in the program.
  (+) Mutation is free (a `write_long` effect), unlike immutable enums.
  (−) Every access costs a tick (the read result lands next tick — see
  *Handle-Capture*, §4.7). (−) Bounds safety is manual (see *Bounded
  Cursor*, §3.4). (−) Leaks until process exit.
- **Related.** GoF *Flyweight* (extrinsic state held outside the object);
  the classic "indices into a side array" of data-oriented design.

### 3.3 Fixed-Width Record String

- **Intent.** Implement an associative lookup table as a single
  delimited, fixed-stride Z3 `String`, so "find name" becomes pure
  string arithmetic with no per-entry tick.
- **Motivation.** A symbol table needs name→slot lookup, but iterating a
  list costs a tick per entry and the names can't go in the FTI byte
  buffer cheaply (they're variable length). Z3's string theory *can*
  index a big string in one solve. So names are packed into one
  `String` of fixed-width records and looked up with `index_of` / `/
  stride`.
- **Structure.** `names ∈ String`, each entry `"|" ++ pad(name, 31)`
  (32 bytes). The `"|"` delimiter never occurs inside a record (idents
  are alnum/`_`, padding is spaces), so every `index_of` hit is
  stride-aligned and `slot = index_of(names, key) / 32` is exact and
  pure (no tick).
- **Example.** `compiler2/driver_symtab.ev:39-50` (`st_names`, the
  `"|name<pad31>"` layout, the purity argument in the header comment);
  the lookup itself at `compiler2/driver.ev:748-750`
  (`d_lk_key`/`d_lk_pos`); the claim-index `ci_names` and the call-floor
  `d_cb_names` at `compiler2/driver_pratt.ev:35-52`.
- **Consequences.** (+) O(1)-tick lookup (one pure solve), the key to
  not spending a tick per symbol. (+) Reuses Z3's industrial string
  solver as a hash map. (−) Truncation-aliasing if a name exceeds the
  fixed width (corpus max is 21 chars vs. a 31 budget — a latent bound).
  (−) The companion handle still lives in the FTI heap, so a full lookup
  is "pure index_of + one `read_long` tick."
- **Related.** Perfect-hash / interning tables; the element constraint
  (array indexing) from CP, here implemented over strings rather than
  int-indexed arrays.

### 3.4 Bounded Cursor

- **Intent.** Pair a monotonic counter with a capacity bound expressed as
  a *constraint*, so overflow becomes UNSAT (a clean halt) rather than a
  native buffer overrun.
- **Motivation.** The FTI buffers (§3.2) are fixed-size foreign
  allocations. A `write_long` past the end would corrupt memory
  silently. By declaring the cursor with an upper bound, the solver
  *refuses* to produce a model that overflows — the error surfaces as
  the kernel's UNSAT halt (exit 2), not a segfault.
- **Structure.** `count ∈ Int < CAP`; the bound is a real membership
  constraint, not a runtime check.
- **Example.** `compiler2/driver_symtab.ev:38` —
  `st_cnt ∈ Int < 8192`, with the header note "a declare past the buffer
  end makes the model UNSAT rather than a native overrun."
- **Consequences.** (+) Memory safety for free, enforced by the solver.
  (+) The bound documents the capacity. (−) Overflow is a hard,
  uninformative halt (exit 2), not a graceful error. (−) The CAP must be
  picked statically and conservatively.
- **Related.** MiniZinc's "always bound your variables" best practice —
  here repurposed from search-efficiency to memory-safety. ASP's
  integrity constraint ("this must not happen") in scalar form.

### 3.5 Offset-Partitioned Arena

- **Intent.** Carve one foreign allocation into fixed, named byte-offset
  sub-regions used as typed scratch space, reused across phases that
  never overlap in time.
- **Motivation.** Many builder steps need small scratch buffers (an
  args[] array for an n-ary `Z3_mk_add`, a query-output block for
  datatype harvesting). Allocating per use would cost ticks and leak.
  Instead one `z_arena` is partitioned by convention: `+200` is the
  2-slot binop args buffer, `+224` the ctor-args buffer, the ED machine
  uses `+0..+152`.
- **Structure.** A single base pointer (`z_arena`) plus a documented
  offset map; callers pass `base + OFFSET` to builders. Temporal
  non-overlap (phase A's `+200` use finishes before phase B's) is the
  invariant that makes reuse safe.
- **Example.** `compiler2/driver.ev:1271-1289` (`z_arena + 200`/`+208`
  for binop operand slots; `+224` for ctor args at `:1366`);
  `compiler2/driver_enum.ev:49-55` (the ED machine's `+0 fnames /
  +24 fsorts / +48 srefs / +64 ctors / +152 query block` map).
- **Consequences.** (+) Zero per-use allocation, zero leak growth.
  (−) Temporal-non-overlap is an unchecked invariant; two live uses of
  the same offset is a silent corruption. (−) The offset map is
  tribal knowledge in comments.
- **Related.** Arena/region allocation; the classic C union-of-scratch
  trick; stack-frame layout.

### 3.6 Cons-Stack (Recursive Enum as Stack)

- **Intent.** Use a recursive enum carried in the model as a bounded
  LIFO stack / list, for the *small, hot* working structures that must
  be first-class values (not foreign bytes).
- **Motivation.** Not everything can be externalized: the operand
  **handle stack** of the work-item interpreter, the **work-item program**
  itself, and the **inline-frame stack** are small, change shape every
  tick, and must be matched/destructured — exactly what the FTI heap is
  bad at. Recursive enums are the language's only first-class container,
  so they become the stack.
- **Structure.** `enum Stack = Nil | Cons(Elem, Stack)`; the whole stack
  is one carried field, rebuilt each tick by cons/peel.
- **Example.** `compiler2/driver_ir.ev:40-64` — `C2Items`
  (the per-line instruction program), `C2H` (the Z3-handle operand
  stack), `C2Binds` (slot bindings), `C2Frames` (the call-frame stack).
- **Consequences.** (+) First-class, matchable, no tick to read.
  (+) Pattern-match peels it cleanly. (−) Re-asserted in full every tick
  — only viable because depth is bounded small (handle stack ~6,
  frames ≤8). (−) No random access (see *Peel-View Bank*, §3.7).
- **Related.** The functional cons-list-as-stack; GoF doesn't have this
  because OO has mutable arrays.

### 3.7 Peel-View Bank

- **Intent.** Destructure the top-N of a carried cons-list into a flat
  bank of indexable named members, because the language has no
  random-access into recursive data.
- **Motivation.** To pop two operands you need the 1st and 2nd elements
  of `C2H`; to bind k slots you need the top-k handles; to classify a
  line you need the next 8 tokens. Each requires a *fixed* depth of
  access into a recursive value. Since `xs[i]` doesn't exist for enums,
  the program materializes a ladder of match-peel views: tail, head of
  tail, tail of tail, ... down to the needed depth.
- **Structure.** A repetitive cascade: `t1 = match xs (Cons(_,r)⇒r)`,
  `h1 = match t1 (Cons(h,_)⇒h)`, ... one pair per depth level. The bank
  is then addressed positionally (`d_h_top`, `d_h_2nd`, ...).
- **Example.** `compiler2/driver_symtab.ev:66-101` (`d_h_top` …
  `d_h_6th` + the `d_h_t1..t6` tails over `C2H`);
  `compiler2/driver_compose.ev:66-130` (`ilb_n0..n5`/`ilb_h0..h5` over
  `C2Binds`, plus the suspended-frame peel); `compiler2/driver_window.ev:255-299`
  (`ww_t0..ww_t7` over the token window).
- **Consequences.** (+) Gives random-ish access where the language gives
  none. (−) Enormous boilerplate (the single most voluminous shape in
  the corpus); the access depth is hard-capped by how many views you
  wrote (this is *why* the window is 8 and slots cap at 6). (−) Widening
  the cap is "mechanical but tedious" — the comments say so repeatedly.
- **Related.** This is the constraint-world stand-in for array indexing;
  the *element constraint* of CP, paid for in source volume.

### 3.8 Decoded Sliding Window

- **Intent.** Present a fixed-size, decoded, random-access view over an
  unbounded externalized buffer, refilled on demand — so parser logic
  reads "the next k tokens" without ever holding the whole stream.
- **Motivation.** Combines §3.2 (the token stream is in the FTI heap)
  with §3.7 (peel views) and a refill protocol: the parser needs
  lookahead, but mustn't carry a `TokenList` of the whole file. So it
  carries an 8-entry window decoded from the buffer at the current
  cursor; a consumer acts only when the window holds its lookahead need,
  otherwise a fetch burst refills.
- **Structure.** `cursor` (absolute index) + `wend` (fetched coverage) +
  `wtoks` (the 8 decoded tokens) + a `w_need`/`tok_ready`/`fetch_go`
  gate. A refill is a 3-tick burst: 16 `read_long`s (8 tags + 8
  payloads), a slot-aligned string-copy tick, then a pure rebuild.
- **Example.** `compiler2/driver_window.ev` in full — esp. the
  `w_need` per-mode lookahead table (`:309-325`), the `tok_ready` /
  `fetch_go` gate (`:326-327`), and the `FtiTok` tag→`Token` decode
  (`:29-86`).
- **Consequences.** (+) Bounded model size + arbitrary-length input.
  (+) The decision token equals the consumed token (refill refetches
  from cursor), so lookahead is consistent. (−) The window width caps
  the maximum single-shape lookahead (8 tokens → "3-field payload
  variants not covered," per the driver header). (−) Refill is a
  multi-tick stall in the middle of parsing.
- **Related.** The lexer/parser "lookahead buffer"; OS demand-paging
  (fetch on miss); GoF *Iterator* with a bounded buffer.
