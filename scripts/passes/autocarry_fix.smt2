;; manifest: state-fields = acc_a:String acc_b:String bare_reg:String bind_reg:String carry_reg:String cur_a:Int cur_b:Int eff_nop:Effect eff_out:Effect eff_ov_exit:Effect eff_ov_msg:Effect fs_n:Int fsm_set_0:String fsm_set_1:String fsm_set_10:String fsm_set_100:String fsm_set_101:String fsm_set_102:String fsm_set_103:String fsm_set_11:String fsm_set_12:String fsm_set_13:String fsm_set_14:String fsm_set_15:String fsm_set_16:String fsm_set_17:String fsm_set_18:String fsm_set_19:String fsm_set_2:String fsm_set_20:String fsm_set_21:String fsm_set_22:String fsm_set_23:String fsm_set_24:String fsm_set_25:String fsm_set_26:String fsm_set_27:String fsm_set_28:String fsm_set_29:String fsm_set_3:String fsm_set_30:String fsm_set_31:String fsm_set_32:String fsm_set_33:String fsm_set_34:String fsm_set_35:String fsm_set_36:String fsm_set_37:String fsm_set_38:String fsm_set_39:String fsm_set_4:String fsm_set_40:String fsm_set_41:String fsm_set_42:String fsm_set_43:String fsm_set_44:String fsm_set_45:String fsm_set_46:String fsm_set_47:String fsm_set_48:String fsm_set_49:String fsm_set_5:String fsm_set_50:String fsm_set_51:String fsm_set_52:String fsm_set_53:String fsm_set_54:String fsm_set_55:String fsm_set_56:String fsm_set_57:String fsm_set_58:String fsm_set_59:String fsm_set_6:String fsm_set_60:String fsm_set_61:String fsm_set_62:String fsm_set_63:String fsm_set_64:String fsm_set_65:String fsm_set_66:String fsm_set_67:String fsm_set_68:String fsm_set_69:String fsm_set_7:String fsm_set_70:String fsm_set_71:String fsm_set_72:String fsm_set_73:String fsm_set_74:String fsm_set_75:String fsm_set_76:String fsm_set_77:String fsm_set_78:String fsm_set_79:String fsm_set_8:String fsm_set_80:String fsm_set_81:String fsm_set_82:String fsm_set_83:String fsm_set_84:String fsm_set_85:String fsm_set_86:String fsm_set_87:String fsm_set_88:String fsm_set_89:String fsm_set_9:String fsm_set_90:String fsm_set_91:String fsm_set_92:String fsm_set_93:String fsm_set_94:String fsm_set_95:String fsm_set_96:String fsm_set_97:String fsm_set_98:String fsm_set_99:String fsm_set_len:Int inj_out:String ins_out:String ment_reg:String phase:Int slot_reg:String tk_cr_add:String tk_dr_bkey:String tk_dr_bp:Int tk_dr_done:Bool tk_dr_dot:Int tk_dr_f:String tk_dr_f2:String tk_dr_fkey:String tk_dr_fsm_ok:Bool tk_dr_gt:Int tk_dr_hdr_go:Bool tk_dr_item_done:Bool tk_dr_m:Int tk_dr_new:Bool tk_dr_run:Bool tk_dr_val:String tk_dr_vc:Int tk_dr_vs:Int tk_dr_x:String tk_fx_run:Bool tk_inj_add:String tk_iw_add:String tk_iw_c1:Int tk_iw_c2:Int tk_iw_cp:Int tk_iw_done:Bool tk_iw_gt:Int tk_iw_isc:Bool tk_iw_key:String tk_iw_ln0:Bool tk_iw_run:Bool tk_iw_x:String tk_jw_add:String tk_jw_brk:Int tk_jw_cA:Int tk_jw_done:Bool tk_jw_fc:Int tk_jw_fkey:String tk_jw_flush:Bool tk_jw_give:Bool tk_jw_have:Bool tk_jw_hdr:Bool tk_jw_hp:Int tk_jw_ln:String tk_jw_nc:Int tk_jw_newgrp:Bool tk_jw_run:Bool tk_jw_slot:String tk_jw_sub:String tk_jw_val:String tk_jw_vc:Int tk_kind:String tk_mn_bkey:String tk_mn_bp:Int tk_mn_done:Bool tk_mn_e:Int tk_mn_new:Bool tk_mn_on:Bool tk_mn_p:Int tk_mn_par:String tk_ov_acca:Bool tk_ov_accb:Bool tk_ov_any:Bool tk_ov_bare:Bool tk_ov_bind:Bool tk_ov_carry:Bool tk_ov_code:Int tk_ov_fs:Bool tk_ov_inj:Bool tk_ov_ins:Bool tk_ov_ment:Bool tk_ov_msg:String tk_ov_slot:Bool tk_ov_work:Bool tk_payload:String tk_ph:Int tk_rd:Bool tk_read_go:Bool tk_reof:Bool tk_rline:String tk_wl_add:String work_list:String
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
(declare-fun fsm_set_len () Int)
(declare-fun bare_reg () String)
(declare-fun bind_reg () String)
(declare-fun slot_reg () String)
(declare-fun ment_reg () String)
(declare-fun carry_reg () String)
(declare-fun work_list () String)
(declare-fun ins_out () String)
(declare-fun inj_out () String)
(declare-fun acc_a () String)
(declare-fun acc_b () String)
(declare-fun _phase () Int)
(declare-fun is_first_tick () Bool)
(declare-fun tk_ph () Int)
(declare-fun last_results () (Array Int Result))
(declare-fun tk_rline () String)
(declare-fun tk_reof () Bool)
(declare-fun tk_rd () Bool)
(declare-fun tk_kind () String)
(declare-fun tk_payload () String)
(declare-fun _work_list () String)
(declare-fun _cur_a () Int)
(declare-fun tk_dr_done () Bool)
(declare-fun tk_dr_run () Bool)
(declare-fun tk_fx_run () Bool)
(declare-fun tk_dr_dot () Int)
(declare-fun tk_dr_gt () Int)
(declare-fun tk_dr_f () String)
(declare-fun tk_dr_x () String)
(declare-fun tk_dr_fkey () String)
(declare-fun fsm_set_103 () String)
(declare-fun fsm_set_102 () String)
(declare-fun fsm_set_101 () String)
(declare-fun fsm_set_100 () String)
(declare-fun fsm_set_99 () String)
(declare-fun fsm_set_98 () String)
(declare-fun fsm_set_97 () String)
(declare-fun fsm_set_96 () String)
(declare-fun fsm_set_95 () String)
(declare-fun fsm_set_94 () String)
(declare-fun fsm_set_93 () String)
(declare-fun fsm_set_92 () String)
(declare-fun fsm_set_91 () String)
(declare-fun fsm_set_90 () String)
(declare-fun fsm_set_89 () String)
(declare-fun fsm_set_88 () String)
(declare-fun fsm_set_87 () String)
(declare-fun fsm_set_86 () String)
(declare-fun fsm_set_85 () String)
(declare-fun fsm_set_84 () String)
(declare-fun fsm_set_83 () String)
(declare-fun fsm_set_82 () String)
(declare-fun fsm_set_81 () String)
(declare-fun fsm_set_80 () String)
(declare-fun fsm_set_79 () String)
(declare-fun fsm_set_78 () String)
(declare-fun fsm_set_77 () String)
(declare-fun fsm_set_76 () String)
(declare-fun fsm_set_75 () String)
(declare-fun fsm_set_74 () String)
(declare-fun fsm_set_73 () String)
(declare-fun fsm_set_72 () String)
(declare-fun fsm_set_71 () String)
(declare-fun fsm_set_70 () String)
(declare-fun fsm_set_69 () String)
(declare-fun fsm_set_68 () String)
(declare-fun fsm_set_67 () String)
(declare-fun fsm_set_66 () String)
(declare-fun fsm_set_65 () String)
(declare-fun fsm_set_64 () String)
(declare-fun fsm_set_63 () String)
(declare-fun fsm_set_62 () String)
(declare-fun fsm_set_61 () String)
(declare-fun fsm_set_60 () String)
(declare-fun fsm_set_59 () String)
(declare-fun fsm_set_58 () String)
(declare-fun fsm_set_57 () String)
(declare-fun fsm_set_56 () String)
(declare-fun fsm_set_55 () String)
(declare-fun fsm_set_54 () String)
(declare-fun fsm_set_53 () String)
(declare-fun fsm_set_52 () String)
(declare-fun fsm_set_51 () String)
(declare-fun fsm_set_50 () String)
(declare-fun fsm_set_49 () String)
(declare-fun fsm_set_48 () String)
(declare-fun fsm_set_47 () String)
(declare-fun fsm_set_46 () String)
(declare-fun fsm_set_45 () String)
(declare-fun fsm_set_44 () String)
(declare-fun fsm_set_43 () String)
(declare-fun fsm_set_42 () String)
(declare-fun fsm_set_41 () String)
(declare-fun fsm_set_40 () String)
(declare-fun fsm_set_39 () String)
(declare-fun fsm_set_38 () String)
(declare-fun fsm_set_37 () String)
(declare-fun fsm_set_36 () String)
(declare-fun fsm_set_35 () String)
(declare-fun fsm_set_34 () String)
(declare-fun fsm_set_33 () String)
(declare-fun fsm_set_32 () String)
(declare-fun fsm_set_31 () String)
(declare-fun fsm_set_30 () String)
(declare-fun fsm_set_29 () String)
(declare-fun fsm_set_28 () String)
(declare-fun fsm_set_27 () String)
(declare-fun fsm_set_26 () String)
(declare-fun fsm_set_25 () String)
(declare-fun fsm_set_24 () String)
(declare-fun fsm_set_23 () String)
(declare-fun fsm_set_22 () String)
(declare-fun fsm_set_21 () String)
(declare-fun fsm_set_20 () String)
(declare-fun fsm_set_19 () String)
(declare-fun fsm_set_18 () String)
(declare-fun fsm_set_17 () String)
(declare-fun fsm_set_16 () String)
(declare-fun fsm_set_15 () String)
(declare-fun fsm_set_14 () String)
(declare-fun fsm_set_13 () String)
(declare-fun fsm_set_12 () String)
(declare-fun fsm_set_11 () String)
(declare-fun fsm_set_10 () String)
(declare-fun fsm_set_9 () String)
(declare-fun fsm_set_8 () String)
(declare-fun fsm_set_7 () String)
(declare-fun fsm_set_6 () String)
(declare-fun fsm_set_5 () String)
(declare-fun fsm_set_4 () String)
(declare-fun fsm_set_3 () String)
(declare-fun fsm_set_2 () String)
(declare-fun fsm_set_1 () String)
(declare-fun fsm_set_0 () String)
(declare-fun tk_dr_fsm_ok () Bool)
(declare-fun _cur_b () Int)
(declare-fun _bind_reg () String)
(declare-fun tk_dr_m () Int)
(declare-fun tk_dr_vs () Int)
(declare-fun tk_dr_vc () Int)
(declare-fun tk_dr_val () String)
(declare-fun tk_dr_f2 () String)
(declare-fun tk_dr_bkey () String)
(declare-fun _bare_reg () String)
(declare-fun tk_dr_bp () Int)
(declare-fun _carry_reg () String)
(declare-fun tk_dr_new () Bool)
(declare-fun tk_dr_item_done () Bool)
(declare-fun tk_dr_hdr_go () Bool)
(declare-fun tk_mn_on () Bool)
(declare-fun _ment_reg () String)
(declare-fun tk_mn_p () Int)
(declare-fun tk_mn_done () Bool)
(declare-fun tk_mn_e () Int)
(declare-fun tk_mn_par () String)
(declare-fun tk_mn_bkey () String)
(declare-fun tk_mn_bp () Int)
(declare-fun tk_mn_new () Bool)
(declare-fun tk_iw_done () Bool)
(declare-fun tk_iw_run () Bool)
(declare-fun tk_iw_gt () Int)
(declare-fun tk_iw_key () String)
(declare-fun tk_iw_x () String)
(declare-fun tk_iw_c1 () Int)
(declare-fun tk_iw_c2 () Int)
(declare-fun tk_iw_cp () Int)
(declare-fun tk_iw_isc () Bool)
(declare-fun tk_iw_ln0 () Bool)
(declare-fun tk_jw_done () Bool)
(declare-fun tk_jw_run () Bool)
(declare-fun tk_jw_cA () Int)
(declare-fun tk_jw_brk () Int)
(declare-fun tk_jw_sub () String)
(declare-fun tk_jw_slot () String)
(declare-fun tk_jw_vc () Int)
(declare-fun tk_jw_val () String)
(declare-fun tk_jw_fc () Int)
(declare-fun tk_jw_nc () Int)
(declare-fun tk_jw_ln () String)
(declare-fun _acc_b () String)
(declare-fun tk_jw_newgrp () Bool)
(declare-fun _acc_a () String)
(declare-fun tk_jw_flush () Bool)
(declare-fun _slot_reg () String)
(declare-fun tk_jw_hp () Int)
(declare-fun tk_jw_have () Bool)
(declare-fun tk_jw_fkey () String)
(declare-fun tk_jw_give () Bool)
(declare-fun tk_jw_hdr () Bool)
(declare-fun tk_jw_add () String)
(declare-fun phase () Int)
(declare-fun _fs_n () Int)
(declare-fun tk_ov_fs () Bool)
(declare-fun _fsm_set_0 () String)
(declare-fun _fsm_set_len () Int)
(declare-fun _fsm_set_1 () String)
(declare-fun _fsm_set_2 () String)
(declare-fun _fsm_set_3 () String)
(declare-fun _fsm_set_4 () String)
(declare-fun _fsm_set_5 () String)
(declare-fun _fsm_set_6 () String)
(declare-fun _fsm_set_7 () String)
(declare-fun _fsm_set_8 () String)
(declare-fun _fsm_set_9 () String)
(declare-fun _fsm_set_10 () String)
(declare-fun _fsm_set_11 () String)
(declare-fun _fsm_set_12 () String)
(declare-fun _fsm_set_13 () String)
(declare-fun _fsm_set_14 () String)
(declare-fun _fsm_set_15 () String)
(declare-fun _fsm_set_16 () String)
(declare-fun _fsm_set_17 () String)
(declare-fun _fsm_set_18 () String)
(declare-fun _fsm_set_19 () String)
(declare-fun _fsm_set_20 () String)
(declare-fun _fsm_set_21 () String)
(declare-fun _fsm_set_22 () String)
(declare-fun _fsm_set_23 () String)
(declare-fun _fsm_set_24 () String)
(declare-fun _fsm_set_25 () String)
(declare-fun _fsm_set_26 () String)
(declare-fun _fsm_set_27 () String)
(declare-fun _fsm_set_28 () String)
(declare-fun _fsm_set_29 () String)
(declare-fun _fsm_set_30 () String)
(declare-fun _fsm_set_31 () String)
(declare-fun _fsm_set_32 () String)
(declare-fun _fsm_set_33 () String)
(declare-fun _fsm_set_34 () String)
(declare-fun _fsm_set_35 () String)
(declare-fun _fsm_set_36 () String)
(declare-fun _fsm_set_37 () String)
(declare-fun _fsm_set_38 () String)
(declare-fun _fsm_set_39 () String)
(declare-fun _fsm_set_40 () String)
(declare-fun _fsm_set_41 () String)
(declare-fun _fsm_set_42 () String)
(declare-fun _fsm_set_43 () String)
(declare-fun _fsm_set_44 () String)
(declare-fun _fsm_set_45 () String)
(declare-fun _fsm_set_46 () String)
(declare-fun _fsm_set_47 () String)
(declare-fun _fsm_set_48 () String)
(declare-fun _fsm_set_49 () String)
(declare-fun _fsm_set_50 () String)
(declare-fun _fsm_set_51 () String)
(declare-fun _fsm_set_52 () String)
(declare-fun _fsm_set_53 () String)
(declare-fun _fsm_set_54 () String)
(declare-fun _fsm_set_55 () String)
(declare-fun _fsm_set_56 () String)
(declare-fun _fsm_set_57 () String)
(declare-fun _fsm_set_58 () String)
(declare-fun _fsm_set_59 () String)
(declare-fun _fsm_set_60 () String)
(declare-fun _fsm_set_61 () String)
(declare-fun _fsm_set_62 () String)
(declare-fun _fsm_set_63 () String)
(declare-fun _fsm_set_64 () String)
(declare-fun _fsm_set_65 () String)
(declare-fun _fsm_set_66 () String)
(declare-fun _fsm_set_67 () String)
(declare-fun _fsm_set_68 () String)
(declare-fun _fsm_set_69 () String)
(declare-fun _fsm_set_70 () String)
(declare-fun _fsm_set_71 () String)
(declare-fun _fsm_set_72 () String)
(declare-fun _fsm_set_73 () String)
(declare-fun _fsm_set_74 () String)
(declare-fun _fsm_set_75 () String)
(declare-fun _fsm_set_76 () String)
(declare-fun _fsm_set_77 () String)
(declare-fun _fsm_set_78 () String)
(declare-fun _fsm_set_79 () String)
(declare-fun _fsm_set_80 () String)
(declare-fun _fsm_set_81 () String)
(declare-fun _fsm_set_82 () String)
(declare-fun _fsm_set_83 () String)
(declare-fun _fsm_set_84 () String)
(declare-fun _fsm_set_85 () String)
(declare-fun _fsm_set_86 () String)
(declare-fun _fsm_set_87 () String)
(declare-fun _fsm_set_88 () String)
(declare-fun _fsm_set_89 () String)
(declare-fun _fsm_set_90 () String)
(declare-fun _fsm_set_91 () String)
(declare-fun _fsm_set_92 () String)
(declare-fun _fsm_set_93 () String)
(declare-fun _fsm_set_94 () String)
(declare-fun _fsm_set_95 () String)
(declare-fun _fsm_set_96 () String)
(declare-fun _fsm_set_97 () String)
(declare-fun _fsm_set_98 () String)
(declare-fun _fsm_set_99 () String)
(declare-fun _fsm_set_100 () String)
(declare-fun _fsm_set_101 () String)
(declare-fun _fsm_set_102 () String)
(declare-fun _fsm_set_103 () String)
(declare-fun fs_n () Int)
(declare-fun tk_ov_bare () Bool)
(declare-fun tk_ov_bind () Bool)
(declare-fun tk_ov_slot () Bool)
(declare-fun tk_ov_ment () Bool)
(declare-fun tk_cr_add () String)
(declare-fun tk_ov_carry () Bool)
(declare-fun tk_wl_add () String)
(declare-fun tk_ov_work () Bool)
(declare-fun tk_iw_add () String)
(declare-fun _ins_out () String)
(declare-fun tk_ov_ins () Bool)
(declare-fun tk_inj_add () String)
(declare-fun _inj_out () String)
(declare-fun tk_ov_inj () Bool)
(declare-fun cur_a () Int)
(declare-fun cur_b () Int)
(declare-fun tk_ov_acca () Bool)
(declare-fun tk_ov_accb () Bool)
(declare-fun eff_nop () Effect)
(declare-fun eff_out () Effect)
(declare-fun tk_ov_any () Bool)
(declare-fun tk_ov_code () Int)
(declare-fun tk_ov_msg () String)
(declare-fun eff_ov_msg () Effect)
(declare-fun eff_ov_exit () Effect)
(declare-fun tk_read_go () Bool)
(declare-fun effects () (Array Int Effect))
(assert (>= effects__len 0))
(assert (>= last_results__len 0))
(assert (<= 0 fsm_set_len))
(assert (<= fsm_set_len 104))
(assert (<= (str.len bare_reg) 82500))
(assert (<= (str.len bind_reg) 102500))
(assert (<= (str.len slot_reg) 17500))
(assert (<= (str.len ment_reg) 2000))
(assert (<= (str.len carry_reg) 30500))
(assert (<= (str.len work_list) 25000))
(assert (<= (str.len ins_out) 24500))
(assert (<= (str.len inj_out) 1000))
(assert (<= (str.len acc_a) 4000))
(assert (<= (str.len acc_b) 4000))
(assert (= tk_ph (ite is_first_tick 0 _phase)))
(assert (= tk_rline
   (ite ((_ is StringResult) (select last_results 1))
        (StringResult__f0 (select last_results 1))
        "")))
