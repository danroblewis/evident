;; Wave-5a pivot proof: drive compiler/translate_arith.ev's new
;; BuildZ3*-emitting shape end-to-end. Hand-written .smt2 because
;; compiler.smt2 today can't emit the shape this FSM needs (multi-tick
;; state-carry + match on last_results[N] + per-phase ITE chain on
;; Seq(Effect) — see tests/kernel/wave-5a/README.md and STATE.md "open
;; known issues"). The Evident source this fixture models is at
;; tests/kernel/test_translate_arith_via_z3.ev.
;;
;; Expression: 3 + 4 * 5. Built in Z3 in-memory (no SMT-LIB text
;; concatenation), then Z3_ast_to_string + __cstr.copy recover the
;; canonical pretty-print "(+ 3 (* 4 5))". Exit 0 iff match.
;;
;; Phase plan (each phase emits ONE effect; the return lands on the
;; next tick):
;;
;;   0  Z3_mk_config                                  → cfg_h
;;   1  Z3_mk_context(cfg)                            → ctx_h
;;   2  Z3_mk_int_sort(ctx)                           → int_sort_h
;;   3  Z3_mk_int(ctx, 3, sort)                       → n3_h
;;   4  Z3_mk_int(ctx, 4, sort)                       → n4_h
;;   5  Z3_mk_int(ctx, 5, sort)                       → n5_h
;;   6  malloc(16)                                    → mul_buf
;;   7  __mem.write_long(mul_buf,    n4_h)
;;   8  __mem.write_long(mul_buf+8,  n5_h)
;;   9  Z3_mk_mul(ctx, 2, mul_buf)                    → mul_h
;;  10  Z3_inc_ref(ctx, mul_h)                        (pin past tick)
;;  11  malloc(16)                                    → add_buf
;;  12  __mem.write_long(add_buf,    n3_h)
;;  13  __mem.write_long(add_buf+8,  mul_h)
;;  14  Z3_mk_add(ctx, 2, add_buf)                    → add_h
;;  15  Z3_inc_ref(ctx, add_h)
;;  16  Z3_ast_to_string(ctx, add_h)                  → str_ptr
;;  17  __cstr.copy(str_ptr)                          → rendered (StringResult)
;;  18  Exit (0 iff rendered == "(+ 3 (* 4 5))", else 4..6)
;;
;; Expected: kernel exit 0.
;; Run:   kernel/target/release/kernel tests/kernel/wave-5a-arith/translate_arith_via_z3.smt2

;; manifest: state-fields = phase:Int cfg_h:Int ctx_h:Int int_sort_h:Int n3_h:Int n4_h:Int n5_h:Int mul_buf:Int mul_h:Int add_buf:Int add_h:Int str_ptr:Int rendered:String
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

(declare-fun cfg_h () Int)        (declare-fun _cfg_h () Int)
(declare-fun ctx_h () Int)        (declare-fun _ctx_h () Int)
(declare-fun int_sort_h () Int)   (declare-fun _int_sort_h () Int)
(declare-fun n3_h () Int)         (declare-fun _n3_h () Int)
(declare-fun n4_h () Int)         (declare-fun _n4_h () Int)
(declare-fun n5_h () Int)         (declare-fun _n5_h () Int)
(declare-fun mul_buf () Int)      (declare-fun _mul_buf () Int)
(declare-fun mul_h () Int)        (declare-fun _mul_h () Int)
(declare-fun add_buf () Int)      (declare-fun _add_buf () Int)
(declare-fun add_h () Int)        (declare-fun _add_h () Int)
(declare-fun str_ptr () Int)      (declare-fun _str_ptr () Int)
(declare-fun rendered () String)  (declare-fun _rendered () String)

