# Kernel input spec

The contract between `evident emit` (producer) and `kernel` (consumer).
A `.smt2` file that conforms to this spec is **kernel-runnable**.

This is the missing artifact called out in the iteration-1 plan
critique — without it D1 (kernel) and D2 (emit) can't be built
independently. With it, they share only this doc.

## File shape, at a glance

```
;; manifest: state-fields = state.foo:Int state.bar:String
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 16

(set-logic ALL)

;; Effect + Result Datatype declarations
(declare-datatypes ((Result 0) (Effect 0)) ( … ))

;; State + per-tick variable declarations
(declare-const state.foo Int)
(declare-const _state.foo Int)
(declare-const state.bar String)
(declare-const _state.bar String)
(declare-const is_first_tick Bool)
(declare-const effects (Seq Effect))
(declare-const last_results (Seq Result))

;; Body constraints (per the FSM body, all hard)
(assert ( … ))
(assert ( … ))
```

The `;; manifest:` lines are part of the **wire format**, not
human-only commentary. The kernel parses them before anything else.

## Manifest header

Required prefix. Every line: `;; manifest: <key> = <value>`. Order is
fixed; the kernel parses in order and errors if a required key is
missing or misplaced.

| Key | Format | Meaning |
|---|---|---|
| `state-fields` | space-separated `name:Type` pairs | Top-level constants the kernel reads from the model post-solve and pre-asserts as `_<name>` next tick. |
| `effects-name` | single identifier | The SMT-LIB constant of type `(Seq <effect-enum-name>)` that the kernel walks. |
| `effect-enum-name` | single identifier | The Datatype declared in the file that the kernel pattern-matches `Exit(_)`, `Println(_)`, etc. against. |
| `result-enum-name` | single identifier | The Datatype the kernel re-encodes `last_results` into. |
| `max-effects` | integer | Upper bound on `#effects` per tick. Kernel asserts `(<= (seq.len effects) <max-effects>)` before each solve. Prevents Z3 from producing unbounded Seqs. |

**Required** keys: all of the above. Missing → kernel exits 3 with
`manifest: missing required key <name>`.

**Order**: as listed. Kernel doesn't reshuffle. Out-of-order →
kernel exits 3 with `manifest: key <name> at unexpected position`.

**Format errors**: a malformed `name:Type` pair, an unrecognized type
in `state-fields`, a non-integer `max-effects` — kernel exits 3 with
a single-line diagnostic naming the offending line number.

### Recognized types in `state-fields`

| Type | SMT-LIB sort | Z3 model read |
|---|---|---|
| `Int` | `Int` | `.as_i64()` |
| `Bool` | `Bool` | `.as_bool()` |
| `Real` | `Real` | rationals → `f64` via the existing `real_value_to_f64` helper |
| `String` | `String` | `.as_string()` then unescape via existing `unescape_z3_string` |
| `<EnumName>` | declared Datatype | recurse via the variant-tester pattern from `translate/eval/decode.rs:extract_enum_value` |

Composite (record) state fields are flattened by `emit` into multiple
`state.x`, `state.y` flat fields BEFORE the SMT-LIB is written. The
kernel sees no nesting in the manifest.

## Datatype declarations

The kernel doesn't ship a built-in `Effect` enum. It reads the
declared Datatype from the file and pattern-matches by **variant
name**. This decouples the kernel from `stdlib/kernel.ev` versioning.

**Variant names the kernel recognizes** (built-ins). All others are
treated as `LibCall(...)` unless they fall through to dispatch error.

| Variant | Payload sorts | Built-in dispatch |
|---|---|---|
| `Println(String)` | (String) | `println!("{}", s)` → `NoResult` |
| `Print(String)` | (String) | `print!("{}", s)` → `NoResult` |
| `ReadLine` | () | stdin readline → `StringResult(line)` / `EofResult` |
| `ReadFile(String)` | (String) | `fs::read_to_string` → `StringResult(_)` / `ErrorResult(_)` |
| `WriteFile(String, String)` | (String, String) | `fs::write` → `NoResult` / `ErrorResult(_)` |
| `Time` | () | `SystemTime::now()` ms since epoch → `IntResult(_)` |
| `Exit(Int)` | (Int) | **short-circuit**: kernel exits with the Int payload. No `Result` emitted. |
| `LibCall(String, String, Seq[LibArg])` | (String, String, Seq[LibArg]) | dlopen + dlsym + libffi call → result by C return type |

For built-in variants, the kernel matches on the variant constructor
name exactly. A misspelled variant in `stdlib/kernel.ev` (`Printlin`)
falls through to the dispatch-error path → `ErrorResult("unknown
effect variant Printlin")` and execution continues.

**`Exit(_)` is the only short-circuit**. Every other variant produces
a `Result` value that's appended to the tick's `last_results` for the
next solve.

**`Result` enum, recognized variants**:

