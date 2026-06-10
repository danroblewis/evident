# Functionizer interp-throughput wall: lowered-IR interpreter

Date: 2026-06-10. Addresses the twice-measured wall in
`docs/plans/sample-rung-walls.md` (~0.5 ms/tick, 90% func, on the
compiler2 driver) and `docs/plans/passes-in-evident-walls.md` (the
autocarry pass at 1.46 s vs its ≤1 s wire-in budget).

## Shape histogram (the measure-first step)

`EVIDENT_FUNCTIONIZE_STATS=verbose`, per-step category × run mode.

compiler2 driver (stage1 on fixture-001; 1,970 extracted steps):

| category      | JIT | interp |
| ------------- | --- | ------ |
| ite           | 273 | **620** |
| scalar (str ops at top) | 21 | **352** |
| binop         | 317 | **166** |
| datatype      | 0   | **203** |
| var           | 8   | 9      |
| guarded-seq   | 0   | 1      |

autocarry_analyze (82 steps): 39 interp-ite, 8 interp-binop,
2 interp-scalar, 2 interp-datatype, 1 guarded-seq; 27 JIT.
fti_buffer_loop: 3 JIT / 1 interp (guarded-seq).

Every dominant interp shape is blocked from Cranelift by the same two
sorts: String and Datatype values. There was no per-shape gap worth
closing one shape at a time — the cost was the *interpreter itself*:

- 3–6 Z3 FFI calls per AST node per tick (`Z3_get_ast_kind`,
  `Z3_to_app`, `Z3_get_decl_kind`, children…);
- a symbol decode + fresh `String` + HashMap lookup per variable read;
- a full datatype-sort rescan (`accessor_field_index`) per accessor
  eval, with per-field symbol decodes;
- two `std::env::var` calls per recognizer eval (trace gating);
- per tick: `build_inputs` rebuilt a `format!("_{name}")`-keyed map over
  all manifest fields (driver: 1,543) and `run_program` cloned it.

## What landed (kernel/src/functionize/, commit-paired with this note)

1. **`low.rs` — one-time total lowering** of every step/predicate AST to
   a native `LExpr` IR: variables → interned slot ids, literals decoded,
   accessor field indices + recognizer targets precomputed. Per-tick
   evaluation is a pure Rust tree walk — zero FFI. Semantics mirror
   `eval.rs` arm for arm (same coercions, lazy ITE/∧/∨, same
   refuse-to-Z3 `None`s); any node the legacy interp would refuse lowers
   to `Unsupported` (evals `None`), so lowering cannot fail.
2. **Slot env end-to-end**: `run_program` fills a `Vec<Option<Sv>>`
   directly from `prev_state`/results (no HashMap build, no env clone);
   `RunOut` returns a manifest-aligned state vec (no per-field clone in
   tick.rs); JIT steps pack inputs by slot id through a reused scratch
   buffer.
3. **`Cow<Sv>` evaluation**: Var reads / field access / seq indexing /
   recognizers borrow; the single clone happens at step binding.
4. **String-scan fast paths** (the FSM text-filter pattern): ASCII byte
   paths for len/substr/indexof/at, plus a per-tick per-SLOT memo
   (ascii-ness, char count, char→byte cursor) for non-ASCII strings —
   Evident source is full of `∈`/`⟨⟩`, so the ASCII path alone did not
   fire on `input`. The cursor seeks **bidirectionally** O(delta)
   (topo order ≠ offset order; a reset-to-0 on backward seeks re-walked
   the whole prefix and erased the win). The memo is slot-keyed, never
   pointer-keyed (freed+reallocated buffers could alias), and is
   invalidated on every slot write.

Escape hatch: `EVIDENT_FUNCTIONIZE_LOWER=0` runs the legacy FFI
interpreter (kept intact). Probe: `EVIDENT_FZ_STEPTIME=<tick>` prints
the 25 costliest steps of that tick. The tick-0/1 verify-vs-Z3 gate is
unchanged and exercises the lowered path.

## Before → after (same binary workloads, stdout byte-identical, exit codes equal)

| workload | before | after | × |
| --- | --- | --- | --- |
| driver stage1 compiles fixture-001 (6,269 ticks) | 20.5 s wall / 16.36 s func (2.6 ms/tick) | 3.7 s wall / 2.43 s func (0.39 ms/tick) | **5.5× wall, 6.7× func** |
| autocarry 3-kernel pipeline, 8,463-line driver stream | 1.42 s | **0.30 s** | **4.7×** (clears the ≤1 s wire-in budget) |
| fti_buffer_loop | 7.6 ms total / 5.0 ms func | 2.3 ms / 0.7 ms | 3.3× / 7× |

JIT/interp counts are unchanged (619/1351 on the driver) — the interp
steps themselves got ~7× cheaper; Cranelift coverage was not extended.

## What was NOT done, and why

- **Cranelift for String/Datatype steps**: heap values need runtime
  helpers + a string heap ABI; after lowering, the remaining driver tick
  is a flat ~0.1 µs/step tail across 1,763 steps — no single shape
  dominates, so per-shape JIT work buys little for its risk. The probe
  data is in this note's history if that changes.
- **Skipping unchanged steps across ticks** (dirty tracking): the next
  real lever for the tail, but it changes evaluation order semantics
  (guards observing stale vs recomputed values must be proven
  equivalent) — architectural, not this wave.
- The remaining driver profile: one ~4 µs cursor seek (`next_char`
  after a far backward jump) + ~1 µs ITE chains; everything else
  ≤0.5 µs.
