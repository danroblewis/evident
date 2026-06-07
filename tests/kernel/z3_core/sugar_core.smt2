;; stdlib/z3_core.ev shape proof: drive the §1/§2/§10/§12 lifecycle
;; end-to-end through the kernel — config → context →
;; set_ast_print_mode → int sort → string symbol → const → mk_int →
;; mk_func_decl (caller-built domain array) → mk_app (caller-built
;; args array) → ast_to_string → cstr copy → compare → Exit.
;;
;; This file is HAND-WRITTEN .smt2, NOT produced by compiler.smt2.
;; The Evident source it models is tests/kernel/z3_core/sugar_core.ev.
;; compiler.smt2 today can't emit that shape (multi-tick state-carry
;; + match on last_results[N] + per-phase ITE chain — the same gap
;; tests/kernel/wave-5a/z3_solve_x42.smt2 documents: output silently
;; truncates to the phase-only carry). Same precedent as wave-5a:
;; the .ev fixture is the source of truth, this file proves the
;; LibCall shapes against the real libz3.
;;
;; Every LibCall below is byte-for-byte the constraint body of the
;; corresponding Build* sugar claim:
;;   BuildZ3MkConfig / MkContext        (stdlib/kernel.ev)
;;   BuildZ3SetAstPrintMode             (stdlib/z3_core.ev)
;;   BuildZ3MkIntSort / MkStringSymbol / MkConst / MkInt /
;;   IncRef / AstToString / Malloc / MemWriteLong / CstrCopy
;;                                      (stdlib/z3_ast.ev)
;;   BuildZ3MkFuncDecl / MkApp          (stdlib/z3_core.ev)
;;
;; f : Int × Int → Int is declared via a 16-byte caller-built domain
;; array (malloc + 2× __mem.write_long of the Int sort handle),
;; applied to (x, 5) via a second caller-built args array, and Z3
;; renders the application as "(f x 5)".
;;
;; Refcount note: each composite Z3 handle is inc_ref'd on the tick
;; after capture (wave-5a discipline).
;;
;; Expected: kernel exit code 0 (rendered string matches).
;; Run: kernel/target/release/kernel tests/kernel/z3_core/sugar_core.smt2

;; manifest: state-fields = phase:Int cfg_h:Int ctx_h:Int int_sort_h:Int xsym_h:Int x_h:Int n5_h:Int fsym_h:Int dom_buf:Int f_h:Int args_buf:Int app_h:Int str_ptr:Int rendered:String
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

