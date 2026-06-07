;; stage0 sizing-spike toy — HAND-WRITTEN driver shell template.
;;
;; Style precedent: tests/kernel/z3_ops/sugar_ops.smt2 (28-phase FSM,
;; exit 0). This shell supplies the four capabilities the fossil cannot
;; emit (fossil-subset.md, "honest bottom line"):
;;   1. conditional effects writer   (phase-1 parse gate, phase-11 verdict)
;;   2. last_results readback        (ir_at / sr_at + per-phase capture)
;;   3. payload extraction           (StringResult__f0 etc. in ir_at/sr_at)
;;   4. state-phase machine          (phase counter + ITE effect chain)
;; Everything else — token classification (match→ite), parse-shape
;; dispatch (bare-name composition + single-binop pins) — is
;; FOSSIL-COMPILED from tests/seam/stage0_toy/{lexkind,parsedispatch}.ev
;; and spliced in at the @splice markers by scripts/stitch-stage0.sh.
;;
;; Program: read one line "x = 5" (ReadLine), lex it into three fields,
;; classify each via the spliced lexkind instances, gate on the spliced
;; parse-shape dispatcher (132 = letter '=' digit), then build the Z3 AST
;; (= x 5) via libz3 calls, read back Z3_ast_to_string via __cstr.copy,
;; print it, and Exit(0) iff it rendered as expected.
;;
;; Run:   echo "x = 5" | kernel tests/seam/stage0_toy/toy.smt2
;; Exit:  0 = rendered "(= x 5)"; 4 = AST rendered differently;
;;        5 = parse gate rejected the input line.

;; manifest: state-fields = phase:Int line:String cfg_h:Int ctx_h:Int sort_h:Int sym_h:Int xc_h:Int n_h:Int eq_h:Int str_ptr:Int rendered:String
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

;; -- captures (effect emitted at phase N returns at phase N+1) --------
(declare-fun line () String) (declare-fun _line () String)
(assert (= line (ite is_first_tick "" (ite (= phase 1) (sr_at 0) _line))))
(declare-fun cfg_h () Int) (declare-fun _cfg_h () Int)
(assert (= cfg_h (ite is_first_tick 0 (ite (= phase 2) (ir_at 0) _cfg_h))))
(declare-fun ctx_h () Int) (declare-fun _ctx_h () Int)
(assert (= ctx_h (ite is_first_tick 0 (ite (= phase 3) (ir_at 0) _ctx_h))))
(declare-fun sort_h () Int) (declare-fun _sort_h () Int)
(assert (= sort_h (ite is_first_tick 0 (ite (= phase 4) (ir_at 0) _sort_h))))
(declare-fun sym_h () Int) (declare-fun _sym_h () Int)
(assert (= sym_h (ite is_first_tick 0 (ite (= phase 5) (ir_at 0) _sym_h))))
(declare-fun xc_h () Int) (declare-fun _xc_h () Int)
(assert (= xc_h (ite is_first_tick 0 (ite (= phase 6) (ir_at 0) _xc_h))))
(declare-fun n_h () Int) (declare-fun _n_h () Int)
(assert (= n_h (ite is_first_tick 0 (ite (= phase 7) (ir_at 0) _n_h))))
(declare-fun eq_h () Int) (declare-fun _eq_h () Int)
(assert (= eq_h (ite is_first_tick 0 (ite (= phase 8) (ir_at 0) _eq_h))))
(declare-fun str_ptr () Int) (declare-fun _str_ptr () Int)
(assert (= str_ptr (ite is_first_tick 0 (ite (= phase 10) (ir_at 0) _str_ptr))))
(declare-fun rendered () String) (declare-fun _rendered () String)
(assert (= rendered (ite is_first_tick "" (ite (= phase 11) (sr_at 0) _rendered))))

;; -- lexer: split `line` into three space-separated fields ------------
;; (hand-written shell privilege: full SMT String theory)
(define-fun sp1 () Int (str.indexof line " " 0))
(define-fun sp2 () Int (str.indexof line " " (+ sp1 1)))
(define-fun fld1 () String (str.substr line 0 sp1))
(define-fun fld2 () String (str.substr line (+ sp1 1) (- sp2 (+ sp1 1))))
(define-fun fld3 () String (str.substr line (+ sp2 1) (- (str.len line) (+ sp2 1))))
(define-fun is_letter ((c String)) Bool (and (str.<= "a" c) (str.<= c "z")))
(define-fun is_digit ((c String)) Bool (and (str.<= "0" c) (str.<= c "9")))
;; char-class encoding contract with lexkind.ev:
;;   letter→StringResult(field)  '='→RealResult(0.0)  digit→IntResult(value)
(define-fun classify ((f String)) Result
  (ite (= f "") NoResult
  (ite (is_letter (str.at f 0)) (StringResult f)
  (ite (is_digit (str.at f 0)) (IntResult (str.to_int f))
  (ite (= (str.at f 0) "=") (RealResult 0.0)
  NoResult)))))
