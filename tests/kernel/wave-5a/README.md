# Wave 5a — Z3-from-Evident end-to-end proofs

Hand-written `.smt2` fixtures that exercise the kernel + libz3
substrate for wave 5a's "Z3-as-library FTI" goal. These are
checked into the repo (not generated) because `compiler.smt2`
today can't emit the multi-tick state-carry + `match` on
`last_results[N]` shape the language source needs. The Evident
source these model lives at:

    tests/kernel/test_z3_libcall_solve.ev

## Files

- `z3_solve_x42.smt2` — drives the full lifecycle (MkConfig →
  MkContext → MkSolver → SolverIncRef → Parse → AstVectorIncRef →
  AstVectorGet → IncRef → SolverAssert → SolverCheck → Exit) and
  observes sat (lbool 1) for the trivial formula `(assert (= x 42))`.
  Exit code 0 = SAT.

## Run

    kernel/target/release/kernel tests/kernel/wave-5a/z3_solve_x42.smt2
    echo $?    # 0 = sat, 3 = unknown, 4 = unsat, 5 = non-IntResult

## What this proves

1. `libz3.dylib` resolves via the kernel's runtime dlopen search
   path (`/opt/homebrew/lib`, `/opt/anaconda3/lib/.../z3/lib`, …)
   without the user setting `DYLD_LIBRARY_PATH`.
2. Z3 handles (Z3_config, Z3_context, Z3_solver, Z3_ast_vector,
   Z3_ast) round-trip through the kernel's libffi dispatch as i64
   via the existing `ArgInt`/`IntResult` shape — no new marshaling
   primitives required.
3. The kernel's per-tick Z3 solve (used to satisfy the FSM body)
   coexists with libz3 calls dispatched in effects, AS LONG AS
   the user's ASTs are explicitly `Z3_*_inc_ref`'d on capture.
   Without inc_ref Z3 GCs them between ticks and the next call
   crashes with `mutex lock failed: Invalid argument`.

## What this does NOT prove

- That `compiler.smt2` can produce this output from the Evident
  source. It cannot (verified in session — multi-tick state carry
  + `match` on `last_results[N]` + per-phase ITE chain on
  `Seq(Effect)` RHS all drop in the current compiler).
- That arbitrary Z3 use scales (the chain is hand-tuned; a richer
  pattern needs e.g. `Z3_get_numeral_int64` with an out-pointer,
  which requires `__mem`-backed scratch and per-call inc/dec_ref
  ceremony).
