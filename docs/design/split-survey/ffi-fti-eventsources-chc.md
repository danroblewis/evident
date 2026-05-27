# Split survey — ffi / fti / event_sources / chc / effect_dispatch

## Summary

- 13 files; 3021 LOC total.
  - **Engine**: 11 files (ffi.rs, effect_dispatch.rs, fti.rs, event_sources/mod.rs, frame_timer.rs, sigint.rs, stdin.rs, wall_clock.rs, file_line_reader.rs, file_watcher.rs, chc.rs)
  - **Entangled (minor)**: 2 files (event_sources/declarative_install.rs, event_sources/reflection.rs)
  - **Standalone/additive**: chc.rs (no live path beyond tests)

### Headline findings

1. **This cluster IS the clean engine-side IO/solver-FFI kernel** — with two targeted exceptions. `ffi.rs`, `effect_dispatch.rs`, the six pure event-source bridges, and `fti.rs` are all value-level: they consume decoded `Effect`/`Value`/`EffectResult` and drive OS/C APIs. No translate/ contact, no AST walking. They "stay Rust forever" cleanly.

2. **Effects and event-sources communicate with the scheduler entirely at value-level.** The `EventSource` trait (event_sources/mod.rs:33) has three methods: `start(tx: Sender<SchedulerEvent>)`, `stop()`, and `drain_writes() -> Vec<(String, Value)>`. The write path is a `Vec<(field_name: String, Value)>` queue (event_sources/mod.rs:39–46). The wake path is `SchedulerEvent::Tick { name: String }` (event_sources/mod.rs:26). Both are pure value-level; neither crosses into AST or translate types. The `dispatch_one` entry point (effect_dispatch.rs:94) takes `&Effect` — a decoded, fully-concrete enum value extracted from the Z3 model; it never touches live Z3 handles or the translate layer.

3. **`effect_dispatch.rs` is value-driven, not Z3-coupled.** Its only imports are `crate::core::ast::{Effect, EffectFfiArg, EffectResult}` (effect_dispatch.rs:7) and `crate::ffi` (effect_dispatch.rs:8). `Effect` is the plain decoded Rust enum (not a Z3 AST). All FFI orchestration happens via the `HandleRegistry` (opaque `u64` IDs); no Z3 handle leaks through. The `PackedBuf(Vec<PackedField>)` arm in `FfiArg` (ffi.rs:26) references `crate::core::ast::PackedField` — this is a minor data-type dependency on the AST crate, but `PackedField` is a plain serialization type (3 variants: U8/I32/F32) with no parsing or Z3 coupling (core/ast.rs:243–255).

4. **Two files carry genuine front-end coupling:** `declarative_install.rs` calls `rt.query_with_pins_and_given(...)` (declarative_install.rs:49) and `crate::translate::ast_decoder::decode_install_step_list` (declarative_install.rs:10); it is effectively a mini query-engine client. `reflection.rs` calls `ctx.encode_program()` (reflection.rs:77) — a closure injected by `effect_loop/mod.rs:126–139` that invokes `crate::translate::ast_encoder::program_to_value`. Both are one-shot startup paths (not per-tick), but they cross the seam in both directions (translate/decode + runtime query).

5. **`chc.rs` is additive/standalone — not on any live runtime path.** It is `pub mod chc` in lib.rs (lib.rs:4) and is consumed only by `runtime/tests/chc_countdown.rs`. No production code path calls into it. It is a raw z3-sys Fixedpoint/Spacer wrapper, engine-side (it takes `z3::Context` directly), wiring it into `effect_loop/` or `compose.rs` is noted as "a later slice" in the module doc (chc.rs:33).

---

## Per-file classification

| File | LOC | Class | Why | Seam difficulty | Cross-seam coupling |
|---|---|---|---|---|---|
| `ffi.rs` | 517 | engine | dlopen/dlsym/libffi marshaling + HandleRegistry; pure C-ABI boundary | low | `crate::core::ast::PackedField` (data type only, no parser/Z3 touch) |
| `effect_dispatch.rs` | 978 | engine | Maps decoded `Effect` enum → real OS/FFI calls; `dispatch_one`/`dispatch_all` entry points | low | `crate::core::ast::{Effect, EffectFfiArg, EffectResult}` only (decoded values, not Z3 handles) |
| `fti.rs` | 104 | engine | FTI registry: type-name → FrameTimer install fn; minimal AST touch is pin-value extraction only | low | `crate::core::ast::Pins` to read literal pin values; no translate coupling |
| `event_sources/mod.rs` | 163 | engine | `EventSource` trait, `SchedulerEvent`, `WriteQueue`, `WorldPluginCtx`; entirely value-level interfaces | low | `crate::Value` only; `WorldPluginCtx.encode_program` is a `dyn Fn()` callback (coupling is injected from outside, not internal) |
| `event_sources/frame_timer.rs` | 122 | engine | Periodic tick; writes `Value::Int(count)` to world field | low | `crate::Value` only |
| `event_sources/sigint.rs` | 130 | engine | SIGINT bridge; writes `Value::Int(count)` to world field | low | `crate::Value` only |
| `event_sources/stdin.rs` | 138 | engine | Stdin line reader; writes `Value::Str/Int` to world fields | low | `crate::Value` only |
| `event_sources/wall_clock.rs` | 111 | engine | Wall-clock poller; writes `Value::Int(unix_ms)` | low | `crate::Value` only |
| `event_sources/file_line_reader.rs` | 159 | engine | File line reader; writes `Value::Str/Int/Bool` to world fields | low | `crate::Value` only |
| `event_sources/file_watcher.rs` | 118 | engine | File mtime poller; writes `Value::Int(count)` on change | low | `crate::Value` only |
| `event_sources/reflection.rs` | 87 | entangled | Calls `ctx.encode_program()` which invokes `translate::ast_encoder::program_to_value`; encodes the full AST as a `Value` tree | med | `crate::Value`; `encode_program` closure calls into `translate::ast_encoder` (front-end concern injected at install time) |
| `event_sources/declarative_install.rs` | 101 | entangled | Calls `rt.query_with_pins_and_given(...)` (runtime query) and `translate::ast_decoder::decode_install_step_list` (decode Value → Effect list) | med | `crate::core::ast::Pins`, `crate::translate::{Value, ast_decoder}`, `crate::runtime::EvidentRuntime` (full runtime reference) |
| `chc.rs` | 293 | standalone/additive | Raw z3-sys Fixedpoint/Spacer wrapper; only referenced in test (`tests/chc_countdown.rs`); no production path | low | `z3::Context` + `z3_sys` directly; no AST, no translate, no effect path |

