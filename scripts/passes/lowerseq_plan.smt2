;; manifest: state-fields = b_acc:String eff_nop:Effect eff_out:Effect l_acc:String out_reg:String phase:Int q_acc:String qc:Int tk_b_at:Int tk_b_has:Bool tk_b_key:String tk_b_n:String tk_b_ve:Int tk_b_vs:Int tk_consume:Bool tk_done:Bool tk_eof_now:Bool tk_gt:Int tk_is_b:Bool tk_is_l:Bool tk_is_q:Bool tk_kind:String tk_l_has:Bool tk_name:String tk_ph:Int tk_q_base:String tk_q_claim:String tk_q_d1:Int tk_q_d2:Int tk_q_gt:Int tk_q_lt:Int tk_q_name:String tk_q_next:Int tk_q_scalar:Bool tk_qc:Int tk_reg_entry:String tk_register:Bool tk_reof:Bool tk_resolving:Bool tk_rest:String tk_rline:String
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 16

(declare-datatypes ((Result 0)) (((NoResult) (IntResult (IntResult__f0 Int)) (StringResult (StringResult__f0 String)) (RealResult (RealResult__f0 Real)) (EofResult) (ErrorResult (ErrorResult__f0 String)))))
(declare-datatypes ((LibArg 0)) (((ArgInt (ArgInt__f0 Int)) (ArgStr (ArgStr__f0 String)) (ArgReal (ArgReal__f0 Real)) (ArgRef (ArgRef__f0 Int)))))
(declare-datatypes ((__SeqOf_LibArg 0)) (((__Empty_LibArg) (__Cell_LibArg (head LibArg) (tail __SeqOf_LibArg)))))
(declare-datatypes ((Effect 0)) (((ReadLine) (ReadFile (ReadFile__f0 String)) (WriteFile (WriteFile__f0 String) (WriteFile__f1 String)) (LibCall (LibCall__f0 String) (LibCall__f1 String) (LibCall__f2 __SeqOf_LibArg)) (Exit (Exit__f0 Int)))))
(declare-fun effects__len () Int)
(declare-fun last_results__len () Int)
(declare-fun _phase () Int)
(declare-fun is_first_tick () Bool)
(declare-fun tk_ph () Int)
(declare-fun last_results () (Array Int Result))
(declare-fun tk_rline () String)
(declare-fun tk_reof () Bool)
(declare-fun tk_consume () Bool)
(declare-fun tk_eof_now () Bool)
(declare-fun tk_kind () String)
(declare-fun tk_gt () Int)
(declare-fun tk_name () String)
(declare-fun tk_rest () String)
(declare-fun tk_is_q () Bool)
(declare-fun tk_is_b () Bool)
(declare-fun tk_is_l () Bool)
(declare-fun phase () Int)
(declare-fun _q_acc () String)
(declare-fun q_acc () String)
(declare-fun _b_acc () String)
(declare-fun b_acc () String)
(declare-fun _l_acc () String)
(declare-fun l_acc () String)
(declare-fun _qc () Int)
(declare-fun tk_qc () Int)
(declare-fun tk_resolving () Bool)
(declare-fun tk_done () Bool)
(declare-fun tk_q_lt () Int)
(declare-fun tk_q_gt () Int)
(declare-fun tk_q_name () String)
(declare-fun tk_q_d1 () Int)
(declare-fun tk_q_base () String)
(declare-fun tk_q_d2 () Int)
(declare-fun tk_q_claim () String)
(declare-fun tk_q_next () Int)
(declare-fun tk_q_scalar () Bool)
(declare-fun tk_b_key () String)
(declare-fun tk_b_at () Int)
(declare-fun tk_b_has () Bool)
(declare-fun tk_b_vs () Int)
(declare-fun tk_b_ve () Int)
(declare-fun tk_b_n () String)
(declare-fun tk_l_has () Bool)
(declare-fun tk_register () Bool)
(declare-fun tk_reg_entry () String)
(declare-fun qc () Int)
(declare-fun _out_reg () String)
(declare-fun out_reg () String)
(declare-fun eff_nop () Effect)
(declare-fun eff_out () Effect)
(declare-fun effects () (Array Int Effect))
(assert (>= effects__len 0))
(assert (>= last_results__len 0))
(assert (= tk_ph (ite is_first_tick 0 _phase)))
(assert (= tk_rline
   (ite ((_ is StringResult) (select last_results 1))
        (StringResult__f0 (select last_results 1))
        "")))