| Variant | Payload | When emitted |
|---|---|---|
| `NoResult` | () | After void-returning effects (Println, WriteFile) |
| `IntResult(Int)` | (Int) | Time, libcall returning int, integer reads |
| `StringResult(String)` | (String) | ReadLine (line), ReadFile (contents), libcall returning string |
| `RealResult(Real)` | (Real) | Libcall returning double |
| `EofResult` | () | ReadLine at EOF |
| `ErrorResult(String)` | (String) | Any failed effect — fold libffi errors here, NOT exit 3 |

The kernel constructs Result values as Datatype literals to inject
into `last_results` next tick. The encoding mirrors
`translate/encode_ast.rs:value_enum_to_datatype` but in raw
SMT-LIB-text form (the kernel never goes through `Value`).

## Per-tick protocol

### Tick 0 (initial)

1. Kernel reads the manifest, declares datatypes, parses body asserts.
2. Kernel asserts `(assert (= is_first_tick true))`.
3. Kernel **does not** pre-assert any `_state.*` or `last_results` —
   they remain free. The FSM is expected to either:
   - Use `(is_first_tick ? init : continued)` ternaries in its body.
   - Ignore `_state` on tick 0.
4. Kernel calls `solver.check()`.
5. If UNSAT → exit 2.
6. If SAT → extract `state.*` values + `effects` Seq from the model.
7. Walk effects (see "Effect walk" below).
8. Save state values as "previous-tick state."

### Tick N+1

1. Kernel pushes a fresh solver scope (or builds a fresh solver — see
   "Solver state" below).
2. Kernel asserts `(assert (= is_first_tick false))`.
3. For each `state.<name>` in the manifest, kernel asserts
   `(assert (= _state.<name> <prev-tick-value>))` using the saved
   "previous-tick state."
4. Kernel asserts `(assert (= last_results <Seq[Result] literal>))`
   where the literal is constructed from the prior tick's collected
   results, in walk order.
5. Same body asserts as tick 0 (they live in the file, not
   per-tick-injected).
6. `solver.check()` → SAT/UNSAT as above.

### State change detection

After SAT, kernel compares each `state.<name>` value to its saved
"previous-tick" counterpart. If **all** match, AND no `Exit(_)` was
dispatched this tick → halt with **exit 1** ("stuck").

The comparison is structural: `Int == Int`, `String == String`,
`Enum` by variant name + recursive payload comparison.

## Effect walk

After SAT, the kernel reads `effects` from the model:

1. `len = model.eval((seq.len effects))` → bounded by `max-effects`.
2. For i in 0..len:
   - `e = model.eval((seq.nth effects i))` → an `Effect` Datatype value.
   - Pattern-match `e`'s variant by name.
   - Dispatch (see "Effect dispatch" below).
   - If the result was not "short-circuit," append to a local
     `Vec<Result>` for the next tick.

3. If at any point during the walk a dispatched effect is `Exit(code)`:
   - Stop walking immediately. Remaining effects in the Seq are dropped.
   - Kernel process exits with `code`. No further ticks.

## Effect dispatch

Each variant is dispatched as follows:

### `Println(s)`
`println!("{}", s)`. Emit `NoResult`.

### `Print(s)`
`print!("{}", s)`. Emit `NoResult`.

### `ReadLine`
Read one line from stdin. On EOF: `EofResult`. On line: `StringResult(line)`
(stripped of trailing `\n`). On I/O error: `ErrorResult(<msg>)`.

### `ReadFile(path)`
`fs::read_to_string(path)`. On success: `StringResult(contents)`.
On failure: `ErrorResult(<error description>)`.

### `WriteFile(path, contents)`
`fs::write(path, contents)`. On success: `NoResult`. On failure:
`ErrorResult(<error description>)`.

### `Time`
`SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64`.
Emit `IntResult(ms)`.

### `LibCall(lib, fn, args)`
1. `dlopen(lib)` → handle. Cache opened handles across the kernel's
   lifetime (per `lib` string).
2. `dlsym(handle, fn)` → function pointer.
3. For each `arg` in `args`:
   - `ArgInt(n)` → libffi i64 arg
   - `ArgStr(s)` → libffi pointer arg (null-terminated C string)
   - `ArgReal(r)` → libffi double arg
4. Call via libffi. Capture the return value.
5. **Return-type inference**: the kernel infers C return type from
   the first variant of `Result` that fits. v1 caller must pin the
   expected return type by convention (the libcall returns whatever
   the C function actually returns; the FSM is responsible for
   knowing the shape).
6. On dlopen / dlsym failure: `ErrorResult(<message>)`.
7. On segfault or other catastrophic failure: kernel exits 3.

### `Exit(code)`
Kernel exits with the i64 `code` (truncated to i32 if necessary; values
> 255 are implementation-defined on POSIX).

### Unknown variant
`ErrorResult("unknown effect variant <name>")`. Walk continues.

## Failure semantics