(assert (= tk_reof (ite ((_ is EofResult) (select last_results 1)) true false)))
(assert (= tk_rd (and (= tk_ph 1) (not tk_reof))))
(assert (= tk_kind (ite tk_rd (str.at tk_rline 0) "")))
(assert (let ((a!1 (ite tk_rd (str.substr tk_rline 1 (- (str.len tk_rline) 1)) "")))
  (= tk_payload a!1)))
(assert (= tk_dr_done (and (= tk_ph 7) (>= _cur_a (str.len _work_list)))))
(assert (= tk_dr_run (and (= tk_ph 7) (not tk_dr_done))))
(assert (= tk_fx_run (or tk_dr_run (= tk_ph 6))))
(assert (= tk_dr_dot (ite tk_fx_run (str.indexof _work_list "." _cur_a) 0)))
(assert (= tk_dr_gt (ite tk_fx_run (str.indexof _work_list "\u{27e9}" _cur_a) 0)))
(assert (let ((a!1 (ite tk_fx_run
                (str.substr _work_list (+ _cur_a 1) (- (- tk_dr_dot _cur_a) 1))
                "")))
  (= tk_dr_f a!1)))
(assert (let ((a!1 (ite tk_fx_run
                (str.substr _work_list
                            (+ tk_dr_dot 1)
                            (- (- tk_dr_gt tk_dr_dot) 1))
                "")))
  (= tk_dr_x a!1)))
