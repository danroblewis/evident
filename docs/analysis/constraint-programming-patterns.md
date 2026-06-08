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

## 4. Behavioral patterns

How constraints drive computation across ticks. This is the heart of
FSM-over-SMT: with no loops, no mutation, and a one-tick memory, *all*
control flow is encoded as carried state plus a per-tick transition
constraint. These patterns are the vocabulary of that encoding.

### 4.1 Carry Latch (the fundamental cell)

- **Intent.** Give a value memory: initialize it on the first tick and
  otherwise hold its previous value, overwriting only when a write
  condition fires.
- **Motivation.** The kernel's carry gives you `_x` = "x last tick," but
  every stateful variable must *explicitly* decide each tick whether to
  change or persist. This three-way choice (init / update / hold) is the
  atom from which every other behavioral pattern is built.
- **Structure.** `x = is_first_tick ? INIT : (write_cond ? NEW : _x)`.
  The trailing `: _x` is the "hold"; omitting it would let `x` float to
  any value the solver likes (a silent bug).
- **Example.** Everywhere. Canonical: `compiler2/driver_symtab.ev:47-50`
  (`st_cnt`/`st_names`), `compiler2/driver_compose.ev:241-321` (the
  whole frame-stack bank). The pattern is so pervasive that *forgetting
  the `: _x` tail* is the corpus's archetypal bug.
- **Consequences.** (+) Deterministic, inspectable cell semantics.
  (−) Verbose: every field restates its hold. (−) A missing hold or a
  missing `is_first_tick` guard is a silent wrong-answer (the solver
  picks an arbitrary value). (−) Multiple write conditions must be
  priority-ordered by hand in the ternary.
- **Related.** A hardware register with clock-enable; ASP's frame
  axiom / inertia; the State monad's `get`/`put` collapsed into one
  expression.

### 4.2 Latch-Once Register

- **Intent.** A carry latch whose write fires *exactly once*, at a named
  step value — the building block of staged setup.
- **Motivation.** Foreign handles (a context, a sort) are created once
  and then immutable. Their latch should capture the creating call's
  result on the one tick it lands and never change again.
- **Structure.** `h = is_first_tick ? 0 : (step = N ? captured : _h)` —
  a carry latch keyed on a program-counter equality (§4.3), idempotent
  off-step.
- **Example.** `compiler2/driver_zinit.ev:33-115` — every `z_*` handle
  (`z_cfg = ... zstep = 1 ? d_cap_int : _z_cfg`).
- **Consequences.** (+) Each handle latches deterministically at its
  named step; trivially auditable. (−) Couples the register to a magic
  step number; reordering setup renumbers everything.
- **Related.** Write-once / single-assignment variables; the Co-Traveling
  Handle Bank (§3.1) is a bank of these.

### 4.3 Step Sequencer (Program Counter)

- **Intent.** An `Int` state-field that advances deterministically each
  tick to sequence a fixed straight-line program of setup/teardown
  actions.
- **Motivation.** Z3 lifecycle setup is ~38 ordered steps (config →
  context → solver → sorts → consts → buffers). With no loop, the order
  is encoded as a counter that ticks up, and each action is gated on its
  step number.
- **Structure.** `step = is_first_tick ? 0 : (_step < MAX ? _step+1 :
  _step)`; actions and latches branch on `step = N`.
- **Example.** `compiler2/driver_zinit.ev:27-31` (`zstep`, −2,−1,0,…,
  capped at 60); `istep` (the micro-step counter,
  `compiler2/driver.ev:1059-1061`); `estep` (emit phase); `ed_step`
  (`compiler2/driver_enum.ev`). The `effects` master ternary
  (`driver.ev:1463-1538`) reads `zstep = N ? ⟨...⟩` as a giant jump
  table.
- **Consequences.** (+) Turns "do these N things in order" into pure
  data. (+) Composes with Hold (§4.4) for subroutine calls. (−) Magic
  numbers proliferate; inserting a step renumbers downstream. (−) One
  action per tick — setup is inherently O(steps) ticks.
- **Related.** A microcode program counter; the *Template Method* as a
  linear schedule; the staged-builder pattern in OO.

### 4.4 Counter Hold (stall-as-subroutine-call)

- **Intent.** Park a sequencer at one value while a sub-machine runs,
  resuming when the sub-machine signals it is idle — a poor man's
  subroutine call.