(declare-fun cfg_h () Int) (declare-fun _cfg_h () Int)
(assert (= cfg_h (ite is_first_tick 0 (ite (= phase 1) (ir_at 0) _cfg_h))))
(declare-fun ctx_h () Int) (declare-fun _ctx_h () Int)
(assert (= ctx_h (ite is_first_tick 0 (ite (= phase 2) (ir_at 0) _ctx_h))))
;; int sort captured on phase 4 (mk_int_sort runs on phase 3;
;; phase 2's set_ast_print_mode returns void, nothing to capture)
(declare-fun int_sort_h () Int) (declare-fun _int_sort_h () Int)
(assert (= int_sort_h (ite is_first_tick 0 (ite (= phase 4) (ir_at 0) _int_sort_h))))
(declare-fun xsym_h () Int) (declare-fun _xsym_h () Int)
(assert (= xsym_h (ite is_first_tick 0 (ite (= phase 5) (ir_at 0) _xsym_h))))
(declare-fun x_h () Int) (declare-fun _x_h () Int)
(assert (= x_h (ite is_first_tick 0 (ite (= phase 6) (ir_at 0) _x_h))))
;; n5 captured on phase 8 (mk_int runs on phase 7)
(declare-fun n5_h () Int) (declare-fun _n5_h () Int)
(assert (= n5_h (ite is_first_tick 0 (ite (= phase 8) (ir_at 0) _n5_h))))
;; f's symbol captured on phase 10 (mk_string_symbol runs on phase 9)
(declare-fun fsym_h () Int) (declare-fun _fsym_h () Int)
(assert (= fsym_h (ite is_first_tick 0 (ite (= phase 10) (ir_at 0) _fsym_h))))
(declare-fun dom_buf () Int) (declare-fun _dom_buf () Int)
(assert (= dom_buf (ite is_first_tick 0 (ite (= phase 11) (ir_at 0) _dom_buf))))
;; f decl captured on phase 14 (mk_func_decl runs on phase 13)
(declare-fun f_h () Int) (declare-fun _f_h () Int)
(assert (= f_h (ite is_first_tick 0 (ite (= phase 14) (ir_at 0) _f_h))))
(declare-fun args_buf () Int) (declare-fun _args_buf () Int)
(assert (= args_buf (ite is_first_tick 0 (ite (= phase 16) (ir_at 0) _args_buf))))
;; app captured on phase 19 (mk_app runs on phase 18)
(declare-fun app_h () Int) (declare-fun _app_h () Int)
(assert (= app_h (ite is_first_tick 0 (ite (= phase 19) (ir_at 0) _app_h))))
;; char* captured on phase 21 (ast_to_string runs on phase 20)
(declare-fun str_ptr () Int) (declare-fun _str_ptr () Int)
(assert (= str_ptr (ite is_first_tick 0 (ite (= phase 21) (ir_at 0) _str_ptr))))
;; rendered captured on phase 22 (__cstr.copy runs on phase 21)
(declare-fun rendered () String) (declare-fun _rendered () String)
(assert (= rendered (ite is_first_tick "" (ite (= phase 22) (sr_at 0) _rendered))))

(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (>= effects__len 0))

;; Phases:
;; 0  Z3_mk_config                                  → cfg_h@1
;; 1  Z3_mk_context(cfg)                            → ctx_h@2
;; 2  Z3_set_ast_print_mode(ctx, 2)                   (void)
;; 3  Z3_mk_int_sort(ctx)                           → int_sort_h@4
;; 4  Z3_mk_string_symbol(ctx, "x")                 → xsym_h@5
;; 5  Z3_mk_const(ctx, xsym, int_sort)              → x_h@6
;; 6  Z3_inc_ref(ctx, x_h)
;; 7  Z3_mk_int(ctx, 5, int_sort)                   → n5_h@8
;; 8  Z3_inc_ref(ctx, n5_h)
;; 9  Z3_mk_string_symbol(ctx, "f")                 → fsym_h@10
;; 10 malloc(16)                                    → dom_buf@11
;; 11 __mem.write_long(dom_buf,   int_sort_h)
;; 12 __mem.write_long(dom_buf+8, int_sort_h)
;; 13 Z3_mk_func_decl(ctx, fsym, 2, dom_buf, int_sort) → f_h@14
;; 14 Z3_inc_ref(ctx, f_h)
;; 15 malloc(16)                                    → args_buf@16
;; 16 __mem.write_long(args_buf,   x_h)
;; 17 __mem.write_long(args_buf+8, n5_h)
;; 18 Z3_mk_app(ctx, f_h, 2, args_buf)              → app_h@19
;; 19 Z3_inc_ref(ctx, app_h)
;; 20 Z3_ast_to_string(ctx, app_h)                  → str_ptr@21
;; 21 __cstr.copy(str_ptr)                          → rendered@22
;; 22 Exit(0 iff rendered = "(f x 5)" else 4)

(assert
  (ite (= phase 0) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_config" __Empty_LibArg)))
  (ite (= phase 1) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_context"
              (__Cell_LibArg (ArgInt cfg_h) __Empty_LibArg))))
  (ite (= phase 2) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_set_ast_print_mode"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 2) __Empty_LibArg)))))
  (ite (= phase 3) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int_sort"
              (__Cell_LibArg (ArgInt ctx_h) __Empty_LibArg))))
  (ite (= phase 4) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_string_symbol"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgStr "x") __Empty_LibArg)))))
  (ite (= phase 5) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_const"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt xsym_h)
                (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 6) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt x_h) __Empty_LibArg)))))
  (ite (= phase 7) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt 5)
                (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))
  (ite (= phase 8) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt n5_h) __Empty_LibArg)))))
  (ite (= phase 9) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_string_symbol"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgStr "f") __Empty_LibArg)))))
  (ite (= phase 10) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libc" "malloc"
              (__Cell_LibArg (ArgInt 16) __Empty_LibArg))))
  (ite (= phase 11) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt dom_buf) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg)))))
  (ite (= phase 12) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ dom_buf 8)) (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg)))))
  (ite (= phase 13) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_func_decl"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt fsym_h)
                (__Cell_LibArg (ArgInt 2) (__Cell_LibArg (ArgInt dom_buf)
                  (__Cell_LibArg (ArgInt int_sort_h) __Empty_LibArg))))))))
  (ite (= phase 14) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt f_h) __Empty_LibArg)))))
  (ite (= phase 15) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libc" "malloc"
              (__Cell_LibArg (ArgInt 16) __Empty_LibArg))))
  (ite (= phase 16) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt args_buf) (__Cell_LibArg (ArgInt x_h) __Empty_LibArg)))))
  (ite (= phase 17) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ args_buf 8)) (__Cell_LibArg (ArgInt n5_h) __Empty_LibArg)))))
  (ite (= phase 18) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_app"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt f_h)
                (__Cell_LibArg (ArgInt 2) (__Cell_LibArg (ArgInt args_buf) __Empty_LibArg)))))))
  (ite (= phase 19) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt app_h) __Empty_LibArg)))))
  (ite (= phase 20) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_ast_to_string"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt app_h) __Empty_LibArg)))))
  (ite (= phase 21) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__cstr" "copy"
              (__Cell_LibArg (ArgInt str_ptr) __Empty_LibArg))))
    ;; phase 22
    (and (= effects__len 1) (= (select effects 0)
            (Exit (ite (= rendered "(f x 5)") 0 4))))
  )))))))))))))))))))))))
