# Round 24 — Cranelift JIT codegen (native code, finally)

**Outcome:** SHIPPABLE. `runtime/src/cranelift_jit.rs` compiles
Z3-AST function-shaped components into native machine code via
Cranelift JIT. Integration test passes: hello.ev's
`state_next = match state` runs as machine code, returning the
correct enum value for both Init and Done input states.

Bench: **3.4× faster than the Z3-AST interpreter** on a 3-step
arithmetic chain (`next = current + 1; doubled = next * 2;
triple = doubled * 3`). 100k calls: 35.9ms (JIT) vs 123.9ms
(AST walker). Most of the per-call cost is HashMap
marshalling; the actual arithmetic runs at native speed.

## What this round adds

### `runtime/src/cranelift_jit.rs`

A standalone module that takes a `&Z3Program` and emits
Cranelift IR. Output is a `JitProgram` holding:

- `_module: JITModule` — keeps the generated code alive.
- `func: unsafe extern "C" fn(*const i64, *mut i64)` — function
  pointer to the JIT'd code.
- Slot layout (`input_offsets`, `output_offsets`) and value-kind
  metadata for packing/unpacking `Value` ↔ `i64`.
- `enum_tags` / `enum_variants` for nullary-variant
  conversion (variant name ↔ tag i64).

The compiled function takes two i64 arrays (`*const i64`
inputs, `*mut i64` outputs). The Rust runtime packs `Value`s
into i64 at known offsets before calling, then unpacks the
result.

### Patterns the JIT compiles

```text
  Numeral             → iconst.i64
  ADD, SUB, MUL, DIV  → iadd / isub / imul / sdiv
  UMINUS              → ineg
  EQ                  → icmp_eq + uextend to i64
  LT, LE, GT, GE      → icmp_signed_* + uextend
  AND, OR, NOT        → band / bor / bxor with 1
  ITE                 → select
  UNINTERPRETED (var) → load from inputs_ptr[offset * 8]
  DT_CONSTRUCTOR (0)  → iconst.i64 (variant tag)
  DT_RECOGNISER       → icmp_eq against the variant tag
```

The Z3Step::Guarded variant (from Round 23's guarded-equality
extraction) compiles to a cascade of `select`s — one per
branch, walked in reverse order with the last one being the
default. This makes `match state` dispatch a chain of
conditional selects, all native code.

### What doesn't compile (yet)

The JIT returns `None` (caller falls back to the AST walker
which falls back to Z3) when the program contains:

- `Z3Step::Seq` (Seq output construction).
- `DT_CONSTRUCTOR` with payload (e.g., `Println("hi")`).
- String literals / sort.
- Set operations.
- Function calls into Rust (string formatting, hashing).

These are all reachable in v1+ but require either heap
allocation (strings, payloads) or external calls (string
formatting). The AST walker handles them today; the JIT
just defers to it.

### Runtime integration

```rust
EvidentRuntime {
    functionize_z3_cache:  ...,  // Z3Program (AST walker source)
    jit_cache:             ...,  // JitProgram (native function)
}
```

`try_functionize_z3` flow:

1. Cache lookup. JIT hit → call native function, return.
2. AST walker fallback if JIT cache has `None`.
3. Cache miss:
   a. Build CachedSchema, run simplify+propagate-values.
   b. Extract `Z3Program`.
   c. Try to compile to JIT. Cache both program and JIT result.
   d. If JIT succeeded, call it. Else walk the AST.
4. The Z3 program (always) and the JIT function (when
   available) are cached for subsequent calls.

## Measured speedup

`tests/cranelift_jit_bench.rs`:

```
3-step int-arithmetic chain, 100,000 calls
  JIT:    35.92ms  (359 ns/call)
  Walker: 123.90ms (1239 ns/call)
  Speedup: 3.4×
```

The 359 ns/call includes:
- 2 HashMap allocations (per-call env + output).
- 2 HashMap lookups (one input).
- Native arithmetic (sub-ns).
- 3 HashMap inserts (outputs).

The actual native code path is the smallest part of the
overhead. A future round could replace the HashMap interface
with a pre-allocated slot array (the compiled function takes
raw pointers; the wrapper does the packing). That would cut
per-call cost to tens of nanoseconds.

## Tests

- `tests/cranelift_jit_hello.rs::jit_compiles_hello_state_next`:
  hello.ev's `state_next = match state` compiles, runs for both
  `state=Init` and `state=Done`, produces correct Value::Enum
  outputs.
- `tests/cranelift_jit_hello.rs::jit_compiles_int_arithmetic`:
  hand-built Z3Program `sum = a + b; prod = a * b` compiles,
  runs with different inputs, produces correct Int outputs.
- `tests/cranelift_jit_bench.rs::jit_vs_walker_int_chain`: the
  bench above.
- All 444 cargo + 119 conformance still pass — the JIT is
  guarded under `EVIDENT_FUNCTIONIZE_Z3=1` (off by default) and
  has no effect when disabled.

## What this completes

The architecture pivot the user asked for: walk Z3 ASTs (not
Evident ASTs), and execute the canonical form as native code
(not tree-walking). The infrastructure is in place. The JIT
runs whenever the Z3 functionizer is enabled AND the program's
patterns are within the JIT's coverage.

## What's next

1. **Flip `EVIDENT_FUNCTIONIZE_Z3` default to ON.** Requires
   first refactoring `translate_bool` to return `Result`
   instead of fatal-exiting (the issue documented in Round
   23). Without that, the Z3 functionizer is unsafe to enable
   broadly.
2. **Widen JIT coverage:** Seq output construction, string
   handling (intern table?), payload-bearing enums via a
   heap-allocated layout.
3. **Bench Mario:** once enabled by default with wider
   coverage, measure the actual frame-rate improvement on
   `examples/test_21_mario`.
4. **Optimize the FFI boundary:** the HashMap marshalling
   dominates per-call cost. A slot-array interface could cut
   it to tens of nanoseconds.

The user asked for native code. This round delivers native
code. The remaining work is broadening what compiles and
removing the gates so the runtime uses it by default.
