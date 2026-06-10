;; manifest: state-fields = acc_a:String acc_b:String bare_reg:String bind_reg:String carry_reg:String cur_a:Int cur_b:Int eff_nop:Effect eff_out:Effect fsm_set:String inj_out:String ins_out:String ment_reg:String phase:Int slot_reg:String work_list:String
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
(declare-fun _t_ph () Int)
(declare-fun last_results () (Array Int Result))
(declare-fun _t_rline () String)
(declare-fun _t_reof () Bool)
(declare-fun _t_rd () Bool)
(declare-fun _t_kind () String)
(declare-fun _t_payload () String)
(declare-fun _work_list () String)
(declare-fun _cur_a () Int)
(declare-fun _t_dr_done () Bool)
(declare-fun _t_dr_run () Bool)
(declare-fun _t_fx_run () Bool)
(declare-fun _t_dr_dot () Int)
(declare-fun _t_dr_gt () Int)
(declare-fun _t_dr_f () String)
(declare-fun _t_dr_x () String)
(declare-fun _fsm_set () String)
(declare-fun _t_dr_fsm_ok () Bool)
(declare-fun _cur_b () Int)
(declare-fun _bind_reg () String)
(declare-fun _t_dr_m () Int)
(declare-fun _t_dr_vs () Int)
(declare-fun _t_dr_vc () Int)
(declare-fun _t_dr_val () String)
(declare-fun _t_dr_f2 () String)
(declare-fun _t_dr_bkey () String)
(declare-fun _bare_reg () String)
(declare-fun _t_dr_bp () Int)
(declare-fun _carry_reg () String)
(declare-fun _t_dr_new () Bool)
(declare-fun _t_dr_item_done () Bool)
(declare-fun _t_dr_hdr_go () Bool)
(declare-fun _t_mn_on () Bool)
(declare-fun _ment_reg () String)
(declare-fun _t_mn_p () Int)
(declare-fun _t_mn_done () Bool)
(declare-fun _t_mn_e () Int)
(declare-fun _t_mn_par () String)
(declare-fun _t_mn_bkey () String)
(declare-fun _t_mn_bp () Int)
(declare-fun _t_mn_new () Bool)
(declare-fun _t_iw_done () Bool)
(declare-fun _t_iw_run () Bool)
(declare-fun _t_iw_gt () Int)
(declare-fun _t_iw_key () String)
(declare-fun _t_iw_x () String)
(declare-fun _t_iw_c1 () Int)
(declare-fun _t_iw_c2 () Int)
(declare-fun _t_iw_cp () Int)
(declare-fun _t_iw_isc () Bool)
(declare-fun _t_iw_ln0 () Bool)
(declare-fun _t_jw_done () Bool)
(declare-fun _t_jw_run () Bool)
(declare-fun _t_jw_cA () Int)
(declare-fun _t_jw_brk () Int)
(declare-fun _t_jw_sub () String)
(declare-fun _t_jw_slot () String)
(declare-fun _t_jw_vc () Int)
(declare-fun _t_jw_val () String)
(declare-fun _t_jw_fc () Int)
(declare-fun _t_jw_nc () Int)
(declare-fun _t_jw_ln () String)
(declare-fun _acc_b () String)
(declare-fun _t_jw_newgrp () Bool)
(declare-fun _acc_a () String)
(declare-fun _t_jw_flush () Bool)
(declare-fun _slot_reg () String)
(declare-fun _t_jw_hp () Int)
(declare-fun _t_jw_have () Bool)
(declare-fun _t_jw_give () Bool)
(declare-fun _t_jw_hdr () Bool)
(declare-fun _t_jw_add () String)
(declare-fun phase () Int)
(declare-fun fsm_set () String)
(declare-fun bare_reg () String)
(declare-fun bind_reg () String)
(declare-fun slot_reg () String)
(declare-fun ment_reg () String)
(declare-fun carry_reg () String)
(declare-fun work_list () String)
(declare-fun _ins_out () String)
(declare-fun ins_out () String)
(declare-fun _inj_out () String)
(declare-fun inj_out () String)
(declare-fun cur_a () Int)
(declare-fun cur_b () Int)
(declare-fun acc_a () String)
(declare-fun acc_b () String)
(declare-fun eff_nop () Effect)
(declare-fun eff_out () Effect)
(declare-fun _t_read_go () Bool)
(declare-fun effects () (Array Int Effect))
(assert (>= effects__len 0))
(assert (>= last_results__len 0))
(assert (= _t_ph (ite is_first_tick 0 _phase)))
(assert (= _t_rline
   (ite ((_ is StringResult) (select last_results 1))
        (StringResult__f0 (select last_results 1))
        "")))
