# The loop-functionizer + stack-of-FSMs

> A third functionizer strategy: compile **one** FSM step (which we
> already do — that's what makes Mario fast) and wrap it in a real
> native `while` loop that runs the step until halt. Pair it with an
> **explicit work-stack carried in the FSM's state** so a recursion
> over a tree becomes an iteration over an explicit stack. Together
> these self-host the tree-walking passes (`pretty`,
> `subscriptions::walk_expr`, `validate::find_ffi_call`) that the
> recursion gap (`examples/COUNTEREXAMPLES.md` #15) currently blocks —
> **without** adding functional recursion / `define-fun-rec` to the
> constraint language, and **without** the symbolic unrolling that Z
> proved fails on branching bodies.
>
> Companion reading: [`fsm-halts-within.md`](fsm-halts-within.md) (CC's
> symbolic unroll — the sibling strategy), [`cegar-scaffolding.md`](cegar-scaffolding.md)
> (the abstraction/oracle framing this slots into),
> [`nested-fsm-strategies.md`](nested-fsm-strategies.md) (the run-to-halt
> strategy selector — this loop-functionizer **is** its tier 2, sitting
> between symbolic-unroll and the blocking-interpret baseline),
> [`../perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md)
> (Z's branching-wall measurement), and [`../self-hosting.md`](../self-hosting.md)
> (the blocked ports this unlocks).

## § 1 — The three strategies

There are three ways to make an FSM's repeated step do useful work
without the runtime simply ticking it through the multi-FSM scheduler.
They differ in **where** the repetition happens (compile time vs. run
time), **what** they produce (a constraint vs. a value), and **which
bodies** they survive.

| Strategy | Mechanism | Repetition happens | Iteration count | Produces | Works on | Status |
|---|---|---|---|---|---|---|
| **Symbolic unroll** (CC) | Compose the step with itself N× via Z3 substitution, simplify-collapse between doublings | **Compile time**, in Z3, symbolically | Static, bounded N | A Z3 *constraint* (`halt_aggregate`) you assert + check SAT/UNSAT | **Affine bodies only** — branching plateaus at ratio ≈ 2.0 (Z) | landed (`fsm_unroll/`, `halts_within(F, N)`) |
| **Loop-functionizer** (THIS doc) | Functionize **one** step, wrap in native `while !halt { state = step(state) }` | **Run time**, in native code, concretely | Dynamic, runtime-determined | A *value* (the final state / accumulated output) | **Branching OK** — the branch is one `ite` in the step, evaluated per iteration at native speed | to design |
| **Functional recursion** (`define-fun-rec`) | Z3 native recursive function definitions | Compile time, in Z3, as a recursive symbol | n/a (recursion depth) | A Z3 recursive function | tree descent | **rejected direction** — see § 2 |

### Why the loop-functionizer survives branching where symbolic unroll can't

This is the load-bearing distinction, and it is *entirely* about where
the repetition lives.

**Symbolic unroll asks Z3 to expand the composition.** To build `F^N`,
CC substitutes `F`'s output expressions back into its inputs and
simplifies — `F^2`, `F^4`, … (`fsm_unroll/compose.rs::double`). For an
*affine* body (`count_next = count - 1`) the composed transition folds
to closed form: `count - 1` composed with itself is `count - 2`, still
three AST nodes regardless of N. But for a **branching** body — one
whose update is `ite(cond(state), …, …)` — each composition nests the
*next* step's `ite` tree inside the *previous* step's branches, and the
branch conditions are data-dependent on the symbolic state, so Z3's
simplifier cannot fold them. The formula grows ~2× per doubling. Z
measured exactly this (`../perf/log-unroll-feasibility.md`):

| shape | n=1 | n=2 | n=4 | n=8 | n=16 | n=32 | n=64 | tail ratio | verdict |
|---|---|---|---|---|---|---|---|---|---|
| pure counter | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 1.00× | flat — collapses |
| linear recurrence | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 1.00× | flat — collapses |
| Fibonacci | 3 | 6 | 11 | 11 | 11 | 11 | 11 | 1.00× | flat — collapses |
| **conditional update** | 6 | 9 | 15 | 27 | 51 | 99 | 195 | **1.97×** | **linear — no win** |
| **3-state machine** | 9 | 16 | 33 | 69 | 141 | 285 | 573 | **2.01×** | **linear — no win** |
| **real Mario `game`** (goal-level) | — | — | — | — | — | — | — | **1.98×** | **linear — no win** |

CC's affine-step detector (`fsm_unroll/detector.rs`) refuses cleanly the
moment the post-doubling node-count ratio stays above 1.5 by `F^8` — an
honest "I can't prove this via log-unroll" rather than a 30-second
solver blow-up. That refusal is a *wall*: the entire branching half of
the table is off-limits to symbolic unroll.

**The loop-functionizer never composes symbolically.** It compiles
**one** step — one `ite` tree, fixed size, exactly what
`functionize/cranelift.rs` already emits for a Mario component — and
runs it inside a real `while` loop. The branch is *evaluated* (taken or
not) once per iteration on concrete data, not *expanded* into a growing
formula. A branching step costs O(1) per iteration; N iterations cost
O(N) total, with the constant being a native machine-code step (~µs),
not a Z3 solve. Z3 is **never asked to unroll**. Z's branching wall is a
wall about *formula growth under symbolic composition* — and the loop
does no symbolic composition at all. The cost of a branch drops from
"the formula multiplies" to "a conditional jump."

The trade is exactly the usual prove-vs-run trade. Symbolic unroll
gives you a *closed-form constraint* over **all** initial states
(`∃ k ∈ [1,N] : halt_k`, suitable for verification) but only for affine
bodies. The loop-functionizer gives you the *concrete result* for **one**
initial state, for any body, but tells you nothing about the others. See
§ 6 for when to pick which.

## § 2 — Why not functional recursion

The obvious-sounding fix for "Evident can't walk a tree" is to add
recursive functions to the constraint language — Z3's `define-fun-rec`,
or a fold/catamorphism primitive. We are **not** taking that path.

**Evident's primitives are FSMs + external constraint evaluation, not a
call stack.** A claim is a set of constraints over a set; an FSM is a
state-transition relation the scheduler ticks. There is no call stack
the runtime manages, and nothing in the execution model wants one. A
recursive function needs a stack — for activation records, for pending
continuations, for the partial results of sub-calls. Rather than ask the
constraint language to grow that machinery, we make the stack **explicit
data in the FSM's state** (§ 4) and let a native loop drain it. The
recursion becomes an ordinary `Seq`-valued field plus a `while`.

`define-fun-rec` remains a *possible future option*, but it is not the
chosen path, for two concrete reasons:

1. **It would inject unbounded recursion into the constraint language.**
   Today every claim is a finite, non-recursive term that the
   translate → simplify → functionize pipeline can fully expand and
   reason about. A recursive function symbol changes that: the solver's
   termination and decidability story shifts (recursive definitions in
   SMT are semi-decidable at best), `simplify_assertions` would have to
   cope with recursive declarations, and the functionizer would need a
   codegen story for a recursive symbol. That is a large, invasive
   change to the kernel of the language for the sake of one feature.

2. **It throws away the step-functionizer we already have.** The
   loop-functionizer is purely *additive*: it reuses
   `functionize/cranelift.rs` verbatim to compile the step, and adds
   only a thin native loop on top (§ 3). No change to `translate/`, no
   change to the constraint semantics, no new solver obligation. We get
   tree-walking for the price of a loop wrapper, not a language
   redesign.

The framing to keep in mind: **the stack the language would otherwise
have to manage becomes explicit data the runtime manages.** That is the
whole move, and it is the same move that makes an iterative
tree-traversal possible in any language without recursion — a manual
work-stack. We are doing it one level down, in the FSM state.

## § 3 — The loop-functionizer mechanism

### We already compile one step

The expensive thing — turning an FSM's `state → state'` transition into
native code — is **already done**. `functionize/cranelift.rs::compile_program`
takes a `Z3Program` (the simplified, extracted body of one claim /
component) and emits a `JitProgram` whose calling convention is

```text
extern "C" fn(inputs: *const Value, outputs: *mut Value, pool: *const Value, bail: *mut i64)
```

wrapped behind the trait the rest of the runtime sees:

```rust
// core/functionizer.rs
pub trait CompiledFunction {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>>;
}
```

`call(given)` packs the input bindings, runs the JIT'd step, and returns
the output bindings — or `None` if the step hit an unsupported shape (in
which case today's runtime falls through to a full Z3 solve). This is
what makes Mario's per-tick `display` / `game` / `keyboard` solves run as
JIT calls (~µs) instead of Z3 solves; per-component JIT took the `game`
component from ~16 ms to ~2 ms.

The loop-functionizer adds nothing to this. It *consumes* it.

### The new artifact: a `CompiledFunction` that loops

The loop-functionizer produces a `CompiledFunction` whose `call` runs, in
native code (Rust driving the JIT'd step):

```text
let mut state = initial;            // from `given`
while not halted(state) {
    state = step(state);            // the already-compiled step
    // (optional) push children onto a work-stack / accumulate output
}
return state;                       // the final state / accumulated output
```

`step` is the existing `CompiledFunction` from
`functionize/cranelift.rs`; the loop is the only new code.

### It is a *wrapper*, not a fresh `Functionizer` impl

The `Functionizer` trait is shaped for a **single solve**:

```rust
fn compile(&self, program: &Z3Program, enums: &EnumRegistry, datatypes: &DatatypeRegistry)
    -> Option<Rc<dyn CompiledFunction>>;
```

One `Z3Program` in, one callable out. The loop-functionizer needs
*more* than a single program: it needs the step program **plus** the
iteration contract — which outputs feed back as next-tick inputs (the
state pairing `x` ↔ `x_next`), which Bool signals halt, and the safety
cap. That contract isn't expressible as a `Z3Program`.

**Recommendation: implement it as a wrapper `CompiledFunction`, not as a
new `Functionizer`.** The wrapper takes the step's already-compiled
`CompiledFunction` and a loop contract, and itself implements
`CompiledFunction`. Rationale:

- **Zero duplication.** A fresh `Functionizer` impl would have to
  re-derive the step from the `Z3Program`, duplicating everything
  `cranelift.rs` does. The wrapper reuses 100% of step compilation.
- **Clean composition.** "An abstraction built on a cheaper
  abstraction" is exactly the CEGAR shape (`cegar-scaffolding.md` § 1):
  Cranelift compiles the step; the loop wrapper drives it; the result is
  still a `CompiledFunction`, so it plugs into the existing query path
  and the CEGAR oracle/refiner loop without redesign (§ 6).
- **The state-pairing detector already exists.** CC's
  `fsm_unroll/compose.rs::detect_state_pairs` already finds the
  `(name, name_next)` pairs in a claim body — the loop wrapper reuses it
  to build the contract.

The proposed interface:

```rust
/// The iteration contract a loop needs beyond a single Z3Program.
pub struct LoopContract {
    /// (input_name, output_name) per threaded accumulator var. After
    /// each step the output value is copied to the input slot for the
    /// next iteration. From `detect_state_pairs`. E.g.
    /// [("reads","reads_next"), ("writes","writes_next")], or
    /// [("count","count_next")] for the counter.
    pub state_pairs: Vec<(String, String)>,

    /// Optional work-stack driving (the stack-of-FSMs case, § 4). When
    /// set, the loop owns a native Vec<Value> work-stack: each
    /// iteration pops one item, binds it to `item_in`, runs the step,
    /// and pushes every element of the step's `children_out` Seq.
    /// Halts when the stack drains.
    pub work_stack: Option<WorkStack>,

    /// Optional body-Bool halt (CC's `halt ∈ Bool` convention). When
    /// set, the loop also halts the moment the step emits halt = true.
    pub halt_var: Option<String>,

    /// Hard cap. Overrun is a loud diagnostic, not a silent answer
    /// (see "Termination" below). Default from EVIDENT_LOOP_MAX_ITERS,
    /// or derived from input size for the work-stack case.
    pub max_iters: u64,
}

pub struct WorkStack {
    pub item_in:      String,  // step input slot the popped item binds to ("node")
    pub children_out: String,  // step output Seq of children to push ("children")
    pub seed_from:    String,  // `given` Seq the stack is seeded from (⟨root⟩)
}

pub struct LoopFn {
    step:     Rc<dyn CompiledFunction>,   // produced by the existing functionizer
    contract: LoopContract,
}
```

and the `call`, in full enough detail to implement:

```rust
impl CompiledFunction for LoopFn {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let mut state = given.clone();                 // accumulator state
        let mut work: Vec<Value> = match &self.contract.work_stack {
            Some(ws) => seq_elems(given.get(&ws.seed_from)?),  // seed ⟨root⟩
            None     => Vec::new(),
        };

        for _ in 0..self.contract.max_iters {
            // termination by drained work-stack
            if let Some(ws) = &self.contract.work_stack {
                if work.is_empty() { return Some(state); }
                state.insert(ws.item_in.clone(), work.pop().unwrap());
            }

            let out = self.step.call(&state)?;         // None → step bailed; see below

            // body-Bool halt (CC convention)
            if let Some(h) = &self.contract.halt_var {
                if out.get(h) == Some(&Value::Bool(true)) { return Some(out); }
            }

            // push children (work-stack case)
            if let Some(ws) = &self.contract.work_stack {
                for child in seq_elems(out.get(&ws.children_out)?) { work.push(child); }
            }

            // thread accumulator outputs → next inputs
            for (inp, outp) in &self.contract.state_pairs {
                if let Some(v) = out.get(outp) { state.insert(inp.clone(), v.clone()); }
            }
        }
        loop_overrun_diagnostic(self.contract.max_iters);  // loud; then:
        None
    }
}
```

### Halt detection: two sources, one convention

The loop terminates on **either** signal — they cover the two worked
examples cleanly:

1. **Body-Bool halt** (CC's convention, `fsm-halts-within.md`). The step
   declares `halt ∈ Bool` in its body; the loop checks `out[halt]` each
   iteration. This is the *value-iteration* case — a counter
   (`halt = count ≤ 0`), an accumulator that converges. The same `halt`
   that `halts_within(F, N)` reads, reused as a run-time loop guard.
2. **Empty work-stack** (§ 4). When the loop owns a work-stack, it halts
   when the stack drains — no body Bool needed; "no more subtrees to
   visit" *is* the halt.

CC reads `halt` on the tick's *input* state, so a body Bool halts one
iteration after the terminal state is produced — harmless for an
evaluator. A body that wants to halt a step earlier writes `halt` over
`x_next` (the output state) instead.

### Termination / safety: the max-iteration guard

A non-halting FSM — a malformed step that never drains its stack, a
`halt` that never goes true — must **fail loudly, not hang**. The guard
is the configurable `max_iters` (the run-time analogue of CC's static
N). On overrun, emit a clear diagnostic (`loop-functionizer: <claim> did
not terminate within <N> iterations; work-stack still has <k> items` —
gated trace; § 8) and stop.

**What "stop" means depends on the class, and this is a real difference
from the JIT's `None` contract.** Today, a `CompiledFunction` returning
`None` means "I decline; Z3 is the authoritative fallback." For a
*value-iteration* loop (a counter), Z3 genuinely can answer, so overrun
→ `None` → Z3 is a sound fallback. But for a *tree-walk* loop, Z3
**cannot** serve as the oracle — recursion over an unbounded tree is
exactly the recursion gap (COUNTEREXAMPLES #15) that motivated this
whole design. So for the recursive class, overrun is a genuine bug in
the FSM (a dispatch arm that fails to shrink the stack), and the loud
diagnostic *is* the failure surface; falling through to Z3 would just
produce the unconstrained-garbage answer the recursion gap is infamous
for. The wrapper therefore distinguishes:

- **Step bail mid-walk** (`self.step.call(...)` returns `None`): an
  unsupported *shape* in one step — propagate `None`, Z3 re-solves that
  one step correctly (it's non-recursive).
- **Loop overrun** (`max_iters` exhausted): a *termination* failure —
  loud diagnostic; `None` only as the trait's required return, with the
  understanding that for the recursive class no fallback can help, so
  the query fails visibly rather than silently.

## § 4 — Stack-of-FSMs: recursion → iteration

This is the section everything downstream depends on. The claim is
simple: **a recursive tree-walk is an iteration over an explicit
work-stack**, and the loop-functionizer drives that iteration. The
recursion's implicit call stack becomes an explicit `Seq`; the
recursion's return values become an explicit accumulator threaded
through the FSM state.

### The conceptual model

Conceptually, the FSM's state carries two pieces of explicit data that a
recursive function would keep implicit:

- **`stack ∈ Seq(WorkItem)`** — the work-stack: subtrees still to
  process (plus, in the general case, any pending continuation). This is
  the recursion's *call stack*, made explicit.
- **the accumulator** — the partial result built so far. This is the
  recursion's *return values*, made explicit and threaded.

One step:
1. **Pop** the top `WorkItem`.
2. **Dispatch** on its node variant.
3. **Push** the node's children as new `WorkItem`s.
4. **Fold** the node's contribution into the accumulator.

The native loop drains the stack; when it's empty, the accumulator is
the answer. This reproduces the recursive walk's visit set exactly:
pushing a node's children and later popping them visits the same nodes
the recursion would descend into; an order-independent accumulator (a
set union) makes visit *order* irrelevant.

### Recommended realization: the wrapper owns the work-stack

The conceptual "stack in the FSM state" can be realized two ways, and
the implementer should pick the efficient one:

- **(A) Marshal the whole stack through the step each iteration** —
  `state = {stack, acc}`, step is `state → state'`, the step pops/pushes
  *inside* its body. Faithful to "stack is state," but it (i) needs an
  in-step `Seq` tail/drop operation the runtime doesn't clearly have,
  and (ii) re-marshals the entire stack `Value` through `given`/bindings
  every iteration — O(n²) for an n-node walk (§ 8).
- **(B, recommended) The loop wrapper holds the work-stack as a native
  `Vec<Value>`** (the `WorkStack` field of § 3). The step is then a pure
  *per-node* function: given **one** popped node + the accumulator,
  return the node's **children** + the updated accumulator. No in-step
  slicing, O(n) total marshaling. Semantically identical to (A) — the
  stack is still explicit, still drained by the loop; it just lives in
  the driver instead of being marshaled through Z3-value-land each tick.
  The accumulator still threads through the step's state-pairs.

(B) is the design the rest of this section uses.

### Resolved under tier 3 (session MM): option A works, on an enum spine

> **The (A)-vs-(B) question, answered for tier 3.** Tier 3
> (blocking-interpret, `nested-fsm-strategies.md` §2) *is* the scheduler:
> there is no native loop wrapper to hold a `Vec`, so a tier-3 stack-FSM
> has no choice but option **A** — the stack lives in the FSM state and
> is marshaled whole through the per-tick solve. Session MM built the
> toy this section's plan calls for (`examples/test_36_sum_tree.ev`,
> sum-a-tree under `run(sum_tree, init)`) and the result is:
>
> **Option A works under tier 3 — but not on a `Seq(T)` stack.** The
> in-step `Seq` tail/drop this section (and §8) flagged as the suspected
> weak point is *confirmed missing*: `Seq(T)` only flattens static
> literals at load time; a constraint body cannot pop a `Seq`'s tail or
> cons onto an opaque `Seq` (`examples/COUNTEREXAMPLES.md` #19a). The fix
> is **not** a Seq op — it is to encode the work-stack as a **recursive
> enum cons-list** (`enum Stack = Empty | Push(Tree, Stack)`), where
> **pop** is a `match`-destructure (head + tail fall out together) and
> **push** is a constructor call (`Push(l, Push(r, rest))`). Both lower
> cleanly. The stack is still explicit FSM state, drained tick-by-tick;
> only the substrate changes from `Seq` to enum spine. The O(n²)
> whole-state marshaling §8 predicted for option A is real but fine at
> toy/AST scale.
>
> So: **tier 3 can host a stack-FSM (option A) on a recursive-enum
> stack.**
>
> **Update (session NN): tier 3 now hosts a *real* composite-state
> tree-walk — the cons-list workaround for composite I/O is no longer
> required.** The composite-`init` seeding (`run(F, Node(...))`,
> `run(F, ⟨root⟩)`) and composite *final-state return* (a nested-enum /
> `Seq` accumulator) the `walk_step` self-host needs **landed in the
> tier-3 surface** (`runtime/nested.rs` / `effect_loop/nested.rs`), along
> with nested-constructor deep-matching — COUNTEREXAMPLES #19b/#19c/#19d
> are struck. `examples/test_37_tree_walk.ev` is the proof: a
> variable-arity rose-tree walked under `run()`, **seeded with a composite
> tree and returning a composite label-list**, the agenda transition
> deep-matching `ACons(NLCons(node, more), rest)`. So tier 3 no longer has
> to drain into a flat `Done(Int)` or rebuild the tree from a bare-Int
> selector (what `test_36` did) — it round-trips real composite state. The
> one fact still standing is #19a (`Seq(T)` has no in-step pop/tail/cons,
> and a `Seq` payload can't be bound from a `match`), so the work-stack
> rides a recursive-enum cons-list spine rather than a `Seq`. **Tier 2's
> native-`Vec` wrapper (option B) remains the path that would hold a
> literal `Seq` stack natively** and skip the marshaling round-trip — but
> it is now an *optimization* of a working tier-3 walk, not a prerequisite
> for composite I/O. Full write-up + the runtime constraints:
> `examples/COUNTEREXAMPLES.md` #19.

### Worked example: `subscriptions::walk_expr`, end to end

`subscriptions::walk_expr` (`runtime/src/subscriptions.rs`) is the
simplest real walk: it accumulates the set of `world.X` field reads and
`world_next.X` field writes referenced anywhere in a claim body. It's the
right first target precisely because the accumulator is a **set union** —
commutative and associative, so there is no output-ordering headache
(contrast `pretty`, § 8). The Rust recursion is:

```rust
fn walk_expr(e: &Expr, sets: &mut AccessSets) {
    match e {
        Expr::Identifier(name) => { /* world.X → read, world_next.X → write */ }
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) => for x in es { walk_expr(x, sets) },
        Expr::Binary(_, a, b) | Expr::Range(a,b) | Expr::InExpr(a,b) | Expr::Index(a,b)
            => { walk_expr(a, sets); walk_expr(b, sets); }
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => { walk_expr(r, sets); walk_expr(b, sets); }
        Expr::Call(_, args) => for a in args { walk_expr(a, sets) },
        Expr::Cardinality(i) | Expr::Not(i) => walk_expr(i, sets),
        Expr::Field(recv, _) => walk_expr(recv, sets),
        Expr::Ternary(c, t, f) => { walk_expr(c, sets); walk_expr(t, sets); walk_expr(f, sets); }
        Expr::Match(scrut, arms) => { walk_expr(scrut, sets); for a in arms { walk_expr(&a.body, sets) } }
        Expr::Matches(e, _) => walk_expr(e, sets),
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}  // leaves
    }
}
```

**The WorkItem type.** Because every child of an `Expr` is itself an
`Expr`, the work-stack is homogeneous: `WorkItem = Expr`, stack is
`Seq(Expr)`. (`EMatch` is the one wrinkle — its children are the arm
*bodies*, which must be extracted from `Seq(MatchArm)`; see the note
below.) No continuation is needed because the accumulator is
order-independent.

**The step (one node → its children + folded accumulator).** Using
`stdlib/ast.ev`'s `Expr` enum:

```evident
import "stdlib/ast.ev"

-- One step: dispatch on `node`, return its children to push and the
-- accumulator with this node's contribution folded in. The wrapper
-- owns the work-stack (design B); `reads`/`writes` thread via state-pairs.
claim walk_step
    node ∈ Expr                         -- the popped work-item (wrapper supplies)
    reads,  reads_next  ∈ Set(String)   -- threaded accumulator (state-pair)
    writes, writes_next ∈ Set(String)   -- threaded accumulator (state-pair)
    children ∈ Seq(Expr)                -- children to push (wrapper consumes)

    children = match node
        ESetLit(es)         ⇒ es
        ESeqLit(es)         ⇒ es
        ETuple(es)          ⇒ es
        ECall(_, args)      ⇒ args
        ERange(a, b)        ⇒ ⟨a, b⟩
        EInExpr(a, b)       ⇒ ⟨a, b⟩
        EIndex(a, b)        ⇒ ⟨a, b⟩
        EBinary(_, a, b)    ⇒ ⟨a, b⟩
        EForall(_, r, bdy)  ⇒ ⟨r, bdy⟩
        EExists(_, r, bdy)  ⇒ ⟨r, bdy⟩
        ETernary(c, t, f)   ⇒ ⟨c, t, f⟩
        ECardinality(e)     ⇒ ⟨e⟩
        ENot(e)             ⇒ ⟨e⟩
        EField(recv, _)     ⇒ ⟨recv⟩
        EMatches(e, _)      ⇒ ⟨e⟩
        EMatch(scrut, arms) ⇒ ⟨scrut⟩ ++ arm_bodies(arms)   -- see note
        _                   ⇒ ⟨⟩        -- EIdentifier / EInt / EReal / EBool / EStr: leaves

    -- Only an identifier contributes to the sets. The prefix test
    -- (`world.X` → read, `world_next.X` → write) is the classifier
    -- already ported in stdlib/passes/subscriptions.ev.
    reads_next  = (node matches EIdentifier(_) ? add_read(reads,  node)  : reads)
    writes_next = (node matches EIdentifier(_) ? add_write(writes, node) : writes)
```

**The loop reproduces the recursive walk's leaf set.** The wrapper seeds
the work-stack with `⟨root_expr⟩` (the claim-body expression) and an
empty `reads`/`writes`. Each iteration pops a node, the step returns its
children + the folded accumulator, the wrapper pushes the children. When
the stack drains, `reads`/`writes` hold exactly the union the recursion
would have produced — every `EIdentifier` is popped exactly once, and
set-union is order-insensitive, so LIFO drain = recursive descent for
this accumulator. Equivalence is already pinned byte-for-byte by
`runtime/tests/subscriptions_equivalence.rs` against every FSM-shaped
claim in `examples/` (including Mario's three FSMs); the loop-functionized
walk must match it.

> **`EMatch` note.** Its children are the arm *bodies*
> (`Seq(MatchArm)` → `Seq(Expr)`), so `arm_bodies` is a small map. Two
> honest options: (a) a helper that maps `MakeMatchArm(_, body) ⇒ body`
> over the arm Seq (clean if Seq-map is supported in a step body), or
> (b) widen the work-stack to a heterogeneous `WorkItem = WExpr(Expr) |
> WArm(MatchArm)` so popping a `WArm` pushes its body. (b) is the
> general answer for trees with mixed child types (§ 8); (a) keeps this
> example homogeneous.

### "The stack is a stack of FSMs, not a stack of claims"

The unit pushed and popped is an **FSM activation** — a state (the
subtree) plus what it's resuming (for ordered walks, its continuation) —
**not** an inlined claim body. That distinction is the entire reason
this avoids the recursion gap.

Recall the failed approach the recursion gap documents (COUNTEREXAMPLES
#15, `self-hosting.md` Gap #1): try to self-host `pretty` by having the
claim `pretty(l)` **recursively inline its own body**. The runtime does
bounded inlining (depth-capped at `EVIDENT_MAX_INLINE_DEPTH=64`), but
**the inlined frames' outputs are left unconstrained** — `pretty(l)`'s
result is a free Z3 variable that Z3 fills with *whatever value
satisfies*, so both the correct rendering and arbitrary garbage come
back SAT. A claim call nested in an expression (`out = pretty(l) ++ …`)
is silently dropped entirely. Claim-inlining tries to make Z3 *represent*
the whole recursion as one giant symbolic term with free leaves — and
the leaves are unconstrained, so the answer is meaningless.

The explicit-stack FSM avoids this because **the output is threaded
through the FSM state across iterations, never left free for Z3 to
fill**. There is no "inlined frame whose result Z3 must guess." Each
iteration's outputs (`children`, `reads_next`, `writes_next`) are
*fully constrained* by the step body as a total function of that
iteration's inputs. The loop carries the concrete accumulator value from
one iteration to the next. Z3 (via the compiled step) is only ever asked
the **non-recursive** question "given *this* node and *this* accumulator,
what are the children and the next accumulator?" — a finite, fully
constrained, JIT-able function. The recursion is unrolled by the native
`while`, with the partial result living in a `Set`/`Seq`/`Vec` in the
state, not in unconstrained Z3 variables.

That is why it's a stack of FSMs: each stack entry is a pending FSM
activation (a subtree + its resume point), and the loop runs them one at
a time — exactly the multi-FSM scheduler's "state in, state out,
threaded" coordination model (`docs/design/multi-fsm.md`), driven
depth-first by a LIFO stack instead of round-robin by subscriptions.
Claim-inlining is "ask Z3 to be the call stack and guess the returns";
stack-of-FSMs is "the runtime *is* the call stack, and the step computes
each return."

## § 5 — What this unlocks

The three blocked tree-walk ports can finally move their **walk** into
Evident — not just the leaf classifier.

Today the self-hosting swap interface (`self-hosting.md`) is structurally
**net-positive on line count**, because the recursion gap forces the
tree walk to stay in Rust: each ported pass keeps the Rust walk, *and*
adds a structurally-identical duplicate of that walk under
`runtime/src/portable/` (so both impls visit the same leaves in the same
order), *and* an Evident pass that owns only the small per-leaf decision.
Net Rust LOC goes **up**, not down. From `self-hosting.md`:

| Pass | Rust LOC | What's in Evident today | What the recursion gap forces to stay in Rust |
|---|---|---|---|
| `subscriptions` | **313** | the `world.X` / `world_next.X` prefix classifier (`world_read_match`, `world_next_write_match`) | the entire `walk_body` / `walk_expr` tree walk — duplicated in `portable/subscriptions.rs` so both impls match |
| `validate` | **88** | the per-`Call` banned-name decision (`ValidateExpr`) | `find_ffi_call`'s tree walk — duplicated in `portable/validate.rs` 1:1 |
| `pretty` | — | `EIdentifier` + a couple of flat `BodyItem` shapes (ASCII, non-recursive subset) | every shape with sub-`Expr`s (`EBinary`, `ECall`, `ESetLit`, quantifiers, mapping lists) — i.e. essentially all of it |

The loop-functionizer + stack-of-FSMs is the mechanism that **inverts**
this. Once the walk is a loop-functionized `walk_step` (§ 4), you can:

- **Delete the Rust walk** in `subscriptions.rs` / `validate.rs`
  (`walk_body`, `walk_expr`, `find_ffi_call`).
- **Delete the `portable/` duplicate** kept solely so the two impls visit
  identical leaves — its only reason to exist is that the walk couldn't
  move, and now it can.
- Keep just the marshaler (Rust `Value` ⇄ `stdlib/ast.ev` enum) and the
  Evident pass, which now owns **both** the walk and the classifier.

That is the first time a self-hosting port makes the Rust line count go
*down* by more than the Evident pass costs to add — the whole point of
the ≤11K-LOC runtime target (`docs/plans/README.md`). `subscriptions`
alone is 313 LOC of Rust whose tree-walking core (and its `portable/`
mirror) leaves once `walk_step` is loop-functionized; `validate`'s 88
LOC follows the same shape; `pretty` becomes possible at all once the
ordered-output story (§ 8) is settled.

## § 6 — Relationship to the other strategies

### vs. CC's symbolic unroll — *prove* vs. *run*

Pick by what you're asking and what the body looks like:

| You want to… | …on a body that's… | Use | Because |
|---|---|---|---|
| **verify** a property over *all* initial states ("does it halt within N, for any start?") | **affine** | **symbolic unroll (CC)** | it yields a *closed-form constraint* (`∃ k : halt_k`) you assert + check SAT/UNSAT; folds to O(1) in N |
| **run** it on *one* concrete input and get the answer | **branching or affine** | **loop-functionizer** | it yields a *value*; the branch is a per-iteration `ite`, never a growing formula; dynamic iteration count |

Symbolic unroll is a **prover**: it reasons about the transition
symbolically and answers a ∀/∃ question, but only where the composition
collapses (Z's affine regime). The loop-functionizer is an **evaluator**:
it computes one trajectory's result for any body, but tells you nothing
about the inputs it didn't run. They are complementary, not competing —
and the affine-step detector (`fsm_unroll/detector.rs`) is precisely the
switch: if it *accepts*, you can have the closed-form proof; if it
*refuses* (branching), the loop-functionizer is how you still *run* the
thing.

### vs. CEGAR (GG) — the loop-functionizer is an *abstraction*

In CEGAR's vocabulary (`cegar-scaffolding.md`), the loop-functionizer is
an **abstraction**: a fast callable (`CompiledFunction` = `Abstraction`).
It composes with the oracle/refiner loop with no redesign — its
`call(given) → Option<…>` is already the `Abstraction::query` shape. If
the abstraction declines (a step bails mid-walk → `None`), the oracle
(Z3) is the fallback for *that step*, exactly as today.

But there's a sharp difference from the JIT-as-abstraction case worth
stating: **for the recursive tree-walk class, Z3 is not a sound oracle.**
The JIT's `None → Z3` fallthrough is safe because Z3 can always re-solve
what the JIT declined. The loop-functionizer applied to a recursive walk
has *no* such fallback — Z3 over an unbounded recursion *is* the
recursion gap. So here the loop-functionizer isn't "a fast path with Z3
as a safety net"; it is **the only path**, and its `max_iters` guard
guards against *bugs* (a non-draining stack), not against
"Z3-could-do-better." When the guard trips, the honest outcome is a loud
diagnostic, not a silent escalation to an oracle that can't help (§ 3,
Termination). For the *value-iteration* case (a counter), Z3 *can* serve
as oracle, and the standard CEGAR composition applies unchanged. The
loop-functionizer thus plays *both* CEGAR roles depending on the body
class — which is itself a useful thing the abstraction/oracle framing
makes visible.

## § 7 — Implementation plan

Three sessions, smallest-first, each independently landable. The user
chose "design doc first"; this seeds the follow-ons.

### 1. Loop-functionizer mechanism alone — prove on `test_34`'s counter

**Smallest.** Build `LoopFn` + `LoopContract` (the value-iteration half:
`state_pairs` + `halt_var`, no `work_stack`). Reuse
`fsm_unroll/compose.rs::detect_state_pairs` to build the contract from a
claim body. Prove it on the existing `decrement` claim in
`examples/test_34_halts_within.ev`: from `count = 50`, run-to-halt in one
`call`, expect `count = 0` and the same trajectory the operational
`countdown` fsm prints. No new Evident-language features — just the loop
wrapper around the already-compiled step and the threading.

*Size:* ~150–300 LOC Rust + one test. *Risk:* low — the step is already
JIT-compiled by `cranelift.rs`; this is pure orchestration.

### 2. Stack-of-FSMs on a toy tree — sum-a-tree

**Medium.** Add the `WorkStack` half of the contract (wrapper-owned
native `Vec<Value>`, design B). Define `enum Tree = Leaf(Int) |
Node(Tree, Tree)`, a `sum_step(node, acc) → (children, acc_next)`, seed
`⟨root⟩`, drain, expect the recursive sum. This is where you learn what
the step JIT can actually compile on this shape: `match` over a
recursive enum, `Seq` children construction (`⟨l, r⟩` / `⟨⟩`), and Int
accumulation. Exercises `seq_elems` marshaling in/out of the wrapper.

*Size:* ~200–400 LOC Rust + a small `.ev` fixture + test. *Risk:* medium
— may surface step-JIT gaps (enum `match` → `Seq` output); those route
to the correct-but-slow Z3 path per step, so correctness holds even if
speed doesn't.

### 3. Self-host `subscriptions::walk_expr` end to end — watch LOC drop

**Largest, the payoff.** Loop-functionize the `walk_step` of § 4. Wire
it behind `EvidentSubscriptions`, reusing the existing classifier in
`stdlib/passes/subscriptions.ev`. Make `runtime/tests/subscriptions_equivalence.rs`
pass with the walk in Evident. Then delete the Rust `walk_body` /
`walk_expr` **and** the `portable/subscriptions.rs` duplicate, and record
the net LOC change in `docs/plans/PROGRESS.md`.

*Size:* larger — needs the full `Expr` dispatch in the step + the
`EMatch` arm-extraction decision (§ 4 note) + byte-exact equivalence.
*Risk:* higher — depends on the step's string operations (the prefix
test) JIT-ing or slow-solving *correctly*; the given-pinned-enum
string-equality gaps (COUNTEREXAMPLES #16, #18) are the specific hazard,
already worked around SAT-shaped in `subscriptions.ev` but worth
re-verifying through the loop path. *Payoff:* the first net-negative
self-hosting port — the line-count inversion this whole doc exists to
enable.

## § 8 — Limits & open questions

Honest unknowns, roughly in order of how much they'd bite:

- **Heterogeneous WorkItem encoding.** `Expr`'s children are all `Expr`,
  so its stack is homogeneous — but a *whole-`SchemaDecl`* walk descends
  `SchemaDecl → BodyItem → Expr`, three node types. That needs a sum
  `WorkItem = WSchema(…) | WBodyItem(…) | WExpr(…)` and a dispatch arm
  per type. Mechanical, but it's more enum surface and the `match` in the
  step grows. `EMatch`'s arm-body extraction (§ 4) is the smallest
  instance of this.

- **Output-assembly order for `pretty`.** `subscriptions` works because
  its accumulator is a commutative set union — order-free. `pretty`
  assembles a *string*, where a parent's rendering needs its children's
  renderings concatenated **in order**, which a single LIFO drain
  doesn't give you. The standard fixes — a two-phase ("expand" then
  "reduce") stack where each node is pushed twice, or an explicit result
  stack — both need a continuation in the WorkItem (`WorkItem(node,
  phase)`). This is exactly why `subscriptions` is the § 7 step-3 target
  and `pretty` is deferred; the ordered-output story is its own design
  increment.

- **Is `Seq` an efficient stack today?** Design A (marshal the whole
  stack through the step) re-clones an O(n) `Value` every iteration →
  O(n²) for an n-node walk, and needs an in-step `Seq` tail/drop the
  runtime doesn't clearly expose. Design B (wrapper-owned native `Vec`)
  sidesteps both — O(n) total, native push/pop — which is *why* it's
  recommended (§ 4). For ASTs (hundreds of nodes) even A is tolerable,
  but B is the right default. A future optimization could keep even the
  accumulator native across iterations to avoid re-marshaling it too.

- **`max_iters` ergonomics.** A counter's bound is value-driven (CC's
  N); a walk's bound is the node count (hundreds, not 64). A fixed
  constant à la `EVIDENT_MAX_INLINE_DEPTH=64` is too small for a walk.
  Options: derive the cap from the seed work-stack's transitive size, or
  a generous env-configurable default (`EVIDENT_LOOP_MAX_ITERS`). Wrong
  here means either spurious overrun (too low) or a real hang masked
  until the cap (too high).

- **Step-JIT dependence for *speed*, and string gaps for *correctness*.**
  The loop is only fast if the step JIT-compiles; if the step falls to
  the slow Z3 path it runs one Z3 solve *per node* — correct but slow.
  Worse, `subscriptions`' classifier touches strings, and the
  given-pinned-enum string-equality gaps (COUNTEREXAMPLES #18) and
  non-ASCII mangling (#16) can break *correctness* of the per-node
  decision, not just speed. `subscriptions.ev` already works around #18
  SAT-shaped; the loop path must preserve that workaround.

- **Debugging a compiled loop vs. CC's per-doubling trace.**
  `halts_within` has a clean `EVIDENT_FSM_UNROLL_TRACE` showing each
  doubling's node count and the detector verdict. A native loop is more
  opaque — but the wrapper is *Rust* driving the step in a plain loop, so
  a per-iteration trace (dump the popped node + accumulator + stack depth
  each iteration, behind `EVIDENT_LOOP_TRACE`) is cheap to add and is the
  natural analogue. Worth building alongside the mechanism so step-3's
  equivalence debugging isn't blind.
