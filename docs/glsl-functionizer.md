# GLSL functionizer — compile a `Z3Program` to a fragment shader

`runtime/src/functionize/glsl.rs` is the third `Functionizer` strategy
(after Cranelift and the symbolic/LLM experiments). Where Cranelift
*translates* an extracted `Z3Program` into native machine code, the
GLSL strategy *transpiles* it into a GLSL fragment shader, uploads it to
a headless OpenGL context, and at `call` time runs a one-row draw whose
pixels carry the output values back via `glReadPixels`.

It is **opt-in and macOS-only**. `EvidentRuntime::new()` still returns
Cranelift; nothing changes unless you do:

```rust
let rt = EvidentRuntime::with_functionizer(
    Box::new(evident_runtime::functionize::glsl::GlslFunctionizer::new()?));
```

`GlslFunctionizer::new()` returns `Result<_, String>` — it creates the
headless GL context eagerly so a missing context surfaces as a clear
error at construction, not on the first query.

## Why a GPU backend at all?

A `Z3Program` is a pure function from inputs to outputs. On **single-shot
latency Cranelift wins decisively** — there is no GPU dispatch in native
code (see the benchmark below). The architectural point of the GLSL
backend is **throughput**: the very same shader can evaluate the function
for thousands of inputs *in parallel*, one per pixel. A future
batch-evaluation workload (a per-pixel shader effect, a particle update,
a Monte-Carlo sweep over a model) is where a GPU wins by orders of
magnitude. This v1 establishes the call site and proves the transpile +
readback path; the batch payoff is for downstream consumers (see
[Deferred work](#deferred-work)).

## What compiles

Only **pure scalar `Int` / `Bool` arithmetic** programs. Concretely, a
`Z3Program` compiles iff:

- every step is `Z3Step::Scalar` (no `Seq` / `Guarded` / `PreBaked`),
- every output and every input has Z3 sort `Int` or `Bool`,
- there are no residual `checks` / `predicates` (a conditional / partial
  body is not a total function), and
- every AST node is in the supported `DeclKind` set below.

Validated against Cranelift (`runtime/tests/glsl_functionizer.rs`) across
five shapes: scalar Int arithmetic, Bool comparison, ternary/ITE,
chained Int conditional updates (output-references-earlier-output, the
test_29 chain shape), and multi-output.

### Z3 `DeclKind` → GLSL

Mirrors `cranelift.rs`'s `emit_compute_i64`, emitting GLSL strings:

| Z3 DeclKind | GLSL fragment | Notes |
|---|---|---|
| `NUMERAL` | `123` | refused if outside `i32` range |
| `TRUE` / `FALSE` | `1` / `0` | Bool is `int` 0/1 |
| `UNINTERPRETED` (0-arity) | `inN` / `vN` | input uniform / earlier-output local |
| `ADD` `SUB` `MUL` | `(a + b …)` etc. | n-ary left fold |
| `UMINUS` | `(-a)` | |
| `IDIV` / `DIV` | `(a / b)` | truncates toward 0 — matches Cranelift `sdiv` |
| `MOD` / `REM` | `(a % b)` | matches Cranelift `srem` |
| `LT` `LE` `GT` `GE` `EQ` | `((a < b) ? 1 : 0)` | yields `int` 0/1 |
| `AND` `OR` | `(a & b …)` / `(a \| b …)` | bitwise on 0/1, matches `band`/`bor` |
| `NOT` | `(a ^ 1)` | matches `bxor(v, 1)` |
| `ITE` | `((c) != 0 ? t : e)` | |

Everything else (`SELECT`, `STORE`, `DT_*`, `SEQ_CONCAT`, …) → `compile`
returns `None`, and the runtime falls through to a full Z3 solve. A
`None` is always *correct*, just slower.

### Representation choices

- **`Int` → GLSL `int` (32-bit).** The output buffer is `RGBA32I`, so
  readback is exact (no 8-bit quantization). Values outside the `i32`
  range overflow silently — the same *class* of limit as Cranelift's
  `i64`, but 32 bits sooner. (Float-`int` emulation for a 53-bit exact
  range was considered and rejected for v1 — see Deferred work.)
- **`Bool` → `int` 0/1.** Comparisons emit `… ? 1 : 0`; logic uses
  bitwise `& | ^` on those values, byte-for-byte matching Cranelift's
  encoding. Decoded back to `Value::Bool` on the kind recorded per
  output.
- **Integer division/mod** use GLSL's native `/` and `%`, which truncate
  toward zero — matching Cranelift's `sdiv`/`srem`. Both differ from
  Z3's Euclidean (floor) division for negative operands; that is a
  *shared* limitation with Cranelift, not GLSL-specific. The
  cross-validation tests pass because GLSL and Cranelift agree with each
  other.

## How it runs

### Headless GL context (macOS)

A single process-wide **CGL** (Core OpenGL) core-profile context, created
with no window via `CGLChoosePixelFormat` + `CGLCreateContext` (selector
`kCGLOGLPVersion_3_2_Core = 0x3200`, which yields a 4.1-capable core
context reporting GLSL 4.10 on Apple Silicon). GL function pointers are
loaded with the `gl` crate's `load_with`, resolving symbols via `dlsym`
from `/System/Library/Frameworks/OpenGL.framework/OpenGL`.

This needs **no display server** — it runs in `cargo test` with a fully
stripped environment. The only new dependency is `gl = "0.14"` (pure
Rust, no native build); the OpenGL framework is linked by a `#[link]`
attribute on the CGL extern block. No window, no GLFW/glutin, no cmake.

CGL contexts are current *per thread*, so the backend serializes all GL
work behind a `Mutex` and makes the context current on the locking
thread for each `compile` / `call`, detaching on drop. That keeps it
correct under `cargo test`'s multithreaded runner without sharing GL
objects across contexts.

### Output readback

Outputs are laid out **one per pixel** in a `1×N` `RGBA32I` texture
attached to an FBO. The generated fragment shader computes every output
into a local (`v0`, `v1`, …), then selects by `int(gl_FragCoord.x)` which
one this pixel emits into the `.r` channel:

```glsl
#version 330 core
uniform int in0;            // one per input
out ivec4 o;
void main() {
    int idx = int(gl_FragCoord.x);
    int v0 = (3 * in0) + 5;  // each output
    int sel = 0;
    if (idx == 0) sel = v0;
    o = ivec4(sel, 0, 0, 0);
}
```

A fullscreen triangle (generated from `gl_VertexID`, no vertex buffer)
covers the `N`-wide viewport; one `glDrawArrays` + one `glReadPixels`
(`GL_RGBA_INTEGER` / `GL_INT`) extracts all N exact `i32` values, decoded
back into the output `HashMap<String, Value>`.

## Benchmark — single-shot vs Cranelift

test_29's chain A (30 branch-dependent conditional-update steps over one
`seed_a`), 5000 single-shot calls, Apple M4 (run
`cargo test --release --test glsl_functionizer bench -- --ignored --nocapture`):

```
=== bench: 30-step chain, 5000 single-shot calls ===
  GLSL      :   238.63 µs/call
  Cranelift :     1.94 µs/call
  ratio     : 123.1× (GLSL / Cranelift)
```

**GLSL loses by ~123× on single-shot**, as expected: each call pays GPU
dispatch + a synchronous `glReadPixels` stall (~200–250 µs floor), while
Cranelift is native code at ~2 µs. This is the honest single-shot
picture and exactly why the default stays Cranelift. The GLSL backend is
*not* for single queries — it is the call site for a future batch path
where one draw amortizes the dispatch over thousands of inputs.

## Deferred work

What it would take to widen the scope, roughly in order of value:

- **Batch evaluation (the actual point).** Add a `call_batch(inputs:
  &[HashMap]) -> Vec<HashMap>` path: a `K×N` framebuffer, one input set
  per column-block, uploaded as a uniform buffer / texture instead of
  scalar uniforms, with a single draw + single readback. This is where
  the GPU wins; the single-shot `call` here is the degenerate `K=1` case.
  Requires a `Functionizer`/`CompiledFunction` trait extension (out of
  v1 scope — the trait is shared with Cranelift).
- **64-bit / exact-large integers.** GLSL 3.3 has no `int64`. Options:
  represent `Int` as `float` (exact to ±2⁵³ but division differs and
  needs `floor` emulation to match Z3), or two-component `uint`
  emulation. v1 uses 32-bit `int` and documents the overflow limit.
- **`Real` outputs.** Add an `RGBA32F` attachment (or `floatBitsToInt`
  packing into the existing `RGBA32I` buffer) and a float codegen path.
- **`Seq` / record / enum / String.** These have no natural fixed-width
  pixel encoding. A `Seq(Int)` could map to a wider buffer row, but
  variable-length outputs, payload-bearing enums, and strings are a
  fundamentally poor fit for a fragment-shader value channel — these
  stay refused, the same shapes Cranelift handles via Rust-side helper
  calls that have no GPU analogue.
- **Cross-platform.** The module is macOS-gated (CGL). Linux would use
  EGL surfaceless / a `glutin` headless context; the codegen + readback
  are portable, only context creation is platform-specific.
- **CLI selection.** No `EVIDENT_FUNCTIONIZER=glsl` env wiring was added.
  Because GLSL refuses every non-scalar component (FSM effect lists,
  `match`-to-Seq, records), running an `effect-run` demo under a global
  GLSL functionizer would just route those components to the *slow Z3
  solve* (GLSL replaces Cranelift, it doesn't stack with it) — not a
  meaningful GLSL-vs-Cranelift comparison. The honest per-call number is
  the Rust microbenchmark above, which isolates the two backends on a
  workload both fully compile.
```