(assert (= tk_dr_fkey (ite tk_fx_run (str.++ "\u{27e8}" tk_dr_f "\u{27e9}") "")))
(assert (let ((a!1 (and tk_dr_run
                (or (and (< 0 fsm_set_len) (= fsm_set_0 tk_dr_fkey))
                    (and (< 1 fsm_set_len) (= fsm_set_1 tk_dr_fkey))
                    (and (< 2 fsm_set_len) (= fsm_set_2 tk_dr_fkey))
                    (and (< 3 fsm_set_len) (= fsm_set_3 tk_dr_fkey))
                    (and (< 4 fsm_set_len) (= fsm_set_4 tk_dr_fkey))
                    (and (< 5 fsm_set_len) (= fsm_set_5 tk_dr_fkey))
                    (and (< 6 fsm_set_len) (= fsm_set_6 tk_dr_fkey))
                    (and (< 7 fsm_set_len) (= fsm_set_7 tk_dr_fkey))
                    (and (< 8 fsm_set_len) (= fsm_set_8 tk_dr_fkey))
                    (and (< 9 fsm_set_len) (= fsm_set_9 tk_dr_fkey))
                    (and (< 10 fsm_set_len) (= fsm_set_10 tk_dr_fkey))
                    (and (< 11 fsm_set_len) (= fsm_set_11 tk_dr_fkey))
                    (and (< 12 fsm_set_len) (= fsm_set_12 tk_dr_fkey))
                    (and (< 13 fsm_set_len) (= fsm_set_13 tk_dr_fkey))
                    (and (< 14 fsm_set_len) (= fsm_set_14 tk_dr_fkey))
                    (and (< 15 fsm_set_len) (= fsm_set_15 tk_dr_fkey))
                    (and (< 16 fsm_set_len) (= fsm_set_16 tk_dr_fkey))
                    (and (< 17 fsm_set_len) (= fsm_set_17 tk_dr_fkey))
                    (and (< 18 fsm_set_len) (= fsm_set_18 tk_dr_fkey))
                    (and (< 19 fsm_set_len) (= fsm_set_19 tk_dr_fkey))
                    (and (< 20 fsm_set_len) (= fsm_set_20 tk_dr_fkey))
                    (and (< 21 fsm_set_len) (= fsm_set_21 tk_dr_fkey))
                    (and (< 22 fsm_set_len) (= fsm_set_22 tk_dr_fkey))
                    (and (< 23 fsm_set_len) (= fsm_set_23 tk_dr_fkey))
                    (and (< 24 fsm_set_len) (= fsm_set_24 tk_dr_fkey))
                    (and (< 25 fsm_set_len) (= fsm_set_25 tk_dr_fkey))
                    (and (< 26 fsm_set_len) (= fsm_set_26 tk_dr_fkey))
                    (and (< 27 fsm_set_len) (= fsm_set_27 tk_dr_fkey))
                    (and (< 28 fsm_set_len) (= fsm_set_28 tk_dr_fkey))
                    (and (< 29 fsm_set_len) (= fsm_set_29 tk_dr_fkey))
                    (and (< 30 fsm_set_len) (= fsm_set_30 tk_dr_fkey))
                    (and (< 31 fsm_set_len) (= fsm_set_31 tk_dr_fkey))
                    (and (< 32 fsm_set_len) (= fsm_set_32 tk_dr_fkey))
                    (and (< 33 fsm_set_len) (= fsm_set_33 tk_dr_fkey))
                    (and (< 34 fsm_set_len) (= fsm_set_34 tk_dr_fkey))
                    (and (< 35 fsm_set_len) (= fsm_set_35 tk_dr_fkey))
                    (and (< 36 fsm_set_len) (= fsm_set_36 tk_dr_fkey))
                    (and (< 37 fsm_set_len) (= fsm_set_37 tk_dr_fkey))
                    (and (< 38 fsm_set_len) (= fsm_set_38 tk_dr_fkey))
                    (and (< 39 fsm_set_len) (= fsm_set_39 tk_dr_fkey))
                    (and (< 40 fsm_set_len) (= fsm_set_40 tk_dr_fkey))
                    (and (< 41 fsm_set_len) (= fsm_set_41 tk_dr_fkey))
                    (and (< 42 fsm_set_len) (= fsm_set_42 tk_dr_fkey))
                    (and (< 43 fsm_set_len) (= fsm_set_43 tk_dr_fkey))
                    (and (< 44 fsm_set_len) (= fsm_set_44 tk_dr_fkey))
                    (and (< 45 fsm_set_len) (= fsm_set_45 tk_dr_fkey))
                    (and (< 46 fsm_set_len) (= fsm_set_46 tk_dr_fkey))
                    (and (< 47 fsm_set_len) (= fsm_set_47 tk_dr_fkey))
                    (and (< 48 fsm_set_len) (= fsm_set_48 tk_dr_fkey))
                    (and (< 49 fsm_set_len) (= fsm_set_49 tk_dr_fkey))
                    (and (< 50 fsm_set_len) (= fsm_set_50 tk_dr_fkey))
                    (and (< 51 fsm_set_len) (= fsm_set_51 tk_dr_fkey))
                    (and (< 52 fsm_set_len) (= fsm_set_52 tk_dr_fkey))
                    (and (< 53 fsm_set_len) (= fsm_set_53 tk_dr_fkey))
                    (and (< 54 fsm_set_len) (= fsm_set_54 tk_dr_fkey))
                    (and (< 55 fsm_set_len) (= fsm_set_55 tk_dr_fkey))
                    (and (< 56 fsm_set_len) (= fsm_set_56 tk_dr_fkey))
                    (and (< 57 fsm_set_len) (= fsm_set_57 tk_dr_fkey))
                    (and (< 58 fsm_set_len) (= fsm_set_58 tk_dr_fkey))
                    (and (< 59 fsm_set_len) (= fsm_set_59 tk_dr_fkey))
                    (and (< 60 fsm_set_len) (= fsm_set_60 tk_dr_fkey))
                    (and (< 61 fsm_set_len) (= fsm_set_61 tk_dr_fkey))
                    (and (< 62 fsm_set_len) (= fsm_set_62 tk_dr_fkey))
                    (and (< 63 fsm_set_len) (= fsm_set_63 tk_dr_fkey))
                    (and (< 64 fsm_set_len) (= fsm_set_64 tk_dr_fkey))
                    (and (< 65 fsm_set_len) (= fsm_set_65 tk_dr_fkey))
                    (and (< 66 fsm_set_len) (= fsm_set_66 tk_dr_fkey))
                    (and (< 67 fsm_set_len) (= fsm_set_67 tk_dr_fkey))
                    (and (< 68 fsm_set_len) (= fsm_set_68 tk_dr_fkey))
                    (and (< 69 fsm_set_len) (= fsm_set_69 tk_dr_fkey))
                    (and (< 70 fsm_set_len) (= fsm_set_70 tk_dr_fkey))
                    (and (< 71 fsm_set_len) (= fsm_set_71 tk_dr_fkey))
                    (and (< 72 fsm_set_len) (= fsm_set_72 tk_dr_fkey))
                    (and (< 73 fsm_set_len) (= fsm_set_73 tk_dr_fkey))
                    (and (< 74 fsm_set_len) (= fsm_set_74 tk_dr_fkey))
                    (and (< 75 fsm_set_len) (= fsm_set_75 tk_dr_fkey))
                    (and (< 76 fsm_set_len) (= fsm_set_76 tk_dr_fkey))
                    (and (< 77 fsm_set_len) (= fsm_set_77 tk_dr_fkey))
                    (and (< 78 fsm_set_len) (= fsm_set_78 tk_dr_fkey))
                    (and (< 79 fsm_set_len) (= fsm_set_79 tk_dr_fkey))
                    (and (< 80 fsm_set_len) (= fsm_set_80 tk_dr_fkey))
                    (and (< 81 fsm_set_len) (= fsm_set_81 tk_dr_fkey))
                    (and (< 82 fsm_set_len) (= fsm_set_82 tk_dr_fkey))
                    (and (< 83 fsm_set_len) (= fsm_set_83 tk_dr_fkey))
                    (and (< 84 fsm_set_len) (= fsm_set_84 tk_dr_fkey))
                    (and (< 85 fsm_set_len) (= fsm_set_85 tk_dr_fkey))
                    (and (< 86 fsm_set_len) (= fsm_set_86 tk_dr_fkey))
                    (and (< 87 fsm_set_len) (= fsm_set_87 tk_dr_fkey))
                    (and (< 88 fsm_set_len) (= fsm_set_88 tk_dr_fkey))
                    (and (< 89 fsm_set_len) (= fsm_set_89 tk_dr_fkey))
                    (and (< 90 fsm_set_len) (= fsm_set_90 tk_dr_fkey))
                    (and (< 91 fsm_set_len) (= fsm_set_91 tk_dr_fkey))
                    (and (< 92 fsm_set_len) (= fsm_set_92 tk_dr_fkey))
                    (and (< 93 fsm_set_len) (= fsm_set_93 tk_dr_fkey))
                    (and (< 94 fsm_set_len) (= fsm_set_94 tk_dr_fkey))
                    (and (< 95 fsm_set_len) (= fsm_set_95 tk_dr_fkey))
                    (and (< 96 fsm_set_len) (= fsm_set_96 tk_dr_fkey))
                    (and (< 97 fsm_set_len) (= fsm_set_97 tk_dr_fkey))
                    (and (< 98 fsm_set_len) (= fsm_set_98 tk_dr_fkey))
                    (and (< 99 fsm_set_len) (= fsm_set_99 tk_dr_fkey))
                    (and (< 100 fsm_set_len) (= fsm_set_100 tk_dr_fkey))
                    (and (< 101 fsm_set_len) (= fsm_set_101 tk_dr_fkey))
                    (and (< 102 fsm_set_len) (= fsm_set_102 tk_dr_fkey))
                    (and (< 103 fsm_set_len) (= fsm_set_103 tk_dr_fkey))))))
  (= tk_dr_fsm_ok a!1)))
