;; Validation proof for stdlib/z3_ops.ev (z3-sugar-inventory §3+§4).
;;
;; This file is HAND-WRITTEN .smt2, NOT produced by compiler.smt2,
;; following the tests/kernel/wave-5a/z3_solve_x42.smt2 precedent:
;; the committed compiler.smt2 cannot emit the multi-tick
;; state-carry + match-on-last_results + per-phase ITE chain shape
;; (verified again 2026-06-07 — output silently truncates at the
;; phase carry). The Evident source this models is the sibling
;; tests/kernel/z3_ops/sugar_ops.ev.
;;
;; The FSM builds `(ite (and (< 3 4) (not false)) (+ 1 2) 9)` via
;; the exact LibCall shapes stdlib/z3_ops.ev's sugar claims wrap:
;;   Z3_mk_lt   (ctx, l, r)            — §3 binary comparison
;;   Z3_mk_not  (ctx, x)               — §4 unary
;;   Z3_mk_and  (ctx, num_args, args*) — §4 array-form (malloc'd buffer)
;;   Z3_mk_add  (ctx, num_args, args*) — §3 array-form (z3_ast.ev)
;;   Z3_mk_ite  (ctx, cond, then, else)— §4 4-arg ternary
;; then reads back Z3's canonical printing (Z3_ast_to_string +
;; __cstr.copy) and exits 0 iff it equals the expected string
;; (cross-checked against a native C libz3 probe).
;;
;; Array marshaling: malloc 16 bytes, two __mem.write_long stores
;; of the operand handles, pass the buffer pointer + num_args = 2.
;; Composite AST handles are Z3_inc_ref'd on the tick after capture
;; (never captured FROM an inc_ref tick — void returns surface as
;; garbage IntResult).
;;
;; Expected: kernel exit code 0 (rendered string matches).
;;           4 = built AST printed differently (stdout shows it),
;;           5..8 unused here.
;; Run:   kernel/target/release/kernel tests/kernel/z3_ops/sugar_ops.smt2

;; manifest: state-fields = phase:Int cfg_h:Int ctx_h:Int int_sort_h:Int n1_h:Int n2_h:Int n3_h:Int n4_h:Int n9_h:Int false_h:Int lt_h:Int not_h:Int and_buf:Int and_h:Int add_buf:Int add_h:Int ite_h:Int str_ptr:Int rendered:String
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
(define-fun sr_at ((i Int)) String
  (ite ((_ is StringResult) (select last_results i)) (StringResult__f0 (select last_results i)) ""))

;; Captures. Effect emitted at phase N returns at phase N+1; each
;; handle is pinned on its capture tick, else carried.
(declare-fun cfg_h () Int) (declare-fun _cfg_h () Int)
(assert (= cfg_h (ite is_first_tick 0 (ite (= phase 1) (ir_at 0) _cfg_h))))
(declare-fun ctx_h () Int) (declare-fun _ctx_h () Int)
(assert (= ctx_h (ite is_first_tick 0 (ite (= phase 2) (ir_at 0) _ctx_h))))
(declare-fun int_sort_h () Int) (declare-fun _int_sort_h () Int)
(assert (= int_sort_h (ite is_first_tick 0 (ite (= phase 3) (ir_at 0) _int_sort_h))))
(declare-fun n1_h () Int) (declare-fun _n1_h () Int)
(assert (= n1_h (ite is_first_tick 0 (ite (= phase 4) (ir_at 0) _n1_h))))
(declare-fun n2_h () Int) (declare-fun _n2_h () Int)
(assert (= n2_h (ite is_first_tick 0 (ite (= phase 5) (ir_at 0) _n2_h))))
(declare-fun n3_h () Int) (declare-fun _n3_h () Int)
(assert (= n3_h (ite is_first_tick 0 (ite (= phase 6) (ir_at 0) _n3_h))))
(declare-fun n4_h () Int) (declare-fun _n4_h () Int)
(assert (= n4_h (ite is_first_tick 0 (ite (= phase 7) (ir_at 0) _n4_h))))
(declare-fun n9_h () Int) (declare-fun _n9_h () Int)
(assert (= n9_h (ite is_first_tick 0 (ite (= phase 8) (ir_at 0) _n9_h))))
(declare-fun false_h () Int) (declare-fun _false_h () Int)
(assert (= false_h (ite is_first_tick 0 (ite (= phase 9) (ir_at 0) _false_h))))
(declare-fun lt_h () Int) (declare-fun _lt_h () Int)
(assert (= lt_h (ite is_first_tick 0 (ite (= phase 10) (ir_at 0) _lt_h))))
(declare-fun not_h () Int) (declare-fun _not_h () Int)
(assert (= not_h (ite is_first_tick 0 (ite (= phase 12) (ir_at 0) _not_h))))
(declare-fun and_buf () Int) (declare-fun _and_buf () Int)
(assert (= and_buf (ite is_first_tick 0 (ite (= phase 14) (ir_at 0) _and_buf))))
(declare-fun and_h () Int) (declare-fun _and_h () Int)
(assert (= and_h (ite is_first_tick 0 (ite (= phase 17) (ir_at 0) _and_h))))
(declare-fun add_buf () Int) (declare-fun _add_buf () Int)
(assert (= add_buf (ite is_first_tick 0 (ite (= phase 19) (ir_at 0) _add_buf))))
(declare-fun add_h () Int) (declare-fun _add_h () Int)
(assert (= add_h (ite is_first_tick 0 (ite (= phase 22) (ir_at 0) _add_h))))
(declare-fun ite_h () Int) (declare-fun _ite_h () Int)
(assert (= ite_h (ite is_first_tick 0 (ite (= phase 24) (ir_at 0) _ite_h))))
(declare-fun str_ptr () Int) (declare-fun _str_ptr () Int)
(assert (= str_ptr (ite is_first_tick 0 (ite (= phase 26) (ir_at 0) _str_ptr))))
(declare-fun rendered () String) (declare-fun _rendered () String)
(assert (= rendered (ite is_first_tick "" (ite (= phase 27) (sr_at 0) _rendered))))