(assert (= tk_reof (ite ((_ is EofResult) (select last_results 1)) true false)))
(assert (= tk_consume (and (= tk_ph 1) (not tk_reof))))
(assert (= tk_eof_now (and (= tk_ph 1) tk_reof)))
(assert (let ((a!1 (ite (and tk_consume (>= (str.len tk_rline) 1))
                (str.substr tk_rline 0 1)
                "")))
  (= tk_kind a!1)))
(assert (= tk_gt (ite tk_consume (str.indexof tk_rline "\u{27e9}" 0) (- 0 1))))
(assert (= tk_name
   (ite (and tk_consume (>= tk_gt 0)) (str.substr tk_rline 2 (- tk_gt 2)) "")))
(assert (let ((a!1 (str.substr tk_rline (+ tk_gt 1) (- (- (str.len tk_rline) tk_gt) 1))))
  (= tk_rest (ite (and tk_consume (>= tk_gt 0)) a!1 ""))))
(assert (= tk_is_q (= tk_kind "Q")))
(assert (= tk_is_b (= tk_kind "B")))
(assert (= tk_is_l (= tk_kind "L")))
(assert (= phase (ite is_first_tick 1 (ite tk_consume 1 (ite tk_eof_now 13 13)))))
(assert (= q_acc
   (ite is_first_tick
        ""
        (ite tk_is_q
             (str.++ _q_acc "\u{27e6}" tk_name "\u{27e7}" tk_rest)
             _q_acc))))
(assert (= b_acc
   (ite is_first_tick
        ""
        (ite tk_is_b
             (str.++ _b_acc "\u{27e6}" tk_name "\u{27e7}" tk_rest)
             _b_acc))))
(assert (= l_acc
   (ite is_first_tick
        ""
        (ite tk_is_l (str.++ _l_acc "\u{27e6}" tk_name "\u{27e7}") _l_acc))))
(assert (= tk_qc (ite tk_eof_now 0 _qc)))
(assert (= tk_resolving (and (= tk_ph 13) (< _qc (str.len _q_acc)))))
(assert (= tk_done (and (= tk_ph 13) (>= _qc (str.len _q_acc)))))
(assert (= tk_q_lt (ite tk_resolving _qc (- 0 1))))
(assert (= tk_q_gt (ite tk_resolving (str.indexof _q_acc "\u{27e7}" _qc) (- 0 1))))
(assert (let ((a!1 (ite tk_resolving
                (str.substr _q_acc (+ _qc 1) (- (- tk_q_gt _qc) 1))
                "")))
  (= tk_q_name a!1)))
(assert (= tk_q_d1 (ite tk_resolving (str.indexof _q_acc "\u{2982}" tk_q_gt) (- 0 1))))
(assert (let ((a!1 (ite tk_resolving
                (str.substr _q_acc (+ tk_q_gt 1) (- (- tk_q_d1 tk_q_gt) 1))
                "")))
  (= tk_q_base a!1)))
(assert (= tk_q_d2
   (ite tk_resolving (str.indexof _q_acc "\u{2982}" (+ tk_q_d1 1)) (- 0 1))))
(assert (let ((a!1 (ite tk_resolving
                (str.substr _q_acc (+ tk_q_d1 1) (- (- tk_q_d2 tk_q_d1) 1))
                "")))
  (= tk_q_claim a!1)))
(assert (= tk_q_next (ite tk_resolving (+ tk_q_d2 1) _qc)))
(assert (= tk_q_scalar
   (or (= tk_q_base "Int") (= tk_q_base "String") (= tk_q_base "Bool"))))