(assert (= _t_reof (ite ((_ is EofResult) (select last_results 1)) true false)))
(assert (= _t_rd (and (= _t_ph 1) (not _t_reof))))
(assert (= _t_kind (ite _t_rd (str.at _t_rline 0) "")))
(assert (let ((a!1 (ite _t_rd (str.substr _t_rline 1 (- (str.len _t_rline) 1)) "")))
  (= _t_payload a!1)))
(assert (= _t_dr_done (and (= _t_ph 7) (>= _cur_a (str.len _work_list)))))
(assert (= _t_dr_run (and (= _t_ph 7) (not _t_dr_done))))
(assert (= _t_fx_run (or _t_dr_run (= _t_ph 6))))
(assert (= _t_dr_dot (ite _t_fx_run (str.indexof _work_list "." _cur_a) 0)))
(assert (= _t_dr_gt (ite _t_fx_run (str.indexof _work_list "\u{27e9}" _cur_a) 0)))
(assert (let ((a!1 (ite _t_fx_run
                (str.substr _work_list (+ _cur_a 1) (- (- _t_dr_dot _cur_a) 1))
                "")))
  (= _t_dr_f a!1)))
(assert (let ((a!1 (ite _t_fx_run
                (str.substr _work_list
                            (+ _t_dr_dot 1)
                            (- (- _t_dr_gt _t_dr_dot) 1))
                "")))
  (= _t_dr_x a!1)))
(assert (= _t_dr_fsm_ok
   (and _t_dr_run
        (str.contains _fsm_set (str.++ "\u{27e8}" _t_dr_f "\u{27e9}")))))
(assert (= _t_dr_m
   (ite _t_dr_fsm_ok
        (str.indexof _bind_reg
                     (str.++ "\u{2772}" _t_dr_f "\u{2982}" _t_dr_x "\u{2773}")
                     _cur_b)
        (- 0 1))))
(assert (= _t_dr_vs
   (ite (>= _t_dr_m 0) (+ _t_dr_m (str.len _t_dr_f) (str.len _t_dr_x) 3) 0)))
(assert (= _t_dr_vc (ite (>= _t_dr_m 0) (str.indexof _bind_reg "\u{2982}" _t_dr_vs) 0)))
(assert (= _t_dr_val
   (ite (>= _t_dr_m 0)
        (str.substr _bind_reg _t_dr_vs (- _t_dr_vc _t_dr_vs))
        "0")))
(assert (let ((a!1 (- (- (str.indexof _bind_reg "\u{2982}" (+ _t_dr_vc 1)) _t_dr_vc) 1)))
  (= _t_dr_f2 (ite (>= _t_dr_m 0) (str.substr _bind_reg (+ _t_dr_vc 1) a!1) ""))))
(assert (= _t_dr_bkey (str.++ "\u{27e8}" _t_dr_f2 "." _t_dr_val "\u{27e9}")))
(assert (let ((a!1 (ite (and (>= _t_dr_m 0) (not (= _t_dr_val "0")))
                (str.indexof _bare_reg _t_dr_bkey 0)
                (- 0 1))))
  (= _t_dr_bp a!1)))
(assert (= _t_dr_new (and (>= _t_dr_bp 0) (not (str.contains _carry_reg _t_dr_bkey)))))
(assert (= _t_dr_item_done (and _t_dr_run (or (not _t_dr_fsm_ok) (< _t_dr_m 0)))))
(assert (let ((a!1 (and _t_dr_item_done
                (>= (str.indexof _bind_reg
                                 (str.++ "\u{2772}"
                                         _t_dr_f
                                         "\u{2982}"
                                         _t_dr_x
                                         "\u{2773}\u{2208}")
                                 0)
                    0))))
  (= _t_dr_hdr_go a!1)))
(assert (= _t_mn_on (= _t_ph 6)))
(assert (= _t_mn_p
   (ite _t_mn_on
        (str.indexof _ment_reg (str.++ "\u{2768}" _t_dr_f "\u{2982}") _cur_b)
        (- 0 1))))
(assert (= _t_mn_done (and _t_mn_on (< _t_mn_p 0))))
(assert (= _t_mn_e (ite (>= _t_mn_p 0) (str.indexof _ment_reg "\u{2769}" _t_mn_p) 0)))
(assert (let ((a!1 (str.substr _ment_reg
                       (+ _t_mn_p (str.len _t_dr_f) 2)
                       (- (- (- _t_mn_e _t_mn_p) (str.len _t_dr_f)) 2))))
  (= _t_mn_par (ite (>= _t_mn_p 0) a!1 ""))))