(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (>= effects__len 0))

;; Phases:
;; 0  Z3_mk_config                      → cfg_h
;; 1  Z3_mk_context(cfg)                → ctx_h
;; 2  Z3_mk_int_sort(ctx)               → int_sort_h
;; 3  Z3_mk_int(ctx, 1, sort)           → n1_h
;; 4  Z3_mk_int(ctx, 2, sort)           → n2_h
;; 5  Z3_mk_int(ctx, 3, sort)           → n3_h
;; 6  Z3_mk_int(ctx, 4, sort)           → n4_h
;; 7  Z3_mk_int(ctx, 9, sort)           → n9_h
;; 8  Z3_mk_false(ctx)                  → false_h
;; 9  Z3_mk_lt(ctx, n3, n4)             → lt_h     [BuildZ3MkLt]
;; 10 Z3_inc_ref(ctx, lt_h)
;; 11 Z3_mk_not(ctx, false_h)           → not_h    [BuildZ3MkNot]
;; 12 Z3_inc_ref(ctx, not_h)
;; 13 malloc 16                         → and_buf
;; 14 __mem.write_long(and_buf, lt_h)
;; 15 __mem.write_long(and_buf+8, not_h)
;; 16 Z3_mk_and(ctx, 2, and_buf)        → and_h    [BuildZ3MkAnd]
;; 17 Z3_inc_ref(ctx, and_h)
;; 18 malloc 16                         → add_buf
;; 19 __mem.write_long(add_buf, n1_h)
;; 20 __mem.write_long(add_buf+8, n2_h)
;; 21 Z3_mk_add(ctx, 2, add_buf)        → add_h    [BuildZ3MkAdd]
;; 22 Z3_inc_ref(ctx, add_h)
;; 23 Z3_mk_ite(ctx, and_h, add_h, n9)  → ite_h    [BuildZ3MkIte]
;; 24 Z3_inc_ref(ctx, ite_h)
;; 25 Z3_ast_to_string(ctx, ite_h)      → str_ptr
;; 26 __cstr.copy(str_ptr)              → rendered
;; 27 Exit(0) iff rendered = expected; else print rendered, Exit(4)

(assert
  (ite (= phase 0) (and (= effects__len 1) (= (select effects 0) (LibCall "libz3" "Z3_mk_config" __Empty_LibArg)))
  (ite (= phase 1) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_context" (__Cell_LibArg (ArgInt cfg_h) __Empty_LibArg))))
  (ite (= phase 2) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int_sort" (__Cell_LibArg (ArgInt ctx_h) __Empty_LibArg))))
  (ite (= phase 3) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 1) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 4) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 2) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 5) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 3) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 6) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 4) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 7) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 9) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 8) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_false" (__Cell_LibArg (ArgInt ctx_h) __Empty_LibArg))))
  (ite (= phase 9) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_lt"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt n3_h) (__Cell_LibArg (ArgInt n4_h) __Empty_LibArg))))))
  (ite (= phase 10) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt lt_h) __Empty_LibArg)))))
  (ite (= phase 11) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_not"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt false_h) __Empty_LibArg)))))
  (ite (= phase 12) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt not_h) __Empty_LibArg)))))
  (ite (= phase 13) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libc" "malloc" (__Cell_LibArg (ArgInt 16) __Empty_LibArg))))
  (ite (= phase 14) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt and_buf) (__Cell_LibArg (ArgInt lt_h) __Empty_LibArg)))))
  (ite (= phase 15) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ and_buf 8)) (__Cell_LibArg (ArgInt not_h) __Empty_LibArg)))))
  (ite (= phase 16) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_and"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 2) (__Cell_LibArg (ArgInt and_buf) __Empty_LibArg))))))
  (ite (= phase 17) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt and_h) __Empty_LibArg)))))
  (ite (= phase 18) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libc" "malloc" (__Cell_LibArg (ArgInt 16) __Empty_LibArg))))
  (ite (= phase 19) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt add_buf) (__Cell_LibArg (ArgInt n1_h) __Empty_LibArg)))))
  (ite (= phase 20) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ add_buf 8)) (__Cell_LibArg (ArgInt n2_h) __Empty_LibArg)))))
  (ite (= phase 21) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_add"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 2) (__Cell_LibArg (ArgInt add_buf) __Empty_LibArg))))))
  (ite (= phase 22) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt add_h) __Empty_LibArg)))))
  (ite (= phase 23) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_ite"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt and_h) (__Cell_LibArg (ArgInt add_h) (__Cell_LibArg (ArgInt n9_h) __Empty_LibArg)))))))
  (ite (= phase 24) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt ite_h) __Empty_LibArg)))))
  (ite (= phase 25) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_ast_to_string"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt ite_h) __Empty_LibArg)))))
  (ite (= phase 26) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__cstr" "copy" (__Cell_LibArg (ArgInt str_ptr) __Empty_LibArg))))
    ;; phase 27 — verdict
    (ite (= rendered "(ite (and (< 3 4) (not false)) (+ 1 2) 9)")
      (and (= effects__len 1) (= (select effects 0) (Exit 0)))
      (and (= effects__len 2)
           (= (select effects 0) (LibCall "libc" "puts" (__Cell_LibArg (ArgStr rendered) __Empty_LibArg)))
           (= (select effects 1) (Exit 4))))
  ))))))))))))))))))))))))))))