(assert (= tk_dr_m
   (ite tk_dr_fsm_ok
        (str.indexof _bind_reg
                     (str.++ "\u{2772}" tk_dr_f "\u{2982}" tk_dr_x "\u{2773}")
                     _cur_b)
        (- 0 1))))
(assert (= tk_dr_vs
   (ite (>= tk_dr_m 0) (+ tk_dr_m (str.len tk_dr_f) (str.len tk_dr_x) 3) 0)))
(assert (= tk_dr_vc (ite (>= tk_dr_m 0) (str.indexof _bind_reg "\u{2982}" tk_dr_vs) 0)))
(assert (= tk_dr_val
   (ite (>= tk_dr_m 0)
        (str.substr _bind_reg tk_dr_vs (- tk_dr_vc tk_dr_vs))
        "0")))
(assert (let ((a!1 (- (- (str.indexof _bind_reg "\u{2982}" (+ tk_dr_vc 1)) tk_dr_vc) 1)))
  (= tk_dr_f2 (ite (>= tk_dr_m 0) (str.substr _bind_reg (+ tk_dr_vc 1) a!1) ""))))
(assert (= tk_dr_bkey (str.++ "\u{27e8}" tk_dr_f2 "." tk_dr_val "\u{27e9}")))
(assert (let ((a!1 (ite (and (>= tk_dr_m 0) (not (= tk_dr_val "0")))
                (str.indexof _bare_reg tk_dr_bkey 0)
                (- 0 1))))
  (= tk_dr_bp a!1)))
(assert (= tk_dr_new (and (>= tk_dr_bp 0) (not (str.contains _carry_reg tk_dr_bkey)))))
(assert (= tk_dr_item_done (and tk_dr_run (or (not tk_dr_fsm_ok) (< tk_dr_m 0)))))
(assert (let ((a!1 (and tk_dr_item_done
                (>= (str.indexof _bind_reg
                                 (str.++ "\u{2772}"
                                         tk_dr_f
                                         "\u{2982}"
                                         tk_dr_x
                                         "\u{2773}\u{2208}")
                                 0)
                    0))))
  (= tk_dr_hdr_go a!1)))
(assert (= tk_mn_on (= tk_ph 6)))
(assert (= tk_mn_p
   (ite tk_mn_on
        (str.indexof _ment_reg (str.++ "\u{2768}" tk_dr_f "\u{2982}") _cur_b)
        (- 0 1))))
(assert (= tk_mn_done (and tk_mn_on (< tk_mn_p 0))))
(assert (= tk_mn_e (ite (>= tk_mn_p 0) (str.indexof _ment_reg "\u{2769}" tk_mn_p) 0)))
(assert (let ((a!1 (str.substr _ment_reg
                       (+ tk_mn_p (str.len tk_dr_f) 2)
                       (- (- (- tk_mn_e tk_mn_p) (str.len tk_dr_f)) 2))))
  (= tk_mn_par (ite (>= tk_mn_p 0) a!1 ""))))
(assert (= tk_mn_bkey (str.++ "\u{27e8}" tk_mn_par "." tk_dr_x "\u{27e9}")))
(assert (= tk_mn_bp (ite (>= tk_mn_p 0) (str.indexof _bare_reg tk_mn_bkey 0) (- 0 1))))
(assert (= tk_mn_new (and (>= tk_mn_bp 0) (not (str.contains _carry_reg tk_mn_bkey)))))
(assert (= tk_iw_done (and (= tk_ph 8) (>= _cur_a (str.len _bare_reg)))))
(assert (= tk_iw_run (and (= tk_ph 8) (not tk_iw_done))))
(assert (= tk_iw_gt (ite tk_iw_run (str.indexof _bare_reg "\u{27e9}" _cur_a) 0)))
(assert (let ((a!1 (ite tk_iw_run
                (str.substr _bare_reg _cur_a (- (+ tk_iw_gt 1) _cur_a))
                "")))
  (= tk_iw_key a!1)))
(assert (let ((a!1 (str.substr _bare_reg
                       (+ (str.indexof _bare_reg "." _cur_a) 1)
                       (- (- tk_iw_gt (str.indexof _bare_reg "." _cur_a)) 1))))
  (= tk_iw_x (ite tk_iw_run a!1 ""))))
(assert (= tk_iw_c1 (ite tk_iw_run (str.indexof _bare_reg "\u{2982}" tk_iw_gt) 0)))
(assert (= tk_iw_c2 (ite tk_iw_run (str.indexof _bare_reg "\u{2982}" (+ tk_iw_c1 1)) 0)))
(assert (= tk_iw_cp (ite tk_iw_run (str.indexof _carry_reg tk_iw_key 0) (- 0 1))))
(assert (= tk_iw_isc (>= tk_iw_cp 0)))
(assert (let ((a!1 (= (str.substr _bare_reg (+ tk_iw_c1 1) (- (- tk_iw_c2 tk_iw_c1) 1))
              "0")))
  (= tk_iw_ln0 (and tk_iw_run a!1))))
(assert (= tk_jw_done (and (= tk_ph 9) (>= _cur_a (str.len _bind_reg)))))
(assert (= tk_jw_run (and (= tk_ph 9) (not tk_jw_done))))
(assert (= tk_jw_cA (ite tk_jw_run (str.indexof _bind_reg "\u{2982}" _cur_a) 0)))
(assert (= tk_jw_brk (ite tk_jw_run (str.indexof _bind_reg "\u{2773}" _cur_a) 0)))
(assert (let ((a!1 (ite tk_jw_run
                (str.substr _bind_reg (+ _cur_a 1) (- (- tk_jw_cA _cur_a) 1))
                "")))
  (= tk_jw_sub a!1)))
(assert (let ((a!1 (ite tk_jw_run
                (str.substr _bind_reg
                            (+ tk_jw_cA 1)
                            (- (- tk_jw_brk tk_jw_cA) 1))
                "")))
  (= tk_jw_slot a!1)))
(assert (= tk_jw_vc (ite tk_jw_run (str.indexof _bind_reg "\u{2982}" tk_jw_brk) 0)))
(assert (let ((a!1 (ite tk_jw_run
                (str.substr _bind_reg
                            (+ tk_jw_brk 1)
                            (- (- tk_jw_vc tk_jw_brk) 1))
                "0")))
  (= tk_jw_val a!1)))