(assert (= _t_mn_bkey (str.++ "\u{27e8}" _t_mn_par "." _t_dr_x "\u{27e9}")))
(assert (= _t_mn_bp (ite (>= _t_mn_p 0) (str.indexof _bare_reg _t_mn_bkey 0) (- 0 1))))
(assert (= _t_mn_new (and (>= _t_mn_bp 0) (not (str.contains _carry_reg _t_mn_bkey)))))
(assert (= _t_iw_done (and (= _t_ph 8) (>= _cur_a (str.len _bare_reg)))))
(assert (= _t_iw_run (and (= _t_ph 8) (not _t_iw_done))))
(assert (= _t_iw_gt (ite _t_iw_run (str.indexof _bare_reg "\u{27e9}" _cur_a) 0)))
(assert (let ((a!1 (ite _t_iw_run
                (str.substr _bare_reg _cur_a (- (+ _t_iw_gt 1) _cur_a))
                "")))
  (= _t_iw_key a!1)))
(assert (let ((a!1 (str.substr _bare_reg
                       (+ (str.indexof _bare_reg "." _cur_a) 1)
                       (- (- _t_iw_gt (str.indexof _bare_reg "." _cur_a)) 1))))
  (= _t_iw_x (ite _t_iw_run a!1 ""))))
(assert (= _t_iw_c1 (ite _t_iw_run (str.indexof _bare_reg "\u{2982}" _t_iw_gt) 0)))
(assert (= _t_iw_c2 (ite _t_iw_run (str.indexof _bare_reg "\u{2982}" (+ _t_iw_c1 1)) 0)))
(assert (= _t_iw_cp (ite _t_iw_run (str.indexof _carry_reg _t_iw_key 0) (- 0 1))))
(assert (= _t_iw_isc (>= _t_iw_cp 0)))
(assert (let ((a!1 (= (str.substr _bare_reg (+ _t_iw_c1 1) (- (- _t_iw_c2 _t_iw_c1) 1))
              "0")))
  (= _t_iw_ln0 (and _t_iw_run a!1))))
(assert (= _t_jw_done (and (= _t_ph 9) (>= _cur_a (str.len _bind_reg)))))
(assert (= _t_jw_run (and (= _t_ph 9) (not _t_jw_done))))
(assert (= _t_jw_cA (ite _t_jw_run (str.indexof _bind_reg "\u{2982}" _cur_a) 0)))
(assert (= _t_jw_brk (ite _t_jw_run (str.indexof _bind_reg "\u{2773}" _cur_a) 0)))
(assert (let ((a!1 (ite _t_jw_run
                (str.substr _bind_reg (+ _cur_a 1) (- (- _t_jw_cA _cur_a) 1))
                "")))
  (= _t_jw_sub a!1)))
(assert (let ((a!1 (ite _t_jw_run
                (str.substr _bind_reg
                            (+ _t_jw_cA 1)
                            (- (- _t_jw_brk _t_jw_cA) 1))
                "")))
  (= _t_jw_slot a!1)))
(assert (= _t_jw_vc (ite _t_jw_run (str.indexof _bind_reg "\u{2982}" _t_jw_brk) 0)))
(assert (let ((a!1 (ite _t_jw_run
                (str.substr _bind_reg
                            (+ _t_jw_brk 1)
                            (- (- _t_jw_vc _t_jw_brk) 1))
                "0")))
  (= _t_jw_val a!1)))
(assert (= _t_jw_fc (ite _t_jw_run (str.indexof _bind_reg "\u{2982}" (+ _t_jw_vc 1)) 0)))
(assert (= _t_jw_nc (ite _t_jw_run (str.indexof _bind_reg "\u{2982}" (+ _t_jw_fc 1)) 0)))
(assert (let ((a!1 (ite _t_jw_run
                (str.substr _bind_reg
                            (+ _t_jw_fc 1)
                            (- (- _t_jw_nc _t_jw_fc) 1))
                "")))
  (= _t_jw_ln a!1)))
(assert (= _t_jw_newgrp (and _t_jw_run (not (= _t_jw_ln _acc_b)))))
(assert (= _t_jw_flush
   (and (or _t_jw_newgrp _t_jw_done) (not (= _acc_a "")) (= _t_ph 9))))