---

## Seam notes

### Effect/event-source ↔ scheduler interface: value-level throughout

The `EventSource` trait (event_sources/mod.rs:33–41) is the sole interface between sources and the scheduler. Both channels are value-level:

- **Wake channel**: `Sender<SchedulerEvent>` where `SchedulerEvent` is `Tick { name: String }` or `Closed { name: String }` (event_sources/mod.rs:25–30). A string name, nothing more.
- **Write channel**: `drain_writes() -> Vec<(String, Value)>` (event_sources/mod.rs:39). Drained each tick; the scheduler writes `(field_name, Value)` pairs into world state. `Value` is the runtime's plain decoded value type, not a Z3 AST.

The `WorldPluginCtx` struct (event_sources/mod.rs:59–87) is what the scheduler passes to installers at startup. Its fields are all value-level or callback-typed: `world_fields: &HashMap<String, String>`, `fsm_event_subscriptions: &HashSet<String>`, env vars, and two `&dyn Fn` callbacks. The `encode_program` callback (event_sources/mod.rs:79) is the single seam touch — it is a closure injected by `effect_loop/mod.rs:126–139` that calls into `translate::ast_encoder`. But the injection point is in `effect_loop/`, not inside `event_sources/`; the trait itself stays clean.

### `effect_dispatch.rs:7–8` — the only AST imports

`effect_dispatch.rs` imports exactly `crate::core::ast::{Effect, EffectFfiArg, EffectResult}` and `crate::ffi`. `Effect` and `EffectFfiArg` are the decoded/extracted result types from the Z3 model decoding layer; they carry no live Z3 state. `dispatch_one` (effect_dispatch.rs:94) takes `&Effect` — a plain Rust enum. No live Z3 handle leaks into the dispatch layer; the engine-seam is clean at this entry point.

### `ffi.rs:26` — `PackedField` reference

`FfiArg::PackedBuf` carries `Vec<crate::core::ast::PackedField>` (ffi.rs:26). `PackedField` is a trivial 3-variant enum (`U8`, `I32`, `F32`) with a `write_le` helper (core/ast.rs:243–255) — a pure data/serialization type. It is not a parser node, carries no Z3 state, and is the thinnest possible dependency on the AST crate. Post-split this could be moved to `core/value.rs` or a shared types crate to eliminate the FFI kernel's dependency on the front-end AST.

### `fti.rs:56–73` — AST `Pins` access

`fti.rs` imports `crate::core::ast::Pins` to read integer and string pin values out of a `Pins::Named(Vec<Mapping>)` (fti.rs:56–73). This is value extraction only — no schema-walking, no translate, no Z3. The coupling is limited to reading `Expr::Int` and `Expr::Str` literals from a declaration-site pin list. Also extractable into a shared helper if the seam requires it.

### `declarative_install.rs` — the deeper entanglement

`declarative_install.rs` holds a reference to `crate::runtime::EvidentRuntime` and calls `rt.query_with_pins_and_given(type_name, ...)` (declarative_install.rs:49). This is a full runtime solve — the install schema is queried via Z3. It also calls `crate::translate::ast_decoder::decode_install_step_list` (declarative_install.rs:10) to decode the `install` binding from the query result into a `Vec<InstallStep>`. This is an engine-startup-only path (one-shot; not per-tick), but it is a genuine front-end dependency: it needs the runtime to be fully loaded and Z3-capable at install time. On the engine side of the split, this file either (a) remains in the engine with the full runtime present (the natural position since the engine owns the runtime), or (b) is restructured so the engine only receives a pre-decoded effect list from the front-end compile step.

### `chc.rs` — additive, not wired

`chc.rs` is declared `pub mod chc` in `lib.rs:4` to expose it for the test at `runtime/tests/chc_countdown.rs:13`. No production runtime path calls into it. The module doc (chc.rs:33) explicitly states "ADDITIVE: this module is on no existing runtime path" and notes wiring into `compose.rs::build_f1` is a later slice. It consumes `z3::Context` directly (engine-side tool), no AST or translate coupling.