- **Motivation.** The ZINIT sequencer must declare four datatypes
  mid-stream (the ED machine, §5.2), which itself takes many ticks. The
  outer counter *stalls* at step 9 until the inner machine is done, then
  advances. No call stack needed — just a guarded non-increment.
- **Structure.** `step = (_step = K ∧ sub_busy) ? K : <advance>`, where
  `sub_busy` is the sub-machine's not-idle signal.
- **Example.** `compiler2/driver_zinit.ev:28-31` —
  `ed_hold = (_ed_act ≠ 0 ∨ _ed_src < 4)`, and `zstep` holds at 9 while
  `ed_hold`. The arena latches only on the *first* 9-tick
  (`:48-50`, `zstep = 9 ∧ _zstep = 8`) — the entry edge.
- **Consequences.** (+) Lets a linear sequencer invoke a variable-length
  sub-process without a stack. (−) Only nests one level cleanly; deeper
  nesting needs explicit save/restore (which the *call frame* pattern,
  §6.2, does for inlining). (−) "First-tick-at-K" edge detection
  (`step = K ∧ _step = K-1`) is a recurring subtlety.
- **Related.** Coroutine yield; a CPU wait-state; the Hold is the
  control-flow dual of the call-frame stack.

### 4.5 Mode Mux (the priority-ternary dispatch table)

- **Intent.** A single `Int` "mode" register selects which of many
  mutually-exclusive sub-machines is live this tick; one huge
  priority-ordered ternary *is* the dispatch table.
- **Motivation.** The driver is really ~15 interleaved machines (lex,
  fetch, dispatch, skip, claim-walk, Pratt, group-walk, compose,
  match-pin, set/seq literals, quantifier, positional-bind, emit). Only
  one acts per tick. A `pmode`/`fmode` register names the active one, and
  every shared output (`witems`, `hstk`, `pmode`, `tcur`, `effects`)
  is a single big ternary that dispatches on it.
- **Structure.** `pmode ∈ Int`; outputs computed as
  `out = cond_A ? expr_A : cond_B ? expr_B : … : _out`, where the
  conditions encode the mode + sub-state and the *order* encodes
  priority.
- **Example.** `compiler2/driver.ev:1205-1251` (the `pmode` transition —
  ~40 branches); the `witems` and `hstk` selectors
  (`:981-1049`); `dcons` token-consumption (`:1088-1142`);
  `compiler2/driver_classify.ev` (the pure-classifier half of the same
  dispatch). The modes themselves: 0 dispatch, 1 skip, 2 claim, 3 Pratt,
  4 enum-decl, 5 effects-elem, 6 match-pin, 7 set, 8 seq-lit, 9 group,
  10 compose-call, 11 quant-range, 12 positional, 13 record-decl,
  14 set-lit.
- **Consequences.** (+) Makes "exactly one machine acts" structurally
  true and gives a single audit point per output. (+) New machine = new
  mode number + branches, no new control plumbing. (−) The ternaries
  grow monstrous (the `effects` selector is 75 lines); priority ordering
  is load-bearing and fragile. (−) All machines' state-fields coexist
  even when dormant.
- **Related.** GoF *State* pattern (the mode is the state object) fused
  with a jump table / `switch`; a CPU's instruction-decode mux.

### 4.6 Pure Classifier Tick

- **Intent.** Separate decision from action: a tick that does no I/O and
  changes no state, computing only a pure function of the window to pick
  the next mode and consumption.
- **Motivation.** Deciding "what kind of line is this?" needs to inspect
  lookahead against every subsystem's entry condition. Folding that into
  an acting tick would entangle decision with effect. Instead one
  classifier tick produces a fan of boolean `c_*` flags; the next tick
  acts on them.
- **Structure.** A gate (`d_classify`, true only when the work-item list
  is empty and tokens are ready) guards a block of *pure* `Bool`/`Int`
  members (no `effects`, no carried writes) that compute the line kind
  and the dispatch target.
- **Example.** `compiler2/driver_classify.ev` in full — `d_classify`
  (`:116`), the `c_is_mem`/`c_pinned`/`c_eff_line`/`c_comp_line`/`c_ty_line`
  fan, and the `d_enter_*` dispatch signals. The header explicitly notes
  "classification is a PURE function of the window + the registries (no
  own carried state)."