(assert (= tk_jw_fc (ite tk_jw_run (str.indexof _bind_reg "\u{2982}" (+ tk_jw_vc 1)) 0)))
(assert (= tk_jw_nc (ite tk_jw_run (str.indexof _bind_reg "\u{2982}" (+ tk_jw_fc 1)) 0)))
(assert (let ((a!1 (ite tk_jw_run
                (str.substr _bind_reg
                            (+ tk_jw_fc 1)
                            (- (- tk_jw_nc tk_jw_fc) 1))
                "")))
  (= tk_jw_ln a!1)))
(assert (= tk_jw_newgrp (and tk_jw_run (not (= tk_jw_ln _acc_b)))))
(assert (= tk_jw_flush
   (and (or tk_jw_newgrp tk_jw_done) (not (= _acc_a "")) (= tk_ph 9))))
(assert (= tk_jw_hp
   (ite tk_jw_run
        (str.indexof _slot_reg (str.++ "\u{2770}" tk_jw_ln "\u{2771}") 0)
        (- 0 1))))
(assert (let ((a!1 (- (str.indexof _slot_reg
                           "\u{2771}"
                           (+ tk_jw_hp (str.len tk_jw_ln) 2))
              tk_jw_hp)))
(let ((a!2 (str.substr _slot_reg
                       (+ tk_jw_hp (str.len tk_jw_ln) 2)
                       (- (- a!1 (str.len tk_jw_ln)) 2))))
  (= tk_jw_have
     (and (>= tk_jw_hp 0)
          (str.contains a!2 (str.++ "\u{2982}_" tk_jw_slot "\u{2982}")))))))
(assert (= tk_jw_fkey (ite tk_jw_run (str.++ "\u{27e8}" tk_jw_sub "\u{27e9}") "")))
(assert (let ((a!1 (and tk_jw_run
                (not (= tk_jw_val "0"))
                (not tk_jw_have)
                (or (and (< 0 fsm_set_len) (= fsm_set_0 tk_jw_fkey))
                    (and (< 1 fsm_set_len) (= fsm_set_1 tk_jw_fkey))
                    (and (< 2 fsm_set_len) (= fsm_set_2 tk_jw_fkey))
                    (and (< 3 fsm_set_len) (= fsm_set_3 tk_jw_fkey))
                    (and (< 4 fsm_set_len) (= fsm_set_4 tk_jw_fkey))
                    (and (< 5 fsm_set_len) (= fsm_set_5 tk_jw_fkey))
                    (and (< 6 fsm_set_len) (= fsm_set_6 tk_jw_fkey))
                    (and (< 7 fsm_set_len) (= fsm_set_7 tk_jw_fkey))
                    (and (< 8 fsm_set_len) (= fsm_set_8 tk_jw_fkey))
                    (and (< 9 fsm_set_len) (= fsm_set_9 tk_jw_fkey))
                    (and (< 10 fsm_set_len) (= fsm_set_10 tk_jw_fkey))
                    (and (< 11 fsm_set_len) (= fsm_set_11 tk_jw_fkey))
                    (and (< 12 fsm_set_len) (= fsm_set_12 tk_jw_fkey))
                    (and (< 13 fsm_set_len) (= fsm_set_13 tk_jw_fkey))
                    (and (< 14 fsm_set_len) (= fsm_set_14 tk_jw_fkey))
                    (and (< 15 fsm_set_len) (= fsm_set_15 tk_jw_fkey))
                    (and (< 16 fsm_set_len) (= fsm_set_16 tk_jw_fkey))
                    (and (< 17 fsm_set_len) (= fsm_set_17 tk_jw_fkey))
                    (and (< 18 fsm_set_len) (= fsm_set_18 tk_jw_fkey))
                    (and (< 19 fsm_set_len) (= fsm_set_19 tk_jw_fkey))
                    (and (< 20 fsm_set_len) (= fsm_set_20 tk_jw_fkey))
                    (and (< 21 fsm_set_len) (= fsm_set_21 tk_jw_fkey))
                    (and (< 22 fsm_set_len) (= fsm_set_22 tk_jw_fkey))
                    (and (< 23 fsm_set_len) (= fsm_set_23 tk_jw_fkey))
                    (and (< 24 fsm_set_len) (= fsm_set_24 tk_jw_fkey))
                    (and (< 25 fsm_set_len) (= fsm_set_25 tk_jw_fkey))
                    (and (< 26 fsm_set_len) (= fsm_set_26 tk_jw_fkey))
                    (and (< 27 fsm_set_len) (= fsm_set_27 tk_jw_fkey))
                    (and (< 28 fsm_set_len) (= fsm_set_28 tk_jw_fkey))
                    (and (< 29 fsm_set_len) (= fsm_set_29 tk_jw_fkey))
                    (and (< 30 fsm_set_len) (= fsm_set_30 tk_jw_fkey))
                    (and (< 31 fsm_set_len) (= fsm_set_31 tk_jw_fkey))
                    (and (< 32 fsm_set_len) (= fsm_set_32 tk_jw_fkey))
                    (and (< 33 fsm_set_len) (= fsm_set_33 tk_jw_fkey))
                    (and (< 34 fsm_set_len) (= fsm_set_34 tk_jw_fkey))
                    (and (< 35 fsm_set_len) (= fsm_set_35 tk_jw_fkey))
                    (and (< 36 fsm_set_len) (= fsm_set_36 tk_jw_fkey))
                    (and (< 37 fsm_set_len) (= fsm_set_37 tk_jw_fkey))
                    (and (< 38 fsm_set_len) (= fsm_set_38 tk_jw_fkey))
                    (and (< 39 fsm_set_len) (= fsm_set_39 tk_jw_fkey))
                    (and (< 40 fsm_set_len) (= fsm_set_40 tk_jw_fkey))
                    (and (< 41 fsm_set_len) (= fsm_set_41 tk_jw_fkey))
                    (and (< 42 fsm_set_len) (= fsm_set_42 tk_jw_fkey))
                    (and (< 43 fsm_set_len) (= fsm_set_43 tk_jw_fkey))
                    (and (< 44 fsm_set_len) (= fsm_set_44 tk_jw_fkey))
                    (and (< 45 fsm_set_len) (= fsm_set_45 tk_jw_fkey))
                    (and (< 46 fsm_set_len) (= fsm_set_46 tk_jw_fkey))
                    (and (< 47 fsm_set_len) (= fsm_set_47 tk_jw_fkey))
                    (and (< 48 fsm_set_len) (= fsm_set_48 tk_jw_fkey))
                    (and (< 49 fsm_set_len) (= fsm_set_49 tk_jw_fkey))
                    (and (< 50 fsm_set_len) (= fsm_set_50 tk_jw_fkey))
                    (and (< 51 fsm_set_len) (= fsm_set_51 tk_jw_fkey))
                    (and (< 52 fsm_set_len) (= fsm_set_52 tk_jw_fkey))
                    (and (< 53 fsm_set_len) (= fsm_set_53 tk_jw_fkey))
                    (and (< 54 fsm_set_len) (= fsm_set_54 tk_jw_fkey))
                    (and (< 55 fsm_set_len) (= fsm_set_55 tk_jw_fkey))
                    (and (< 56 fsm_set_len) (= fsm_set_56 tk_jw_fkey))
                    (and (< 57 fsm_set_len) (= fsm_set_57 tk_jw_fkey))
                    (and (< 58 fsm_set_len) (= fsm_set_58 tk_jw_fkey))
                    (and (< 59 fsm_set_len) (= fsm_set_59 tk_jw_fkey))
                    (and (< 60 fsm_set_len) (= fsm_set_60 tk_jw_fkey))
                    (and (< 61 fsm_set_len) (= fsm_set_61 tk_jw_fkey))
                    (and (< 62 fsm_set_len) (= fsm_set_62 tk_jw_fkey))
                    (and (< 63 fsm_set_len) (= fsm_set_63 tk_jw_fkey))
                    (and (< 64 fsm_set_len) (= fsm_set_64 tk_jw_fkey))
                    (and (< 65 fsm_set_len) (= fsm_set_65 tk_jw_fkey))
                    (and (< 66 fsm_set_len) (= fsm_set_66 tk_jw_fkey))
                    (and (< 67 fsm_set_len) (= fsm_set_67 tk_jw_fkey))
                    (and (< 68 fsm_set_len) (= fsm_set_68 tk_jw_fkey))
                    (and (< 69 fsm_set_len) (= fsm_set_69 tk_jw_fkey))
                    (and (< 70 fsm_set_len) (= fsm_set_70 tk_jw_fkey))
                    (and (< 71 fsm_set_len) (= fsm_set_71 tk_jw_fkey))
                    (and (< 72 fsm_set_len) (= fsm_set_72 tk_jw_fkey))
                    (and (< 73 fsm_set_len) (= fsm_set_73 tk_jw_fkey))
                    (and (< 74 fsm_set_len) (= fsm_set_74 tk_jw_fkey))
                    (and (< 75 fsm_set_len) (= fsm_set_75 tk_jw_fkey))
                    (and (< 76 fsm_set_len) (= fsm_set_76 tk_jw_fkey))
                    (and (< 77 fsm_set_len) (= fsm_set_77 tk_jw_fkey))
                    (and (< 78 fsm_set_len) (= fsm_set_78 tk_jw_fkey))
                    (and (< 79 fsm_set_len) (= fsm_set_79 tk_jw_fkey))
                    (and (< 80 fsm_set_len) (= fsm_set_80 tk_jw_fkey))
                    (and (< 81 fsm_set_len) (= fsm_set_81 tk_jw_fkey))
                    (and (< 82 fsm_set_len) (= fsm_set_82 tk_jw_fkey))
                    (and (< 83 fsm_set_len) (= fsm_set_83 tk_jw_fkey))
                    (and (< 84 fsm_set_len) (= fsm_set_84 tk_jw_fkey))
                    (and (< 85 fsm_set_len) (= fsm_set_85 tk_jw_fkey))
                    (and (< 86 fsm_set_len) (= fsm_set_86 tk_jw_fkey))
                    (and (< 87 fsm_set_len) (= fsm_set_87 tk_jw_fkey))
                    (and (< 88 fsm_set_len) (= fsm_set_88 tk_jw_fkey))
                    (and (< 89 fsm_set_len) (= fsm_set_89 tk_jw_fkey))
                    (and (< 90 fsm_set_len) (= fsm_set_90 tk_jw_fkey))
                    (and (< 91 fsm_set_len) (= fsm_set_91 tk_jw_fkey))
                    (and (< 92 fsm_set_len) (= fsm_set_92 tk_jw_fkey))
                    (and (< 93 fsm_set_len) (= fsm_set_93 tk_jw_fkey))
                    (and (< 94 fsm_set_len) (= fsm_set_94 tk_jw_fkey))
                    (and (< 95 fsm_set_len) (= fsm_set_95 tk_jw_fkey))
                    (and (< 96 fsm_set_len) (= fsm_set_96 tk_jw_fkey))
                    (and (< 97 fsm_set_len) (= fsm_set_97 tk_jw_fkey))
                    (and (< 98 fsm_set_len) (= fsm_set_98 tk_jw_fkey))
                    (and (< 99 fsm_set_len) (= fsm_set_99 tk_jw_fkey))
                    (and (< 100 fsm_set_len) (= fsm_set_100 tk_jw_fkey))
                    (and (< 101 fsm_set_len) (= fsm_set_101 tk_jw_fkey))
                    (and (< 102 fsm_set_len) (= fsm_set_102 tk_jw_fkey))
                    (and (< 103 fsm_set_len) (= fsm_set_103 tk_jw_fkey)))
                (str.contains _carry_reg
                              (str.++ "\u{27e8}"
                                      tk_jw_sub
                                      "."
                                      tk_jw_slot
                                      "\u{27e9}")))))
  (= tk_jw_give a!1)))
