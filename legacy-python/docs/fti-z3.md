# The Z3 FTI — design (M5 prerequisite)

This doc is the design-before-implementation pass for the Z3 FTI.
The prelude plan describes a single Z3 FTI that lets composing FSMs
build constraint models by asserting Formula values into a Seq, then
read back sat results. The plan deferred the details; this doc
resolves them, learning from how M3 (Stack) actually played out.

## What we're building

A single FTI named `Z3` that materializes constraint models against
Z3's C API. Composing FSMs declare `z ∈ Z3`, constrain `z.formulas`
relationally (the same shape as Stack's `s.contents`), and read
`z.sat` once the FTI has run its check.

The user-facing surface stays purely relational:

```
fsm find_sum()
    z ∈ Z3
    z.formulas = match is_init:
        true => [
            Eq(Var("x", "Int"), IntLit(3)),
            Eq(Var("y", "Int"), IntLit(5)),
            Eq(Var("z", "Int"), Add(Var("x", "Int"), Var("y", "Int")))
        ]
        false => _z.formulas    ; don't change them after init
```

Tick 0: the user asserts the formulas, the FTI materializes them in
Z3 and runs check. Tick 1: `z.sat = Sat`; the user reacts.

## What's different from Stack

Stack's elements are flat `Int`s. The "what changed" detection on
each push is `last(contents)` — a single int — passed to a libcall.

Z3's elements are `Formula` values, which are **tree-shaped**. A
single push to `z.formulas` like `Eq(Var("x", "Int"), IntLit(3))` is
one Formula value, but it expands into multiple Z3 C-API libcalls:

```
Z3_mk_int_sort(ctx) → int_sort_handle
Z3_mk_string_symbol(ctx, "x") → x_sym
Z3_mk_const(ctx, x_sym, int_sort_handle) → x_ast
Z3_mk_int(ctx, 3, int_sort_handle) → three_ast
Z3_mk_eq(ctx, x_ast, three_ast) → eq_ast
Z3_solver_assert(ctx, solver, eq_ast)
```

Six libcalls for one Formula tree node and its children. This is the
core complexity: a single push to `formulas` generates a libcall
sequence whose length depends on the Formula's tree shape.

Three sub-problems fall out:

1. **Tree marshaling.** For a Formula constructor `Eq(l, r)`, we
   need libcalls to first marshal `l` and `r` (recursively), then
   the libcall to `Z3_mk_eq(ctx, l_handle, r_handle)`. The effect
   sequence is the post-order traversal of the Formula tree.

2. **Handle threading.** Each libcall returns a handle (a pointer
   as `long`). Subsequent libcalls need that handle as an argument.
   This means the FTI body has to reference the result of one
   libcall in the args of another — within the same tick's effect
   sequence.

3. **Effect-sequence length is unbounded.** Stack/Queue emit at
   most 1 libcall per tick. Z3 emits N libcalls where N depends on
   the size of the Formula trees pushed in that tick. The effects
   channel is `Seq Effect` — Z3 sequence values can hold any number
   of elements — so this works in principle, but the body has to
   *construct* a Seq of effects whose length depends on the Formula
   structure.

## The four sub-problems

### Sub-problem 1: handle threading inside an effects Seq

LibCall has an `ok_dest` field — the name of a Z3 const the
runtime's effect dispatcher pins to the libcall's return value.
Pinning happens **between ticks**: tick N emits LibCall(..., "x_h",
""), tick N+1 sees `x_h` bound to the returned handle.

But in M5, we need the OPPOSITE: within tick N, multiple libcalls
chain together — the result of the first is the arg to the second.

Two ways to handle this:

**Option A — multi-tick marshaling.** A single push to `formulas`
takes many ticks: one per libcall in the formula tree. The FTI
walks the Formula tree across multiple ticks, pinning intermediate
handles in `_x` slots and consuming them on the next tick.

Problem: very slow. Pushing one `Eq(Var("x", "Int"), IntLit(3))`
costs 6 ticks. A program with 10 formulas of average depth 5 costs
50 ticks just for marshaling. Plus the user's composing FSM has to
sit idle during those marshaling ticks, which breaks the natural
control flow.

**Option B — chained libcalls within a single effect dispatch.**
Extend the runtime's effect dispatcher to allow LibCall args to
reference results from earlier LibCalls in the same effects
sequence. Mechanism: a LibCall's `ok_dest` writes the result into a
**tick-local** scratchpad, and a new ArgRef variant of FFIArg lets
later args read from that scratchpad.

```
type FFIArg =
    | ArgInt(value ∈ Int)
    | ArgStr(value ∈ String)
    | ArgRef(name ∈ String)    ; read from tick-local scratchpad
```

A single Formula push produces a Seq of LibCall effects in order,
each one putting its handle into the scratchpad under a chosen name,
later ones reading from those names. The runtime processes the
sequence in order; the scratchpad is reset per tick.

Option B is **what we're committing to.** It's a bug-fix-shaped
extension to FFI (~30 lines: scratchpad, ArgRef handling, sig grammar
unchanged) and saves us hundreds of ticks per program.

### Sub-problem 2: detecting "new" formulas added

The composing FSM constrains `z.formulas = _z.formulas ++ [new_formula]`.
The FTI needs to detect this and only marshal `new_formula`, not
re-marshal everything in `_z.formulas`.

Same shape as Stack's push detection:

- Supported transitions for `z.formulas`:
  - `z.formulas = _z.formulas` (no-op)
  - `len(z.formulas) = len(_z.formulas) + 1 ∧ init(z.formulas) = _z.formulas` (single push)
  - Multi-push (`len(z.formulas) = len(_z.formulas) + N` for N > 1) — defer for v1; require single-push at a time.

If the user wants to push multiple formulas, they can either:
- Push one per tick across multiple ticks (verbose but works)
- Use a different driving pattern (e.g., emit them all in init then immediately re-pin them across ticks)

The single-push restriction matches Stack/Queue's design and keeps
the body small. Future bug fix: support N-push via a length-delta
match and unrolled tree-walk over the last N formulas.

### Sub-problem 3: marshaling a Formula tree to a libcall sequence

Given a Formula like `Eq(Var("x", "Int"), IntLit(3))`, produce:

```
[
  LibCall("libz3", "Z3_mk_int_sort", "l(l)",
          [ArgRef("ctx")], "int_sort", ""),
  LibCall("libz3", "Z3_mk_string_symbol", "l(ls)",
          [ArgRef("ctx"), ArgStr("x")], "x_sym", ""),
  LibCall("libz3", "Z3_mk_const", "l(lll)",
          [ArgRef("ctx"), ArgRef("x_sym"), ArgRef("int_sort")],
          "x_ast", ""),
  LibCall("libz3", "Z3_mk_int", "l(lil)",
          [ArgRef("ctx"), ArgInt(3), ArgRef("int_sort")],
          "three", ""),
  LibCall("libz3", "Z3_mk_eq", "l(lll)",
          [ArgRef("ctx"), ArgRef("x_ast"), ArgRef("three")],
          "eq_ast", ""),
  LibCall("libz3", "Z3_solver_assert", "v(lll)",
          [ArgRef("ctx"), ArgRef("solver"), ArgRef("eq_ast")],
          "", "")
]
```

The transpiler / FTI body needs to generate this. Two approaches:

**Approach P (procedural).** The FTI's body recursively walks the
Formula tree and emits the libcalls. Problem: SMT-LIB doesn't have
recursion in the conventional sense, and the body's `effects`
Seq has to be a closed-form expression.

**Approach S (structural).** Each Formula constructor has its own
"materialize" sub-formula. For a Formula tree, the effects are the
concatenation of each sub-tree's materialize sequence.

In SMT-LIB:

```
(define-fun materialize ((f Formula)) (Seq Effect)
  (ite (is IntLit f) (single libcall for IntLit)
  (ite (is Var f)    (single libcall for Var)
  (ite (is Eq f)     (seq.++ (materialize (Eq_l f))
                              (seq.++ (materialize (Eq_r f))
                                      (single libcall for Eq)))
  ...)))
```

This is structural recursion over the Formula datatype. **Z3
supports recursive function definitions** (`define-fun-rec`). The
FTI body can use this to express the materialize relation
cleanly.

There's a concern: Z3's handling of recursive functions can be slow
for deep recursion. For small Formula trees (depth < 10), it should
be fine. For very deep trees, we'd hit performance issues.

For v1 we go with Approach S — recursive materialize function over
the Formula datatype — accepting the depth limit.

### Sub-problem 4: getting the result back

After marshaling and assertion, the FTI calls `Z3_solver_check(ctx,
solver)` and gets an int (sat=1, unsat=-1, unknown=0). Then optionally
`Z3_solver_to_string(ctx, solver)` for the model.

Both of these come back via `ok_dest`. The first lands in
`z.sat_raw` (an Int), the FTI converts it to the SatResult sum type:

```
sat = match _sat_raw:
    1  => Sat
    -1 => Unsat
    _  => Unknown
```

For the model string, similar: a String ok_dest. The model is just
the SMT-LIB text Z3 produces for `(get-model)`.

The two-tick latency: tick N emits Z3_solver_check; tick N+1 sees
sat_raw bound, derives sat. The composing FSM reads `z.sat` on
tick N+1. Documented behavior.

## The Formula datatype

For v1, a deliberately limited set:

```
type Formula =
    | IntLit(value ∈ Int)
    | BoolLit(value ∈ Bool)
    | Var(name ∈ String, sort_name ∈ String)
    | Eq(l ∈ Formula, r ∈ Formula)
    | Add(l ∈ Formula, r ∈ Formula)
    | Sub(l ∈ Formula, r ∈ Formula)
    | Mul(l ∈ Formula, r ∈ Formula)
    | Lt(l ∈ Formula, r ∈ Formula)
    | Le(l ∈ Formula, r ∈ Formula)
    | Gt(l ∈ Formula, r ∈ Formula)
    | Ge(l ∈ Formula, r ∈ Formula)
    | And(l ∈ Formula, r ∈ Formula)
    | Or(l ∈ Formula, r ∈ Formula)
    | Not(arg ∈ Formula)
```

14 constructors, no set theory, no quantifiers — those come in M6.

`sort_name` on Var is a String specifying the Z3 sort: "Int",
"Bool", "Real", "String". The FTI uses this to dispatch the right
Z3_mk_* sort libcalls.

## The Z3 FTI body — sketch

```
fti Z3
    ctx ∈ Int                       ; Z3_context handle
    solver ∈ Int                    ; Z3_solver handle
    formulas ∈ Seq(Formula)         ; assertions the host wants
    sat_raw ∈ Int                   ; raw result of Z3_solver_check
    sat ∈ SatResult                 ; derived enum form
    model ∈ String                  ; SMT-LIB text of get-model
    effects ∈ Seq(Effect)

    ; sat_raw → sat enum
    sat = match sat_raw:
        1  => Sat
        ; -1 isn't expressible until we have signed literal patterns,
        ; so we test via Boolean: anything not 1 and not 0 is Unsat
        ; (since Z3 only returns -1, 0, or 1)
        0  => Unknown
        _  => Unsat

    ; Supported transitions for formulas.
    (formulas = _formulas
       ∨ len(formulas) = len(_formulas) + 1
         ∧ init(formulas) = _formulas)

    ; Effects.
    effects = match is_init:
        true =>
            ; Allocate Z3 context and solver. Each libcall's ok_dest
            ; pins the result for the next tick to use.
            [LibCall("libz3", "Z3_mk_config", "l()",
                     [], "_cfg_tmp", ""),
             LibCall("libz3", "Z3_mk_context", "l(l)",
                     [ArgRef("_cfg_tmp")], "ctx", ""),
             LibCall("libz3", "Z3_mk_simple_solver", "l(l)",
                     [ArgRef("ctx")], "solver", ""),
             LibCall("libz3", "Z3_solver_inc_ref", "v(ll)",
                     [ArgRef("ctx"), ArgRef("solver")], "", "")]
        false =>
            ; If a new formula was pushed, marshal it + assert it.
            match len(formulas) = len(_formulas) + 1:
                true =>
                    materialize(last(formulas))
                    ++ [LibCall("libz3", "Z3_solver_check", "i(ll)",
                                [ArgRef("ctx"), ArgRef("solver")],
                                "sat_raw", "")]
                false => []
```

Where `materialize(f)` is a recursive function defined in the FTI's
body (via `define-fun-rec` in SMT-LIB). For each Formula
constructor, it returns a Seq of LibCalls in post-order:

```
materialize(f) =
    match f:
        IntLit(n) =>
            [LibCall("libz3", "Z3_mk_int_sort", "l(l)",
                     [ArgRef("ctx")], "_isort_tmp", ""),
             LibCall("libz3", "Z3_mk_int", "l(lil)",
                     [ArgRef("ctx"), ArgInt(n), ArgRef("_isort_tmp")],
                     "_ast_tmp", "")]
        Eq(l, r) =>
            materialize(l)
            ++ materialize(r)
            ++ [LibCall("libz3", "Z3_mk_eq", "l(lll)",
                        [ArgRef("ctx"),
                         ArgRef("_ast_tmp_l"),    ; from materialize(l)
                         ArgRef("_ast_tmp_r")],   ; from materialize(r)
                        "_ast_tmp", "")]
        ...
```

The naming convention for intermediate handles: every `materialize`
call writes its result handle to a scratchpad slot named `_ast_tmp`.
But that's a problem — we need unique names per sub-tree, because
`Eq(materialize(l), materialize(r))` needs both `l`'s and `r`'s
result.

This is the same issue as variable shadowing in a recursive
function. The fix: each `materialize` call uses a different
scratchpad slot. But we can't generate unique names in a closed-form
SMT-LIB expression — we'd need a counter, which means stateful
generation, which we don't have.

**Resolution: use a stack-shaped scratchpad.** Each `materialize`
result is *pushed* into a list of pending handles. Operations
*consume* from the top. For `Eq(l, r)`:

- materialize(l) leaves handle on stack: top = l_handle
- materialize(r) leaves handle on stack: top = r_handle, below = l_handle
- Eq libcall pops two: args = [stack[-2], stack[-1]] = [l_handle, r_handle]
- Eq pushes its result handle: top = eq_handle

This works because it mirrors how a stack-based evaluator processes
post-order. Implementation: the scratchpad isn't a name→value map
but a stack (a list). LibCall has a new ok_dest behavior:
ok_dest="@push" means "push the result onto the scratchpad stack",
and ArgRef can read with negative indices (e.g.,
`ArgRef("@stack[-1]")` for the top, `ArgRef("@stack[-2]")` for the
one below).

This is a meaningful runtime change. ~50 lines of Python in ffi.py
to add the stack scratchpad + ArgRef variants + @push sentinel
handling.

## What the runtime changes look like

This is bug-fix-shaped (foundational missing primitive), but it's
the largest bug fix yet. ~80 lines of Python across ffi.py and
runtime.py.

1. **FFI: add ArgRef variant to FFIArg datatype.** In the prelude
   PRELUDE constant in transpile.py:
   ```
   (declare-datatypes ((FFIArg 0))
     (((ArgInt (ArgInt_0 Int))
       (ArgStr (ArgStr_0 String))
       (ArgRef (ArgRef_0 String)))))
   ```

2. **Runtime: tick-local scratchpad.** In `Runtime.run()` or
   `_dispatch_effects()`, maintain a per-tick stack-shaped
   scratchpad. ArgRef("@stack[-N]") reads from index `len(stack)-N`.

3. **Runtime: ok_dest = "@push" means "push to scratchpad", and
   normal ok_dest names still work.** Both can be used in the same
   effect sequence.

4. **ffi.py: when marshaling args, resolve ArgRef before calling
   the C function.** Look up the named slot or the @stack[-N] form,
   substitute the actual int/long value.

This unlocks not just Z3 but any C library that uses
chained-handle APIs (OpenSSL contexts, libcurl handles, GPU buffer
chains, etc.). The pattern is general.

## Recursive define-fun-rec — can the bootstrap parser do it?

Probably not. The bootstrap parser handles `claim`, `fsm`, `type`,
`fti` declarations and the `match` expression form. Defining a
recursive function inside an FTI body isn't currently expressible.

Three options:

**Option R1: Add `def` keyword to the parser.** `def name(params) =
body` lowers to `(define-fun-rec name params body)`. Bug-fix-shaped
(~15 lines).

**Option R2: Inline the recursion.** Don't use a recursive function;
instead, the FTI body for each push computes the materialize sequence
inline via a deeply nested `match`. Problem: the body becomes O(2^depth)
big because every Formula constructor case has to handle all possible
sub-trees. Doesn't scale.

**Option R3: Process formulas tree-walk in Python.** Don't make the
FTI's body emit Z3 marshaling logic. Instead, push the work to the
runtime: when the runtime sees a Formula value in `formulas`, it
walks the tree in Python and emits the libcall sequence directly.

R3 is interesting because it makes the FTI body shorter (the Z3 FTI
basically just declares state and minimal effects, with the actual
marshaling work done by the runtime). But it special-cases the Z3
FTI in the runtime, which violates the "runtime is generic, FTIs
are user code" principle.

**Picking R1 — `def` keyword.** Bug-fix-shaped, generally useful,
keeps the runtime simple. ~15 lines in parser.py + ~10 in
transpile.py.

## What needs to land for M5

Concrete steps:

1. **Bug fix: `def` keyword for recursive functions.** Parser +
   transpiler emit `(define-fun-rec ...)`. ~25 lines.

2. **Bug fix: ArgRef variant + tick-local scratchpad.** Update
   PRELUDE in transpile.py to declare the new FFIArg variant; add
   scratchpad + ArgRef handling to ffi.py and runtime.py. ~80 lines.

3. **prelude/z3.ev — the Z3 FTI.** Formula type, SatResult type,
   Z3 FTI with init libcalls, supported-transition assertion,
   `def materialize` for the 14 Formula constructors, effects
   channel. ~250 lines.

4. **Test program: solve a simple model.** examples/z3_demo.ev that
   asserts `x = 42` and prints SAT. Spans the two-tick latency
   correctly (tick 0 asserts, tick 1 reads sat).

5. **UNSAT test:** examples/z3_unsat.ev that asserts `x = 1 ∧ x = 2`
   and prints UNSAT.

Estimated total: ~110 lines of Python (bug fixes), ~250 lines of
Evident (the FTI), ~80 lines of test code.

## What's deferred to M6

- Set-theoretic Formula constructors (SetEmpty, SetAdd, SetUnion, etc.)
- Quantifier Formula constructors (Forall, Exists)
- The Z3 set sort vs Int distinction in `Var(name, sort_name)`

M6 is mechanical extension of the materialize function with more
arms. Once M5 lands, M6 is just typing.

## What's deferred to M8

- A real demo (zebra puzzle or 4x4 sudoku)
- Using the Z3 FTI from a non-trivial composing FSM

## Open questions for v1

1. **Refcounting for Z3 handles.** Z3 ASTs are refcounted. The
   materialize sequence builds AST nodes but never decrements refs.
   For v1, we leak — the program is a single-shot solver, total
   memory is bounded by the size of one model. Document this. A
   future bug fix could add Z3_dec_ref calls in the FTI's cleanup
   path (which doesn't exist yet — FTIs have no destructor pattern).

2. **Multiple Z3 instances.** Two `z1 ∈ Z3, z2 ∈ Z3` in the same
   FSM gives two contexts, two solvers, independent. Should work
   out of the box with the namespace mechanism.

3. **The scratchpad lifetime.** Per-tick scratchpad means an
   FTI that emits N libcalls per tick has all their handles in
   the scratchpad during that tick. The scratchpad is wiped at
   tick start. Documented behavior — composing FSMs that want
   to keep a handle across ticks must use a real ok_dest name
   (not @push) so the result lives in `given`.

## How to know we're done with this design

When:

- The `def` keyword bug fix is small and well-scoped.
- The ArgRef + scratchpad mechanism has a clear data flow and
  is general (not Z3-specific).
- The Z3 FTI body, as sketched above, compiles to SMT-LIB the
  runtime can run.
- The two-tick latency for sat results works without special
  casing.

If all four are clear, M5 is ready to implement. If any of them
are still hand-wavy, that's the next design pass.