- **Consequences.** (+) The decision logic is testable in isolation and
  has no temporal coupling. (+) One place to read "how is a line
  recognized." (−) Costs a dedicated tick per line. (−) Must inspect the
  whole world (it is "the central dispatch brain," coupling to every
  subsystem's entry shape).
- **Related.** Lexer/parser "scannerless classify"; GoF *Interpreter*'s
  separation of parse from eval; a pure reducer.

### 4.7 Handle-Capture (deferred foreign result)

- **Intent.** Apply a foreign call's return value on the tick *after* the
  call, using a small register that records what to do with it when it
  arrives.
- **Motivation.** Every effect's result lands in `last_results` on the
  *next* tick. A builder that creates a Z3 node this tick can't use the
  node's handle until next tick. So the FSM emits the call now, records
  in a `pend` register where the result should go (push to the handle
  stack? store to `tmp`?), and next tick reads `last_results[0]` and
  routes it.
- **Structure.** `pend ∈ Int` set the tick the call is emitted; next tick
  `d_cap_int = match last_results[0] (IntResult(n)⇒n)` and the consumers
  branch on `_pend` (e.g. `hstk_in = _pend = 1 ? Cons(cap, _hstk) :
  _hstk`).
- **Example.** `compiler2/driver.ev:387-389` (`d_hstk_in`/`d_tmp_in`
  keyed on `_pend`), the `pend` assignment table (`:1063-1083`), and
  `d_cap_int` (`:295-300`). The header calls it "the PROVEN discipline:
  a builder's result is read from last_results on the NEXT tick."
- **Consequences.** (+) The only correct way to thread foreign return
  values through a tick-delayed world. (−) Every build is inherently
  two-phase (emit, then capture); a missed `pend` silently drops the
  result. (−) Forces "at most one capturing effect per tick" discipline.
- **Related.** Future/promise resolution; asynchronous callback; the
  classic delayed-load pipeline hazard.

### 4.8 Externalize-then-Reread (the two-tick round trip)

- **Intent.** Move a value across the model↔foreign boundary by writing
  it out via one effect tick and reading it back as a Result next tick.
- **Motivation.** A `String` payload in the model can't be stored in the
  FTI byte buffer directly; it must be `strdup`'d (libc returns a
  pointer next tick) and the pointer written into the buffer. Symmetric
  on read: a token's string payload is `__cstr.copy`'d from its pointer
  and arrives as a `StringResult`.
- **Structure.** Tick T emits `strdup`/`copy`; tick T+1's
  `last_results[0]` holds the pointer/string; a `pend`-like address
  register (or slot index) says where it goes.
- **Example.** `compiler2/lex_fti.ev:23-33` (the two-tick `strdup`
  idiom for string-carrying tokens); `compiler2/driver_window.ev:200-232`
  (the FETCH copy burst: `cp*` effects then `f_sr*` reads).
- **Consequences.** (+) Bridges the immutable-model / mutable-foreign gap.
  (−) Two ticks per crossing; (−) requires the *Slot-Aligned Filler*
  (§4.9) to keep batched crossings position-aligned.
- **Related.** Serialization/deserialization across a boundary; DMA
  bounce buffers.

### 4.9 Slot-Aligned Filler

- **Intent.** Keep result positions aligned with a fixed schema across a
  batch of heterogeneous effects by padding the "no-op" slots with a
  cheap dummy call, so `last_results[i]` is always meaningful for slot i.
- **Motivation.** When refilling the 8-token window, only the
  string-carrying slots need a `__cstr.copy`; the rest need nothing. But
  `last_results` is position-aligned to the `effects` Seq, so skipping a
  slot would shift every later result. The fix: emit a `getpid` filler
  (cheap, side-effect-free-enough) in every slot that doesn't need a real
  call, so slot i's result is exactly `last_results[i]`.
- **Structure.** `cp_i = needs_real(i) ? real_call(i) : LibCall("libc",
  "getpid", ⟨⟩)` for a fixed-width batch.
- **Example.** `compiler2/driver_window.ev:200-207` (`cp0..cp7`, getpid
  filler vs. `__cstr.copy`); the lexer's filler at
  `compiler2/driver.ev:1419` (`d_eff_filler`).
- **Consequences.** (+) Preserves a rigid result schema under sparse
  real work. (−) Wastes effect slots on dummies; (−) `getpid` as a
  "harmless" call is a convention, not a guarantee.
- **Related.** NOP-padding in VLIW/microcode; struct field alignment;
  the null-object pattern applied to effect slots.

### 4.10 Work-Item Micro-Step Interpreter