| Condition | Exit code | Detail |
|---|---:|---|
| `Exit(code)` emitted | `code` | Kernel returns user's code. |
| `state_next == state` and no Exit | 1 | "Stuck halt." |
| UNSAT on a tick | 2 | Print `UNSAT on tick N` to stderr. |
| Manifest parse failure | 3 | Print parse error to stderr with line number. |
| Z3 init / context creation failure | 3 | Print Z3 error. |
| Libffi catastrophic failure (segfault, sig) | 3 | Print signal + libcall details. |
| OOM / panic | 3 | Standard Rust panic abort path. |

**Libcall non-catastrophic failures** (dlopen returns null, dlsym
returns null, C function returns non-zero error code, etc.) fold into
`ErrorResult(<msg>)` and continue. They're **not** exit 3. The FSM
is expected to branch on `ErrorResult` and decide what to do.

**UNSAT mid-execution**: print `UNSAT on tick N (no diagnostic — see
docs/plans/kernel-iteration-1.md "deferred: UNSAT-core diagnostics")`
to stderr, exit 2. Better diagnostics are a deferred feature.

## Solver state across ticks

Two valid implementations:

**Option A** (preferred for v1): **fresh solver per tick.** Each tick:
1. New `Solver`.
2. Assert the body constraints from the parsed file.
3. Assert tick-specific values (`_state.*`, `last_results`,
   `is_first_tick`).
4. `check()`.
5. Drop the solver.

Simple. Stateless. The cost is re-parsing body asserts each tick,
which is fast for v1 program sizes.

**Option B** (defer): **persistent solver with push/pop.** One
solver for the kernel's lifetime; push tick-specific asserts, pop
after the solve. Faster but trickier; reserve for later if v1 perf
is poor.

The kernel-input-spec doesn't constrain this — both implementations
produce the same observable behavior.

## What `evident emit` must guarantee

Compliance checks the emit subcommand owes:

1. **Manifest header is correct.** All required keys present, in order,
   types valid.
2. **`effects` is constrained by exactly one SeqLit-equality.** After
   `++` flattening (via the existing `desugar_seq_concat`), the body
   contains exactly one constraint of the shape
   `effects = ⟨e0, e1, …⟩`. Multiple constraints → emit-time error
   `schema X has N constraints on 'effects'; exactly 1 SeqLit-shaped
   constraint required`.
3. **All `state.*` flat fields appear in declared form** at the SMT-LIB
   top-level, with their corresponding `_state.*` counterparts.
4. **`max-effects` is set to a finite upper bound.** v1 default: 16.
   Future: user-overridable.
5. **Body asserts are SAT-shaped** — no soft assertions, no
   optimization directives. Pure SAT, per the iteration-1 plan.

If any check fails, `evident emit` exits non-zero with a clear
single-line diagnostic. No `.smt2` is written.

## Example: hello world

Source:

```evident
import "stdlib/kernel.ev"

fsm hello
    state.mode ∈ String = "Done"
    effects = ⟨Println("hello world"), Exit(0)⟩
```

Emitted SMT-LIB (abbreviated):

```
;; manifest: state-fields = state.mode:String
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 16

(set-logic ALL)

(declare-datatypes ((LibArg 0) (Result 0) (Effect 0))
  (((ArgInt (ai-val Int)) (ArgStr (as-val String)) (ArgReal (ar-val Real)))
   ((NoResult) (IntResult (ir-val Int)) (StringResult (sr-val String))
    (RealResult (rr-val Real)) (EofResult) (ErrorResult (er-msg String)))
   ((Println (pn-s String)) (Print (pr-s String)) (ReadLine)
    (ReadFile (rf-path String)) (WriteFile (wf-path String) (wf-contents String))
    (LibCall (lc-lib String) (lc-fn String) (lc-args (Seq LibArg)))
    (Time) (Exit (ex-code Int)))))

(declare-const state.mode String)
(declare-const _state.mode String)
(declare-const is_first_tick Bool)
(declare-const effects (Seq Effect))
(declare-const last_results (Seq Result))

(assert (= state.mode "Done"))
(assert (= effects
          (seq.++ (seq.unit (Println "hello world"))
                  (seq.unit (Exit 0)))))
(assert (<= (seq.len effects) 16))
```

Kernel behavior:
1. Tick 0: solve, get `state.mode = "Done"`, `effects = ⟨Println("hello world"), Exit(0)⟩`.
2. Walk effects:
   - `Println("hello world")` → stdout `hello world\n`, emit `NoResult`.
   - `Exit(0)` → exit 0.

Process exits 0. No tick 1.

## Things NOT in the spec

- **Multi-FSM scheduling.** There is one top-level FSM (after
  composition); the kernel runs ticks linearly.
- **Async I/O.** All effects are synchronous. The kernel blocks
  during stdin read, file I/O, libcall.
- **Effect ordering inference / toposort.** Effects are walked in
  `Seq` literal order, full stop. Reordering is the emit / FSM-author's
  job, not the kernel's.
- **State Datatype encoding.** State is flat fields per the manifest.
  Composite state is flattened by emit.
- **`default` keyword / soft constraints.** Pure SAT only.

Anything not specified here, the kernel may handle differently across
versions. Anything specified here is the v1 contract.
