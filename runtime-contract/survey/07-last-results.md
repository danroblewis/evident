# last_results / effect feedback

## Contract

At the end of each tick K, `dispatch_all` executes the FSM's effect list in
order and returns a `Vec<EffectResult>` of identical length. The scheduler
stores this as `fsm_rt[idx].last_results`. On tick K+1, before the solve,
the scheduler calls `rt.effect_results_to_value(&fsm_rt[idx].last_results)`
to build a `Value::SeqEnum` of `Result` enum values, and pins it into the
FSM's query view under the name stored in `fsm.last_results_var` (the variable
the FSM body declares as `last_results ∈ Seq(Result)`). The result list is
therefore position-aligned with the prior tick's effect list: `last_results[i]`
is the outcome of the i-th effect emitted on tick K. On tick 0, `last_results`
is an empty `Value::SeqEnum` (no prior tick). Source: `scheduler.rs` lines
238–241, `encode_ast.rs:effect_results_to_value`.

## Result mapping table

| Dispatched `Effect` | `EffectResult` returned | `Result` variant seen by FSM body |
|---|---|---|
| `NoEffect` | `NoResult` | `NoResult` |
| `Print(s)` | `NoResult` | `NoResult` |
| `Println(s)` | `NoResult` | `NoResult` |
| `Exit(n)` | `NoResult` (deferred) | `NoResult` |
| `ReadLine` | `Str(line)` or `Error(msg)` | `StringResult(s)` / `ErrorResult(s)` |
| `IntToStr(n)` | `Str(n.to_string())` | `StringResult(s)` |
| `RealToStr(f)` | `Str(f.to_string())` | `StringResult(s)` |
| `ParseInt(s)` | `Int(n)` or `Error(msg)` | `IntResult(n)` / `ErrorResult(s)` |
| `ParseReal(s)` | `Real(f)` or `Error(msg)` | `RealResult(f)` / `ErrorResult(s)` |
| `Time` | `Int(ms_since_start)` | `IntResult(n)` |
| `MonotonicTime` | `Int(ns_since_first_call)` | `IntResult(n)` |
| `ShellRun(cmd)` | `Str(stdout)` or `Error(msg)` | `StringResult(s)` / `ErrorResult(s)` |
| `SpawnFsm(name, arg)` | `Int(tentative_idx)` | `IntResult(n)` |
| `FFIOpen(path)` | `Handle(h)` or `Error(msg)` | `HandleResult(h)` / `ErrorResult(s)` |
| `FFILookup(lib, sym)` | `Handle(h)` or `Error(msg)` | `HandleResult(h)` / `ErrorResult(s)` |
| `FFICall(fn, sig, args)` | `NoResult`/`Int`/`Bool`/`Str`/`Real`/`Handle`/`Error` | corresponding `Result` variant |
| `LibCall(lib, sym, sig, args)` | same as `FFICall` | corresponding `Result` variant |
| `Malloc(n)` | `Int(handle_id)` | `IntResult(n)` |
| `ReadByte/I16/I32/I64(h, off)` | `Int(n)` | `IntResult(n)` |
| `ReadF32/F64(h, off)` | `Real(f)` | `RealResult(f)` |
| `ReadStr(h, off)` | `Str(s)` | `StringResult(s)` |
| `Write*(h, off, v)` | `NoResult` | `NoResult` |
| `CloseHandle(h)` | `NoResult` or `Error(msg)` | `NoResult` / `ErrorResult(s)` |

Source: `effect_dispatch.rs` `dispatch_one_inner`.

## Encoding

`effect_results_to_value` (in `runtime/src/translate/encode_ast.rs:613`) maps
`Vec<EffectResult>` to a `Value::SeqEnum` where each element is a
`Value::Enum { enum_name: "Result", variant: "...", fields: [...] }`:

```
EffectResult::NoResult    → Result::NoResult       (no fields)
EffectResult::Int(n)      → Result::IntResult      (fields: [Value::Int(n)])
EffectResult::Str(s)      → Result::StringResult   (fields: [Value::Str(s)])
EffectResult::Bool(b)     → Result::BoolResult     (fields: [Value::Bool(b)])
EffectResult::Real(f)     → Result::RealResult     (fields: [Value::Real(f)])
EffectResult::Handle(h)   → Result::HandleResult   (fields: [Value::Int(h as i64)])
EffectResult::Error(s)    → Result::ErrorResult    (fields: [Value::Str(s)])
```

The `Result` enum is declared in `stdlib/runtime.ev:206`. The
`Value::SeqEnum` is pinned into the solver's "given" map under the
`last_results_var` name, so Z3 sees it as a fixed Seq(Result) on that tick.
The FSM body matches on it with `match last_results[i]`, using Seq indexing
(`last_results[0]`, `last_results[1]`, etc.) to select by position.

## Self-feedback wake

`had_effects_last: Vec<bool>` in `scheduler.rs:116` tracks, per FSM, whether
it emitted at least one effect on the just-completed tick. After `dispatch_all`
returns (`scheduler.rs:387–391`):

```rust
let emitted_anything = !effects.is_empty();
let results = dispatch_all(ctx, &effects);
fsm_rt[fsm_idx].last_results = results;
had_effects_last[fsm_idx] = emitted_anything;
```

On the next tick's wake check (`scheduler.rs:204`):

```rust
let woken = had_effects_last[idx]
    || !pending_changes[idx].is_empty()
    || state_changed_last[idx]
    || external_event[idx];
```

An FSM that emitted any effect — even a `NoEffect` or `Println` — is
automatically scheduled for the next tick. This ensures the FSM always
gets to observe the results it requested. Note: `had_effects_last` is
reset to `false` for scheduled FSMs at the start of dispatch
(`scheduler.rs:383–385`), so only a fresh emission keeps the FSM awake.

## Fixture candidates

1. **test_02 Format tick — `last_results` pinned as `[StringResult("3")]`**.
   The claim `sat_count_emits_int_to_str` (`examples/test_02_counter.ev:49`)
   pins `state = Count(3)` and asserts `effects = ⟨IntToStr(3)⟩`. A
   companion fixture for the *next* tick can pin `state = Format(3)` and
   `last_results = ⟨StringResult("3")⟩` and assert `effects = ⟨Println("tick 3")⟩`
   — a single-tick slice that proves the feedback-read path without running
   the real dispatcher. This is the canonical `IntToStr → StringResult`
   feedback pattern documented in the file's header comment (line 14–15).

2. **test_04 Read tick — `last_results` pinned as `[IntResult(42), ErrorResult("...")]`**.
   The claim `sat_issue_emits_two_parses` (`examples/test_04_parse_int.ev:37`)
   covers the emit side. A companion fixture pins `state = Read` and
   `last_results = ⟨IntResult(42), ErrorResult("ParseInt: ...")⟩` and asserts
   `effects = ⟨Println("good: parsed an Int"), Println("bad: ERROR was correct"), Exit(0)⟩`
   — proves multi-position `last_results` indexing (`last_results[0]`,
   `last_results[1]`) without live dispatch.

3. **test_19 second-tick read — `last_results[1]` is a `StringResult`**.
   `examples/test_19_prev_tick.ev` emits `⟨Println(...), IntToStr(count)⟩` each
   Counting tick; the next tick reads `last_results[1]` (the `IntToStr` result
   at position 1) as a `StringResult`. Claim `sat_subsequent_tick_increments`
   already pins `is_first_tick = false` and `_count = 7`; a fixture that also
   pins `last_results = ⟨NoResult, StringResult("7")⟩` would assert
   `prev_str = "7"` — the only existing example of `last_results[1]` indexing.