- **Intent.** Compile one source line into a tiny stack-machine program,
  then interpret it one instruction per tick against an operand stack —
  the program's central computational engine.
- **Motivation.** A line like `x = (a + b) * c` must become a tree of Z3
  builder calls, but each builder is a tick-delayed effect (§4.7) and the
  operands are handles produced by earlier builders. The clean encoding:
  lower the line to a postfix `C2Items` program; carry it as a cons-stack
  (§3.6); each tick pop the head item, emit its one builder call, and
  push/pop the `C2H` handle stack. Recursion (a `C2Process(expr)` item
  expanding into sub-items) is handled by *prepending* the expansion to
  the work list — the list is both program and continuation.
- **Structure.** `witems ∈ C2Items` (the program/continuation),
  `hstk ∈ C2H` (operand stack), `istep` (intra-item micro-step for
  multi-tick items), `pend`/`tmp` (capture plumbing). The per-item
  dispatch (`d_it_proc`/`d_it_op`/`d_it_app`/…) picks the action; the
  item's effect goes through the Mode Mux into `effects`.
- **Example.** `compiler2/driver_ir.ev:10-38` (the `C2Item` opcode set);
  `compiler2/driver_symtab.ev:104-213` (item decode + per-opcode
  discriminators); `compiler2/driver.ev:1002-1049` (the `witems`
  transition — the interpreter's fetch/expand/advance) and `:1421-1453`
  (the per-item builder selection `d_eff_lib`). A `C2Process` of a binop
  expands to `⟨Process(l), Process(r), Op(op), …tail⟩`
  (`:1026-1027`).
- **Consequences.** (+) Arbitrarily nested expressions compile correctly
  because every sub-expression is a handle, not text — the legacy
  text-concatenation compiler's "dropped compound argument" bug class is
  *impossible by construction* (driver header, `:7`, `:111`). (+) One
  uniform engine handles arithmetic, boolean, ctor-application, string
  ops, select, all of it. (−) One Z3 builder per tick → deep expressions
  are many ticks. (−) The opcode set + dispatch is large and grows with
  every language feature.
- **Related.** A bytecode VM / Forth inner interpreter; GoF *Interpreter*
  + *Command* (each `C2Item` is a command object); CPS (the work list as
  explicit continuation).

### 4.11 Postfix Stack-Compose

- **Intent.** Build a compound foreign AST as a postfix sequence of
  stack-manipulation items (dup/swap/rot + binary combiners), emitted as
  work items.
- **Motivation.** Some target shapes (the conditional-effects guard tree
  `(and (=> g B) (=> ¬g T))`) aren't a direct lowering of one surface
  node; they're a *rewrite* of stack contents. Rather than special-case
  them, the interpreter gets stack-shuffle opcodes and the rewrite is a
  fixed item sequence.
- **Structure.** Opcodes `C2Dup3`/`C2Swap`/`C2Rot3` plus combiners
  (`C2Not`, `C2Op(OpImpl)`, `C2Op(OpConj)`) emitted in postfix order
  over `C2H`.
- **Example.** `compiler2/driver.ev:263-268` (the D2 guard-fold spec:
  "Dup3 · Not · Swap · Impl · Rot3 · Impl · Swap · Conj"); the
  stack-op transitions at `:977-993`.
- **Consequences.** (+) Reuses the one interpreter for tree rewrites the
  parser can't express directly. (−) Stack choreography is write-only;
  the comment is the only spec. (−) Easy to get the shuffle wrong.
- **Related.** Forth/RPN; SSA-free stack-machine codegen; the
  "concatenative" programming style.

### 4.11b Bounded-Quantifier Re-Walk

- **Intent.** Implement `∀`/`∃` over a finite range or sequence by
  unrolling: re-parse/re-execute the body once per element under a loop
  counter, rather than emitting a real quantified formula.
- **Motivation.** A real Z3 `∀` over a user sequence is hard to build and
  often hard to solve; for the corpus's small bounded ranges, unrolling
  to a conjunction is simpler and stays in the decidable fragment. The
  body is parsed once to an `Expr`, then a loop walks the elements,
  substituting the bound variable (a numeral, or `(select seq i)`).
- **Structure.** A loop-state bank (`fl_on`/`fl_var`/`fl_v`/`fl_seq`/
  `fl_kind`) drives repeated emission; an in-scope bound-name leaf
  expands to the element value (§ shadowing in the symbol resolver).
- **Example.** `compiler2/driver_quant.ev` (the re-walk loop);
  `compiler2/driver.ev:804-834` (`d_vb_hit`/`d_vb_items` — bound-name
  expansion to numeral or `select`).
- **Consequences.** (+) Stays in an easy solver fragment; reuses the
  expression engine. (−) Unrolling is O(range) ticks and only works for
  statically bounded ranges. (−) Quantifiers in *expression* position
  aren't covered (driver header "NOT covered").
- **Related.** Loop unrolling; bounded model checking; CP's reified
  decomposition of a global constraint into a conjunction.

### 4.12 Single-Writer Effects Funnel

- **Intent.** Route every sub-machine's per-tick effect into one master
  `effects = …` constraint, because the kernel forbids two unconditional
  writers.
- **Motivation.** `effects = ⟨a⟩ ∧ effects = ⟨b⟩` is UNSAT — the kernel
  enforces a single unconditional writer. With ~15 machines each wanting
  to emit, the orchestrator owns the one `effects` ternary and selects
  each tick's rows from the lifted modules by mode/step.
- **Structure.** One top-level `effects ∈ Seq(Effect) = (cond ? rows :
  …)`, a priority ternary keyed on the same modes as the Mode Mux; each
  module exposes its candidate rows (e.g. `ze_*`, `rd_*`, `d_eff_lib`)
  as ordinary members that the funnel references.
- **Example.** `compiler2/driver.ev:1458-1538` — the master `effects`
  selector, with the comment "driver_main owns the single unconditional
  `effects = …` writer (the orchestrator's one output funnel)."
- **Consequences.** (+) Satisfies the single-writer rule; one place to
  see everything the program can do per tick. (+) Composable: a lifted
  module contributes rows, not writes. (−) The funnel is a 75-line
  ternary that every feature must thread through; (−) it centrally
  couples all machines' output timing.
- **Related.** The single-writer principle (Disruptor/LMAX); a hardware
  output bus arbiter; GoF *Mediator* (one object coordinates many).

## 5. Creational patterns

How Z3 objects and effects get built. In a normal language these would
be constructors and factory methods; here the "object" is a foreign Z3
AST/sort/decl, and "construction" is a tick-delayed libffi call (or a
sequence of them) whose result is captured into a register.

### 5.1 Build\* Effect Constructor

- **Intent.** Name one foreign-call shape as a reusable claim that
  constrains `eff` to a specific `LibCall`, so the FSM just selects
  *which* `Build*`'s `eff` to emit this tick.
- **Motivation.** A raw `LibCall("libz3", "Z3_mk_add", ⟨...⟩)` is noisy
  and easy to get wrong. A `Build*` claim captures the argument shape
  once and gives it a domain name; the kernel's whole "add a syscall =
  add a `BuildXyz` claim, no kernel change" extensibility story rests on
  this.
- **Structure.** `claim BuildFoo(args…, eff ∈ Effect) ; eff =
  LibCall("lib","fn", ⟨…⟩)`. The caller writes `BuildFoo(a ↦ …, eff ↦
  x)` and uses `x`.
- **Example.** `stdlib/kernel.ev:72-241` (`BuildPrintln`, `BuildTime`,
  the whole `BuildZ3*` lifecycle set); `stdlib/z3_ast.ev:38-127` (the
  AST-builder set); used throughout `compiler2/driver.ev:1259-1418`.
- **Consequences.** (+) Self-documenting, single source of truth per
  call shape, kernel-stable extensibility. (−) One claim per shape — the
  `Build*` namespace grows with the foreign surface. (−) The
  n-ary-array shapes still need caller-side marshaling (§5.4).
- **Related.** GoF *Factory Method* / *Builder*; a typed FFI binding
  layer; ASP's "fact" (a named, parameterized atom).

### 5.2 Staged Builder Machine

- **Intent.** A multi-tick sub-FSM that constructs a complex foreign
  object (a Z3 datatype/sort) through a fixed sequence of
  constructor/finalize/harvest micro-steps, latching the resulting
  handles into registers.
- **Motivation.** Declaring a Z3 datatype is not one call: you build
  symbols, per-variant constructors, a constructor list, call
  `mk_datatypes`, then *harvest* each variant's func_decl and tester and
  accessor handles back out. That is dozens of ordered, capture-bearing
  steps — a whole machine. It runs five times (four floor enums +
  one user enum), so it is parameterized, not inlined.
- **Structure.** An `(act, step)` micro-coordinate (declare / finalize /
  harvest phases), one effect per tick, captures keyed off the previous
  tick's `(act, step)`; a reusable arena (§3.5) for the intermediate
  arrays; results latched into a Harvest Register Bank (§5.3).
- **Example.** `compiler2/driver_enum.ev` — the `ed_act`/`ed_step`/
  `ed_src` walk; the act/step table is spelled out at `:36-55`. Driven
  via the Counter Hold (§4.4) from ZINIT.
- **Consequences.** (+) One machine declares *any* enum, floor or user.
  (+) Encapsulates a genuinely intricate foreign protocol. (−) The
  most complex single module; the act/step table is the only spec.
  (−) Reuse across five runs means its state must be reset between runs.
- **Related.** GoF *Builder* (a director walking a fixed construction
  sequence); a multi-phase compiler pass; staged initialization.

### 5.3 Harvest Register Bank

- **Intent.** After a staged build, latch the resulting foreign decls /
  values into a flat bank of named registers that serves as the runtime
  symbol table for that constructed type.
- **Motivation.** Once a datatype exists, the program constantly needs
  "the tester decl for variant V" and "the accessor for field F" to
  lower `matches` and field access. Those handles are harvested once and
  parked in a parallel-array register bank, looked up by name.
- **Structure.** Parallel banks `uev_n0..n5` (names), `uev_d0..d5`
  (ctor decls), `uev_t0..t5` (testers), accessors, plus a value table
  `evt_*` for nullary variants; floor types get dedicated registers
  (`z_irtest`, `z_srtest`, ...).
- **Example.** `compiler2/driver_enum.ev` (the `uev_*`/`evt_*` harvest);
  consumed at `compiler2/driver.ev:574-600` (ctor + tester resolution),
  `:771-779` (enum-value resolution).
- **Consequences.** (+) Constant-time (pure) resolution of variant
  machinery. (−) Fixed width (6 variants) caps "one user enum per
  compile, ≤6 variants." (−) Parallel arrays are the very "parallel Seqs"
  the style guide warns against — tolerated here for flat O(1) access.
- **Related.** A linker symbol table; a vtable; the Co-Traveling Handle
  Bank (§3.1) specialized to a constructed type.

### 5.4 Args-Array Marshaling

- **Intent.** Call an n-ary foreign function that takes a C array, when
  the effect grammar has no array shape, by writing operand handles into
  consecutive arena slots and passing the base pointer.
- **Motivation.** `Z3_mk_add(ctx, n, args[])` needs a pointer to `n`
  i64s. `LibArg` can't express an array. So the FSM writes each operand
  to `arena+0, +8, …` via `write_long` effects (themselves work items),
  then calls the builder with the base pointer.
- **Structure.** A run of `BuildMemWriteLong(addr ↦ base + 8*i, value ↦
  handle_i)` items, then the n-ary `Build*` with `args_arr_ptr ↦ base`.
  For binops this is the fixed `z_arena+200`/`+208` pair.
- **Example.** `stdlib/translate2_bool.ev:43-52` (the marshaling spec);
  `compiler2/driver.ev:1286-1289` (`d_eff_wl0`/`d_eff_wl1` write the two
  operand slots) feeding `d_eff_nary`/`d_eff_arith` (`:1273-1276`);
  ctor-arg marshaling via `CtorArgWriteStep` (`:1365-1370`).
- **Consequences.** (+) Bridges the array-shaped C ABI through a
  scalar-only effect grammar. (−) Several ticks per n-ary call (one
  write per arg). (−) Relies on the Offset-Partitioned Arena's temporal
  non-overlap invariant.
- **Related.** Manual `va_args` marshaling; FFI struct-by-pointer
  passing; the *element constraint* used as a write rather than a read.

### 5.5 Reify-to-Handle

- **Intent.** Lower every surface expression — boolean *and* arithmetic —
  to a first-class Z3 AST handle, so compound subexpressions are values
  that can't be dropped or mis-nested.
- **Motivation.** This is reification (§2) taken to the limit. The
  legacy compiler concatenated SMT-LIB *text*, and lost compound
  arguments (the `Exit(3+4)` bug). compiler2 instead *builds the AST*:
  `3 + 4` becomes a `Z3_mk_add` handle, and `Exit(3+4)` is
  `mk_app(Exit, handle)`. Every node is reified to a handle on the
  operand stack.
- **Structure.** The Work-Item Interpreter (§4.10) over `C2H`: a
  `C2Process(expr)` ultimately yields a handle; operators consume
  handles and push a handle; nothing is text until the final
  `solver_to_string` emit.
- **Example.** `compiler2/driver.ev:1-7` and `:106-111` (the design
  rationale — "compound args come free… the legacy dropped-Exit(3+4) bug
  class cannot exist here"); the entire `d_eff_lib` builder dispatch
  (`:1421-1453`).
- **Consequences.** (+) Correctness by construction for arbitrary
  nesting — the headline win of the whole compiler2 architecture.
  (+) Uniform: one mechanism for all expression kinds. (−) Every node
  costs a build tick. (−) Refcount discipline (inc_ref on capture) is a
  manual hazard (noted across the stdlib).
- **Related.** SMT/CP *reification*; AST-building vs. string-templating
  codegen; SSA value numbering.

---

## 6. Compositional patterns

How claims combine. This is the axis with the least prior art and the
most Evident-specific content — the language offers seven composition
forms, and compiler2 uses them as deliberate engineering tools with
different scoping/hiding semantics. This is the closest analog to GoF's
class-vs-object structural axis.

### 6.1 Names-Match Lift (`..Claim`)

- **Intent.** Splice a claim's entire body into the host scope sharing
  all names — no hiding, no renaming — to assemble one big FSM from many
  files.
- **Motivation.** `driver_main` is ~15 machines totaling 7,800 lines.
  Holding it in one file is unmanageable. `..DriverX` inlines each
  module's body verbatim; because the modules share the host's name
  space, a field one module defines (`zstep`, `st_cnt`) is directly
  visible to the others. The split is purely textual/organizational —
  semantically it is one flat claim.
- **Structure.** `..ClaimName` at the splice site; the callee's
  top-level memberships become the host's carried state-fields.
- **Example.** `compiler2/driver.ev:303-456` and throughout — the ~25
  `..DriverZInit` / `..DriverWindow` / `..DriverSymtab` / … splices that
  build `driver_main`; each module file is an `fsm DriverX` whose header
  documents what host fields it CONSUMES / PRODUCES / MAINTAINS.
- **Consequences.** (+) Modular *source* with a single shared state
  space — the only way to keep the mega-FSM legible. (−) No
  encapsulation whatsoever: every module can read/write every other's
  fields; name collisions are silent. (−) The CONSUMES/PRODUCES header
  comments are the *only* interface contract.
- **Related.** C `#include`; a mixin / trait with open recursion; GoF
  has no direct analog (OO always hides).

### 6.2 Carry-Preserving Composition (state composition)

- **Intent.** Recognize that because `..`-lifted modules carry their own
  `_x` state, *composition is also state composition* — the host tick
  advances all lifted machines simultaneously.
- **Motivation.** This is the consequence that makes §6.1 powerful rather
  than merely textual: each module is a little FSM with its own carried
  registers, and splicing them means their transition functions all fire
  every host tick, coordinated only by the shared Mode Mux (§4.5).
  Composition of claims *is* parallel composition of state machines.
- **Structure.** Lifted modules declare carried fields (`x = is_first_tick
  ? … : _x`); the host's single tick re-solves all of them at once; only
  the active mode's writes are non-identity.
- **Example.** The interplay of `compiler2/driver_zinit.ev` (`zstep`),
  `compiler2/driver_compose.ev` (`il_*` frame stack), and
  `compiler2/driver_window.ev` (`fmode`/`wtoks`) — all advancing each
  tick, gated by mode so dormant machines hold.
- **Consequences.** (+) Free concurrency: N machines, one clock.
  (−) Every dormant machine still costs solver variables every tick.
  (−) A machine that forgets to hold (`: _x`) corrupts state even while
  "inactive."
- **Related.** Synchronous-language parallel composition (Esterel/Lustre);
  hardware modules on a shared clock; the Actor model with one scheduler.

### 6.3 Slot-Bind Inline with α-Prefix (`Helper(slot ↦ value)`)

- **Intent.** Inline a claim *with* hiding: bind its parameters to
  caller expressions and α-rename its locals under a per-call-site prefix
  so they can't collide — a real call with argument passing.
- **Motivation.** Unlike `..` (names-match, no hiding), a slot call needs
  the callee's body locals to be private to that call (so the same
  helper can be inlined twice). The compose machine renames every callee
  local to `"__cN_" ++ name` and binds the slots to caller handles.
- **Structure.** A call-frame stack (`C2Frames` = return cursor + saved
  prefix + saved binds), a binds table (`C2Binds` = slot→handle), and an
  α-prefix (`il_pfx`); the jump sets the cursor into the callee body, the
  body end pops the frame.
- **Example.** `compiler2/driver_compose.ev` in full — `il_frames`/
  `il_binds`/`il_pfx`/`il_depth` (`:40-45`), the slot-call walk
  (`cw_*`, `:138-185`), the α-prefix `"__cN_"` (`:186`), frame
  push/pop (`:241-283`). Bare `Name` / `..Name` splices *keep* the
  caller scope (`cw_bare`); only slot calls install fresh binds+prefix.
- **Consequences.** (+) True subroutine inlining with private locals and
  argument passing, depth ≤8. (−) The most intricate control machine in
  the program (a manual call stack over constraints). (−) Caps (4 slots,
  depth 8) are hand-set; widening is mechanical but touches many peel
  views.
- **Related.** Procedure inlining with hygienic renaming; macro hygiene
  (α-conversion); a software call stack reified as data.

### 6.4 No-Redeclare Resolution (idempotent splice)

- **Intent.** Make re-inlining safe: a names-match splice that
  re-mentions an already-declared (or bound) name resolves the existing
  handle instead of redeclaring it.
- **Motivation.** Because `..` shares scope, a spliced body that
  declares `count` when the host already has `count` would emit a
  duplicate Z3 const (a malformed unit). The classifier checks the
  symbol table (and the current frame's binds) first; if the scoped name
  already exists, it resolves rather than declares — which is exactly
  what makes idempotent re-splicing of shared claims correct.
- **Structure.** `c_dup = index_of(st_names, scoped_key) ≥ 0`;
  `c_bnd = name ∈ current frame binds`; `c_nodecl = c_bnd ∨ c_dup`; the
  membership head becomes a *resolve* (`C2Process(EIdent …)`) instead of
  a `C2DeclConst`.
- **Example.** `compiler2/driver_classify.ev:214-233` (`c_nodecl`,
  `c_mem_head`); the header note "what keeps names-match splices
  (`..Base`, bare `IsPositive`) from emitting duplicate
  declares/manifest fields/carry lines."
- **Consequences.** (+) Safe composition of overlapping claims; the
  enabling rule for §6.1/§6.3 to coexist. (−) Subtle: the
  declare-vs-resolve decision is buried in the classifier and depends on
  the scoped-name key construction. (−) Silent if the scoping prefix is
  computed wrong.
- **Related.** Idempotent `CREATE IF NOT EXISTS`; the diamond-import
  problem; ASP's set semantics (re-deriving a fact is harmless).

### 6.5 Pass-Claim Dispatch Table

- **Intent.** Factor per-node-kind translation into independent "pass"
  claims, each owning the single decision "which libcall builds *this*
  node family," selected at the call site by a discriminator.
- **Motivation.** Arithmetic, comparison, n-ary boolean, ternary,
  string-op, and ctor-app nodes each map to a different Z3 builder. Rather
  than one giant function, each family is a claim (`ArithBinopBuildZ3`,
  `BoolCmpBuildZ3`, `BoolNaryBuildZ3`, `StrOpBuildZ3`, …) that constrains
  `eff` for its case; the interpreter dispatches by item/op kind.
- **Structure.** One claim per node family, each `(inputs…, eff, ok)`;
  the work-item interpreter binds the right one per `d_it_*`/`d_op`
  discriminator and routes its `eff` through the funnel.
- **Example.** `compiler2/translate2_bool.ev` (`BoolCmpBuildZ3`,
  `BoolNaryBuildZ3`, `TernaryBuildZ3`, `BoolNotBuildZ3`);
  `compiler/translate_arith.ev` (`ArithBinopBuildZ3`);
  `compiler2/translate2_seq.ev` (`StrOpBuildZ3` — a name-keyed sub-table);
  bound together at `compiler2/driver.ev:1421-1453`.
- **Consequences.** (+) Each node family is a small, independently
  testable unit owning one concern ("which libcall"). (+) Mirrors the
  legacy text renderers one-to-one, easing oracle comparison. (−) The
  caller still hand-wires the dispatch; adding a node kind touches both
  the pass claim and the interpreter's mux.
- **Related.** GoF *Strategy* (one strategy per node kind) + *Visitor*
  (the interpreter visits nodes, dispatching to pass claims); a
  syntax-directed translation scheme.

---