(assert (let ((a!1 (and tk_jw_run (= (str.at _bind_reg (+ tk_jw_brk 1)) "\u{2208}"))))
  (= tk_jw_hdr a!1)))
(assert (= tk_jw_add
   (ite tk_jw_hdr
        (str.++ (str.++ ", _" tk_jw_slot) " " tk_jw_val)
        (str.++ (str.++ ", _" tk_jw_slot) " \u{21a6} _" tk_jw_val))))
(assert (let ((a!1 (ite (= tk_ph 8)
                (ite tk_iw_done 9 8)
                (ite (= tk_ph 9) (ite tk_jw_done 10 9) (ite (= tk_ph 10) 11 11)))))
(let ((a!2 (ite (= tk_ph 1)
                (ite tk_reof 7 1)
                (ite (= tk_ph 7)
                     (ite tk_dr_done 8 (ite tk_dr_hdr_go 6 7))
                     (ite (= tk_ph 6) (ite tk_mn_done 7 6) a!1)))))
  (= phase (ite is_first_tick 1 a!2)))))
(assert (= tk_ov_fs (and (= tk_kind "F") (>= _fs_n 104))))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 0))
                     tk_payload
                     _fsm_set_0))))
  (= fsm_set_0 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 1))
                     tk_payload
                     _fsm_set_1))))
  (= fsm_set_1 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 2))
                     tk_payload
                     _fsm_set_2))))
  (= fsm_set_2 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 3))
                     tk_payload
                     _fsm_set_3))))
  (= fsm_set_3 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 4))
                     tk_payload
                     _fsm_set_4))))
  (= fsm_set_4 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 5))
                     tk_payload
                     _fsm_set_5))))
  (= fsm_set_5 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 6))
                     tk_payload
                     _fsm_set_6))))
  (= fsm_set_6 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 7))
                     tk_payload
                     _fsm_set_7))))
  (= fsm_set_7 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 8))
                     tk_payload
                     _fsm_set_8))))
  (= fsm_set_8 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 9))
                     tk_payload
                     _fsm_set_9))))
  (= fsm_set_9 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 10))
                     tk_payload
                     _fsm_set_10))))
  (= fsm_set_10 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 11))
                     tk_payload
                     _fsm_set_11))))
  (= fsm_set_11 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 12))
                     tk_payload
                     _fsm_set_12))))
  (= fsm_set_12 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 13))
                     tk_payload
                     _fsm_set_13))))
  (= fsm_set_13 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 14))
                     tk_payload
                     _fsm_set_14))))
  (= fsm_set_14 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 15))
                     tk_payload
                     _fsm_set_15))))
  (= fsm_set_15 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 16))
                     tk_payload
                     _fsm_set_16))))
  (= fsm_set_16 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 17))
                     tk_payload
                     _fsm_set_17))))
  (= fsm_set_17 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 18))
                     tk_payload
                     _fsm_set_18))))
  (= fsm_set_18 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 19))
                     tk_payload
                     _fsm_set_19))))
  (= fsm_set_19 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 20))
                     tk_payload
                     _fsm_set_20))))
  (= fsm_set_20 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 21))
                     tk_payload
                     _fsm_set_21))))
  (= fsm_set_21 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 22))
                     tk_payload
                     _fsm_set_22))))
  (= fsm_set_22 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 23))
                     tk_payload
                     _fsm_set_23))))
  (= fsm_set_23 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 24))
                     tk_payload
                     _fsm_set_24))))
  (= fsm_set_24 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 25))
                     tk_payload
                     _fsm_set_25))))
  (= fsm_set_25 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 26))
                     tk_payload
                     _fsm_set_26))))
  (= fsm_set_26 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 27))
                     tk_payload
                     _fsm_set_27))))
  (= fsm_set_27 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 28))
                     tk_payload
                     _fsm_set_28))))
  (= fsm_set_28 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 29))
                     tk_payload
                     _fsm_set_29))))
  (= fsm_set_29 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 30))
                     tk_payload
                     _fsm_set_30))))
  (= fsm_set_30 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 31))
                     tk_payload
                     _fsm_set_31))))
  (= fsm_set_31 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 32))
                     tk_payload
                     _fsm_set_32))))
  (= fsm_set_32 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 33))
                     tk_payload
                     _fsm_set_33))))
  (= fsm_set_33 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 34))
                     tk_payload
                     _fsm_set_34))))
  (= fsm_set_34 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 35))
                     tk_payload
                     _fsm_set_35))))
  (= fsm_set_35 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 36))
                     tk_payload
                     _fsm_set_36))))
  (= fsm_set_36 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 37))
                     tk_payload
                     _fsm_set_37))))
  (= fsm_set_37 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 38))
                     tk_payload
                     _fsm_set_38))))
  (= fsm_set_38 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 39))
                     tk_payload
                     _fsm_set_39))))
  (= fsm_set_39 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 40))
                     tk_payload
                     _fsm_set_40))))
  (= fsm_set_40 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 41))
                     tk_payload
                     _fsm_set_41))))
  (= fsm_set_41 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 42))
                     tk_payload
                     _fsm_set_42))))
  (= fsm_set_42 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 43))
                     tk_payload
                     _fsm_set_43))))
  (= fsm_set_43 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 44))
                     tk_payload
                     _fsm_set_44))))
  (= fsm_set_44 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 45))
                     tk_payload
                     _fsm_set_45))))
  (= fsm_set_45 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 46))
                     tk_payload
                     _fsm_set_46))))
  (= fsm_set_46 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 47))
                     tk_payload
                     _fsm_set_47))))
  (= fsm_set_47 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 48))
                     tk_payload
                     _fsm_set_48))))
  (= fsm_set_48 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 49))
                     tk_payload
                     _fsm_set_49))))
  (= fsm_set_49 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 50))
                     tk_payload
                     _fsm_set_50))))
  (= fsm_set_50 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 51))
                     tk_payload
                     _fsm_set_51))))
  (= fsm_set_51 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 52))
                     tk_payload
                     _fsm_set_52))))
  (= fsm_set_52 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 53))
                     tk_payload
                     _fsm_set_53))))
  (= fsm_set_53 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 54))
                     tk_payload
                     _fsm_set_54))))
  (= fsm_set_54 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 55))
                     tk_payload
                     _fsm_set_55))))
  (= fsm_set_55 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 56))
                     tk_payload
                     _fsm_set_56))))
  (= fsm_set_56 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 57))
                     tk_payload
                     _fsm_set_57))))
  (= fsm_set_57 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 58))
                     tk_payload
                     _fsm_set_58))))
  (= fsm_set_58 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 59))
                     tk_payload
                     _fsm_set_59))))
  (= fsm_set_59 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 60))
                     tk_payload
                     _fsm_set_60))))
  (= fsm_set_60 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 61))
                     tk_payload
                     _fsm_set_61))))
  (= fsm_set_61 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 62))
                     tk_payload
                     _fsm_set_62))))
  (= fsm_set_62 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 63))
                     tk_payload
                     _fsm_set_63))))
  (= fsm_set_63 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 64))
                     tk_payload
                     _fsm_set_64))))
  (= fsm_set_64 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 65))
                     tk_payload
                     _fsm_set_65))))
  (= fsm_set_65 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 66))
                     tk_payload
                     _fsm_set_66))))
  (= fsm_set_66 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 67))
                     tk_payload
                     _fsm_set_67))))
  (= fsm_set_67 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 68))
                     tk_payload
                     _fsm_set_68))))
  (= fsm_set_68 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 69))
                     tk_payload
                     _fsm_set_69))))
  (= fsm_set_69 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 70))
                     tk_payload
                     _fsm_set_70))))
  (= fsm_set_70 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 71))
                     tk_payload
                     _fsm_set_71))))
  (= fsm_set_71 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 72))
                     tk_payload
                     _fsm_set_72))))
  (= fsm_set_72 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 73))
                     tk_payload
                     _fsm_set_73))))
  (= fsm_set_73 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 74))
                     tk_payload
                     _fsm_set_74))))
  (= fsm_set_74 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 75))
                     tk_payload
                     _fsm_set_75))))
  (= fsm_set_75 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 76))
                     tk_payload
                     _fsm_set_76))))
  (= fsm_set_76 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 77))
                     tk_payload
                     _fsm_set_77))))
  (= fsm_set_77 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 78))
                     tk_payload
                     _fsm_set_78))))
  (= fsm_set_78 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 79))
                     tk_payload
                     _fsm_set_79))))
  (= fsm_set_79 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 80))
                     tk_payload
                     _fsm_set_80))))
  (= fsm_set_80 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 81))
                     tk_payload
                     _fsm_set_81))))
  (= fsm_set_81 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 82))
                     tk_payload
                     _fsm_set_82))))
  (= fsm_set_82 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 83))
                     tk_payload
                     _fsm_set_83))))
  (= fsm_set_83 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 84))
                     tk_payload
                     _fsm_set_84))))
  (= fsm_set_84 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 85))
                     tk_payload
                     _fsm_set_85))))
  (= fsm_set_85 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 86))
                     tk_payload
                     _fsm_set_86))))
  (= fsm_set_86 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 87))
                     tk_payload
                     _fsm_set_87))))
  (= fsm_set_87 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 88))
                     tk_payload
                     _fsm_set_88))))
  (= fsm_set_88 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 89))
                     tk_payload
                     _fsm_set_89))))
  (= fsm_set_89 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 90))
                     tk_payload
                     _fsm_set_90))))
  (= fsm_set_90 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 91))
                     tk_payload
                     _fsm_set_91))))
  (= fsm_set_91 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 92))
                     tk_payload
                     _fsm_set_92))))
  (= fsm_set_92 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 93))
                     tk_payload
                     _fsm_set_93))))
  (= fsm_set_93 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 94))
                     tk_payload
                     _fsm_set_94))))
  (= fsm_set_94 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 95))
                     tk_payload
                     _fsm_set_95))))
  (= fsm_set_95 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 96))
                     tk_payload
                     _fsm_set_96))))
  (= fsm_set_96 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 97))
                     tk_payload
                     _fsm_set_97))))
  (= fsm_set_97 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 98))
                     tk_payload
                     _fsm_set_98))))
  (= fsm_set_98 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 99))
                     tk_payload
                     _fsm_set_99))))
  (= fsm_set_99 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 100))
                     tk_payload
                     _fsm_set_100))))
  (= fsm_set_100 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 101))
                     tk_payload
                     _fsm_set_101))))
  (= fsm_set_101 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 102))
                     tk_payload
                     _fsm_set_102))))
  (= fsm_set_102 a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "F") (not tk_ov_fs) (= _fsm_set_len 103))
                     tk_payload
                     _fsm_set_103))))
  (= fsm_set_103 a!1)))