(assert (= _t_jw_hp
   (ite _t_jw_run
        (str.indexof _slot_reg (str.++ "\u{2770}" _t_jw_ln "\u{2771}") 0)
        (- 0 1))))
(assert (let ((a!1 (- (str.indexof _slot_reg
                           "\u{2771}"
                           (+ _t_jw_hp (str.len _t_jw_ln) 2))
              _t_jw_hp)))
(let ((a!2 (str.substr _slot_reg
                       (+ _t_jw_hp (str.len _t_jw_ln) 2)
                       (- (- a!1 (str.len _t_jw_ln)) 2))))
  (= _t_jw_have
     (and (>= _t_jw_hp 0)
          (str.contains a!2 (str.++ "\u{2982}_" _t_jw_slot "\u{2982}")))))))
(assert (let ((a!1 (and _t_jw_run
                (not (= _t_jw_val "0"))
                (not _t_jw_have)
                (str.contains _fsm_set
                              (str.++ (str.++ "\u{27e8}" _t_jw_sub) "\u{27e9}"))
                (str.contains _carry_reg
                              (str.++ (str.++ "\u{27e8}" _t_jw_sub)
                                      "."
                                      _t_jw_slot
                                      "\u{27e9}")))))
  (= _t_jw_give a!1)))
(assert (let ((a!1 (and _t_jw_run (= (str.at _bind_reg (+ _t_jw_brk 1)) "\u{2208}"))))
  (= _t_jw_hdr a!1)))
(assert (= _t_jw_add
   (ite _t_jw_hdr
        (str.++ (str.++ ", _" _t_jw_slot) " " _t_jw_val)
        (str.++ (str.++ ", _" _t_jw_slot) " \u{21a6} _" _t_jw_val))))
(assert (let ((a!1 (ite (= _t_ph 8)
                (ite _t_iw_done 9 8)
                (ite (= _t_ph 9) (ite _t_jw_done 10 9) (ite (= _t_ph 10) 11 11)))))
(let ((a!2 (ite (= _t_ph 1)
                (ite _t_reof 7 1)
                (ite (= _t_ph 7)
                     (ite _t_dr_done 8 (ite _t_dr_hdr_go 6 7))
                     (ite (= _t_ph 6) (ite _t_mn_done 7 6) a!1)))))
  (= phase (ite is_first_tick 1 a!2)))))
(assert (= fsm_set
   (ite is_first_tick
        ""
        (ite (= _t_kind "F") (str.++ _fsm_set _t_payload) _fsm_set))))
(assert (= bare_reg
   (ite is_first_tick
        ""
        (ite (= _t_kind "B") (str.++ _bare_reg _t_payload) _bare_reg))))
(assert (= bind_reg
   (ite is_first_tick
        ""
        (ite (= _t_kind "D") (str.++ _bind_reg _t_payload) _bind_reg))))
(assert (= slot_reg
   (ite is_first_tick
        ""
        (ite (= _t_kind "S") (str.++ _slot_reg _t_payload) _slot_reg))))
(assert (= ment_reg
   (ite is_first_tick
        ""
        (ite (= _t_kind "M") (str.++ _ment_reg _t_payload) _ment_reg))))
(assert (let ((a!1 (- (str.indexof _bare_reg
                           "\u{2982}"
                           (+ _t_dr_bp (str.len _t_dr_bkey)))
              _t_dr_bp))
      (a!3 (- (str.indexof _bare_reg
                           "\u{2982}"
                           (+ _t_mn_bp (str.len _t_mn_bkey)))
              _t_mn_bp)))
(let ((a!2 (str.++ _carry_reg
                   _t_dr_bkey
                   (str.substr _bare_reg
                               (+ _t_dr_bp (str.len _t_dr_bkey))
                               (- a!1 (str.len _t_dr_bkey)))
                   "\u{2982}"))
      (a!4 (str.++ _carry_reg
                   _t_mn_bkey
                   (str.substr _bare_reg
                               (+ _t_mn_bp (str.len _t_mn_bkey))
                               (- a!3 (str.len _t_mn_bkey)))
                   "\u{2982}")))
(let ((a!5 (ite is_first_tick
                ""
                (ite (= _t_kind "C")
                     (str.++ _carry_reg _t_payload)
                     (ite _t_dr_new a!2 (ite _t_mn_new a!4 _carry_reg))))))
  (= carry_reg a!5)))))