;; Capture rule per handle: result of phase N's effect is readable on
;; phase N+1, so the capture compares phase = (effect_phase + 1).
(assert (= cfg_h      (ite is_first_tick 0  (ite (= phase 1)  (ir_at 0) _cfg_h))))
(assert (= ctx_h      (ite is_first_tick 0  (ite (= phase 2)  (ir_at 0) _ctx_h))))
(assert (= int_sort_h (ite is_first_tick 0  (ite (= phase 3)  (ir_at 0) _int_sort_h))))
(assert (= n3_h       (ite is_first_tick 0  (ite (= phase 4)  (ir_at 0) _n3_h))))
(assert (= n4_h       (ite is_first_tick 0  (ite (= phase 5)  (ir_at 0) _n4_h))))
(assert (= n5_h       (ite is_first_tick 0  (ite (= phase 6)  (ir_at 0) _n5_h))))
(assert (= mul_buf    (ite is_first_tick 0  (ite (= phase 7)  (ir_at 0) _mul_buf))))
;; mul_h: Z3_mk_mul runs on phase 9, so capture on phase 10.
(assert (= mul_h      (ite is_first_tick 0  (ite (= phase 10) (ir_at 0) _mul_h))))
;; add_buf: malloc runs on phase 11, capture on phase 12.
(assert (= add_buf    (ite is_first_tick 0  (ite (= phase 12) (ir_at 0) _add_buf))))
;; add_h: Z3_mk_add runs on phase 14, capture on phase 15.
(assert (= add_h      (ite is_first_tick 0  (ite (= phase 15) (ir_at 0) _add_h))))
;; str_ptr: Z3_ast_to_string runs on phase 16, capture on phase 17.
(assert (= str_ptr    (ite is_first_tick 0  (ite (= phase 17) (ir_at 0) _str_ptr))))
;; rendered: __cstr.copy runs on phase 17, capture on phase 18 (StringResult).
(assert (= rendered   (ite is_first_tick "" (ite (= phase 18) (sr_at 0) _rendered))))

(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (>= effects__len 0))

(assert
  (ite (= phase 0) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_config" __Empty_LibArg)))
  (ite (= phase 1) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_context"
              (__Cell_LibArg (ArgInt cfg_h) __Empty_LibArg))))
  (ite (= phase 2) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int_sort"
              (__Cell_LibArg (ArgInt ctx_h) __Empty_LibArg))))
  (ite (= phase 3) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt 3)
                  (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 4) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt 4)
                  (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 5) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt 5)
                  (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 6) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libc" "malloc"
              (__Cell_LibArg (ArgInt 16) __Empty_LibArg))))
  (ite (= phase 7) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt mul_buf)
                (__Cell_LibArg (ArgInt n4_h) __Empty_LibArg)))))
  (ite (= phase 8) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ mul_buf 8))
                (__Cell_LibArg (ArgInt n5_h) __Empty_LibArg)))))
  (ite (= phase 9) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_mul"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt 2)
                  (__Cell_LibArg (ArgInt mul_buf) __Empty_LibArg))))))
  (ite (= phase 10) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt mul_h) __Empty_LibArg)))))
  (ite (= phase 11) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libc" "malloc"
              (__Cell_LibArg (ArgInt 16) __Empty_LibArg))))
  (ite (= phase 12) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt add_buf)
                (__Cell_LibArg (ArgInt n3_h) __Empty_LibArg)))))
  (ite (= phase 13) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ add_buf 8))
                (__Cell_LibArg (ArgInt mul_h) __Empty_LibArg)))))
  (ite (= phase 14) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_add"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt 2)
                  (__Cell_LibArg (ArgInt add_buf) __Empty_LibArg))))))
  (ite (= phase 15) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt add_h) __Empty_LibArg)))))
  (ite (= phase 16) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_ast_to_string"
              (__Cell_LibArg (ArgInt ctx_h)
                (__Cell_LibArg (ArgInt add_h) __Empty_LibArg)))))
  (ite (= phase 17) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__cstr" "copy"
              (__Cell_LibArg (ArgInt str_ptr) __Empty_LibArg))))
    ;; phase ≥ 18: Exit. 0 iff rendered matches; 4 on mismatch; 5 on
    ;; not-yet-set (shouldn't happen at this phase).
    (and (= effects__len 1) (= (select effects 0)
            (Exit (ite (= rendered "(+ 3 (* 4 5))") 0
                  (ite (= rendered "") 5 4)))))
  )))))))))))))))))))