(assert (let ((a!1 (ite is_first_tick
                0
                (ite (and (= tk_kind "F") (not tk_ov_fs))
                     (+ _fsm_set_len 1)
                     _fsm_set_len))))
  (= fsm_set_len a!1)))
(assert (let ((a!1 (ite is_first_tick
                0
                (ite (and (= tk_kind "F") (not tk_ov_fs)) (+ _fs_n 1) _fs_n))))
  (= fs_n a!1)))
(assert (let ((a!1 (and (= tk_kind "B")
                (> (+ (str.len _bare_reg) (str.len tk_payload)) 82500))))
  (= tk_ov_bare a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "B") (not tk_ov_bare))
                     (str.++ _bare_reg tk_payload)
                     _bare_reg))))
  (= bare_reg a!1)))
(assert (let ((a!1 (and (= tk_kind "D")
                (> (+ (str.len _bind_reg) (str.len tk_payload)) 102500))))
  (= tk_ov_bind a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "D") (not tk_ov_bind))
                     (str.++ _bind_reg tk_payload)
                     _bind_reg))))
  (= bind_reg a!1)))
(assert (let ((a!1 (and (= tk_kind "S")
                (> (+ (str.len _slot_reg) (str.len tk_payload)) 17500))))
  (= tk_ov_slot a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "S") (not tk_ov_slot))
                     (str.++ _slot_reg tk_payload)
                     _slot_reg))))
  (= slot_reg a!1)))
(assert (let ((a!1 (and (= tk_kind "M")
                (> (+ (str.len _ment_reg) (str.len tk_payload)) 2000))))
  (= tk_ov_ment a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and (= tk_kind "M") (not tk_ov_ment))
                     (str.++ _ment_reg tk_payload)
                     _ment_reg))))
  (= ment_reg a!1)))
(assert (let ((a!1 (- (str.indexof _bare_reg
                           "\u{2982}"
                           (+ tk_dr_bp (str.len tk_dr_bkey)))
              tk_dr_bp))
      (a!3 (- (str.indexof _bare_reg
                           "\u{2982}"
                           (+ tk_mn_bp (str.len tk_mn_bkey)))
              tk_mn_bp)))
(let ((a!2 (str.++ tk_dr_bkey
                   (str.substr _bare_reg
                               (+ tk_dr_bp (str.len tk_dr_bkey))
                               (- a!1 (str.len tk_dr_bkey)))
                   "\u{2982}"))
      (a!4 (str.++ tk_mn_bkey
                   (str.substr _bare_reg
                               (+ tk_mn_bp (str.len tk_mn_bkey))
                               (- a!3 (str.len tk_mn_bkey)))
                   "\u{2982}")))
  (= tk_cr_add
     (ite (= tk_kind "C") tk_payload (ite tk_dr_new a!2 (ite tk_mn_new a!4 "")))))))
(assert (let ((a!1 (and (not (= tk_cr_add ""))
                (> (+ (str.len _carry_reg) (str.len tk_cr_add)) 30500))))
  (= tk_ov_carry a!1)))
(assert (let ((a!1 (ite (and (not (= tk_cr_add "")) (not tk_ov_carry))
                (str.++ _carry_reg tk_cr_add)
                _carry_reg)))
  (= carry_reg (ite is_first_tick "" a!1))))
(assert (let ((a!1 (ite (= tk_kind "C")
                (str.substr tk_payload
                            0
                            (+ (str.indexof tk_payload "\u{27e9}" 0) 1))
                (ite tk_dr_new tk_dr_bkey (ite tk_mn_new tk_mn_bkey "")))))
  (= tk_wl_add a!1)))
(assert (let ((a!1 (and (not (= tk_wl_add ""))
                (> (+ (str.len _work_list) (str.len tk_wl_add)) 25000))))
  (= tk_ov_work a!1)))
(assert (let ((a!1 (ite (and (not (= tk_wl_add "")) (not tk_ov_work))
                (str.++ _work_list tk_wl_add)
                _work_list)))
  (= work_list (ite is_first_tick "" a!1))))
(assert (let ((a!1 (- (str.indexof _carry_reg
                           "\u{2982}"
                           (+ tk_iw_cp (str.len tk_iw_key)))
              tk_iw_cp)))
(let ((a!2 (str.++ "\u{27e6}"
                   (str.substr _bare_reg
                               (+ tk_iw_c1 1)
                               (- (- tk_iw_c2 tk_iw_c1) 1))
                   "\u{27e7}_"
                   tk_iw_x
                   " \u{2208} "
                   (str.substr _carry_reg
                               (+ tk_iw_cp (str.len tk_iw_key))
                               (- a!1 (str.len tk_iw_key))))))
  (= tk_iw_add (ite (and tk_iw_run tk_iw_isc (not tk_iw_ln0)) a!2 "")))))
(assert (let ((a!1 (and (not (= tk_iw_add ""))
                (> (+ (str.len _ins_out) (str.len tk_iw_add)) 24500))))
  (= tk_ov_ins a!1)))
(assert (let ((a!1 (ite (and (not (= tk_iw_add "")) (not tk_ov_ins))
                (str.++ _ins_out tk_iw_add)
                _ins_out)))
  (= ins_out (ite is_first_tick "" a!1))))
(assert (= tk_inj_add (ite tk_jw_flush (str.++ "\u{27e6}" _acc_b "\u{27e7}" _acc_a) "")))
(assert (let ((a!1 (and tk_jw_flush
                (> (+ (str.len _inj_out) (str.len tk_inj_add)) 1000))))
  (= tk_ov_inj a!1)))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite (and tk_jw_flush (not tk_ov_inj))
                     (str.++ _inj_out tk_inj_add)
                     _inj_out))))
  (= inj_out a!1)))
(assert (let ((a!1 (ite tk_iw_run
                (+ tk_iw_c2 1)
                (ite (and (= tk_ph 8) tk_iw_done)
                     0
                     (ite tk_jw_run (+ tk_jw_nc 1) _cur_a)))))
(let ((a!2 (ite (and (= tk_ph 7) tk_dr_done)
                0
                (ite (= tk_ph 6) (ite tk_mn_done (+ tk_dr_gt 1) _cur_a) a!1))))
(let ((a!3 (ite tk_dr_run
                (ite tk_dr_hdr_go
                     _cur_a
                     (ite tk_dr_item_done (+ tk_dr_gt 1) _cur_a))
                a!2)))
  (= cur_a (ite is_first_tick 0 (ite (= tk_ph 1) 0 a!3)))))))