(define-fun rhs_val () Int (str.to_int fld3))

;; -- FOSSIL-COMPILED dispatch claims spliced here ----------------------
;; @splice lexkind.out.smt2 lk_=lxa_
;; @splice lexkind.out.smt2 lk_=lxb_
;; @splice lexkind.out.smt2 lk_=lxc_
;; @splice parsedispatch.out.smt2 pd_=pd_ pw_=pw_

;; -- shell↔claim bindings ----------------------------------------------
(assert (= lxa_cls (classify fld1)))
(assert (= lxb_cls (classify fld2)))
(assert (= lxc_cls (classify fld3)))
(assert (= pd_ka lxa_kind))
(assert (= pd_kb lxb_kind))
(assert (= pd_kc lxc_kind))
(define-fun parse_ok () Bool (= pd_shape 132))

(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (>= effects__len 0))

;; Phases:
;; 0  ReadLine                                  → line
;; 1  gate: parse_ok ? Z3_mk_config : Exit(5)   → cfg_h
;; 2  Z3_mk_context(cfg)                        → ctx_h
;; 3  Z3_mk_int_sort(ctx)                       → sort_h
;; 4  Z3_mk_string_symbol(ctx, fld1)            → sym_h
;; 5  Z3_mk_const(ctx, sym, sort)               → xc_h
;; 6  Z3_mk_int(ctx, rhs_val, sort)             → n_h
;; 7  Z3_mk_eq(ctx, xc, n)                      → eq_h
;; 8  Z3_inc_ref(ctx, eq)
;; 9  Z3_ast_to_string(ctx, eq)                 → str_ptr
;; 10 __cstr.copy(str_ptr)                      → rendered
;; 11 puts(rendered); Exit(0 iff rendered = "(= x 5)" else 4)

(assert
  (ite (= phase 0) (and (= effects__len 1) (= (select effects 0) ReadLine))
  (ite (= phase 1)
    (ite parse_ok
      (and (= effects__len 1) (= (select effects 0) (LibCall "libz3" "Z3_mk_config" __Empty_LibArg)))
      (and (= effects__len 2)
           (= (select effects 0) (LibCall "libc" "puts" (__Cell_LibArg (ArgStr "stage0: parse error") __Empty_LibArg)))
           (= (select effects 1) (Exit 5))))
  (ite (= phase 2) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_context" (__Cell_LibArg (ArgInt cfg_h) __Empty_LibArg))))
  (ite (= phase 3) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int_sort" (__Cell_LibArg (ArgInt ctx_h) __Empty_LibArg))))
  (ite (= phase 4) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_string_symbol"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgStr fld1) __Empty_LibArg)))))
  (ite (= phase 5) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_const"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt sym_h) (__Cell_LibArg (ArgInt sort_h) __Empty_LibArg))))))
  (ite (= phase 6) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_int"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt rhs_val) (__Cell_LibArg (ArgInt sort_h) __Empty_LibArg))))))
  (ite (= phase 7) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_mk_eq"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt xc_h) (__Cell_LibArg (ArgInt n_h) __Empty_LibArg))))))
  (ite (= phase 8) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_inc_ref"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt eq_h) __Empty_LibArg)))))
  (ite (= phase 9) (and (= effects__len 1) (= (select effects 0)
            (LibCall "libz3" "Z3_ast_to_string"
              (__Cell_LibArg (ArgInt ctx_h) (__Cell_LibArg (ArgInt eq_h) __Empty_LibArg)))))
  (ite (= phase 10) (and (= effects__len 1) (= (select effects 0)
            (LibCall "__cstr" "copy" (__Cell_LibArg (ArgInt str_ptr) __Empty_LibArg))))
    ;; phase 11 — verdict
    (and (= effects__len 2)
         (= (select effects 0) (LibCall "libc" "puts" (__Cell_LibArg (ArgStr rendered) __Empty_LibArg)))
         (= (select effects 1) (Exit (ite (= rendered "(= x 5)") 0 4))))
  ))))))))))))