(assert (let ((a!1 (str.++ _work_list
                   (str.substr _t_payload
                               0
                               (+ (str.indexof _t_payload "\u{27e9}" 0) 1)))))
(let ((a!2 (ite (= _t_kind "C")
                a!1
                (ite _t_dr_new
                     (str.++ _work_list _t_dr_bkey)
                     (ite _t_mn_new (str.++ _work_list _t_mn_bkey) _work_list)))))
  (= work_list (ite is_first_tick "" a!2)))))
(assert (let ((a!1 (- (str.indexof _carry_reg
                           "\u{2982}"
                           (+ _t_iw_cp (str.len _t_iw_key)))
              _t_iw_cp)))
(let ((a!2 (str.++ _ins_out
                   "\u{27e6}"
                   (str.substr _bare_reg
                               (+ _t_iw_c1 1)
                               (- (- _t_iw_c2 _t_iw_c1) 1))
                   "\u{27e7}_"
                   _t_iw_x
                   " \u{2208} "
                   (str.substr _carry_reg
                               (+ _t_iw_cp (str.len _t_iw_key))
                               (- a!1 (str.len _t_iw_key))))))
(let ((a!3 (ite is_first_tick
                ""
                (ite (and _t_iw_run _t_iw_isc (not _t_iw_ln0)) a!2 _ins_out))))
  (= ins_out a!3)))))
(assert (= inj_out
   (ite is_first_tick
        ""
        (ite _t_jw_flush
             (str.++ _inj_out "\u{27e6}" _acc_b "\u{27e7}" _acc_a)
             _inj_out))))
(assert (let ((a!1 (ite _t_iw_run
                (+ _t_iw_c2 1)
                (ite (and (= _t_ph 8) _t_iw_done)
                     0
                     (ite _t_jw_run (+ _t_jw_nc 1) _cur_a)))))
(let ((a!2 (ite (and (= _t_ph 7) _t_dr_done)
                0
                (ite (= _t_ph 6) (ite _t_mn_done (+ _t_dr_gt 1) _cur_a) a!1))))
(let ((a!3 (ite _t_dr_run
                (ite _t_dr_hdr_go
                     _cur_a
                     (ite _t_dr_item_done (+ _t_dr_gt 1) _cur_a))
                a!2)))
  (= cur_a (ite is_first_tick 0 (ite (= _t_ph 1) 0 a!3)))))))
(assert (let ((a!1 (ite _t_dr_run
                (ite _t_dr_item_done 0 (+ _t_dr_m 1))
                (ite (= _t_ph 6) (ite _t_mn_done 0 (+ _t_mn_p 1)) _cur_b))))
  (= cur_b (ite is_first_tick 0 (ite (= _t_ph 1) 0 a!1)))))
(assert (let ((a!1 (ite (= _t_ph 7)
                ""
                (ite _t_jw_newgrp
                     (ite _t_jw_give _t_jw_add "")
                     (ite (and _t_jw_run _t_jw_give)
                          (str.++ _acc_a _t_jw_add)
                          _acc_a)))))
  (= acc_a (ite is_first_tick "" a!1))))
(assert (= acc_b
   (ite is_first_tick "" (ite (= _t_ph 7) "" (ite _t_jw_run _t_jw_ln _acc_b)))))
(assert (= eff_nop (LibCall "libc" "getpid" __Empty_LibArg)))
(assert (let ((a!1 (__Cell_LibArg (ArgStr (ite (= _t_ph 10) _ins_out _inj_out))
                          __Empty_LibArg)))
  (= eff_out (LibCall "libc" "puts" a!1))))
(assert (= _t_read_go (and (= _t_ph 1) (not _t_reof))))
(assert (let ((a!1 (and (= effects__len 2)
                (= (select effects 0) eff_nop)
                (= (select effects 1) ReadLine)))
      (a!2 (=> (= _t_ph 10)
               (and (= effects__len 2)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) eff_nop))))
      (a!3 (=> (= _t_ph 11)
               (and (= effects__len 2)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) (Exit 0)))))
      (a!4 (=> (not (= _t_ph 11))
               (and (= effects__len 2)
                    (= (select effects 0) eff_nop)
                    (= (select effects 1) eff_nop)))))
(let ((a!5 (and a!2 (=> (not (= _t_ph 10)) (and a!3 a!4)))))
(let ((a!6 (=> (not is_first_tick)
               (and (=> _t_read_go a!1) (=> (not _t_read_go) a!5)))))
  (and (=> is_first_tick a!1) a!6)))))
(declare-fun _eff_nop () Effect)
(declare-fun _eff_out () Effect)