(assert (= tk_b_key (str.++ "\u{27e6}" tk_q_name "\u{27e7}" tk_q_claim "\u{2982}")))
(assert (= tk_b_at (ite tk_resolving (str.indexof _b_acc tk_b_key 0) (- 0 1))))
(assert (= tk_b_has (and tk_resolving (>= tk_b_at 0))))
(assert (= tk_b_vs (ite tk_b_has (+ tk_b_at (str.len tk_b_key)) (- 0 1))))
(assert (= tk_b_ve (ite tk_b_has (str.indexof _b_acc "\u{2982}" tk_b_vs) (- 0 1))))
(assert (= tk_b_n
   (ite (and tk_b_has (>= tk_b_ve tk_b_vs))
        (str.substr _b_acc tk_b_vs (- tk_b_ve tk_b_vs))
        "")))
(assert (let ((a!1 (and tk_resolving
                (>= (str.indexof _l_acc
                                 (str.++ "\u{27e6}" tk_q_name "\u{27e7}")
                                 0)
                    0))))
  (= tk_l_has a!1)))
(assert (= tk_register (and tk_resolving tk_q_scalar tk_b_has)))
(assert (= tk_reg_entry
   (ite tk_register
        (str.++ "\u{27e6}"
                tk_q_name
                "\u{27e7}"
                tk_q_base
                "\u{2982}"
                tk_b_n
                "\u{2982}"
                (ite tk_l_has "1" "0"))
        "")))
(assert (= qc (ite is_first_tick 0 (ite tk_eof_now 0 (ite tk_resolving tk_q_next _qc)))))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite tk_eof_now
                     ""
                     (ite tk_resolving (str.++ _out_reg tk_reg_entry) _out_reg)))))
  (= out_reg a!1)))
(assert (= eff_nop (LibCall "libc" "getpid" __Empty_LibArg)))
(assert (= eff_out
   (LibCall "libc" "puts" (__Cell_LibArg (ArgStr _out_reg) __Empty_LibArg))))
(assert (let ((a!1 (and (= effects__len 2)
                (= (select effects 0) eff_nop)
                (= (select effects 1) ReadLine)))
      (a!2 (=> tk_done
               (and (= effects__len 3)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) eff_nop)
                    (= (select effects 2) (Exit 0)))))
      (a!3 (=> (not tk_done)
               (and (= effects__len 2)
                    (= (select effects 0) eff_nop)
                    (= (select effects 1) eff_nop)))))
(let ((a!4 (=> (not is_first_tick)
               (and (=> tk_consume a!1) (=> (not tk_consume) (and a!2 a!3))))))
  (and (=> is_first_tick a!1) a!4))))
(declare-fun _eff_nop () Effect)
(declare-fun _eff_out () Effect)
(declare-fun _tk_b_at () Int)
(declare-fun _tk_b_has () Bool)
(declare-fun _tk_b_key () String)
(declare-fun _tk_b_n () String)
(declare-fun _tk_b_ve () Int)
(declare-fun _tk_b_vs () Int)
(declare-fun _tk_consume () Bool)
(declare-fun _tk_done () Bool)
(declare-fun _tk_eof_now () Bool)
(declare-fun _tk_gt () Int)
(declare-fun _tk_is_b () Bool)
(declare-fun _tk_is_l () Bool)
(declare-fun _tk_is_q () Bool)
(declare-fun _tk_kind () String)
(declare-fun _tk_l_has () Bool)
(declare-fun _tk_name () String)
(declare-fun _tk_ph () Int)
(declare-fun _tk_q_base () String)
(declare-fun _tk_q_claim () String)
(declare-fun _tk_q_d1 () Int)
(declare-fun _tk_q_d2 () Int)
(declare-fun _tk_q_gt () Int)
(declare-fun _tk_q_lt () Int)
(declare-fun _tk_q_name () String)
(declare-fun _tk_q_next () Int)
(declare-fun _tk_q_scalar () Bool)
(declare-fun _tk_qc () Int)
(declare-fun _tk_reg_entry () String)
(declare-fun _tk_register () Bool)
(declare-fun _tk_reof () Bool)
(declare-fun _tk_resolving () Bool)
(declare-fun _tk_rest () String)
(declare-fun _tk_rline () String)
