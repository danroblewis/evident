;; Wave 5a end-to-end proof: an Evident-shape multi-tick FSM that
;; drives libz3 from inside the kernel and observes `sat` for the
;; trivial formula `(assert (= x 42))`.
;;
;; This file is HAND-WRITTEN .smt2, NOT produced by compiler.smt2.
;; The Evident source it models is at
;; tests/kernel/test_z3_libcall_solve.ev. compiler.smt2 today
;; can't emit this shape (multi-tick state-carry + match on
;; last_results[N] + per-phase ITE chain — verified this session:
;; output silently truncates to the phase-only carry). Closing
;; that gap is a real compiler.smt2 extension. This fixture
;; proves the KERNEL + LIBZ3 substrate is ready for the day the
;; compiler can emit it.
;;
;; Refcount note: each Z3 handle is inc_ref'd on the tick after
;; capture. Without it, Z3 GCs the AST/solver between ticks and
;; the next call crashes (`mutex lock failed: Invalid argument`).
;;
;; Expected: kernel exit code 0 (sat == lbool 1).
;; Run:   kernel/target/release/kernel tests/kernel/wave-5a/z3_solve_x42.smt2

;; manifest: state-fields = phase:Int cfg_h:Int ctx_h:Int sol_h:Int vec_h:Int ast_h:Int sat:Int
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 2

(declare-datatypes ((Result 0)) (((NoResult) (IntResult (IntResult__f0 Int)) (StringResult (StringResult__f0 String)) (RealResult (RealResult__f0 Real)) (EofResult) (ErrorResult (ErrorResult__f0 String)))))
(declare-datatypes ((LibArg 0)) (((ArgInt (ArgInt__f0 Int)) (ArgStr (ArgStr__f0 String)) (ArgReal (ArgReal__f0 Real)))))
(declare-datatypes ((__SeqOf_LibArg 0)) (((__Empty_LibArg) (__Cell_LibArg (head LibArg) (tail __SeqOf_LibArg)))))
(declare-datatypes ((Effect 0)) (((ReadLine) (ReadFile (ReadFile__f0 String)) (WriteFile (WriteFile__f0 String) (WriteFile__f1 String)) (LibCall (LibCall__f0 String) (LibCall__f1 String) (LibCall__f2 __SeqOf_LibArg)) (Exit (Exit__f0 Int)))))

(declare-fun is_first_tick () Bool)
(declare-fun last_results () (Array Int Result))
(declare-fun last_results__len () Int)
(assert (>= last_results__len 0))

(declare-fun phase () Int) (declare-fun _phase () Int)
(assert (= phase (ite is_first_tick 0 (+ _phase 1))))

(define-fun ir_at ((i Int)) Int
  (ite ((_ is IntResult) (select last_results i)) (IntResult__f0 (select last_results i)) 0))

(declare-fun cfg_h () Int) (declare-fun _cfg_h () Int)
(assert (= cfg_h (ite is_first_tick 0 (ite (= phase 1) (ir_at 0) _cfg_h))))
(declare-fun ctx_h () Int) (declare-fun _ctx_h () Int)
(assert (= ctx_h (ite is_first_tick 0 (ite (= phase 2) (ir_at 0) _ctx_h))))
(declare-fun sol_h () Int) (declare-fun _sol_h () Int)
(assert (= sol_h (ite is_first_tick 0 (ite (= phase 3) (ir_at 0) _sol_h))))
(declare-fun vec_h () Int) (declare-fun _vec_h () Int)
;; vec is captured on phase 5 (parse runs on phase 4, returns to phase 5)
(assert (= vec_h (ite is_first_tick 0 (ite (= phase 5) (ir_at 0) _vec_h))))
(declare-fun ast_h () Int) (declare-fun _ast_h () Int)
;; ast captured on phase 7 (vec_get runs on phase 6)
(assert (= ast_h (ite is_first_tick 0 (ite (= phase 7) (ir_at 0) _ast_h))))
(declare-fun sat () Int) (declare-fun _sat () Int)
;; sat captured on phase 10 (check runs on phase 9)
(assert (= sat (ite is_first_tick (- 99) (ite (= phase 10) (ir_at 0) _sat))))

(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (>= effects__len 0))

;; Phases:
;; 0  MkConfig
;; 1  MkContext(cfg)
;; 2  MkSolver(ctx)
;; 3  SolverIncRef(ctx, sol)            (retain solver)
;; 4  Parse(ctx, src)
;; 5  AstVectorIncRef(ctx, vec)         (retain vector)
;; 6  AstVectorGet(ctx, vec, 0)
;; 7  IncRef(ctx, ast)                  (retain AST)
;; 8  SolverAssert(ctx, sol, ast)
;; 9  SolverCheck(ctx, sol)
;; 10 Exit(sat code)

(assert
  (ite (= phase 0) (and (= effects__len 1) (= (select effects 0) (LibCall "libz3" "Z3_mk_config" __Empty_LibArg)))
  (ite (= phase 1) (and (= effects__len 1) (= (select effects 0) (LibCall "libz3" "Z3_mk_context" (__Cell_LibArg (ArgInt cfg_h) __Empty_LibArg))))
  (ite (= phase 2) (and (= effects__len 1) (= (select effects 0) (LibCall "libz3" "Z3_mk_solver" (__Cell_LibArg (ArgInt ctx_h) __Empty_LibArg))))
  (ite (= phase 3) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_solver_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt sol_h) __Empty_LibArg)))))
  (ite (= phase 4) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_parse_smtlib2_string"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgStr "(declare-const x Int) (assert (= x 42))")
                  (__Cell_LibArg (ArgInt 0) (__Cell_LibArg (ArgInt 0)
                    (__Cell_LibArg (ArgInt 0) (__Cell_LibArg (ArgInt 0)
                      (__Cell_LibArg (ArgInt 0) (__Cell_LibArg (ArgInt 0) __Empty_LibArg))))))))))
            )
  (ite (= phase 5) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_ast_vector_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt vec_h) __Empty_LibArg)))))
  (ite (= phase 6) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_ast_vector_get"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt vec_h) (__Cell_LibArg (ArgInt 0) __Empty_LibArg))))))
  (ite (= phase 7) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt ast_h) __Empty_LibArg)))))
  (ite (= phase 8) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_solver_assert"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt sol_h) (__Cell_LibArg (ArgInt ast_h) __Empty_LibArg))))))
  (ite (= phase 9) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_solver_check"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt sol_h) __Empty_LibArg)))))
    ;; phase 10
    (and (= effects__len 1) (= (select effects 0)
            (Exit (ite (= sat 1) 0 (ite (= sat 0) 3 (ite (= sat (- 1)) 4 5))))))
  )))))))))))