(assert (let ((a!1 (ite tk_dr_run
                (ite tk_dr_item_done 0 (+ tk_dr_m 1))
                (ite (= tk_ph 6) (ite tk_mn_done 0 (+ tk_mn_p 1)) _cur_b))))
  (= cur_b (ite is_first_tick 0 (ite (= tk_ph 1) 0 a!1)))))
(assert (let ((a!1 (and tk_jw_run
                (not tk_jw_newgrp)
                tk_jw_give
                (> (+ (str.len _acc_a) (str.len tk_jw_add)) 4000))))
  (= tk_ov_acca a!1)))
(assert (let ((a!1 (ite tk_jw_newgrp
                (ite tk_jw_give tk_jw_add "")
                (ite (and tk_jw_run tk_jw_give (not tk_ov_acca))
                     (str.++ _acc_a tk_jw_add)
                     _acc_a))))
  (= acc_a (ite is_first_tick "" (ite (= tk_ph 7) "" a!1)))))
(assert (= tk_ov_accb (and tk_jw_run (> (str.len tk_jw_ln) 4000))))
(assert (let ((a!1 (ite (= tk_ph 7)
                ""
                (ite (and tk_jw_run (not tk_ov_accb)) tk_jw_ln _acc_b))))
  (= acc_b (ite is_first_tick "" a!1))))
(assert (= eff_nop (LibCall "libc" "getpid" __Empty_LibArg)))
(assert (let ((a!1 (__Cell_LibArg (ArgStr (ite (= tk_ph 10) _ins_out _inj_out))
                          __Empty_LibArg)))
  (= eff_out (LibCall "libc" "puts" a!1))))
(assert (= tk_ov_any
   (or tk_ov_fs
       tk_ov_bare
       tk_ov_bind
       tk_ov_slot
       tk_ov_ment
       tk_ov_carry
       tk_ov_work
       tk_ov_ins
       tk_ov_inj
       tk_ov_acca
       tk_ov_accb)))
(assert (let ((a!1 (ite tk_ov_carry
                75
                (ite tk_ov_work 76 (ite tk_ov_ins 77 (ite tk_ov_inj 78 79))))))
(let ((a!2 (ite tk_ov_bare
                70
                (ite tk_ov_bind 72 (ite tk_ov_slot 73 (ite tk_ov_ment 74 a!1))))))
  (= tk_ov_code (ite tk_ov_fs 71 a!2)))))
(assert (let ((a!1 (ite tk_ov_carry
                "autocarry_fix: carry registry overflow (cap 30500, exit 75)\u{a}"
                (ite tk_ov_work
                     "autocarry_fix: worklist overflow (cap 25000, exit 76)\u{a}"
                     (ite tk_ov_ins
                          "autocarry_fix: insert-script overflow (cap 24500, exit 77)\u{a}"
                          (ite tk_ov_inj
                               "autocarry_fix: inject-script overflow (cap 1000, exit 78)\u{a}"
                               "autocarry_fix: injection-accumulator overflow (cap 4000, exit 79)\u{a}"))))))
(let ((a!2 (ite tk_ov_bare
                "autocarry_fix: bare-decl registry overflow (cap 82500, exit 70)\u{a}"
                (ite tk_ov_bind
                     "autocarry_fix: call-bind registry overflow (cap 102500, exit 72)\u{a}"
                     (ite tk_ov_slot
                          "autocarry_fix: slot-have registry overflow (cap 17500, exit 73)\u{a}"
                          (ite tk_ov_ment
                               "autocarry_fix: bare-mention registry overflow (cap 2000, exit 74)\u{a}"
                               a!1))))))
  (= tk_ov_msg
     (ite tk_ov_fs
          "autocarry_fix: fsm-name registry overflow (cap 104, exit 71)\u{a}"
          a!2)))))
(assert (let ((a!1 (__Cell_LibArg (ArgStr tk_ov_msg)
                          (__Cell_LibArg (ArgInt (str.len tk_ov_msg))
                                         __Empty_LibArg))))
  (= eff_ov_msg (LibCall "libc" "write" (__Cell_LibArg (ArgInt 2) a!1)))))
(assert (= eff_ov_exit (Exit tk_ov_code)))
(assert (= tk_read_go (and (= tk_ph 1) (not tk_reof))))
(assert (let ((a!1 (and (= effects__len 2)
                (= (select effects 0) eff_nop)
                (= (select effects 1) ReadLine)))
      (a!2 (=> tk_ov_any
               (and (= effects__len 2)
                    (= (select effects 0) eff_ov_msg)
                    (= (select effects 1) eff_ov_exit))))
      (a!3 (=> (= tk_ph 10)
               (and (= effects__len 2)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) eff_nop))))
      (a!4 (=> (= tk_ph 11)
               (and (= effects__len 2)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) (Exit 0)))))
      (a!5 (=> (not (= tk_ph 11))
               (and (= effects__len 2)
                    (= (select effects 0) eff_nop)
                    (= (select effects 1) eff_nop)))))
(let ((a!6 (and a!3 (=> (not (= tk_ph 10)) (and a!4 a!5)))))
(let ((a!7 (=> (not tk_ov_any)
               (and (=> tk_read_go a!1) (=> (not tk_read_go) a!6)))))
  (and (=> is_first_tick a!1) (=> (not is_first_tick) (and a!2 a!7)))))))
(declare-fun _eff_nop () Effect)
(declare-fun _eff_out () Effect)
(declare-fun _eff_ov_exit () Effect)
(declare-fun _eff_ov_msg () Effect)
(declare-fun _tk_cr_add () String)
(declare-fun _tk_dr_bkey () String)
(declare-fun _tk_dr_bp () Int)
(declare-fun _tk_dr_done () Bool)
(declare-fun _tk_dr_dot () Int)
(declare-fun _tk_dr_f () String)
(declare-fun _tk_dr_f2 () String)
(declare-fun _tk_dr_fkey () String)
(declare-fun _tk_dr_fsm_ok () Bool)
(declare-fun _tk_dr_gt () Int)
(declare-fun _tk_dr_hdr_go () Bool)
(declare-fun _tk_dr_item_done () Bool)
(declare-fun _tk_dr_m () Int)
(declare-fun _tk_dr_new () Bool)
(declare-fun _tk_dr_run () Bool)
(declare-fun _tk_dr_val () String)
(declare-fun _tk_dr_vc () Int)
(declare-fun _tk_dr_vs () Int)
(declare-fun _tk_dr_x () String)
(declare-fun _tk_fx_run () Bool)
(declare-fun _tk_inj_add () String)
(declare-fun _tk_iw_add () String)
(declare-fun _tk_iw_c1 () Int)
(declare-fun _tk_iw_c2 () Int)
(declare-fun _tk_iw_cp () Int)
(declare-fun _tk_iw_done () Bool)
(declare-fun _tk_iw_gt () Int)
(declare-fun _tk_iw_isc () Bool)
(declare-fun _tk_iw_key () String)
(declare-fun _tk_iw_ln0 () Bool)
(declare-fun _tk_iw_run () Bool)
(declare-fun _tk_iw_x () String)
(declare-fun _tk_jw_add () String)
(declare-fun _tk_jw_brk () Int)
(declare-fun _tk_jw_cA () Int)
(declare-fun _tk_jw_done () Bool)
(declare-fun _tk_jw_fc () Int)
(declare-fun _tk_jw_fkey () String)
(declare-fun _tk_jw_flush () Bool)
(declare-fun _tk_jw_give () Bool)
(declare-fun _tk_jw_have () Bool)
(declare-fun _tk_jw_hdr () Bool)
(declare-fun _tk_jw_hp () Int)
(declare-fun _tk_jw_ln () String)
(declare-fun _tk_jw_nc () Int)
(declare-fun _tk_jw_newgrp () Bool)
(declare-fun _tk_jw_run () Bool)
(declare-fun _tk_jw_slot () String)
(declare-fun _tk_jw_sub () String)
(declare-fun _tk_jw_val () String)
(declare-fun _tk_jw_vc () Int)
(declare-fun _tk_kind () String)
(declare-fun _tk_mn_bkey () String)
(declare-fun _tk_mn_bp () Int)
(declare-fun _tk_mn_done () Bool)
(declare-fun _tk_mn_e () Int)
(declare-fun _tk_mn_new () Bool)
(declare-fun _tk_mn_on () Bool)
(declare-fun _tk_mn_p () Int)
(declare-fun _tk_mn_par () String)
(declare-fun _tk_ov_acca () Bool)
(declare-fun _tk_ov_accb () Bool)
(declare-fun _tk_ov_any () Bool)
(declare-fun _tk_ov_bare () Bool)
(declare-fun _tk_ov_bind () Bool)
(declare-fun _tk_ov_carry () Bool)
(declare-fun _tk_ov_code () Int)
(declare-fun _tk_ov_fs () Bool)
(declare-fun _tk_ov_inj () Bool)
(declare-fun _tk_ov_ins () Bool)
(declare-fun _tk_ov_ment () Bool)
(declare-fun _tk_ov_msg () String)
(declare-fun _tk_ov_slot () Bool)
(declare-fun _tk_ov_work () Bool)
(declare-fun _tk_payload () String)
(declare-fun _tk_ph () Int)
(declare-fun _tk_rd () Bool)
(declare-fun _tk_read_go () Bool)
(declare-fun _tk_reof () Bool)
(declare-fun _tk_rline () String)
(declare-fun _tk_wl_add () String)
