;; manifest: state-fields = code:String eff_nop:Effect eff_out:Effect emit_base:String emit_haslen:Bool emit_inside:String emit_k:Int emit_kind:Int emit_n:Int emit_ne:Int emit_nm:String indent:String line:String phase:Int reg:String sub_acc:String sub_pos:Int sub_src:String tk_after_lead:Int tk_at:Int tk_bkey:String tk_bound_d1:Int tk_bound_d2:Int tk_bound_hl:Bool tk_bound_hl_at:Int tk_bound_n:String tk_bound_reg:Bool tk_bv_e0:Int tk_bv_s0:Int tk_code:String tk_count_el:Int tk_d0:Int tk_d1:Int tk_d2:Int tk_decl_eq:Int tk_decl_lit:Bool tk_default:Bool tk_default_plain:Bool tk_drop_bound:Bool tk_el:String tk_emit:Bool tk_enter_dual:Bool tk_enter_loop:Bool tk_eof_now:Bool tk_glyph:String tk_has_len_lines:Bool tk_hash_after:String tk_hash_aws:Int tk_hh_e:Int tk_hh_s:Int tk_ie:Int tk_ind:String tk_inside:String tk_inside_tl:Int tk_is_assign:Bool tk_is_bound_line:Bool tk_is_decl:Bool tk_is_hold:Bool tk_is_litassign:Bool tk_is_top:Bool tk_key:String tk_lead:String tk_lead_base:String tk_lead_is_dual:Bool tk_len_lines:String tk_loop_done:Bool tk_loop_run:Bool tk_lt:Int tk_needs_walk:Bool tk_ph:Int tk_print_now:String tk_rbase:String tk_read_go:Bool tk_reg_hit:Bool tk_reg_line:Bool tk_reof:Bool tk_rewrite_bound:Bool tk_rhaslen:Bool tk_rhs:String tk_rhs_s:Int tk_rline:String tk_rn:Int tk_rt:Int tk_slot_line:String tk_slot_pfx:String tk_src:Bool tk_vs:Int tk_walk_done:Bool tk_walk_run:Bool tk_ws:Int tk_zdef:String w_base:String w_base_reg:Bool w_cb:Int w_ch:String w_do_index:Bool w_dot:Bool w_fe:Int w_field:String w_followed_br:Bool w_fs:Int w_has_field:Bool w_has_sub:Bool w_he:Int w_idx:String w_idx_ok:Bool w_index_end:Int w_index_out:String w_inner:String w_is_hash:Bool w_is_ident:Bool w_next:Int w_p:Int w_scb:Int w_sidx_ok:Bool w_sinner:String w_src:String w_sub_br:Bool w_tok:String w_unit:String w_we:Int w_word:String w_word_reg:Bool
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
(declare-fun tk_reg_line () Bool)
(declare-fun tk_src () Bool)
(declare-fun tk_eof_now () Bool)
(declare-fun tk_d0 () Int)
(declare-fun tk_code () String)
(declare-fun tk_ws () Int)
(declare-fun tk_ind () String)
(declare-fun tk_ie () Int)
(declare-fun tk_lead () String)
(declare-fun tk_is_top () Bool)
(declare-fun tk_lead_base () String)
(declare-fun tk_lead_is_dual () Bool)
(declare-fun tk_key () String)
(declare-fun _reg () String)
(declare-fun tk_at () Int)
(declare-fun tk_reg_hit () Bool)
(declare-fun tk_vs () Int)
(declare-fun tk_d1 () Int)
(declare-fun tk_rbase () String)
(declare-fun tk_d2 () Int)
(declare-fun tk_rn () Int)
(declare-fun tk_rhaslen () Bool)
(declare-fun tk_after_lead () Int)
(declare-fun tk_glyph () String)
(declare-fun tk_is_decl () Bool)
(declare-fun tk_is_assign () Bool)
(declare-fun tk_decl_eq () Int)
(declare-fun tk_decl_lit () Bool)
(declare-fun tk_rhs_s () Int)
(declare-fun tk_rhs () String)
(declare-fun tk_is_hold () Bool)
(declare-fun tk_is_litassign () Bool)
(declare-fun tk_lt () Int)
(declare-fun tk_rt () Int)
(declare-fun tk_inside () String)
(declare-fun tk_enter_loop () Bool)
(declare-fun tk_enter_dual () Bool)
(declare-fun tk_hh_s () Int)
(declare-fun tk_hh_e () Int)
(declare-fun tk_hash_after () String)
(declare-fun tk_hash_aws () Int)
(declare-fun tk_is_bound_line () Bool)
(declare-fun tk_bkey () String)
(declare-fun tk_bound_reg () Bool)
(declare-fun tk_bound_hl_at () Int)
(declare-fun tk_bound_d1 () Int)
(declare-fun tk_bound_d2 () Int)
(declare-fun tk_bound_hl () Bool)
(declare-fun tk_drop_bound () Bool)
(declare-fun tk_bv_s0 () Int)
(declare-fun tk_bv_e0 () Int)
(declare-fun tk_bound_n () String)
(declare-fun tk_rewrite_bound () Bool)
(declare-fun tk_default () Bool)
(declare-fun _emit_n () Int)
(declare-fun _emit_k () Int)
(declare-fun tk_loop_run () Bool)
(declare-fun tk_loop_done () Bool)
(declare-fun LsCommaPos__cp15__call10 () Int)
(declare-fun LsCommaPos__cp14__call10 () Int)
(declare-fun LsCommaPos__cp13__call10 () Int)
(declare-fun LsCommaPos__cp12__call10 () Int)
(declare-fun LsCommaPos__cp11__call10 () Int)
(declare-fun LsCommaPos__cp10__call10 () Int)
(declare-fun LsCommaPos__cp9__call10 () Int)
(declare-fun LsCommaPos__cp8__call10 () Int)
(declare-fun LsCommaPos__cp7__call10 () Int)
(declare-fun LsCommaPos__cp6__call10 () Int)
(declare-fun LsCommaPos__cp5__call10 () Int)
(declare-fun LsCommaPos__cp4__call10 () Int)
(declare-fun LsCommaPos__cp3__call10 () Int)
(declare-fun LsCommaPos__cp2__call10 () Int)
(declare-fun LsCommaPos__cp1__call10 () Int)
(declare-fun LsCommaPos__cp0__call10 () Int)
(declare-fun LsNthElem__ne_pstart__call9 () Int)
(declare-fun _emit_inside () String)
(declare-fun LsCommaPos__cp15__call11 () Int)
(declare-fun LsCommaPos__cp14__call11 () Int)
(declare-fun LsCommaPos__cp13__call11 () Int)
(declare-fun LsCommaPos__cp12__call11 () Int)
(declare-fun LsCommaPos__cp11__call11 () Int)
(declare-fun LsCommaPos__cp10__call11 () Int)
(declare-fun LsCommaPos__cp9__call11 () Int)
(declare-fun LsCommaPos__cp8__call11 () Int)
(declare-fun LsCommaPos__cp7__call11 () Int)
(declare-fun LsCommaPos__cp6__call11 () Int)
(declare-fun LsCommaPos__cp5__call11 () Int)
(declare-fun LsCommaPos__cp4__call11 () Int)
(declare-fun LsCommaPos__cp3__call11 () Int)
(declare-fun LsCommaPos__cp2__call11 () Int)
(declare-fun LsCommaPos__cp1__call11 () Int)
(declare-fun LsCommaPos__cp0__call11 () Int)
(declare-fun LsNthElem__ne_pend__call9 () Int)
(declare-fun LsNthElem__ne_raw_s__call9 () Int)
(declare-fun LsNthElem__ne_raw_e__call9 () Int)
(declare-fun LsNthElem__ne_ts__call9 () Int)
(declare-fun LsNthElem__ne_te__call9 () Int)
(declare-fun tk_el () String)
(declare-fun _emit_base () String)
(declare-fun tk_zdef () String)
(declare-fun _emit_nm () String)
(declare-fun _emit_kind () Int)
(declare-fun tk_slot_pfx () String)
(declare-fun _emit_ne () Int)
(declare-fun tk_slot_line () String)
(declare-fun _indent () String)
(declare-fun _emit_haslen () Bool)
(declare-fun tk_len_lines () String)
(declare-fun tk_has_len_lines () Bool)
(declare-fun tk_needs_walk () Bool)
(declare-fun tk_default_plain () Bool)
(declare-fun _sub_src () String)
(declare-fun w_src () String)
(declare-fun _sub_pos () Int)
(declare-fun w_p () Int)
(declare-fun w_ch () String)
(declare-fun w_is_hash () Bool)
(declare-fun w_he () Int)
(declare-fun w_word () String)
(declare-fun w_word_reg () Bool)
(declare-fun w_we () Int)
(declare-fun w_is_ident () Bool)
(declare-fun w_tok () String)
(declare-fun w_followed_br () Bool)
(declare-fun w_base () String)
(declare-fun w_base_reg () Bool)
(declare-fun w_cb () Int)
(declare-fun w_inner () String)
(declare-fun LsStripWs__sw24__call17 () String)
(declare-fun LsIdxEval__ie_t__call16 () String)
(declare-fun LsStripWs__sw_keep23__call17 () String)
(declare-fun LsStripWs__sw_keep22__call17 () String)
(declare-fun LsStripWs__sw_keep21__call17 () String)
(declare-fun LsStripWs__sw_keep20__call17 () String)
(declare-fun LsStripWs__sw_keep19__call17 () String)
(declare-fun LsStripWs__sw_keep18__call17 () String)
(declare-fun LsStripWs__sw_keep17__call17 () String)
(declare-fun LsStripWs__sw_keep16__call17 () String)
(declare-fun LsStripWs__sw_keep15__call17 () String)
(declare-fun LsStripWs__sw_keep14__call17 () String)
(declare-fun LsStripWs__sw_keep13__call17 () String)
(declare-fun LsStripWs__sw_keep12__call17 () String)
(declare-fun LsStripWs__sw_keep11__call17 () String)
(declare-fun LsStripWs__sw_keep10__call17 () String)
(declare-fun LsStripWs__sw_keep9__call17 () String)
(declare-fun LsStripWs__sw_keep8__call17 () String)
(declare-fun LsStripWs__sw_keep7__call17 () String)
(declare-fun LsStripWs__sw_keep6__call17 () String)
(declare-fun LsStripWs__sw_keep5__call17 () String)
(declare-fun LsStripWs__sw_keep4__call17 () String)
(declare-fun LsStripWs__sw_keep3__call17 () String)
(declare-fun LsStripWs__sw_keep2__call17 () String)
(declare-fun LsStripWs__sw_keep1__call17 () String)
(declare-fun LsStripWs__sw_keep0__call17 () String)
(declare-fun LsOnlyIdxChars__oic_bad__call18 () Int)
(declare-fun LsIdxEval__ie_valid_chars__call16 () Bool)
(declare-fun LsOnlyIdxChars__oic_b23__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b22__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b21__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b20__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b19__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b18__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b17__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b16__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b15__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b14__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b13__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b12__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b11__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b10__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b9__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b8__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b7__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b6__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b5__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b4__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b3__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b2__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b1__call18 () Bool)
(declare-fun LsOnlyIdxChars__oic_b0__call18 () Bool)
(declare-fun LsIdxEval__ie_starts_digit__call16 () Bool)
(declare-fun LsIdxEval__ie_ne0__call16 () Int)
(declare-fun LsIdxEval__ie_op0__call16 () String)
(declare-fun LsIdxEval__ie_s1__call16 () Int)
(declare-fun LsIdxEval__ie_ne1__call16 () Int)
(declare-fun LsIdxEval__ie_op1__call16 () String)
(declare-fun LsIdxEval__ie_s2__call16 () Int)
(declare-fun LsIdxEval__ie_ne2__call16 () Int)
(declare-fun LsIdxEval__ie_op2__call16 () String)
(declare-fun LsIdxEval__ie_s3__call16 () Int)
(declare-fun LsIdxEval__ie_ne3__call16 () Int)
(declare-fun LsIdxEval__ie_op3__call16 () String)
(declare-fun LsIdxEval__ie_s4__call16 () Int)
(declare-fun LsIdxEval__ie_ne4__call16 () Int)
(declare-fun LsIdxEval__ie_op4__call16 () String)
(declare-fun LsIdxEval__ie_s5__call16 () Int)
(declare-fun LsIdxEval__ie_ne5__call16 () Int)
(declare-fun LsIdxEval__ie_n0__call16 () Int)
(declare-fun LsIdxEval__ie_n1__call16 () Int)
(declare-fun LsIdxEval__ie_n2__call16 () Int)
(declare-fun LsIdxEval__ie_n3__call16 () Int)
(declare-fun LsIdxEval__ie_n4__call16 () Int)
(declare-fun LsIdxEval__ie_n5__call16 () Int)
(declare-fun LsIdxEval__ie_cnt__call16 () Int)
(declare-fun LsIdxEval__ie_shape_ok__call16 () Bool)
(declare-fun LsIdxEval__ie_g0__call16 () Int)
(declare-fun LsIdxEval__ie_t0__call16 () Int)
(declare-fun LsIdxEval__ie_sg0__call16 () Int)
(declare-fun LsIdxEval__ie_g1__call16 () Int)
(declare-fun LsIdxEval__ie_t1__call16 () Int)
(declare-fun LsIdxEval__ie_sg1__call16 () Int)
(declare-fun LsIdxEval__ie_g2__call16 () Int)
(declare-fun LsIdxEval__ie_t2__call16 () Int)
(declare-fun LsIdxEval__ie_sg2__call16 () Int)
(declare-fun LsIdxEval__ie_g3__call16 () Int)
(declare-fun LsIdxEval__ie_t3__call16 () Int)
(declare-fun LsIdxEval__ie_sg3__call16 () Int)
(declare-fun LsIdxEval__ie_g4__call16 () Int)
(declare-fun LsIdxEval__ie_t4__call16 () Int)
(declare-fun LsIdxEval__ie_sg4__call16 () Int)
(declare-fun LsIdxEval__ie_g5__call16 () Int)
(declare-fun LsIdxEval__ie_t5__call16 () Int)
(declare-fun LsIdxEval__ie_sg5__call16 () Int)
(declare-fun LsIdxEval__ie_total__call16 () Int)
(declare-fun w_idx_ok () Bool)
(declare-fun w_idx () String)
(declare-fun w_do_index () Bool)
(declare-fun w_dot () Bool)
(declare-fun w_fs () Int)
(declare-fun w_fe () Int)
(declare-fun w_has_field () Bool)
(declare-fun w_field () String)
(declare-fun w_sub_br () Bool)
(declare-fun w_scb () Int)
(declare-fun w_sinner () String)
(declare-fun LsAllDigits__ad_ok__call32 () Bool)
(declare-fun w_sidx_ok () Bool)
(declare-fun LsAllDigits__ad_first__call32 () Int)
(declare-fun LsAllDigits__ad_d15__call32 () Bool)
(declare-fun LsAllDigits__ad_d14__call32 () Bool)
(declare-fun LsAllDigits__ad_d13__call32 () Bool)
(declare-fun LsAllDigits__ad_d12__call32 () Bool)
(declare-fun LsAllDigits__ad_d11__call32 () Bool)
(declare-fun LsAllDigits__ad_d10__call32 () Bool)
(declare-fun LsAllDigits__ad_d9__call32 () Bool)
(declare-fun LsAllDigits__ad_d8__call32 () Bool)
(declare-fun LsAllDigits__ad_d7__call32 () Bool)
(declare-fun LsAllDigits__ad_d6__call32 () Bool)
(declare-fun LsAllDigits__ad_d5__call32 () Bool)
(declare-fun LsAllDigits__ad_d4__call32 () Bool)
(declare-fun LsAllDigits__ad_d3__call32 () Bool)
(declare-fun LsAllDigits__ad_d2__call32 () Bool)
(declare-fun LsAllDigits__ad_d1__call32 () Bool)
(declare-fun LsAllDigits__ad_d0__call32 () Bool)
(declare-fun w_has_sub () Bool)
(declare-fun w_index_out () String)
(declare-fun w_index_end () Int)
(declare-fun w_unit () String)
(declare-fun w_next () Int)
(declare-fun tk_walk_run () Bool)
(declare-fun tk_walk_done () Bool)
(declare-fun _sub_acc () String)
(declare-fun tk_print_now () String)
(declare-fun tk_emit () Bool)
(declare-fun phase () Int)
(declare-fun reg () String)
(declare-fun _line () String)
(declare-fun line () String)
(declare-fun _code () String)
(declare-fun code () String)
(declare-fun indent () String)
(declare-fun sub_src () String)
(declare-fun sub_pos () Int)
(declare-fun sub_acc () String)
(declare-fun emit_kind () Int)
(declare-fun emit_nm () String)
(declare-fun emit_base () String)
(declare-fun emit_n () Int)
(declare-fun emit_haslen () Bool)
(declare-fun emit_k () Int)
(declare-fun emit_inside () String)
(declare-fun tk_inside_tl () Int)
(declare-fun tk_count_el () Int)
(declare-fun emit_ne () Int)
(declare-fun LsCountElem__ce_n__call34 () Int)
(declare-fun LsCountElem__ce_scan__call34 () Int)
(declare-fun LsCountElem__ce_count__call34 () Int)
(declare-fun LsCountElem__cP15__call34 () Int)
(declare-fun LsCountElem__cP14__call34 () Int)
(declare-fun LsCountElem__cP13__call34 () Int)
(declare-fun LsCountElem__cP12__call34 () Int)
(declare-fun LsCountElem__cP11__call34 () Int)
(declare-fun LsCountElem__cP10__call34 () Int)
(declare-fun LsCountElem__cP9__call34 () Int)
(declare-fun LsCountElem__cP8__call34 () Int)
(declare-fun LsCountElem__cP7__call34 () Int)
(declare-fun LsCountElem__cP6__call34 () Int)
(declare-fun LsCountElem__cP5__call34 () Int)
(declare-fun LsCountElem__cP4__call34 () Int)
(declare-fun LsCountElem__cP3__call34 () Int)
(declare-fun LsCountElem__cP2__call34 () Int)
(declare-fun LsCountElem__cP1__call34 () Int)
(declare-fun LsCountElem__cP0__call34 () Int)
(declare-fun LsCommaPos__cp15__call35 () Int)
(declare-fun LsCommaPos__cp14__call35 () Int)
(declare-fun LsCommaPos__cp13__call35 () Int)
(declare-fun LsCommaPos__cp12__call35 () Int)
(declare-fun LsCommaPos__cp11__call35 () Int)
(declare-fun LsCommaPos__cp10__call35 () Int)
(declare-fun LsCommaPos__cp9__call35 () Int)
(declare-fun LsCommaPos__cp8__call35 () Int)
(declare-fun LsCommaPos__cp7__call35 () Int)
(declare-fun LsCommaPos__cp6__call35 () Int)
(declare-fun LsCommaPos__cp5__call35 () Int)
(declare-fun LsCommaPos__cp4__call35 () Int)
(declare-fun LsCommaPos__cp3__call35 () Int)
(declare-fun LsCommaPos__cp2__call35 () Int)
(declare-fun LsCommaPos__cp1__call35 () Int)
(declare-fun LsCommaPos__cp0__call35 () Int)
(declare-fun LsCommaPos__cp15__call36 () Int)
(declare-fun LsCommaPos__cp14__call36 () Int)
(declare-fun LsCommaPos__cp13__call36 () Int)
(declare-fun LsCommaPos__cp12__call36 () Int)
(declare-fun LsCommaPos__cp11__call36 () Int)
(declare-fun LsCommaPos__cp10__call36 () Int)
(declare-fun LsCommaPos__cp9__call36 () Int)
(declare-fun LsCommaPos__cp8__call36 () Int)
(declare-fun LsCommaPos__cp7__call36 () Int)
(declare-fun LsCommaPos__cp6__call36 () Int)
(declare-fun LsCommaPos__cp5__call36 () Int)
(declare-fun LsCommaPos__cp4__call36 () Int)
(declare-fun LsCommaPos__cp3__call36 () Int)
(declare-fun LsCommaPos__cp2__call36 () Int)
(declare-fun LsCommaPos__cp1__call36 () Int)
(declare-fun LsCommaPos__cp0__call36 () Int)
(declare-fun LsCommaPos__cp15__call37 () Int)
(declare-fun LsCommaPos__cp14__call37 () Int)
(declare-fun LsCommaPos__cp13__call37 () Int)
(declare-fun LsCommaPos__cp12__call37 () Int)
(declare-fun LsCommaPos__cp11__call37 () Int)
(declare-fun LsCommaPos__cp10__call37 () Int)
(declare-fun LsCommaPos__cp9__call37 () Int)
(declare-fun LsCommaPos__cp8__call37 () Int)
(declare-fun LsCommaPos__cp7__call37 () Int)
(declare-fun LsCommaPos__cp6__call37 () Int)
(declare-fun LsCommaPos__cp5__call37 () Int)
(declare-fun LsCommaPos__cp4__call37 () Int)
(declare-fun LsCommaPos__cp3__call37 () Int)
(declare-fun LsCommaPos__cp2__call37 () Int)
(declare-fun LsCommaPos__cp1__call37 () Int)
(declare-fun LsCommaPos__cp0__call37 () Int)
(declare-fun LsCommaPos__cp15__call38 () Int)
(declare-fun LsCommaPos__cp14__call38 () Int)
(declare-fun LsCommaPos__cp13__call38 () Int)
(declare-fun LsCommaPos__cp12__call38 () Int)
(declare-fun LsCommaPos__cp11__call38 () Int)
(declare-fun LsCommaPos__cp10__call38 () Int)
(declare-fun LsCommaPos__cp9__call38 () Int)
(declare-fun LsCommaPos__cp8__call38 () Int)
(declare-fun LsCommaPos__cp7__call38 () Int)
(declare-fun LsCommaPos__cp6__call38 () Int)
(declare-fun LsCommaPos__cp5__call38 () Int)
(declare-fun LsCommaPos__cp4__call38 () Int)
(declare-fun LsCommaPos__cp3__call38 () Int)
(declare-fun LsCommaPos__cp2__call38 () Int)
(declare-fun LsCommaPos__cp1__call38 () Int)
(declare-fun LsCommaPos__cp0__call38 () Int)
(declare-fun LsCommaPos__cp15__call39 () Int)
(declare-fun LsCommaPos__cp14__call39 () Int)
(declare-fun LsCommaPos__cp13__call39 () Int)
(declare-fun LsCommaPos__cp12__call39 () Int)
(declare-fun LsCommaPos__cp11__call39 () Int)
(declare-fun LsCommaPos__cp10__call39 () Int)
(declare-fun LsCommaPos__cp9__call39 () Int)
(declare-fun LsCommaPos__cp8__call39 () Int)
(declare-fun LsCommaPos__cp7__call39 () Int)
(declare-fun LsCommaPos__cp6__call39 () Int)
(declare-fun LsCommaPos__cp5__call39 () Int)
(declare-fun LsCommaPos__cp4__call39 () Int)
(declare-fun LsCommaPos__cp3__call39 () Int)
(declare-fun LsCommaPos__cp2__call39 () Int)
(declare-fun LsCommaPos__cp1__call39 () Int)
(declare-fun LsCommaPos__cp0__call39 () Int)
(declare-fun LsCommaPos__cp15__call40 () Int)
(declare-fun LsCommaPos__cp14__call40 () Int)
(declare-fun LsCommaPos__cp13__call40 () Int)
(declare-fun LsCommaPos__cp12__call40 () Int)
(declare-fun LsCommaPos__cp11__call40 () Int)
(declare-fun LsCommaPos__cp10__call40 () Int)
(declare-fun LsCommaPos__cp9__call40 () Int)
(declare-fun LsCommaPos__cp8__call40 () Int)
(declare-fun LsCommaPos__cp7__call40 () Int)
(declare-fun LsCommaPos__cp6__call40 () Int)
(declare-fun LsCommaPos__cp5__call40 () Int)
(declare-fun LsCommaPos__cp4__call40 () Int)
(declare-fun LsCommaPos__cp3__call40 () Int)
(declare-fun LsCommaPos__cp2__call40 () Int)
(declare-fun LsCommaPos__cp1__call40 () Int)
(declare-fun LsCommaPos__cp0__call40 () Int)
(declare-fun LsCommaPos__cp15__call41 () Int)
(declare-fun LsCommaPos__cp14__call41 () Int)
(declare-fun LsCommaPos__cp13__call41 () Int)
(declare-fun LsCommaPos__cp12__call41 () Int)
(declare-fun LsCommaPos__cp11__call41 () Int)
(declare-fun LsCommaPos__cp10__call41 () Int)
(declare-fun LsCommaPos__cp9__call41 () Int)
(declare-fun LsCommaPos__cp8__call41 () Int)
(declare-fun LsCommaPos__cp7__call41 () Int)
(declare-fun LsCommaPos__cp6__call41 () Int)
(declare-fun LsCommaPos__cp5__call41 () Int)
(declare-fun LsCommaPos__cp4__call41 () Int)
(declare-fun LsCommaPos__cp3__call41 () Int)
(declare-fun LsCommaPos__cp2__call41 () Int)
(declare-fun LsCommaPos__cp1__call41 () Int)
(declare-fun LsCommaPos__cp0__call41 () Int)
(declare-fun LsCommaPos__cp15__call42 () Int)
(declare-fun LsCommaPos__cp14__call42 () Int)
(declare-fun LsCommaPos__cp13__call42 () Int)
(declare-fun LsCommaPos__cp12__call42 () Int)
(declare-fun LsCommaPos__cp11__call42 () Int)
(declare-fun LsCommaPos__cp10__call42 () Int)
(declare-fun LsCommaPos__cp9__call42 () Int)
(declare-fun LsCommaPos__cp8__call42 () Int)
(declare-fun LsCommaPos__cp7__call42 () Int)
(declare-fun LsCommaPos__cp6__call42 () Int)
(declare-fun LsCommaPos__cp5__call42 () Int)
(declare-fun LsCommaPos__cp4__call42 () Int)
(declare-fun LsCommaPos__cp3__call42 () Int)
(declare-fun LsCommaPos__cp2__call42 () Int)
(declare-fun LsCommaPos__cp1__call42 () Int)
(declare-fun LsCommaPos__cp0__call42 () Int)
(declare-fun LsCommaPos__cp15__call43 () Int)
(declare-fun LsCommaPos__cp14__call43 () Int)
(declare-fun LsCommaPos__cp13__call43 () Int)
(declare-fun LsCommaPos__cp12__call43 () Int)
(declare-fun LsCommaPos__cp11__call43 () Int)
(declare-fun LsCommaPos__cp10__call43 () Int)
(declare-fun LsCommaPos__cp9__call43 () Int)
(declare-fun LsCommaPos__cp8__call43 () Int)
(declare-fun LsCommaPos__cp7__call43 () Int)
(declare-fun LsCommaPos__cp6__call43 () Int)
(declare-fun LsCommaPos__cp5__call43 () Int)
(declare-fun LsCommaPos__cp4__call43 () Int)
(declare-fun LsCommaPos__cp3__call43 () Int)
(declare-fun LsCommaPos__cp2__call43 () Int)
(declare-fun LsCommaPos__cp1__call43 () Int)
(declare-fun LsCommaPos__cp0__call43 () Int)
(declare-fun LsCommaPos__cp15__call44 () Int)
(declare-fun LsCommaPos__cp14__call44 () Int)
(declare-fun LsCommaPos__cp13__call44 () Int)
(declare-fun LsCommaPos__cp12__call44 () Int)
(declare-fun LsCommaPos__cp11__call44 () Int)
(declare-fun LsCommaPos__cp10__call44 () Int)
(declare-fun LsCommaPos__cp9__call44 () Int)
(declare-fun LsCommaPos__cp8__call44 () Int)
(declare-fun LsCommaPos__cp7__call44 () Int)
(declare-fun LsCommaPos__cp6__call44 () Int)
(declare-fun LsCommaPos__cp5__call44 () Int)
(declare-fun LsCommaPos__cp4__call44 () Int)
(declare-fun LsCommaPos__cp3__call44 () Int)
(declare-fun LsCommaPos__cp2__call44 () Int)
(declare-fun LsCommaPos__cp1__call44 () Int)
(declare-fun LsCommaPos__cp0__call44 () Int)
(declare-fun LsCommaPos__cp15__call45 () Int)
(declare-fun LsCommaPos__cp14__call45 () Int)
(declare-fun LsCommaPos__cp13__call45 () Int)
(declare-fun LsCommaPos__cp12__call45 () Int)
(declare-fun LsCommaPos__cp11__call45 () Int)
(declare-fun LsCommaPos__cp10__call45 () Int)
(declare-fun LsCommaPos__cp9__call45 () Int)
(declare-fun LsCommaPos__cp8__call45 () Int)
(declare-fun LsCommaPos__cp7__call45 () Int)
(declare-fun LsCommaPos__cp6__call45 () Int)
(declare-fun LsCommaPos__cp5__call45 () Int)
(declare-fun LsCommaPos__cp4__call45 () Int)
(declare-fun LsCommaPos__cp3__call45 () Int)
(declare-fun LsCommaPos__cp2__call45 () Int)
(declare-fun LsCommaPos__cp1__call45 () Int)
(declare-fun LsCommaPos__cp0__call45 () Int)
(declare-fun LsCommaPos__cp15__call46 () Int)
(declare-fun LsCommaPos__cp14__call46 () Int)
(declare-fun LsCommaPos__cp13__call46 () Int)
(declare-fun LsCommaPos__cp12__call46 () Int)
(declare-fun LsCommaPos__cp11__call46 () Int)
(declare-fun LsCommaPos__cp10__call46 () Int)
(declare-fun LsCommaPos__cp9__call46 () Int)
(declare-fun LsCommaPos__cp8__call46 () Int)
(declare-fun LsCommaPos__cp7__call46 () Int)
(declare-fun LsCommaPos__cp6__call46 () Int)
(declare-fun LsCommaPos__cp5__call46 () Int)
(declare-fun LsCommaPos__cp4__call46 () Int)
(declare-fun LsCommaPos__cp3__call46 () Int)
(declare-fun LsCommaPos__cp2__call46 () Int)
(declare-fun LsCommaPos__cp1__call46 () Int)
(declare-fun LsCommaPos__cp0__call46 () Int)
(declare-fun LsCommaPos__cp15__call47 () Int)
(declare-fun LsCommaPos__cp14__call47 () Int)
(declare-fun LsCommaPos__cp13__call47 () Int)
(declare-fun LsCommaPos__cp12__call47 () Int)
(declare-fun LsCommaPos__cp11__call47 () Int)
(declare-fun LsCommaPos__cp10__call47 () Int)
(declare-fun LsCommaPos__cp9__call47 () Int)
(declare-fun LsCommaPos__cp8__call47 () Int)
(declare-fun LsCommaPos__cp7__call47 () Int)
(declare-fun LsCommaPos__cp6__call47 () Int)
(declare-fun LsCommaPos__cp5__call47 () Int)
(declare-fun LsCommaPos__cp4__call47 () Int)
(declare-fun LsCommaPos__cp3__call47 () Int)
(declare-fun LsCommaPos__cp2__call47 () Int)
(declare-fun LsCommaPos__cp1__call47 () Int)
(declare-fun LsCommaPos__cp0__call47 () Int)
(declare-fun LsCommaPos__cp15__call48 () Int)
(declare-fun LsCommaPos__cp14__call48 () Int)
(declare-fun LsCommaPos__cp13__call48 () Int)
(declare-fun LsCommaPos__cp12__call48 () Int)
(declare-fun LsCommaPos__cp11__call48 () Int)
(declare-fun LsCommaPos__cp10__call48 () Int)
(declare-fun LsCommaPos__cp9__call48 () Int)
(declare-fun LsCommaPos__cp8__call48 () Int)
(declare-fun LsCommaPos__cp7__call48 () Int)
(declare-fun LsCommaPos__cp6__call48 () Int)
(declare-fun LsCommaPos__cp5__call48 () Int)
(declare-fun LsCommaPos__cp4__call48 () Int)
(declare-fun LsCommaPos__cp3__call48 () Int)
(declare-fun LsCommaPos__cp2__call48 () Int)
(declare-fun LsCommaPos__cp1__call48 () Int)
(declare-fun LsCommaPos__cp0__call48 () Int)
(declare-fun LsCommaPos__cp15__call49 () Int)
(declare-fun LsCommaPos__cp14__call49 () Int)
(declare-fun LsCommaPos__cp13__call49 () Int)
(declare-fun LsCommaPos__cp12__call49 () Int)
(declare-fun LsCommaPos__cp11__call49 () Int)
(declare-fun LsCommaPos__cp10__call49 () Int)
(declare-fun LsCommaPos__cp9__call49 () Int)
(declare-fun LsCommaPos__cp8__call49 () Int)
(declare-fun LsCommaPos__cp7__call49 () Int)
(declare-fun LsCommaPos__cp6__call49 () Int)
(declare-fun LsCommaPos__cp5__call49 () Int)
(declare-fun LsCommaPos__cp4__call49 () Int)
(declare-fun LsCommaPos__cp3__call49 () Int)
(declare-fun LsCommaPos__cp2__call49 () Int)
(declare-fun LsCommaPos__cp1__call49 () Int)
(declare-fun LsCommaPos__cp0__call49 () Int)
(declare-fun LsCommaPos__cp15__call50 () Int)
(declare-fun LsCommaPos__cp14__call50 () Int)
(declare-fun LsCommaPos__cp13__call50 () Int)
(declare-fun LsCommaPos__cp12__call50 () Int)
(declare-fun LsCommaPos__cp11__call50 () Int)
(declare-fun LsCommaPos__cp10__call50 () Int)
(declare-fun LsCommaPos__cp9__call50 () Int)
(declare-fun LsCommaPos__cp8__call50 () Int)
(declare-fun LsCommaPos__cp7__call50 () Int)
(declare-fun LsCommaPos__cp6__call50 () Int)
(declare-fun LsCommaPos__cp5__call50 () Int)
(declare-fun LsCommaPos__cp4__call50 () Int)
(declare-fun LsCommaPos__cp3__call50 () Int)
(declare-fun LsCommaPos__cp2__call50 () Int)
(declare-fun LsCommaPos__cp1__call50 () Int)
(declare-fun LsCommaPos__cp0__call50 () Int)
(declare-fun eff_nop () Effect)
(declare-fun eff_out () Effect)
(declare-fun tk_read_go () Bool)
(declare-fun effects () (Array Int Effect))
(assert (>= effects__len 0))
(assert (>= last_results__len 0))
(assert (= tk_ph (ite is_first_tick 0 _phase)))
(assert (= tk_rline
   (ite ((_ is StringResult) (select last_results 1))
        (StringResult__f0 (select last_results 1))
        "")))
(assert (= tk_reof (ite ((_ is EofResult) (select last_results 1)) true false)))
(assert (= tk_reg_line (= tk_ph 2)))
(assert (= tk_src (and (= tk_ph 3) (not tk_reof))))
(assert (= tk_eof_now (and (= tk_ph 3) tk_reof)))
(assert (= tk_d0 (ite tk_src (str.indexof tk_rline "--" 0) (- 0 1))))
(assert (= tk_code
   (ite tk_src (ite (< tk_d0 0) tk_rline (str.substr tk_rline 0 tk_d0)) "")))
(assert (let ((a!1 (not (or (= (str.at tk_code 0) " ") (= (str.at tk_code 0) "\u{9}"))))
      (a!2 (or (= (str.at tk_code (+ 0 1)) " ")
               (= (str.at tk_code (+ 0 1)) "\u{9}")))
      (a!3 (or (= (str.at tk_code (+ 0 2)) " ")
               (= (str.at tk_code (+ 0 2)) "\u{9}")))
      (a!4 (or (= (str.at tk_code (+ 0 3)) " ")
               (= (str.at tk_code (+ 0 3)) "\u{9}")))
      (a!5 (or (= (str.at tk_code (+ 0 4)) " ")
               (= (str.at tk_code (+ 0 4)) "\u{9}")))
      (a!6 (or (= (str.at tk_code (+ 0 5)) " ")
               (= (str.at tk_code (+ 0 5)) "\u{9}")))
      (a!7 (or (= (str.at tk_code (+ 0 6)) " ")
               (= (str.at tk_code (+ 0 6)) "\u{9}")))
      (a!8 (or (= (str.at tk_code (+ 0 7)) " ")
               (= (str.at tk_code (+ 0 7)) "\u{9}")))
      (a!9 (or (= (str.at tk_code (+ 0 8)) " ")
               (= (str.at tk_code (+ 0 8)) "\u{9}")))
      (a!10 (or (= (str.at tk_code (+ 0 9)) " ")
                (= (str.at tk_code (+ 0 9)) "\u{9}")))
      (a!11 (or (= (str.at tk_code (+ 0 10)) " ")
                (= (str.at tk_code (+ 0 10)) "\u{9}")))
      (a!12 (or (= (str.at tk_code (+ 0 11)) " ")
                (= (str.at tk_code (+ 0 11)) "\u{9}")))
      (a!13 (or (= (str.at tk_code (+ 0 12)) " ")
                (= (str.at tk_code (+ 0 12)) "\u{9}")))
      (a!14 (or (= (str.at tk_code (+ 0 13)) " ")
                (= (str.at tk_code (+ 0 13)) "\u{9}")))
      (a!15 (or (= (str.at tk_code (+ 0 14)) " ")
                (= (str.at tk_code (+ 0 14)) "\u{9}")))
      (a!16 (or (= (str.at tk_code (+ 0 15)) " ")
                (= (str.at tk_code (+ 0 15)) "\u{9}")))
      (a!17 (or (= (str.at tk_code (+ 0 16)) " ")
                (= (str.at tk_code (+ 0 16)) "\u{9}")))
      (a!18 (or (= (str.at tk_code (+ 0 17)) " ")
                (= (str.at tk_code (+ 0 17)) "\u{9}")))
      (a!19 (or (= (str.at tk_code (+ 0 18)) " ")
                (= (str.at tk_code (+ 0 18)) "\u{9}")))
      (a!20 (or (= (str.at tk_code (+ 0 19)) " ")
                (= (str.at tk_code (+ 0 19)) "\u{9}")))
      (a!21 (or (= (str.at tk_code (+ 0 20)) " ")
                (= (str.at tk_code (+ 0 20)) "\u{9}")))
      (a!22 (or (= (str.at tk_code (+ 0 21)) " ")
                (= (str.at tk_code (+ 0 21)) "\u{9}")))
      (a!23 (or (= (str.at tk_code (+ 0 22)) " ")
                (= (str.at tk_code (+ 0 22)) "\u{9}")))
      (a!24 (or (= (str.at tk_code (+ 0 23)) " ")
                (= (str.at tk_code (+ 0 23)) "\u{9}")))
      (a!25 (or (= (str.at tk_code (+ 0 24)) " ")
                (= (str.at tk_code (+ 0 24)) "\u{9}")))
      (a!26 (or (= (str.at tk_code (+ 0 25)) " ")
                (= (str.at tk_code (+ 0 25)) "\u{9}")))
      (a!27 (or (= (str.at tk_code (+ 0 26)) " ")
                (= (str.at tk_code (+ 0 26)) "\u{9}")))
      (a!28 (or (= (str.at tk_code (+ 0 27)) " ")
                (= (str.at tk_code (+ 0 27)) "\u{9}")))
      (a!29 (or (= (str.at tk_code (+ 0 28)) " ")
                (= (str.at tk_code (+ 0 28)) "\u{9}")))
      (a!30 (or (= (str.at tk_code (+ 0 29)) " ")
                (= (str.at tk_code (+ 0 29)) "\u{9}")))
      (a!31 (or (= (str.at tk_code (+ 0 30)) " ")
                (= (str.at tk_code (+ 0 30)) "\u{9}")))
      (a!32 (or (= (str.at tk_code (+ 0 31)) " ")
                (= (str.at tk_code (+ 0 31)) "\u{9}")))
      (a!33 (or (= (str.at tk_code (+ 0 32)) " ")
                (= (str.at tk_code (+ 0 32)) "\u{9}")))
      (a!34 (or (= (str.at tk_code (+ 0 33)) " ")
                (= (str.at tk_code (+ 0 33)) "\u{9}")))
      (a!35 (or (= (str.at tk_code (+ 0 34)) " ")
                (= (str.at tk_code (+ 0 34)) "\u{9}")))
      (a!36 (or (= (str.at tk_code (+ 0 35)) " ")
                (= (str.at tk_code (+ 0 35)) "\u{9}")))
      (a!37 (or (= (str.at tk_code (+ 0 36)) " ")
                (= (str.at tk_code (+ 0 36)) "\u{9}")))
      (a!38 (or (= (str.at tk_code (+ 0 37)) " ")
                (= (str.at tk_code (+ 0 37)) "\u{9}")))
      (a!39 (or (= (str.at tk_code (+ 0 38)) " ")
                (= (str.at tk_code (+ 0 38)) "\u{9}")))
      (a!40 (or (= (str.at tk_code (+ 0 39)) " ")
                (= (str.at tk_code (+ 0 39)) "\u{9}")))
      (a!41 (or (= (str.at tk_code (+ 0 40)) " ")
                (= (str.at tk_code (+ 0 40)) "\u{9}")))
      (a!42 (or (= (str.at tk_code (+ 0 41)) " ")
                (= (str.at tk_code (+ 0 41)) "\u{9}")))
      (a!43 (or (= (str.at tk_code (+ 0 42)) " ")
                (= (str.at tk_code (+ 0 42)) "\u{9}")))
      (a!44 (or (= (str.at tk_code (+ 0 43)) " ")
                (= (str.at tk_code (+ 0 43)) "\u{9}")))
      (a!45 (or (= (str.at tk_code (+ 0 44)) " ")
                (= (str.at tk_code (+ 0 44)) "\u{9}")))
      (a!46 (or (= (str.at tk_code (+ 0 45)) " ")
                (= (str.at tk_code (+ 0 45)) "\u{9}")))
      (a!47 (or (= (str.at tk_code (+ 0 46)) " ")
                (= (str.at tk_code (+ 0 46)) "\u{9}")))
      (a!48 (or (= (str.at tk_code (+ 0 47)) " ")
                (= (str.at tk_code (+ 0 47)) "\u{9}")))
      (a!49 (or (= (str.at tk_code (+ 0 48)) " ")
                (= (str.at tk_code (+ 0 48)) "\u{9}")))
      (a!50 (or (= (str.at tk_code (+ 0 49)) " ")
                (= (str.at tk_code (+ 0 49)) "\u{9}")))
      (a!51 (or (= (str.at tk_code (+ 0 50)) " ")
                (= (str.at tk_code (+ 0 50)) "\u{9}")))
      (a!52 (or (= (str.at tk_code (+ 0 51)) " ")
                (= (str.at tk_code (+ 0 51)) "\u{9}")))
      (a!53 (or (= (str.at tk_code (+ 0 52)) " ")
                (= (str.at tk_code (+ 0 52)) "\u{9}")))
      (a!54 (or (= (str.at tk_code (+ 0 53)) " ")
                (= (str.at tk_code (+ 0 53)) "\u{9}")))
      (a!55 (or (= (str.at tk_code (+ 0 54)) " ")
                (= (str.at tk_code (+ 0 54)) "\u{9}")))
      (a!56 (or (= (str.at tk_code (+ 0 55)) " ")
                (= (str.at tk_code (+ 0 55)) "\u{9}")))
      (a!57 (or (= (str.at tk_code (+ 0 56)) " ")
                (= (str.at tk_code (+ 0 56)) "\u{9}")))
      (a!58 (or (= (str.at tk_code (+ 0 57)) " ")
                (= (str.at tk_code (+ 0 57)) "\u{9}")))
      (a!59 (or (= (str.at tk_code (+ 0 58)) " ")
                (= (str.at tk_code (+ 0 58)) "\u{9}")))
      (a!60 (or (= (str.at tk_code (+ 0 59)) " ")
                (= (str.at tk_code (+ 0 59)) "\u{9}")))
      (a!61 (or (= (str.at tk_code (+ 0 60)) " ")
                (= (str.at tk_code (+ 0 60)) "\u{9}")))
      (a!62 (or (= (str.at tk_code (+ 0 61)) " ")
                (= (str.at tk_code (+ 0 61)) "\u{9}")))
      (a!63 (or (= (str.at tk_code (+ 0 62)) " ")
                (= (str.at tk_code (+ 0 62)) "\u{9}")))
      (a!64 (or (= (str.at tk_code (+ 0 63)) " ")
                (= (str.at tk_code (+ 0 63)) "\u{9}"))))
(let ((a!65 (ite (not a!62)
                 (+ 0 61)
                 (ite (not a!63) (+ 0 62) (ite (not a!64) (+ 0 63) (+ 0 64))))))
(let ((a!66 (ite (not a!59)
                 (+ 0 58)
                 (ite (not a!60) (+ 0 59) (ite (not a!61) (+ 0 60) a!65)))))
(let ((a!67 (ite (not a!56)
                 (+ 0 55)
                 (ite (not a!57) (+ 0 56) (ite (not a!58) (+ 0 57) a!66)))))
(let ((a!68 (ite (not a!53)
                 (+ 0 52)
                 (ite (not a!54) (+ 0 53) (ite (not a!55) (+ 0 54) a!67)))))
(let ((a!69 (ite (not a!50)
                 (+ 0 49)
                 (ite (not a!51) (+ 0 50) (ite (not a!52) (+ 0 51) a!68)))))
(let ((a!70 (ite (not a!47)
                 (+ 0 46)
                 (ite (not a!48) (+ 0 47) (ite (not a!49) (+ 0 48) a!69)))))
(let ((a!71 (ite (not a!44)
                 (+ 0 43)
                 (ite (not a!45) (+ 0 44) (ite (not a!46) (+ 0 45) a!70)))))
(let ((a!72 (ite (not a!41)
                 (+ 0 40)
                 (ite (not a!42) (+ 0 41) (ite (not a!43) (+ 0 42) a!71)))))
(let ((a!73 (ite (not a!38)
                 (+ 0 37)
                 (ite (not a!39) (+ 0 38) (ite (not a!40) (+ 0 39) a!72)))))
(let ((a!74 (ite (not a!35)
                 (+ 0 34)
                 (ite (not a!36) (+ 0 35) (ite (not a!37) (+ 0 36) a!73)))))
(let ((a!75 (ite (not a!32)
                 (+ 0 31)
                 (ite (not a!33) (+ 0 32) (ite (not a!34) (+ 0 33) a!74)))))
(let ((a!76 (ite (not a!29)
                 (+ 0 28)
                 (ite (not a!30) (+ 0 29) (ite (not a!31) (+ 0 30) a!75)))))
(let ((a!77 (ite (not a!26)
                 (+ 0 25)
                 (ite (not a!27) (+ 0 26) (ite (not a!28) (+ 0 27) a!76)))))
(let ((a!78 (ite (not a!23)
                 (+ 0 22)
                 (ite (not a!24) (+ 0 23) (ite (not a!25) (+ 0 24) a!77)))))
(let ((a!79 (ite (not a!20)
                 (+ 0 19)
                 (ite (not a!21) (+ 0 20) (ite (not a!22) (+ 0 21) a!78)))))
(let ((a!80 (ite (not a!17)
                 (+ 0 16)
                 (ite (not a!18) (+ 0 17) (ite (not a!19) (+ 0 18) a!79)))))
(let ((a!81 (ite (not a!14)
                 (+ 0 13)
                 (ite (not a!15) (+ 0 14) (ite (not a!16) (+ 0 15) a!80)))))
(let ((a!82 (ite (not a!11)
                 (+ 0 10)
                 (ite (not a!12) (+ 0 11) (ite (not a!13) (+ 0 12) a!81)))))
(let ((a!83 (ite (not a!8)
                 (+ 0 7)
                 (ite (not a!9) (+ 0 8) (ite (not a!10) (+ 0 9) a!82)))))
(let ((a!84 (ite (not a!5)
                 (+ 0 4)
                 (ite (not a!6) (+ 0 5) (ite (not a!7) (+ 0 6) a!83)))))
(let ((a!85 (ite (not a!2)
                 (+ 0 1)
                 (ite (not a!3) (+ 0 2) (ite (not a!4) (+ 0 3) a!84)))))
  (= tk_ws (ite a!1 0 a!85)))))))))))))))))))))))))
(assert (= tk_ind (ite tk_src (str.substr tk_rline 0 tk_ws) "")))
(assert (let ((a!1 (not (and (< tk_ws (str.len tk_code))
                     (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                                   (str.at tk_code tk_ws)))))
      (a!2 (and (< (+ tk_ws 1) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 1)))))
      (a!3 (and (< (+ tk_ws 2) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 2)))))
      (a!4 (and (< (+ tk_ws 3) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 3)))))
      (a!5 (and (< (+ tk_ws 4) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 4)))))
      (a!6 (and (< (+ tk_ws 5) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 5)))))
      (a!7 (and (< (+ tk_ws 6) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 6)))))
      (a!8 (and (< (+ tk_ws 7) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 7)))))
      (a!9 (and (< (+ tk_ws 8) (str.len tk_code))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at tk_code (+ tk_ws 8)))))
      (a!10 (and (< (+ tk_ws 9) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 9)))))
      (a!11 (and (< (+ tk_ws 10) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 10)))))
      (a!12 (and (< (+ tk_ws 11) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 11)))))
      (a!13 (and (< (+ tk_ws 12) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 12)))))
      (a!14 (and (< (+ tk_ws 13) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 13)))))
      (a!15 (and (< (+ tk_ws 14) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 14)))))
      (a!16 (and (< (+ tk_ws 15) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 15)))))
      (a!17 (and (< (+ tk_ws 16) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 16)))))
      (a!18 (and (< (+ tk_ws 17) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 17)))))
      (a!19 (and (< (+ tk_ws 18) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 18)))))
      (a!20 (and (< (+ tk_ws 19) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 19)))))
      (a!21 (and (< (+ tk_ws 20) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 20)))))
      (a!22 (and (< (+ tk_ws 21) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 21)))))
      (a!23 (and (< (+ tk_ws 22) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 22)))))
      (a!24 (and (< (+ tk_ws 23) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 23)))))
      (a!25 (and (< (+ tk_ws 24) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 24)))))
      (a!26 (and (< (+ tk_ws 25) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 25)))))
      (a!27 (and (< (+ tk_ws 26) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 26)))))
      (a!28 (and (< (+ tk_ws 27) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 27)))))
      (a!29 (and (< (+ tk_ws 28) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 28)))))
      (a!30 (and (< (+ tk_ws 29) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 29)))))
      (a!31 (and (< (+ tk_ws 30) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 30)))))
      (a!32 (and (< (+ tk_ws 31) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 31)))))
      (a!33 (and (< (+ tk_ws 32) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 32)))))
      (a!34 (and (< (+ tk_ws 33) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 33)))))
      (a!35 (and (< (+ tk_ws 34) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 34)))))
      (a!36 (and (< (+ tk_ws 35) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 35)))))
      (a!37 (and (< (+ tk_ws 36) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 36)))))
      (a!38 (and (< (+ tk_ws 37) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 37)))))
      (a!39 (and (< (+ tk_ws 38) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 38)))))
      (a!40 (and (< (+ tk_ws 39) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 39)))))
      (a!41 (and (< (+ tk_ws 40) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 40)))))
      (a!42 (and (< (+ tk_ws 41) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 41)))))
      (a!43 (and (< (+ tk_ws 42) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 42)))))
      (a!44 (and (< (+ tk_ws 43) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 43)))))
      (a!45 (and (< (+ tk_ws 44) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 44)))))
      (a!46 (and (< (+ tk_ws 45) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 45)))))
      (a!47 (and (< (+ tk_ws 46) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 46)))))
      (a!48 (and (< (+ tk_ws 47) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 47)))))
      (a!49 (and (< (+ tk_ws 48) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 48)))))
      (a!50 (and (< (+ tk_ws 49) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 49)))))
      (a!51 (and (< (+ tk_ws 50) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 50)))))
      (a!52 (and (< (+ tk_ws 51) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 51)))))
      (a!53 (and (< (+ tk_ws 52) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 52)))))
      (a!54 (and (< (+ tk_ws 53) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 53)))))
      (a!55 (and (< (+ tk_ws 54) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 54)))))
      (a!56 (and (< (+ tk_ws 55) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 55)))))
      (a!57 (and (< (+ tk_ws 56) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 56)))))
      (a!58 (and (< (+ tk_ws 57) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 57)))))
      (a!59 (and (< (+ tk_ws 58) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 58)))))
      (a!60 (and (< (+ tk_ws 59) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 59)))))
      (a!61 (and (< (+ tk_ws 60) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 60)))))
      (a!62 (and (< (+ tk_ws 61) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 61)))))
      (a!63 (and (< (+ tk_ws 62) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 62)))))
      (a!64 (and (< (+ tk_ws 63) (str.len tk_code))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at tk_code (+ tk_ws 63))))))
(let ((a!65 (ite (not a!62)
                 (+ tk_ws 61)
                 (ite (not a!63)
                      (+ tk_ws 62)
                      (ite (not a!64) (+ tk_ws 63) (+ tk_ws 64))))))
(let ((a!66 (ite (not a!59)
                 (+ tk_ws 58)
                 (ite (not a!60)
                      (+ tk_ws 59)
                      (ite (not a!61) (+ tk_ws 60) a!65)))))
(let ((a!67 (ite (not a!56)
                 (+ tk_ws 55)
                 (ite (not a!57)
                      (+ tk_ws 56)
                      (ite (not a!58) (+ tk_ws 57) a!66)))))
(let ((a!68 (ite (not a!53)
                 (+ tk_ws 52)
                 (ite (not a!54)
                      (+ tk_ws 53)
                      (ite (not a!55) (+ tk_ws 54) a!67)))))
(let ((a!69 (ite (not a!50)
                 (+ tk_ws 49)
                 (ite (not a!51)
                      (+ tk_ws 50)
                      (ite (not a!52) (+ tk_ws 51) a!68)))))
(let ((a!70 (ite (not a!47)
                 (+ tk_ws 46)
                 (ite (not a!48)
                      (+ tk_ws 47)
                      (ite (not a!49) (+ tk_ws 48) a!69)))))
(let ((a!71 (ite (not a!44)
                 (+ tk_ws 43)
                 (ite (not a!45)
                      (+ tk_ws 44)
                      (ite (not a!46) (+ tk_ws 45) a!70)))))
(let ((a!72 (ite (not a!41)
                 (+ tk_ws 40)
                 (ite (not a!42)
                      (+ tk_ws 41)
                      (ite (not a!43) (+ tk_ws 42) a!71)))))
(let ((a!73 (ite (not a!38)
                 (+ tk_ws 37)
                 (ite (not a!39)
                      (+ tk_ws 38)
                      (ite (not a!40) (+ tk_ws 39) a!72)))))
(let ((a!74 (ite (not a!35)
                 (+ tk_ws 34)
                 (ite (not a!36)
                      (+ tk_ws 35)
                      (ite (not a!37) (+ tk_ws 36) a!73)))))
(let ((a!75 (ite (not a!32)
                 (+ tk_ws 31)
                 (ite (not a!33)
                      (+ tk_ws 32)
                      (ite (not a!34) (+ tk_ws 33) a!74)))))
(let ((a!76 (ite (not a!29)
                 (+ tk_ws 28)
                 (ite (not a!30)
                      (+ tk_ws 29)
                      (ite (not a!31) (+ tk_ws 30) a!75)))))
(let ((a!77 (ite (not a!26)
                 (+ tk_ws 25)
                 (ite (not a!27)
                      (+ tk_ws 26)
                      (ite (not a!28) (+ tk_ws 27) a!76)))))
(let ((a!78 (ite (not a!23)
                 (+ tk_ws 22)
                 (ite (not a!24)
                      (+ tk_ws 23)
                      (ite (not a!25) (+ tk_ws 24) a!77)))))
(let ((a!79 (ite (not a!20)
                 (+ tk_ws 19)
                 (ite (not a!21)
                      (+ tk_ws 20)
                      (ite (not a!22) (+ tk_ws 21) a!78)))))
(let ((a!80 (ite (not a!17)
                 (+ tk_ws 16)
                 (ite (not a!18)
                      (+ tk_ws 17)
                      (ite (not a!19) (+ tk_ws 18) a!79)))))
(let ((a!81 (ite (not a!14)
                 (+ tk_ws 13)
                 (ite (not a!15)
                      (+ tk_ws 14)
                      (ite (not a!16) (+ tk_ws 15) a!80)))))
(let ((a!82 (ite (not a!11)
                 (+ tk_ws 10)
                 (ite (not a!12)
                      (+ tk_ws 11)
                      (ite (not a!13) (+ tk_ws 12) a!81)))))
(let ((a!83 (ite (not a!8)
                 (+ tk_ws 7)
                 (ite (not a!9) (+ tk_ws 8) (ite (not a!10) (+ tk_ws 9) a!82)))))
(let ((a!84 (ite (not a!5)
                 (+ tk_ws 4)
                 (ite (not a!6) (+ tk_ws 5) (ite (not a!7) (+ tk_ws 6) a!83)))))
(let ((a!85 (ite (not a!2)
                 (+ tk_ws 1)
                 (ite (not a!3) (+ tk_ws 2) (ite (not a!4) (+ tk_ws 3) a!84)))))
  (= tk_ie (ite a!1 tk_ws a!85)))))))))))))))))))))))))
(assert (= tk_lead (ite (> tk_ie tk_ws) (str.substr tk_code tk_ws (- tk_ie tk_ws)) "")))
(assert (= tk_is_top
   (and (>= (str.len tk_rline) 1)
        (str.contains "cftse" (str.at tk_rline 0))
        (or (str.prefixof "claim " tk_rline)
            (str.prefixof "claim\u{9}" tk_rline)
            (str.prefixof "fsm " tk_rline)
            (str.prefixof "fsm\u{9}" tk_rline)
            (str.prefixof "type " tk_rline)
            (str.prefixof "type\u{9}" tk_rline)
            (str.prefixof "schema " tk_rline)
            (str.prefixof "schema\u{9}" tk_rline)
            (str.prefixof "enum " tk_rline)
            (str.prefixof "enum\u{9}" tk_rline)))))
(assert (let ((a!1 (ite (and (not (= tk_lead "")) (= (str.at tk_lead 0) "_"))
                (str.substr tk_lead 1 (- (str.len tk_lead) 1))
                tk_lead)))
  (= tk_lead_base a!1)))
(assert (= tk_lead_is_dual (and (not (= tk_lead "")) (= (str.at tk_lead 0) "_"))))
(assert (= tk_key (str.++ "\u{27e6}" tk_lead_base "\u{27e7}")))
(assert (let ((a!1 (ite (and tk_src (not (= tk_lead_base "")))
                (str.indexof _reg tk_key 0)
                (- 0 1))))
  (= tk_at a!1)))
(assert (= tk_reg_hit (>= tk_at 0)))
(assert (= tk_vs (ite tk_reg_hit (+ tk_at (str.len tk_key)) (- 0 1))))
(assert (= tk_d1 (ite tk_reg_hit (str.indexof _reg "\u{2982}" tk_vs) (- 0 1))))
(assert (= tk_rbase (ite tk_reg_hit (str.substr _reg tk_vs (- tk_d1 tk_vs)) "")))
(assert (= tk_d2 (ite tk_reg_hit (str.indexof _reg "\u{2982}" (+ tk_d1 1)) (- 0 1))))
(assert (let ((a!1 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 1))
      (a!2 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 0)) 0))
      (a!3 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 2))
      (a!4 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 1)) 0))
      (a!5 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 3))
      (a!6 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 2)) 0))
      (a!7 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 4))
      (a!8 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 3)) 0))
      (a!9 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 5))
      (a!10 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 4)) 0))
      (a!11 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 6))
      (a!12 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 5)) 0))
      (a!13 (= (ite tk_reg_hit (- (- tk_d2 tk_d1) 1) 0) 7))
      (a!14 (str.indexof "0123456789" (str.at _reg (+ (+ tk_d1 1) 6)) 0)))
(let ((a!15 (ite a!11
                 (+ (* a!2 100000)
                    (* a!4 10000)
                    (* a!6 1000)
                    (* a!8 100)
                    (* a!10 10)
                    a!12)
                 (ite a!13
                      (+ (* a!2 1000000)
                         (* a!4 100000)
                         (* a!6 10000)
                         (* a!8 1000)
                         (* a!10 100)
                         (* a!12 10)
                         a!14)
                      (- 0 1)))))
(let ((a!16 (ite a!7
                 (+ (* a!2 1000) (* a!4 100) (* a!6 10) a!8)
                 (ite a!9
                      (+ (* a!2 10000) (* a!4 1000) (* a!6 100) (* a!8 10) a!10)
                      a!15))))
(let ((a!17 (ite a!3
                 (+ (* a!2 10) a!4)
                 (ite a!5 (+ (* a!2 100) (* a!4 10) a!6) a!16))))
  (= tk_rn (ite a!1 a!2 a!17)))))))
(assert (let ((a!1 (and tk_reg_hit (= (str.at _reg (+ tk_d2 1)) "1"))))
  (= tk_rhaslen a!1)))
(assert (let ((a!1 (not (or (= (str.at tk_code tk_ie) " ")
                    (= (str.at tk_code tk_ie) "\u{9}"))))
      (a!2 (or (= (str.at tk_code (+ tk_ie 1)) " ")
               (= (str.at tk_code (+ tk_ie 1)) "\u{9}")))
      (a!3 (or (= (str.at tk_code (+ tk_ie 2)) " ")
               (= (str.at tk_code (+ tk_ie 2)) "\u{9}")))
      (a!4 (or (= (str.at tk_code (+ tk_ie 3)) " ")
               (= (str.at tk_code (+ tk_ie 3)) "\u{9}")))
      (a!5 (or (= (str.at tk_code (+ tk_ie 4)) " ")
               (= (str.at tk_code (+ tk_ie 4)) "\u{9}")))
      (a!6 (or (= (str.at tk_code (+ tk_ie 5)) " ")
               (= (str.at tk_code (+ tk_ie 5)) "\u{9}")))
      (a!7 (or (= (str.at tk_code (+ tk_ie 6)) " ")
               (= (str.at tk_code (+ tk_ie 6)) "\u{9}")))
      (a!8 (or (= (str.at tk_code (+ tk_ie 7)) " ")
               (= (str.at tk_code (+ tk_ie 7)) "\u{9}")))
      (a!9 (or (= (str.at tk_code (+ tk_ie 8)) " ")
               (= (str.at tk_code (+ tk_ie 8)) "\u{9}")))
      (a!10 (or (= (str.at tk_code (+ tk_ie 9)) " ")
                (= (str.at tk_code (+ tk_ie 9)) "\u{9}")))
      (a!11 (or (= (str.at tk_code (+ tk_ie 10)) " ")
                (= (str.at tk_code (+ tk_ie 10)) "\u{9}")))
      (a!12 (or (= (str.at tk_code (+ tk_ie 11)) " ")
                (= (str.at tk_code (+ tk_ie 11)) "\u{9}")))
      (a!13 (or (= (str.at tk_code (+ tk_ie 12)) " ")
                (= (str.at tk_code (+ tk_ie 12)) "\u{9}")))
      (a!14 (or (= (str.at tk_code (+ tk_ie 13)) " ")
                (= (str.at tk_code (+ tk_ie 13)) "\u{9}")))
      (a!15 (or (= (str.at tk_code (+ tk_ie 14)) " ")
                (= (str.at tk_code (+ tk_ie 14)) "\u{9}")))
      (a!16 (or (= (str.at tk_code (+ tk_ie 15)) " ")
                (= (str.at tk_code (+ tk_ie 15)) "\u{9}")))
      (a!17 (or (= (str.at tk_code (+ tk_ie 16)) " ")
                (= (str.at tk_code (+ tk_ie 16)) "\u{9}")))
      (a!18 (or (= (str.at tk_code (+ tk_ie 17)) " ")
                (= (str.at tk_code (+ tk_ie 17)) "\u{9}")))
      (a!19 (or (= (str.at tk_code (+ tk_ie 18)) " ")
                (= (str.at tk_code (+ tk_ie 18)) "\u{9}")))
      (a!20 (or (= (str.at tk_code (+ tk_ie 19)) " ")
                (= (str.at tk_code (+ tk_ie 19)) "\u{9}")))
      (a!21 (or (= (str.at tk_code (+ tk_ie 20)) " ")
                (= (str.at tk_code (+ tk_ie 20)) "\u{9}")))
      (a!22 (or (= (str.at tk_code (+ tk_ie 21)) " ")
                (= (str.at tk_code (+ tk_ie 21)) "\u{9}")))
      (a!23 (or (= (str.at tk_code (+ tk_ie 22)) " ")
                (= (str.at tk_code (+ tk_ie 22)) "\u{9}")))
      (a!24 (or (= (str.at tk_code (+ tk_ie 23)) " ")
                (= (str.at tk_code (+ tk_ie 23)) "\u{9}")))
      (a!25 (or (= (str.at tk_code (+ tk_ie 24)) " ")
                (= (str.at tk_code (+ tk_ie 24)) "\u{9}")))
      (a!26 (or (= (str.at tk_code (+ tk_ie 25)) " ")
                (= (str.at tk_code (+ tk_ie 25)) "\u{9}")))
      (a!27 (or (= (str.at tk_code (+ tk_ie 26)) " ")
                (= (str.at tk_code (+ tk_ie 26)) "\u{9}")))
      (a!28 (or (= (str.at tk_code (+ tk_ie 27)) " ")
                (= (str.at tk_code (+ tk_ie 27)) "\u{9}")))
      (a!29 (or (= (str.at tk_code (+ tk_ie 28)) " ")
                (= (str.at tk_code (+ tk_ie 28)) "\u{9}")))
      (a!30 (or (= (str.at tk_code (+ tk_ie 29)) " ")
                (= (str.at tk_code (+ tk_ie 29)) "\u{9}")))
      (a!31 (or (= (str.at tk_code (+ tk_ie 30)) " ")
                (= (str.at tk_code (+ tk_ie 30)) "\u{9}")))
      (a!32 (or (= (str.at tk_code (+ tk_ie 31)) " ")
                (= (str.at tk_code (+ tk_ie 31)) "\u{9}")))
      (a!33 (or (= (str.at tk_code (+ tk_ie 32)) " ")
                (= (str.at tk_code (+ tk_ie 32)) "\u{9}")))
      (a!34 (or (= (str.at tk_code (+ tk_ie 33)) " ")
                (= (str.at tk_code (+ tk_ie 33)) "\u{9}")))
      (a!35 (or (= (str.at tk_code (+ tk_ie 34)) " ")
                (= (str.at tk_code (+ tk_ie 34)) "\u{9}")))
      (a!36 (or (= (str.at tk_code (+ tk_ie 35)) " ")
                (= (str.at tk_code (+ tk_ie 35)) "\u{9}")))
      (a!37 (or (= (str.at tk_code (+ tk_ie 36)) " ")
                (= (str.at tk_code (+ tk_ie 36)) "\u{9}")))
      (a!38 (or (= (str.at tk_code (+ tk_ie 37)) " ")
                (= (str.at tk_code (+ tk_ie 37)) "\u{9}")))
      (a!39 (or (= (str.at tk_code (+ tk_ie 38)) " ")
                (= (str.at tk_code (+ tk_ie 38)) "\u{9}")))
      (a!40 (or (= (str.at tk_code (+ tk_ie 39)) " ")
                (= (str.at tk_code (+ tk_ie 39)) "\u{9}")))
      (a!41 (or (= (str.at tk_code (+ tk_ie 40)) " ")
                (= (str.at tk_code (+ tk_ie 40)) "\u{9}")))
      (a!42 (or (= (str.at tk_code (+ tk_ie 41)) " ")
                (= (str.at tk_code (+ tk_ie 41)) "\u{9}")))
      (a!43 (or (= (str.at tk_code (+ tk_ie 42)) " ")
                (= (str.at tk_code (+ tk_ie 42)) "\u{9}")))
      (a!44 (or (= (str.at tk_code (+ tk_ie 43)) " ")
                (= (str.at tk_code (+ tk_ie 43)) "\u{9}")))
      (a!45 (or (= (str.at tk_code (+ tk_ie 44)) " ")
                (= (str.at tk_code (+ tk_ie 44)) "\u{9}")))
      (a!46 (or (= (str.at tk_code (+ tk_ie 45)) " ")
                (= (str.at tk_code (+ tk_ie 45)) "\u{9}")))
      (a!47 (or (= (str.at tk_code (+ tk_ie 46)) " ")
                (= (str.at tk_code (+ tk_ie 46)) "\u{9}")))
      (a!48 (or (= (str.at tk_code (+ tk_ie 47)) " ")
                (= (str.at tk_code (+ tk_ie 47)) "\u{9}")))
      (a!49 (or (= (str.at tk_code (+ tk_ie 48)) " ")
                (= (str.at tk_code (+ tk_ie 48)) "\u{9}")))
      (a!50 (or (= (str.at tk_code (+ tk_ie 49)) " ")
                (= (str.at tk_code (+ tk_ie 49)) "\u{9}")))
      (a!51 (or (= (str.at tk_code (+ tk_ie 50)) " ")
                (= (str.at tk_code (+ tk_ie 50)) "\u{9}")))
      (a!52 (or (= (str.at tk_code (+ tk_ie 51)) " ")
                (= (str.at tk_code (+ tk_ie 51)) "\u{9}")))
      (a!53 (or (= (str.at tk_code (+ tk_ie 52)) " ")
                (= (str.at tk_code (+ tk_ie 52)) "\u{9}")))
      (a!54 (or (= (str.at tk_code (+ tk_ie 53)) " ")
                (= (str.at tk_code (+ tk_ie 53)) "\u{9}")))
      (a!55 (or (= (str.at tk_code (+ tk_ie 54)) " ")
                (= (str.at tk_code (+ tk_ie 54)) "\u{9}")))
      (a!56 (or (= (str.at tk_code (+ tk_ie 55)) " ")
                (= (str.at tk_code (+ tk_ie 55)) "\u{9}")))
      (a!57 (or (= (str.at tk_code (+ tk_ie 56)) " ")
                (= (str.at tk_code (+ tk_ie 56)) "\u{9}")))
      (a!58 (or (= (str.at tk_code (+ tk_ie 57)) " ")
                (= (str.at tk_code (+ tk_ie 57)) "\u{9}")))
      (a!59 (or (= (str.at tk_code (+ tk_ie 58)) " ")
                (= (str.at tk_code (+ tk_ie 58)) "\u{9}")))
      (a!60 (or (= (str.at tk_code (+ tk_ie 59)) " ")
                (= (str.at tk_code (+ tk_ie 59)) "\u{9}")))
      (a!61 (or (= (str.at tk_code (+ tk_ie 60)) " ")
                (= (str.at tk_code (+ tk_ie 60)) "\u{9}")))
      (a!62 (or (= (str.at tk_code (+ tk_ie 61)) " ")
                (= (str.at tk_code (+ tk_ie 61)) "\u{9}")))
      (a!63 (or (= (str.at tk_code (+ tk_ie 62)) " ")
                (= (str.at tk_code (+ tk_ie 62)) "\u{9}")))
      (a!64 (or (= (str.at tk_code (+ tk_ie 63)) " ")
                (= (str.at tk_code (+ tk_ie 63)) "\u{9}"))))
(let ((a!65 (ite (not a!62)
                 (+ tk_ie 61)
                 (ite (not a!63)
                      (+ tk_ie 62)
                      (ite (not a!64) (+ tk_ie 63) (+ tk_ie 64))))))
(let ((a!66 (ite (not a!59)
                 (+ tk_ie 58)
                 (ite (not a!60)
                      (+ tk_ie 59)
                      (ite (not a!61) (+ tk_ie 60) a!65)))))
(let ((a!67 (ite (not a!56)
                 (+ tk_ie 55)
                 (ite (not a!57)
                      (+ tk_ie 56)
                      (ite (not a!58) (+ tk_ie 57) a!66)))))
(let ((a!68 (ite (not a!53)
                 (+ tk_ie 52)
                 (ite (not a!54)
                      (+ tk_ie 53)
                      (ite (not a!55) (+ tk_ie 54) a!67)))))
(let ((a!69 (ite (not a!50)
                 (+ tk_ie 49)
                 (ite (not a!51)
                      (+ tk_ie 50)
                      (ite (not a!52) (+ tk_ie 51) a!68)))))
(let ((a!70 (ite (not a!47)
                 (+ tk_ie 46)
                 (ite (not a!48)
                      (+ tk_ie 47)
                      (ite (not a!49) (+ tk_ie 48) a!69)))))
(let ((a!71 (ite (not a!44)
                 (+ tk_ie 43)
                 (ite (not a!45)
                      (+ tk_ie 44)
                      (ite (not a!46) (+ tk_ie 45) a!70)))))
(let ((a!72 (ite (not a!41)
                 (+ tk_ie 40)
                 (ite (not a!42)
                      (+ tk_ie 41)
                      (ite (not a!43) (+ tk_ie 42) a!71)))))
(let ((a!73 (ite (not a!38)
                 (+ tk_ie 37)
                 (ite (not a!39)
                      (+ tk_ie 38)
                      (ite (not a!40) (+ tk_ie 39) a!72)))))
(let ((a!74 (ite (not a!35)
                 (+ tk_ie 34)
                 (ite (not a!36)
                      (+ tk_ie 35)
                      (ite (not a!37) (+ tk_ie 36) a!73)))))
(let ((a!75 (ite (not a!32)
                 (+ tk_ie 31)
                 (ite (not a!33)
                      (+ tk_ie 32)
                      (ite (not a!34) (+ tk_ie 33) a!74)))))
(let ((a!76 (ite (not a!29)
                 (+ tk_ie 28)
                 (ite (not a!30)
                      (+ tk_ie 29)
                      (ite (not a!31) (+ tk_ie 30) a!75)))))
(let ((a!77 (ite (not a!26)
                 (+ tk_ie 25)
                 (ite (not a!27)
                      (+ tk_ie 26)
                      (ite (not a!28) (+ tk_ie 27) a!76)))))
(let ((a!78 (ite (not a!23)
                 (+ tk_ie 22)
                 (ite (not a!24)
                      (+ tk_ie 23)
                      (ite (not a!25) (+ tk_ie 24) a!77)))))
(let ((a!79 (ite (not a!20)
                 (+ tk_ie 19)
                 (ite (not a!21)
                      (+ tk_ie 20)
                      (ite (not a!22) (+ tk_ie 21) a!78)))))
(let ((a!80 (ite (not a!17)
                 (+ tk_ie 16)
                 (ite (not a!18)
                      (+ tk_ie 17)
                      (ite (not a!19) (+ tk_ie 18) a!79)))))
(let ((a!81 (ite (not a!14)
                 (+ tk_ie 13)
                 (ite (not a!15)
                      (+ tk_ie 14)
                      (ite (not a!16) (+ tk_ie 15) a!80)))))
(let ((a!82 (ite (not a!11)
                 (+ tk_ie 10)
                 (ite (not a!12)
                      (+ tk_ie 11)
                      (ite (not a!13) (+ tk_ie 12) a!81)))))
(let ((a!83 (ite (not a!8)
                 (+ tk_ie 7)
                 (ite (not a!9) (+ tk_ie 8) (ite (not a!10) (+ tk_ie 9) a!82)))))
(let ((a!84 (ite (not a!5)
                 (+ tk_ie 4)
                 (ite (not a!6) (+ tk_ie 5) (ite (not a!7) (+ tk_ie 6) a!83)))))
(let ((a!85 (ite (not a!2)
                 (+ tk_ie 1)
                 (ite (not a!3) (+ tk_ie 2) (ite (not a!4) (+ tk_ie 3) a!84)))))
  (= tk_after_lead (ite a!1 tk_ie a!85)))))))))))))))))))))))))
(assert (= tk_glyph (ite tk_reg_hit (str.at tk_code tk_after_lead) "")))
(assert (let ((a!1 (str.contains (str.substr tk_code
                                     tk_after_lead
                                     (- (str.len tk_code) tk_after_lead))
                         "Seq(")))
  (= tk_is_decl (and tk_reg_hit (= tk_glyph "\u{2208}") a!1))))
(assert (= tk_is_assign (and tk_reg_hit (= tk_glyph "="))))
(assert (= tk_decl_eq (ite tk_is_decl (str.indexof tk_code "=" 0) (- 0 1))))
(assert (= tk_decl_lit
   (and tk_is_decl (>= tk_decl_eq 0) (str.contains tk_code "\u{27e8}"))))
(assert (let ((a!1 (or (= (str.at tk_code (+ tk_after_lead 1)) " ")
               (= (str.at tk_code (+ tk_after_lead 1)) "\u{9}")))
      (a!2 (= (str.at tk_code (+ (+ tk_after_lead 1) 1)) " "))
      (a!3 (= (str.at tk_code (+ (+ tk_after_lead 1) 1)) "\u{9}"))
      (a!4 (= (str.at tk_code (+ (+ tk_after_lead 1) 2)) " "))
      (a!5 (= (str.at tk_code (+ (+ tk_after_lead 1) 2)) "\u{9}"))
      (a!6 (= (str.at tk_code (+ (+ tk_after_lead 1) 3)) " "))
      (a!7 (= (str.at tk_code (+ (+ tk_after_lead 1) 3)) "\u{9}"))
      (a!8 (= (str.at tk_code (+ (+ tk_after_lead 1) 4)) " "))
      (a!9 (= (str.at tk_code (+ (+ tk_after_lead 1) 4)) "\u{9}"))
      (a!10 (= (str.at tk_code (+ (+ tk_after_lead 1) 5)) " "))
      (a!11 (= (str.at tk_code (+ (+ tk_after_lead 1) 5)) "\u{9}"))
      (a!12 (= (str.at tk_code (+ (+ tk_after_lead 1) 6)) " "))
      (a!13 (= (str.at tk_code (+ (+ tk_after_lead 1) 6)) "\u{9}"))
      (a!14 (= (str.at tk_code (+ (+ tk_after_lead 1) 7)) " "))
      (a!15 (= (str.at tk_code (+ (+ tk_after_lead 1) 7)) "\u{9}"))
      (a!16 (= (str.at tk_code (+ (+ tk_after_lead 1) 8)) " "))
      (a!17 (= (str.at tk_code (+ (+ tk_after_lead 1) 8)) "\u{9}"))
      (a!18 (= (str.at tk_code (+ (+ tk_after_lead 1) 9)) " "))
      (a!19 (= (str.at tk_code (+ (+ tk_after_lead 1) 9)) "\u{9}"))
      (a!20 (= (str.at tk_code (+ (+ tk_after_lead 1) 10)) " "))
      (a!21 (= (str.at tk_code (+ (+ tk_after_lead 1) 10)) "\u{9}"))
      (a!22 (= (str.at tk_code (+ (+ tk_after_lead 1) 11)) " "))
      (a!23 (= (str.at tk_code (+ (+ tk_after_lead 1) 11)) "\u{9}"))
      (a!24 (= (str.at tk_code (+ (+ tk_after_lead 1) 12)) " "))
      (a!25 (= (str.at tk_code (+ (+ tk_after_lead 1) 12)) "\u{9}"))
      (a!26 (= (str.at tk_code (+ (+ tk_after_lead 1) 13)) " "))
      (a!27 (= (str.at tk_code (+ (+ tk_after_lead 1) 13)) "\u{9}"))
      (a!28 (= (str.at tk_code (+ (+ tk_after_lead 1) 14)) " "))
      (a!29 (= (str.at tk_code (+ (+ tk_after_lead 1) 14)) "\u{9}"))
      (a!30 (= (str.at tk_code (+ (+ tk_after_lead 1) 15)) " "))
      (a!31 (= (str.at tk_code (+ (+ tk_after_lead 1) 15)) "\u{9}"))
      (a!32 (= (str.at tk_code (+ (+ tk_after_lead 1) 16)) " "))
      (a!33 (= (str.at tk_code (+ (+ tk_after_lead 1) 16)) "\u{9}"))
      (a!34 (= (str.at tk_code (+ (+ tk_after_lead 1) 17)) " "))
      (a!35 (= (str.at tk_code (+ (+ tk_after_lead 1) 17)) "\u{9}"))
      (a!36 (= (str.at tk_code (+ (+ tk_after_lead 1) 18)) " "))
      (a!37 (= (str.at tk_code (+ (+ tk_after_lead 1) 18)) "\u{9}"))
      (a!38 (= (str.at tk_code (+ (+ tk_after_lead 1) 19)) " "))
      (a!39 (= (str.at tk_code (+ (+ tk_after_lead 1) 19)) "\u{9}"))
      (a!40 (= (str.at tk_code (+ (+ tk_after_lead 1) 20)) " "))
      (a!41 (= (str.at tk_code (+ (+ tk_after_lead 1) 20)) "\u{9}"))
      (a!42 (= (str.at tk_code (+ (+ tk_after_lead 1) 21)) " "))
      (a!43 (= (str.at tk_code (+ (+ tk_after_lead 1) 21)) "\u{9}"))
      (a!44 (= (str.at tk_code (+ (+ tk_after_lead 1) 22)) " "))
      (a!45 (= (str.at tk_code (+ (+ tk_after_lead 1) 22)) "\u{9}"))
      (a!46 (= (str.at tk_code (+ (+ tk_after_lead 1) 23)) " "))
      (a!47 (= (str.at tk_code (+ (+ tk_after_lead 1) 23)) "\u{9}"))
      (a!48 (= (str.at tk_code (+ (+ tk_after_lead 1) 24)) " "))
      (a!49 (= (str.at tk_code (+ (+ tk_after_lead 1) 24)) "\u{9}"))
      (a!50 (= (str.at tk_code (+ (+ tk_after_lead 1) 25)) " "))
      (a!51 (= (str.at tk_code (+ (+ tk_after_lead 1) 25)) "\u{9}"))
      (a!52 (= (str.at tk_code (+ (+ tk_after_lead 1) 26)) " "))
      (a!53 (= (str.at tk_code (+ (+ tk_after_lead 1) 26)) "\u{9}"))
      (a!54 (= (str.at tk_code (+ (+ tk_after_lead 1) 27)) " "))
      (a!55 (= (str.at tk_code (+ (+ tk_after_lead 1) 27)) "\u{9}"))
      (a!56 (= (str.at tk_code (+ (+ tk_after_lead 1) 28)) " "))
      (a!57 (= (str.at tk_code (+ (+ tk_after_lead 1) 28)) "\u{9}"))
      (a!58 (= (str.at tk_code (+ (+ tk_after_lead 1) 29)) " "))
      (a!59 (= (str.at tk_code (+ (+ tk_after_lead 1) 29)) "\u{9}"))
      (a!60 (= (str.at tk_code (+ (+ tk_after_lead 1) 30)) " "))
      (a!61 (= (str.at tk_code (+ (+ tk_after_lead 1) 30)) "\u{9}"))
      (a!62 (= (str.at tk_code (+ (+ tk_after_lead 1) 31)) " "))
      (a!63 (= (str.at tk_code (+ (+ tk_after_lead 1) 31)) "\u{9}"))
      (a!64 (= (str.at tk_code (+ (+ tk_after_lead 1) 32)) " "))
      (a!65 (= (str.at tk_code (+ (+ tk_after_lead 1) 32)) "\u{9}"))
      (a!66 (= (str.at tk_code (+ (+ tk_after_lead 1) 33)) " "))
      (a!67 (= (str.at tk_code (+ (+ tk_after_lead 1) 33)) "\u{9}"))
      (a!68 (= (str.at tk_code (+ (+ tk_after_lead 1) 34)) " "))
      (a!69 (= (str.at tk_code (+ (+ tk_after_lead 1) 34)) "\u{9}"))
      (a!70 (= (str.at tk_code (+ (+ tk_after_lead 1) 35)) " "))
      (a!71 (= (str.at tk_code (+ (+ tk_after_lead 1) 35)) "\u{9}"))
      (a!72 (= (str.at tk_code (+ (+ tk_after_lead 1) 36)) " "))
      (a!73 (= (str.at tk_code (+ (+ tk_after_lead 1) 36)) "\u{9}"))
      (a!74 (= (str.at tk_code (+ (+ tk_after_lead 1) 37)) " "))
      (a!75 (= (str.at tk_code (+ (+ tk_after_lead 1) 37)) "\u{9}"))
      (a!76 (= (str.at tk_code (+ (+ tk_after_lead 1) 38)) " "))
      (a!77 (= (str.at tk_code (+ (+ tk_after_lead 1) 38)) "\u{9}"))
      (a!78 (= (str.at tk_code (+ (+ tk_after_lead 1) 39)) " "))
      (a!79 (= (str.at tk_code (+ (+ tk_after_lead 1) 39)) "\u{9}"))
      (a!80 (= (str.at tk_code (+ (+ tk_after_lead 1) 40)) " "))
      (a!81 (= (str.at tk_code (+ (+ tk_after_lead 1) 40)) "\u{9}"))
      (a!82 (= (str.at tk_code (+ (+ tk_after_lead 1) 41)) " "))
      (a!83 (= (str.at tk_code (+ (+ tk_after_lead 1) 41)) "\u{9}"))
      (a!84 (= (str.at tk_code (+ (+ tk_after_lead 1) 42)) " "))
      (a!85 (= (str.at tk_code (+ (+ tk_after_lead 1) 42)) "\u{9}"))
      (a!86 (= (str.at tk_code (+ (+ tk_after_lead 1) 43)) " "))
      (a!87 (= (str.at tk_code (+ (+ tk_after_lead 1) 43)) "\u{9}"))
      (a!88 (= (str.at tk_code (+ (+ tk_after_lead 1) 44)) " "))
      (a!89 (= (str.at tk_code (+ (+ tk_after_lead 1) 44)) "\u{9}"))
      (a!90 (= (str.at tk_code (+ (+ tk_after_lead 1) 45)) " "))
      (a!91 (= (str.at tk_code (+ (+ tk_after_lead 1) 45)) "\u{9}"))
      (a!92 (= (str.at tk_code (+ (+ tk_after_lead 1) 46)) " "))
      (a!93 (= (str.at tk_code (+ (+ tk_after_lead 1) 46)) "\u{9}"))
      (a!94 (= (str.at tk_code (+ (+ tk_after_lead 1) 47)) " "))
      (a!95 (= (str.at tk_code (+ (+ tk_after_lead 1) 47)) "\u{9}"))
      (a!96 (= (str.at tk_code (+ (+ tk_after_lead 1) 48)) " "))
      (a!97 (= (str.at tk_code (+ (+ tk_after_lead 1) 48)) "\u{9}"))
      (a!98 (= (str.at tk_code (+ (+ tk_after_lead 1) 49)) " "))
      (a!99 (= (str.at tk_code (+ (+ tk_after_lead 1) 49)) "\u{9}"))
      (a!100 (= (str.at tk_code (+ (+ tk_after_lead 1) 50)) " "))
      (a!101 (= (str.at tk_code (+ (+ tk_after_lead 1) 50)) "\u{9}"))
      (a!102 (= (str.at tk_code (+ (+ tk_after_lead 1) 51)) " "))
      (a!103 (= (str.at tk_code (+ (+ tk_after_lead 1) 51)) "\u{9}"))
      (a!104 (= (str.at tk_code (+ (+ tk_after_lead 1) 52)) " "))
      (a!105 (= (str.at tk_code (+ (+ tk_after_lead 1) 52)) "\u{9}"))
      (a!106 (= (str.at tk_code (+ (+ tk_after_lead 1) 53)) " "))
      (a!107 (= (str.at tk_code (+ (+ tk_after_lead 1) 53)) "\u{9}"))
      (a!108 (= (str.at tk_code (+ (+ tk_after_lead 1) 54)) " "))
      (a!109 (= (str.at tk_code (+ (+ tk_after_lead 1) 54)) "\u{9}"))
      (a!110 (= (str.at tk_code (+ (+ tk_after_lead 1) 55)) " "))
      (a!111 (= (str.at tk_code (+ (+ tk_after_lead 1) 55)) "\u{9}"))
      (a!112 (= (str.at tk_code (+ (+ tk_after_lead 1) 56)) " "))
      (a!113 (= (str.at tk_code (+ (+ tk_after_lead 1) 56)) "\u{9}"))
      (a!114 (= (str.at tk_code (+ (+ tk_after_lead 1) 57)) " "))
      (a!115 (= (str.at tk_code (+ (+ tk_after_lead 1) 57)) "\u{9}"))
      (a!116 (= (str.at tk_code (+ (+ tk_after_lead 1) 58)) " "))
      (a!117 (= (str.at tk_code (+ (+ tk_after_lead 1) 58)) "\u{9}"))
      (a!118 (= (str.at tk_code (+ (+ tk_after_lead 1) 59)) " "))
      (a!119 (= (str.at tk_code (+ (+ tk_after_lead 1) 59)) "\u{9}"))
      (a!120 (= (str.at tk_code (+ (+ tk_after_lead 1) 60)) " "))
      (a!121 (= (str.at tk_code (+ (+ tk_after_lead 1) 60)) "\u{9}"))
      (a!122 (= (str.at tk_code (+ (+ tk_after_lead 1) 61)) " "))
      (a!123 (= (str.at tk_code (+ (+ tk_after_lead 1) 61)) "\u{9}"))
      (a!124 (= (str.at tk_code (+ (+ tk_after_lead 1) 62)) " "))
      (a!125 (= (str.at tk_code (+ (+ tk_after_lead 1) 62)) "\u{9}"))
      (a!126 (= (str.at tk_code (+ (+ tk_after_lead 1) 63)) " "))
      (a!127 (= (str.at tk_code (+ (+ tk_after_lead 1) 63)) "\u{9}")))
(let ((a!128 (ite (not (or a!124 a!125))
                  (+ (+ tk_after_lead 1) 62)
                  (ite (not (or a!126 a!127))
                       (+ (+ tk_after_lead 1) 63)
                       (+ (+ tk_after_lead 1) 64)))))
(let ((a!129 (ite (not (or a!120 a!121))
                  (+ (+ tk_after_lead 1) 60)
                  (ite (not (or a!122 a!123)) (+ (+ tk_after_lead 1) 61) a!128))))
(let ((a!130 (ite (not (or a!116 a!117))
                  (+ (+ tk_after_lead 1) 58)
                  (ite (not (or a!118 a!119)) (+ (+ tk_after_lead 1) 59) a!129))))
(let ((a!131 (ite (not (or a!112 a!113))
                  (+ (+ tk_after_lead 1) 56)
                  (ite (not (or a!114 a!115)) (+ (+ tk_after_lead 1) 57) a!130))))
(let ((a!132 (ite (not (or a!108 a!109))
                  (+ (+ tk_after_lead 1) 54)
                  (ite (not (or a!110 a!111)) (+ (+ tk_after_lead 1) 55) a!131))))
(let ((a!133 (ite (not (or a!104 a!105))
                  (+ (+ tk_after_lead 1) 52)
                  (ite (not (or a!106 a!107)) (+ (+ tk_after_lead 1) 53) a!132))))
(let ((a!134 (ite (not (or a!100 a!101))
                  (+ (+ tk_after_lead 1) 50)
                  (ite (not (or a!102 a!103)) (+ (+ tk_after_lead 1) 51) a!133))))
(let ((a!135 (ite (not (or a!96 a!97))
                  (+ (+ tk_after_lead 1) 48)
                  (ite (not (or a!98 a!99)) (+ (+ tk_after_lead 1) 49) a!134))))
(let ((a!136 (ite (not (or a!92 a!93))
                  (+ (+ tk_after_lead 1) 46)
                  (ite (not (or a!94 a!95)) (+ (+ tk_after_lead 1) 47) a!135))))
(let ((a!137 (ite (not (or a!88 a!89))
                  (+ (+ tk_after_lead 1) 44)
                  (ite (not (or a!90 a!91)) (+ (+ tk_after_lead 1) 45) a!136))))
(let ((a!138 (ite (not (or a!84 a!85))
                  (+ (+ tk_after_lead 1) 42)
                  (ite (not (or a!86 a!87)) (+ (+ tk_after_lead 1) 43) a!137))))
(let ((a!139 (ite (not (or a!80 a!81))
                  (+ (+ tk_after_lead 1) 40)
                  (ite (not (or a!82 a!83)) (+ (+ tk_after_lead 1) 41) a!138))))
(let ((a!140 (ite (not (or a!76 a!77))
                  (+ (+ tk_after_lead 1) 38)
                  (ite (not (or a!78 a!79)) (+ (+ tk_after_lead 1) 39) a!139))))
(let ((a!141 (ite (not (or a!72 a!73))
                  (+ (+ tk_after_lead 1) 36)
                  (ite (not (or a!74 a!75)) (+ (+ tk_after_lead 1) 37) a!140))))
(let ((a!142 (ite (not (or a!68 a!69))
                  (+ (+ tk_after_lead 1) 34)
                  (ite (not (or a!70 a!71)) (+ (+ tk_after_lead 1) 35) a!141))))
(let ((a!143 (ite (not (or a!64 a!65))
                  (+ (+ tk_after_lead 1) 32)
                  (ite (not (or a!66 a!67)) (+ (+ tk_after_lead 1) 33) a!142))))
(let ((a!144 (ite (not (or a!60 a!61))
                  (+ (+ tk_after_lead 1) 30)
                  (ite (not (or a!62 a!63)) (+ (+ tk_after_lead 1) 31) a!143))))
(let ((a!145 (ite (not (or a!56 a!57))
                  (+ (+ tk_after_lead 1) 28)
                  (ite (not (or a!58 a!59)) (+ (+ tk_after_lead 1) 29) a!144))))
(let ((a!146 (ite (not (or a!52 a!53))
                  (+ (+ tk_after_lead 1) 26)
                  (ite (not (or a!54 a!55)) (+ (+ tk_after_lead 1) 27) a!145))))
(let ((a!147 (ite (not (or a!48 a!49))
                  (+ (+ tk_after_lead 1) 24)
                  (ite (not (or a!50 a!51)) (+ (+ tk_after_lead 1) 25) a!146))))
(let ((a!148 (ite (not (or a!44 a!45))
                  (+ (+ tk_after_lead 1) 22)
                  (ite (not (or a!46 a!47)) (+ (+ tk_after_lead 1) 23) a!147))))
(let ((a!149 (ite (not (or a!40 a!41))
                  (+ (+ tk_after_lead 1) 20)
                  (ite (not (or a!42 a!43)) (+ (+ tk_after_lead 1) 21) a!148))))
(let ((a!150 (ite (not (or a!36 a!37))
                  (+ (+ tk_after_lead 1) 18)
                  (ite (not (or a!38 a!39)) (+ (+ tk_after_lead 1) 19) a!149))))
(let ((a!151 (ite (not (or a!32 a!33))
                  (+ (+ tk_after_lead 1) 16)
                  (ite (not (or a!34 a!35)) (+ (+ tk_after_lead 1) 17) a!150))))
(let ((a!152 (ite (not (or a!28 a!29))
                  (+ (+ tk_after_lead 1) 14)
                  (ite (not (or a!30 a!31)) (+ (+ tk_after_lead 1) 15) a!151))))
(let ((a!153 (ite (not (or a!24 a!25))
                  (+ (+ tk_after_lead 1) 12)
                  (ite (not (or a!26 a!27)) (+ (+ tk_after_lead 1) 13) a!152))))
(let ((a!154 (ite (not (or a!20 a!21))
                  (+ (+ tk_after_lead 1) 10)
                  (ite (not (or a!22 a!23)) (+ (+ tk_after_lead 1) 11) a!153))))
(let ((a!155 (ite (not (or a!16 a!17))
                  (+ (+ tk_after_lead 1) 8)
                  (ite (not (or a!18 a!19)) (+ (+ tk_after_lead 1) 9) a!154))))
(let ((a!156 (ite (not (or a!12 a!13))
                  (+ (+ tk_after_lead 1) 6)
                  (ite (not (or a!14 a!15)) (+ (+ tk_after_lead 1) 7) a!155))))
(let ((a!157 (ite (not (or a!8 a!9))
                  (+ (+ tk_after_lead 1) 4)
                  (ite (not (or a!10 a!11)) (+ (+ tk_after_lead 1) 5) a!156))))
(let ((a!158 (ite (not (or a!4 a!5))
                  (+ (+ tk_after_lead 1) 2)
                  (ite (not (or a!6 a!7)) (+ (+ tk_after_lead 1) 3) a!157))))
(let ((a!159 (ite (not a!1)
                  (+ tk_after_lead 1)
                  (ite (not (or a!2 a!3)) (+ (+ tk_after_lead 1) 1) a!158))))
  (= tk_rhs_s a!159)))))))))))))))))))))))))))))))))))
(assert (let ((a!1 (ite tk_is_assign
                (str.substr tk_code tk_rhs_s (- (str.len tk_code) tk_rhs_s))
                "")))
  (= tk_rhs a!1)))
(assert (= tk_is_hold
   (and tk_is_assign
        (str.prefixof "(" tk_rhs)
        (str.contains tk_rhs "is_first_tick")
        (str.contains tk_rhs "\u{27e8}\u{27e9}")
        (not (str.contains tk_rhs "++")))))
(assert (= tk_is_litassign
   (and tk_is_assign
        (str.prefixof "\u{27e8}" tk_rhs)
        (str.suffixof "\u{27e9}" tk_rhs))))
(assert (= tk_lt
   (ite (or tk_decl_lit tk_is_litassign)
        (str.indexof tk_code "\u{27e8}" 0)
        (- 0 1))))
(assert (= tk_rt (ite (>= tk_lt 0) (str.indexof tk_code "\u{27e9}" tk_lt) (- 0 1))))
(assert (let ((a!1 (ite (> tk_rt tk_lt)
                (str.substr tk_code (+ tk_lt 1) (- (- tk_rt tk_lt) 1))
                "")))
  (= tk_inside a!1)))
(assert (= tk_enter_loop
   (and tk_src
        tk_reg_hit
        (not tk_lead_is_dual)
        (or tk_is_decl tk_is_hold tk_is_litassign))))
(assert (= tk_enter_dual (and tk_src tk_reg_hit tk_lead_is_dual tk_is_decl)))
(assert (= tk_hh_s (+ tk_ws 1)))
(assert (let ((a!1 (ite (and tk_src (not (= tk_code "")) (= (str.at tk_code tk_ws) "#"))
                tk_code
                "")))
(let ((a!2 (not (and (< tk_hh_s (str.len a!1))
                     (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                                   (str.at a!1 tk_hh_s)))))
      (a!3 (and (< (+ tk_hh_s 1) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 1)))))
      (a!4 (and (< (+ tk_hh_s 2) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 2)))))
      (a!5 (and (< (+ tk_hh_s 3) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 3)))))
      (a!6 (and (< (+ tk_hh_s 4) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 4)))))
      (a!7 (and (< (+ tk_hh_s 5) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 5)))))
      (a!8 (and (< (+ tk_hh_s 6) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 6)))))
      (a!9 (and (< (+ tk_hh_s 7) (str.len a!1))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at a!1 (+ tk_hh_s 7)))))
      (a!10 (and (< (+ tk_hh_s 8) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 8)))))
      (a!11 (and (< (+ tk_hh_s 9) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 9)))))
      (a!12 (and (< (+ tk_hh_s 10) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 10)))))
      (a!13 (and (< (+ tk_hh_s 11) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 11)))))
      (a!14 (and (< (+ tk_hh_s 12) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 12)))))
      (a!15 (and (< (+ tk_hh_s 13) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 13)))))
      (a!16 (and (< (+ tk_hh_s 14) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 14)))))
      (a!17 (and (< (+ tk_hh_s 15) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 15)))))
      (a!18 (and (< (+ tk_hh_s 16) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 16)))))
      (a!19 (and (< (+ tk_hh_s 17) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 17)))))
      (a!20 (and (< (+ tk_hh_s 18) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 18)))))
      (a!21 (and (< (+ tk_hh_s 19) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 19)))))
      (a!22 (and (< (+ tk_hh_s 20) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 20)))))
      (a!23 (and (< (+ tk_hh_s 21) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 21)))))
      (a!24 (and (< (+ tk_hh_s 22) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 22)))))
      (a!25 (and (< (+ tk_hh_s 23) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 23)))))
      (a!26 (and (< (+ tk_hh_s 24) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 24)))))
      (a!27 (and (< (+ tk_hh_s 25) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 25)))))
      (a!28 (and (< (+ tk_hh_s 26) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 26)))))
      (a!29 (and (< (+ tk_hh_s 27) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 27)))))
      (a!30 (and (< (+ tk_hh_s 28) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 28)))))
      (a!31 (and (< (+ tk_hh_s 29) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 29)))))
      (a!32 (and (< (+ tk_hh_s 30) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 30)))))
      (a!33 (and (< (+ tk_hh_s 31) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 31)))))
      (a!34 (and (< (+ tk_hh_s 32) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 32)))))
      (a!35 (and (< (+ tk_hh_s 33) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 33)))))
      (a!36 (and (< (+ tk_hh_s 34) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 34)))))
      (a!37 (and (< (+ tk_hh_s 35) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 35)))))
      (a!38 (and (< (+ tk_hh_s 36) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 36)))))
      (a!39 (and (< (+ tk_hh_s 37) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 37)))))
      (a!40 (and (< (+ tk_hh_s 38) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 38)))))
      (a!41 (and (< (+ tk_hh_s 39) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 39)))))
      (a!42 (and (< (+ tk_hh_s 40) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 40)))))
      (a!43 (and (< (+ tk_hh_s 41) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 41)))))
      (a!44 (and (< (+ tk_hh_s 42) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 42)))))
      (a!45 (and (< (+ tk_hh_s 43) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 43)))))
      (a!46 (and (< (+ tk_hh_s 44) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 44)))))
      (a!47 (and (< (+ tk_hh_s 45) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 45)))))
      (a!48 (and (< (+ tk_hh_s 46) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 46)))))
      (a!49 (and (< (+ tk_hh_s 47) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 47)))))
      (a!50 (and (< (+ tk_hh_s 48) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 48)))))
      (a!51 (and (< (+ tk_hh_s 49) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 49)))))
      (a!52 (and (< (+ tk_hh_s 50) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 50)))))
      (a!53 (and (< (+ tk_hh_s 51) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 51)))))
      (a!54 (and (< (+ tk_hh_s 52) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 52)))))
      (a!55 (and (< (+ tk_hh_s 53) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 53)))))
      (a!56 (and (< (+ tk_hh_s 54) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 54)))))
      (a!57 (and (< (+ tk_hh_s 55) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 55)))))
      (a!58 (and (< (+ tk_hh_s 56) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 56)))))
      (a!59 (and (< (+ tk_hh_s 57) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 57)))))
      (a!60 (and (< (+ tk_hh_s 58) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 58)))))
      (a!61 (and (< (+ tk_hh_s 59) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 59)))))
      (a!62 (and (< (+ tk_hh_s 60) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 60)))))
      (a!63 (and (< (+ tk_hh_s 61) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 61)))))
      (a!64 (and (< (+ tk_hh_s 62) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 62)))))
      (a!65 (and (< (+ tk_hh_s 63) (str.len a!1))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at a!1 (+ tk_hh_s 63))))))
(let ((a!66 (ite (not a!63)
                 (+ tk_hh_s 61)
                 (ite (not a!64)
                      (+ tk_hh_s 62)
                      (ite (not a!65) (+ tk_hh_s 63) (+ tk_hh_s 64))))))
(let ((a!67 (ite (not a!60)
                 (+ tk_hh_s 58)
                 (ite (not a!61)
                      (+ tk_hh_s 59)
                      (ite (not a!62) (+ tk_hh_s 60) a!66)))))
(let ((a!68 (ite (not a!57)
                 (+ tk_hh_s 55)
                 (ite (not a!58)
                      (+ tk_hh_s 56)
                      (ite (not a!59) (+ tk_hh_s 57) a!67)))))
(let ((a!69 (ite (not a!54)
                 (+ tk_hh_s 52)
                 (ite (not a!55)
                      (+ tk_hh_s 53)
                      (ite (not a!56) (+ tk_hh_s 54) a!68)))))
(let ((a!70 (ite (not a!51)
                 (+ tk_hh_s 49)
                 (ite (not a!52)
                      (+ tk_hh_s 50)
                      (ite (not a!53) (+ tk_hh_s 51) a!69)))))
(let ((a!71 (ite (not a!48)
                 (+ tk_hh_s 46)
                 (ite (not a!49)
                      (+ tk_hh_s 47)
                      (ite (not a!50) (+ tk_hh_s 48) a!70)))))
(let ((a!72 (ite (not a!45)
                 (+ tk_hh_s 43)
                 (ite (not a!46)
                      (+ tk_hh_s 44)
                      (ite (not a!47) (+ tk_hh_s 45) a!71)))))
(let ((a!73 (ite (not a!42)
                 (+ tk_hh_s 40)
                 (ite (not a!43)
                      (+ tk_hh_s 41)
                      (ite (not a!44) (+ tk_hh_s 42) a!72)))))
(let ((a!74 (ite (not a!39)
                 (+ tk_hh_s 37)
                 (ite (not a!40)
                      (+ tk_hh_s 38)
                      (ite (not a!41) (+ tk_hh_s 39) a!73)))))
(let ((a!75 (ite (not a!36)
                 (+ tk_hh_s 34)
                 (ite (not a!37)
                      (+ tk_hh_s 35)
                      (ite (not a!38) (+ tk_hh_s 36) a!74)))))
(let ((a!76 (ite (not a!33)
                 (+ tk_hh_s 31)
                 (ite (not a!34)
                      (+ tk_hh_s 32)
                      (ite (not a!35) (+ tk_hh_s 33) a!75)))))
(let ((a!77 (ite (not a!30)
                 (+ tk_hh_s 28)
                 (ite (not a!31)
                      (+ tk_hh_s 29)
                      (ite (not a!32) (+ tk_hh_s 30) a!76)))))
(let ((a!78 (ite (not a!27)
                 (+ tk_hh_s 25)
                 (ite (not a!28)
                      (+ tk_hh_s 26)
                      (ite (not a!29) (+ tk_hh_s 27) a!77)))))
(let ((a!79 (ite (not a!24)
                 (+ tk_hh_s 22)
                 (ite (not a!25)
                      (+ tk_hh_s 23)
                      (ite (not a!26) (+ tk_hh_s 24) a!78)))))
(let ((a!80 (ite (not a!21)
                 (+ tk_hh_s 19)
                 (ite (not a!22)
                      (+ tk_hh_s 20)
                      (ite (not a!23) (+ tk_hh_s 21) a!79)))))
(let ((a!81 (ite (not a!18)
                 (+ tk_hh_s 16)
                 (ite (not a!19)
                      (+ tk_hh_s 17)
                      (ite (not a!20) (+ tk_hh_s 18) a!80)))))
(let ((a!82 (ite (not a!15)
                 (+ tk_hh_s 13)
                 (ite (not a!16)
                      (+ tk_hh_s 14)
                      (ite (not a!17) (+ tk_hh_s 15) a!81)))))
(let ((a!83 (ite (not a!12)
                 (+ tk_hh_s 10)
                 (ite (not a!13)
                      (+ tk_hh_s 11)
                      (ite (not a!14) (+ tk_hh_s 12) a!82)))))
(let ((a!84 (ite (not a!9)
                 (+ tk_hh_s 7)
                 (ite (not a!10)
                      (+ tk_hh_s 8)
                      (ite (not a!11) (+ tk_hh_s 9) a!83)))))
(let ((a!85 (ite (not a!6)
                 (+ tk_hh_s 4)
                 (ite (not a!7)
                      (+ tk_hh_s 5)
                      (ite (not a!8) (+ tk_hh_s 6) a!84)))))
(let ((a!86 (ite (not a!3)
                 (+ tk_hh_s 1)
                 (ite (not a!4)
                      (+ tk_hh_s 2)
                      (ite (not a!5) (+ tk_hh_s 3) a!85)))))
  (= tk_hh_e (ite a!2 tk_hh_s a!86))))))))))))))))))))))))))
(assert (= tk_hash_after
   (ite (> tk_hh_e tk_hh_s) (str.substr tk_code tk_hh_s (- tk_hh_e tk_hh_s)) "")))
(assert (let ((a!1 (not (or (= (str.at tk_code tk_hh_e) " ")
                    (= (str.at tk_code tk_hh_e) "\u{9}"))))
      (a!2 (or (= (str.at tk_code (+ tk_hh_e 1)) " ")
               (= (str.at tk_code (+ tk_hh_e 1)) "\u{9}")))
      (a!3 (or (= (str.at tk_code (+ tk_hh_e 2)) " ")
               (= (str.at tk_code (+ tk_hh_e 2)) "\u{9}")))
      (a!4 (or (= (str.at tk_code (+ tk_hh_e 3)) " ")
               (= (str.at tk_code (+ tk_hh_e 3)) "\u{9}")))
      (a!5 (or (= (str.at tk_code (+ tk_hh_e 4)) " ")
               (= (str.at tk_code (+ tk_hh_e 4)) "\u{9}")))
      (a!6 (or (= (str.at tk_code (+ tk_hh_e 5)) " ")
               (= (str.at tk_code (+ tk_hh_e 5)) "\u{9}")))
      (a!7 (or (= (str.at tk_code (+ tk_hh_e 6)) " ")
               (= (str.at tk_code (+ tk_hh_e 6)) "\u{9}")))
      (a!8 (or (= (str.at tk_code (+ tk_hh_e 7)) " ")
               (= (str.at tk_code (+ tk_hh_e 7)) "\u{9}")))
      (a!9 (or (= (str.at tk_code (+ tk_hh_e 8)) " ")
               (= (str.at tk_code (+ tk_hh_e 8)) "\u{9}")))
      (a!10 (or (= (str.at tk_code (+ tk_hh_e 9)) " ")
                (= (str.at tk_code (+ tk_hh_e 9)) "\u{9}")))
      (a!11 (or (= (str.at tk_code (+ tk_hh_e 10)) " ")
                (= (str.at tk_code (+ tk_hh_e 10)) "\u{9}")))
      (a!12 (or (= (str.at tk_code (+ tk_hh_e 11)) " ")
                (= (str.at tk_code (+ tk_hh_e 11)) "\u{9}")))
      (a!13 (or (= (str.at tk_code (+ tk_hh_e 12)) " ")
                (= (str.at tk_code (+ tk_hh_e 12)) "\u{9}")))
      (a!14 (or (= (str.at tk_code (+ tk_hh_e 13)) " ")
                (= (str.at tk_code (+ tk_hh_e 13)) "\u{9}")))
      (a!15 (or (= (str.at tk_code (+ tk_hh_e 14)) " ")
                (= (str.at tk_code (+ tk_hh_e 14)) "\u{9}")))
      (a!16 (or (= (str.at tk_code (+ tk_hh_e 15)) " ")
                (= (str.at tk_code (+ tk_hh_e 15)) "\u{9}")))
      (a!17 (or (= (str.at tk_code (+ tk_hh_e 16)) " ")
                (= (str.at tk_code (+ tk_hh_e 16)) "\u{9}")))
      (a!18 (or (= (str.at tk_code (+ tk_hh_e 17)) " ")
                (= (str.at tk_code (+ tk_hh_e 17)) "\u{9}")))
      (a!19 (or (= (str.at tk_code (+ tk_hh_e 18)) " ")
                (= (str.at tk_code (+ tk_hh_e 18)) "\u{9}")))
      (a!20 (or (= (str.at tk_code (+ tk_hh_e 19)) " ")
                (= (str.at tk_code (+ tk_hh_e 19)) "\u{9}")))
      (a!21 (or (= (str.at tk_code (+ tk_hh_e 20)) " ")
                (= (str.at tk_code (+ tk_hh_e 20)) "\u{9}")))
      (a!22 (or (= (str.at tk_code (+ tk_hh_e 21)) " ")
                (= (str.at tk_code (+ tk_hh_e 21)) "\u{9}")))
      (a!23 (or (= (str.at tk_code (+ tk_hh_e 22)) " ")
                (= (str.at tk_code (+ tk_hh_e 22)) "\u{9}")))
      (a!24 (or (= (str.at tk_code (+ tk_hh_e 23)) " ")
                (= (str.at tk_code (+ tk_hh_e 23)) "\u{9}")))
      (a!25 (or (= (str.at tk_code (+ tk_hh_e 24)) " ")
                (= (str.at tk_code (+ tk_hh_e 24)) "\u{9}")))
      (a!26 (or (= (str.at tk_code (+ tk_hh_e 25)) " ")
                (= (str.at tk_code (+ tk_hh_e 25)) "\u{9}")))
      (a!27 (or (= (str.at tk_code (+ tk_hh_e 26)) " ")
                (= (str.at tk_code (+ tk_hh_e 26)) "\u{9}")))
      (a!28 (or (= (str.at tk_code (+ tk_hh_e 27)) " ")
                (= (str.at tk_code (+ tk_hh_e 27)) "\u{9}")))
      (a!29 (or (= (str.at tk_code (+ tk_hh_e 28)) " ")
                (= (str.at tk_code (+ tk_hh_e 28)) "\u{9}")))
      (a!30 (or (= (str.at tk_code (+ tk_hh_e 29)) " ")
                (= (str.at tk_code (+ tk_hh_e 29)) "\u{9}")))
      (a!31 (or (= (str.at tk_code (+ tk_hh_e 30)) " ")
                (= (str.at tk_code (+ tk_hh_e 30)) "\u{9}")))
      (a!32 (or (= (str.at tk_code (+ tk_hh_e 31)) " ")
                (= (str.at tk_code (+ tk_hh_e 31)) "\u{9}")))
      (a!33 (or (= (str.at tk_code (+ tk_hh_e 32)) " ")
                (= (str.at tk_code (+ tk_hh_e 32)) "\u{9}")))
      (a!34 (or (= (str.at tk_code (+ tk_hh_e 33)) " ")
                (= (str.at tk_code (+ tk_hh_e 33)) "\u{9}")))
      (a!35 (or (= (str.at tk_code (+ tk_hh_e 34)) " ")
                (= (str.at tk_code (+ tk_hh_e 34)) "\u{9}")))
      (a!36 (or (= (str.at tk_code (+ tk_hh_e 35)) " ")
                (= (str.at tk_code (+ tk_hh_e 35)) "\u{9}")))
      (a!37 (or (= (str.at tk_code (+ tk_hh_e 36)) " ")
                (= (str.at tk_code (+ tk_hh_e 36)) "\u{9}")))
      (a!38 (or (= (str.at tk_code (+ tk_hh_e 37)) " ")
                (= (str.at tk_code (+ tk_hh_e 37)) "\u{9}")))
      (a!39 (or (= (str.at tk_code (+ tk_hh_e 38)) " ")
                (= (str.at tk_code (+ tk_hh_e 38)) "\u{9}")))
      (a!40 (or (= (str.at tk_code (+ tk_hh_e 39)) " ")
                (= (str.at tk_code (+ tk_hh_e 39)) "\u{9}")))
      (a!41 (or (= (str.at tk_code (+ tk_hh_e 40)) " ")
                (= (str.at tk_code (+ tk_hh_e 40)) "\u{9}")))
      (a!42 (or (= (str.at tk_code (+ tk_hh_e 41)) " ")
                (= (str.at tk_code (+ tk_hh_e 41)) "\u{9}")))
      (a!43 (or (= (str.at tk_code (+ tk_hh_e 42)) " ")
                (= (str.at tk_code (+ tk_hh_e 42)) "\u{9}")))
      (a!44 (or (= (str.at tk_code (+ tk_hh_e 43)) " ")
                (= (str.at tk_code (+ tk_hh_e 43)) "\u{9}")))
      (a!45 (or (= (str.at tk_code (+ tk_hh_e 44)) " ")
                (= (str.at tk_code (+ tk_hh_e 44)) "\u{9}")))
      (a!46 (or (= (str.at tk_code (+ tk_hh_e 45)) " ")
                (= (str.at tk_code (+ tk_hh_e 45)) "\u{9}")))
      (a!47 (or (= (str.at tk_code (+ tk_hh_e 46)) " ")
                (= (str.at tk_code (+ tk_hh_e 46)) "\u{9}")))
      (a!48 (or (= (str.at tk_code (+ tk_hh_e 47)) " ")
                (= (str.at tk_code (+ tk_hh_e 47)) "\u{9}")))
      (a!49 (or (= (str.at tk_code (+ tk_hh_e 48)) " ")
                (= (str.at tk_code (+ tk_hh_e 48)) "\u{9}")))
      (a!50 (or (= (str.at tk_code (+ tk_hh_e 49)) " ")
                (= (str.at tk_code (+ tk_hh_e 49)) "\u{9}")))
      (a!51 (or (= (str.at tk_code (+ tk_hh_e 50)) " ")
                (= (str.at tk_code (+ tk_hh_e 50)) "\u{9}")))
      (a!52 (or (= (str.at tk_code (+ tk_hh_e 51)) " ")
                (= (str.at tk_code (+ tk_hh_e 51)) "\u{9}")))
      (a!53 (or (= (str.at tk_code (+ tk_hh_e 52)) " ")
                (= (str.at tk_code (+ tk_hh_e 52)) "\u{9}")))
      (a!54 (or (= (str.at tk_code (+ tk_hh_e 53)) " ")
                (= (str.at tk_code (+ tk_hh_e 53)) "\u{9}")))
      (a!55 (or (= (str.at tk_code (+ tk_hh_e 54)) " ")
                (= (str.at tk_code (+ tk_hh_e 54)) "\u{9}")))
      (a!56 (or (= (str.at tk_code (+ tk_hh_e 55)) " ")
                (= (str.at tk_code (+ tk_hh_e 55)) "\u{9}")))
      (a!57 (or (= (str.at tk_code (+ tk_hh_e 56)) " ")
                (= (str.at tk_code (+ tk_hh_e 56)) "\u{9}")))
      (a!58 (or (= (str.at tk_code (+ tk_hh_e 57)) " ")
                (= (str.at tk_code (+ tk_hh_e 57)) "\u{9}")))
      (a!59 (or (= (str.at tk_code (+ tk_hh_e 58)) " ")
                (= (str.at tk_code (+ tk_hh_e 58)) "\u{9}")))
      (a!60 (or (= (str.at tk_code (+ tk_hh_e 59)) " ")
                (= (str.at tk_code (+ tk_hh_e 59)) "\u{9}")))
      (a!61 (or (= (str.at tk_code (+ tk_hh_e 60)) " ")
                (= (str.at tk_code (+ tk_hh_e 60)) "\u{9}")))
      (a!62 (or (= (str.at tk_code (+ tk_hh_e 61)) " ")
                (= (str.at tk_code (+ tk_hh_e 61)) "\u{9}")))
      (a!63 (or (= (str.at tk_code (+ tk_hh_e 62)) " ")
                (= (str.at tk_code (+ tk_hh_e 62)) "\u{9}")))
      (a!64 (or (= (str.at tk_code (+ tk_hh_e 63)) " ")
                (= (str.at tk_code (+ tk_hh_e 63)) "\u{9}"))))
(let ((a!65 (ite (not a!62)
                 (+ tk_hh_e 61)
                 (ite (not a!63)
                      (+ tk_hh_e 62)
                      (ite (not a!64) (+ tk_hh_e 63) (+ tk_hh_e 64))))))
(let ((a!66 (ite (not a!59)
                 (+ tk_hh_e 58)
                 (ite (not a!60)
                      (+ tk_hh_e 59)
                      (ite (not a!61) (+ tk_hh_e 60) a!65)))))
(let ((a!67 (ite (not a!56)
                 (+ tk_hh_e 55)
                 (ite (not a!57)
                      (+ tk_hh_e 56)
                      (ite (not a!58) (+ tk_hh_e 57) a!66)))))
(let ((a!68 (ite (not a!53)
                 (+ tk_hh_e 52)
                 (ite (not a!54)
                      (+ tk_hh_e 53)
                      (ite (not a!55) (+ tk_hh_e 54) a!67)))))
(let ((a!69 (ite (not a!50)
                 (+ tk_hh_e 49)
                 (ite (not a!51)
                      (+ tk_hh_e 50)
                      (ite (not a!52) (+ tk_hh_e 51) a!68)))))
(let ((a!70 (ite (not a!47)
                 (+ tk_hh_e 46)
                 (ite (not a!48)
                      (+ tk_hh_e 47)
                      (ite (not a!49) (+ tk_hh_e 48) a!69)))))
(let ((a!71 (ite (not a!44)
                 (+ tk_hh_e 43)
                 (ite (not a!45)
                      (+ tk_hh_e 44)
                      (ite (not a!46) (+ tk_hh_e 45) a!70)))))
(let ((a!72 (ite (not a!41)
                 (+ tk_hh_e 40)
                 (ite (not a!42)
                      (+ tk_hh_e 41)
                      (ite (not a!43) (+ tk_hh_e 42) a!71)))))
(let ((a!73 (ite (not a!38)
                 (+ tk_hh_e 37)
                 (ite (not a!39)
                      (+ tk_hh_e 38)
                      (ite (not a!40) (+ tk_hh_e 39) a!72)))))
(let ((a!74 (ite (not a!35)
                 (+ tk_hh_e 34)
                 (ite (not a!36)
                      (+ tk_hh_e 35)
                      (ite (not a!37) (+ tk_hh_e 36) a!73)))))
(let ((a!75 (ite (not a!32)
                 (+ tk_hh_e 31)
                 (ite (not a!33)
                      (+ tk_hh_e 32)
                      (ite (not a!34) (+ tk_hh_e 33) a!74)))))
(let ((a!76 (ite (not a!29)
                 (+ tk_hh_e 28)
                 (ite (not a!30)
                      (+ tk_hh_e 29)
                      (ite (not a!31) (+ tk_hh_e 30) a!75)))))
(let ((a!77 (ite (not a!26)
                 (+ tk_hh_e 25)
                 (ite (not a!27)
                      (+ tk_hh_e 26)
                      (ite (not a!28) (+ tk_hh_e 27) a!76)))))
(let ((a!78 (ite (not a!23)
                 (+ tk_hh_e 22)
                 (ite (not a!24)
                      (+ tk_hh_e 23)
                      (ite (not a!25) (+ tk_hh_e 24) a!77)))))
(let ((a!79 (ite (not a!20)
                 (+ tk_hh_e 19)
                 (ite (not a!21)
                      (+ tk_hh_e 20)
                      (ite (not a!22) (+ tk_hh_e 21) a!78)))))
(let ((a!80 (ite (not a!17)
                 (+ tk_hh_e 16)
                 (ite (not a!18)
                      (+ tk_hh_e 17)
                      (ite (not a!19) (+ tk_hh_e 18) a!79)))))
(let ((a!81 (ite (not a!14)
                 (+ tk_hh_e 13)
                 (ite (not a!15)
                      (+ tk_hh_e 14)
                      (ite (not a!16) (+ tk_hh_e 15) a!80)))))
(let ((a!82 (ite (not a!11)
                 (+ tk_hh_e 10)
                 (ite (not a!12)
                      (+ tk_hh_e 11)
                      (ite (not a!13) (+ tk_hh_e 12) a!81)))))
(let ((a!83 (ite (not a!8)
                 (+ tk_hh_e 7)
                 (ite (not a!9)
                      (+ tk_hh_e 8)
                      (ite (not a!10) (+ tk_hh_e 9) a!82)))))
(let ((a!84 (ite (not a!5)
                 (+ tk_hh_e 4)
                 (ite (not a!6)
                      (+ tk_hh_e 5)
                      (ite (not a!7) (+ tk_hh_e 6) a!83)))))
(let ((a!85 (ite (not a!2)
                 (+ tk_hh_e 1)
                 (ite (not a!3)
                      (+ tk_hh_e 2)
                      (ite (not a!4) (+ tk_hh_e 3) a!84)))))
  (= tk_hash_aws (ite a!1 tk_hh_e a!85)))))))))))))))))))))))))
(assert (= tk_is_bound_line
   (and tk_src
        (not (= tk_code ""))
        (= (str.at tk_code tk_ws) "#")
        (> tk_hh_e tk_hh_s)
        (= (str.at tk_code tk_hash_aws) "\u{2264}"))))
(assert (= tk_bkey (str.++ "\u{27e6}" tk_hash_after "\u{27e7}")))
(assert (= tk_bound_reg (and tk_is_bound_line (>= (str.indexof _reg tk_bkey 0) 0))))
(assert (= tk_bound_hl_at (ite tk_bound_reg (str.indexof _reg tk_bkey 0) (- 0 1))))
(assert (let ((a!1 (ite tk_bound_reg
                (str.indexof _reg
                             "\u{2982}"
                             (+ tk_bound_hl_at (str.len tk_bkey)))
                (- 0 1))))
  (= tk_bound_d1 a!1)))
(assert (= tk_bound_d2
   (ite tk_bound_reg (str.indexof _reg "\u{2982}" (+ tk_bound_d1 1)) (- 0 1))))
(assert (let ((a!1 (and tk_bound_reg (= (str.at _reg (+ tk_bound_d2 1)) "1"))))
  (= tk_bound_hl a!1)))
(assert (= tk_drop_bound (and tk_bound_reg (not tk_bound_hl))))
(assert (let ((a!1 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ tk_hash_aws 1)))
      (a!2 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 1)))
      (a!3 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 2)))
      (a!4 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 3)))
      (a!5 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 4)))
      (a!6 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 5)))
      (a!7 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 6)))
      (a!8 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 7)))
      (a!9 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                   (+ (+ tk_hash_aws 1) 8)))
      (a!10 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 9)))
      (a!11 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 10)))
      (a!12 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 11)))
      (a!13 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 12)))
      (a!14 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 13)))
      (a!15 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 14)))
      (a!16 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 15)))
      (a!17 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 16)))
      (a!18 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 17)))
      (a!19 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 18)))
      (a!20 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 19)))
      (a!21 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 20)))
      (a!22 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 21)))
      (a!23 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 22)))
      (a!24 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 23)))
      (a!25 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 24)))
      (a!26 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 25)))
      (a!27 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 26)))
      (a!28 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 27)))
      (a!29 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 28)))
      (a!30 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 29)))
      (a!31 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 30)))
      (a!32 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 31)))
      (a!33 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 32)))
      (a!34 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 33)))
      (a!35 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 34)))
      (a!36 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 35)))
      (a!37 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 36)))
      (a!38 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 37)))
      (a!39 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 38)))
      (a!40 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 39)))
      (a!41 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 40)))
      (a!42 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 41)))
      (a!43 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 42)))
      (a!44 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 43)))
      (a!45 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 44)))
      (a!46 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 45)))
      (a!47 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 46)))
      (a!48 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 47)))
      (a!49 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 48)))
      (a!50 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 49)))
      (a!51 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 50)))
      (a!52 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 51)))
      (a!53 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 52)))
      (a!54 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 53)))
      (a!55 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 54)))
      (a!56 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 55)))
      (a!57 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 56)))
      (a!58 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 57)))
      (a!59 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 58)))
      (a!60 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 59)))
      (a!61 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 60)))
      (a!62 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 61)))
      (a!63 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 62)))
      (a!64 (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                    (+ (+ tk_hash_aws 1) 63))))
(let ((a!65 (ite (not (or (= a!64 " ") (= a!64 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 63)
                 (+ (+ tk_hash_aws 1) 64))))
(let ((a!66 (ite (not (or (= a!63 " ") (= a!63 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 62)
                 a!65)))
(let ((a!67 (ite (not (or (= a!62 " ") (= a!62 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 61)
                 a!66)))
(let ((a!68 (ite (not (or (= a!61 " ") (= a!61 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 60)
                 a!67)))
(let ((a!69 (ite (not (or (= a!60 " ") (= a!60 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 59)
                 a!68)))
(let ((a!70 (ite (not (or (= a!59 " ") (= a!59 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 58)
                 a!69)))
(let ((a!71 (ite (not (or (= a!58 " ") (= a!58 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 57)
                 a!70)))
(let ((a!72 (ite (not (or (= a!57 " ") (= a!57 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 56)
                 a!71)))
(let ((a!73 (ite (not (or (= a!56 " ") (= a!56 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 55)
                 a!72)))
(let ((a!74 (ite (not (or (= a!55 " ") (= a!55 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 54)
                 a!73)))
(let ((a!75 (ite (not (or (= a!54 " ") (= a!54 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 53)
                 a!74)))
(let ((a!76 (ite (not (or (= a!53 " ") (= a!53 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 52)
                 a!75)))
(let ((a!77 (ite (not (or (= a!52 " ") (= a!52 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 51)
                 a!76)))
(let ((a!78 (ite (not (or (= a!51 " ") (= a!51 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 50)
                 a!77)))
(let ((a!79 (ite (not (or (= a!50 " ") (= a!50 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 49)
                 a!78)))
(let ((a!80 (ite (not (or (= a!49 " ") (= a!49 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 48)
                 a!79)))
(let ((a!81 (ite (not (or (= a!48 " ") (= a!48 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 47)
                 a!80)))
(let ((a!82 (ite (not (or (= a!47 " ") (= a!47 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 46)
                 a!81)))
(let ((a!83 (ite (not (or (= a!46 " ") (= a!46 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 45)
                 a!82)))
(let ((a!84 (ite (not (or (= a!45 " ") (= a!45 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 44)
                 a!83)))
(let ((a!85 (ite (not (or (= a!44 " ") (= a!44 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 43)
                 a!84)))
(let ((a!86 (ite (not (or (= a!43 " ") (= a!43 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 42)
                 a!85)))
(let ((a!87 (ite (not (or (= a!42 " ") (= a!42 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 41)
                 a!86)))
(let ((a!88 (ite (not (or (= a!41 " ") (= a!41 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 40)
                 a!87)))
(let ((a!89 (ite (not (or (= a!40 " ") (= a!40 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 39)
                 a!88)))
(let ((a!90 (ite (not (or (= a!39 " ") (= a!39 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 38)
                 a!89)))
(let ((a!91 (ite (not (or (= a!38 " ") (= a!38 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 37)
                 a!90)))
(let ((a!92 (ite (not (or (= a!37 " ") (= a!37 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 36)
                 a!91)))
(let ((a!93 (ite (not (or (= a!36 " ") (= a!36 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 35)
                 a!92)))
(let ((a!94 (ite (not (or (= a!35 " ") (= a!35 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 34)
                 a!93)))
(let ((a!95 (ite (not (or (= a!34 " ") (= a!34 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 33)
                 a!94)))
(let ((a!96 (ite (not (or (= a!33 " ") (= a!33 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 32)
                 a!95)))
(let ((a!97 (ite (not (or (= a!32 " ") (= a!32 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 31)
                 a!96)))
(let ((a!98 (ite (not (or (= a!31 " ") (= a!31 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 30)
                 a!97)))
(let ((a!99 (ite (not (or (= a!30 " ") (= a!30 "\u{9}")))
                 (+ (+ tk_hash_aws 1) 29)
                 a!98)))
(let ((a!100 (ite (not (or (= a!29 " ") (= a!29 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 28)
                  a!99)))
(let ((a!101 (ite (not (or (= a!28 " ") (= a!28 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 27)
                  a!100)))
(let ((a!102 (ite (not (or (= a!27 " ") (= a!27 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 26)
                  a!101)))
(let ((a!103 (ite (not (or (= a!26 " ") (= a!26 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 25)
                  a!102)))
(let ((a!104 (ite (not (or (= a!25 " ") (= a!25 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 24)
                  a!103)))
(let ((a!105 (ite (not (or (= a!24 " ") (= a!24 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 23)
                  a!104)))
(let ((a!106 (ite (not (or (= a!23 " ") (= a!23 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 22)
                  a!105)))
(let ((a!107 (ite (not (or (= a!22 " ") (= a!22 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 21)
                  a!106)))
(let ((a!108 (ite (not (or (= a!21 " ") (= a!21 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 20)
                  a!107)))
(let ((a!109 (ite (not (or (= a!20 " ") (= a!20 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 19)
                  a!108)))
(let ((a!110 (ite (not (or (= a!19 " ") (= a!19 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 18)
                  a!109)))
(let ((a!111 (ite (not (or (= a!18 " ") (= a!18 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 17)
                  a!110)))
(let ((a!112 (ite (not (or (= a!17 " ") (= a!17 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 16)
                  a!111)))
(let ((a!113 (ite (not (or (= a!16 " ") (= a!16 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 15)
                  a!112)))
(let ((a!114 (ite (not (or (= a!15 " ") (= a!15 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 14)
                  a!113)))
(let ((a!115 (ite (not (or (= a!14 " ") (= a!14 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 13)
                  a!114)))
(let ((a!116 (ite (not (or (= a!13 " ") (= a!13 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 12)
                  a!115)))
(let ((a!117 (ite (not (or (= a!12 " ") (= a!12 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 11)
                  a!116)))
(let ((a!118 (ite (not (or (= a!11 " ") (= a!11 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 10)
                  a!117)))
(let ((a!119 (ite (not (or (= a!10 " ") (= a!10 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 9)
                  a!118)))
(let ((a!120 (ite (not (or (= a!9 " ") (= a!9 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 8)
                  a!119)))
(let ((a!121 (ite (not (or (= a!8 " ") (= a!8 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 7)
                  a!120)))
(let ((a!122 (ite (not (or (= a!7 " ") (= a!7 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 6)
                  a!121)))
(let ((a!123 (ite (not (or (= a!6 " ") (= a!6 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 5)
                  a!122)))
(let ((a!124 (ite (not (or (= a!5 " ") (= a!5 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 4)
                  a!123)))
(let ((a!125 (ite (not (or (= a!4 " ") (= a!4 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 3)
                  a!124)))
(let ((a!126 (ite (not (or (= a!3 " ") (= a!3 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 2)
                  a!125)))
(let ((a!127 (ite (not (or (= a!2 " ") (= a!2 "\u{9}")))
                  (+ (+ tk_hash_aws 1) 1)
                  a!126)))
(let ((a!128 (ite (not (or (= a!1 " ") (= a!1 "\u{9}")))
                  (+ tk_hash_aws 1)
                  a!127)))
  (= tk_bv_s0 a!128)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
(assert (let ((a!1 (< tk_bv_s0
              (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!2 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                                 tk_bv_s0)))
      (a!3 (< (+ tk_bv_s0 1)
              (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!4 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                                 (+ tk_bv_s0 1))))
      (a!5 (< (+ tk_bv_s0 2)
              (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!6 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                                 (+ tk_bv_s0 2))))
      (a!7 (< (+ tk_bv_s0 3)
              (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!8 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (and tk_bound_reg tk_bound_hl) tk_code "")
                                 (+ tk_bv_s0 3))))
      (a!9 (< (+ tk_bv_s0 4)
              (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!10 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 4))))
      (a!11 (< (+ tk_bv_s0 5)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!12 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 5))))
      (a!13 (< (+ tk_bv_s0 6)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!14 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 6))))
      (a!15 (< (+ tk_bv_s0 7)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!16 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 7))))
      (a!17 (< (+ tk_bv_s0 8)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!18 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 8))))
      (a!19 (< (+ tk_bv_s0 9)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!20 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 9))))
      (a!21 (< (+ tk_bv_s0 10)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!22 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 10))))
      (a!23 (< (+ tk_bv_s0 11)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!24 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 11))))
      (a!25 (< (+ tk_bv_s0 12)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!26 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 12))))
      (a!27 (< (+ tk_bv_s0 13)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!28 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 13))))
      (a!29 (< (+ tk_bv_s0 14)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!30 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 14))))
      (a!31 (< (+ tk_bv_s0 15)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!32 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 15))))
      (a!33 (< (+ tk_bv_s0 16)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!34 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 16))))
      (a!35 (< (+ tk_bv_s0 17)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!36 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 17))))
      (a!37 (< (+ tk_bv_s0 18)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!38 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 18))))
      (a!39 (< (+ tk_bv_s0 19)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!40 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 19))))
      (a!41 (< (+ tk_bv_s0 20)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!42 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 20))))
      (a!43 (< (+ tk_bv_s0 21)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!44 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 21))))
      (a!45 (< (+ tk_bv_s0 22)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!46 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 22))))
      (a!47 (< (+ tk_bv_s0 23)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!48 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 23))))
      (a!49 (< (+ tk_bv_s0 24)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!50 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 24))))
      (a!51 (< (+ tk_bv_s0 25)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!52 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 25))))
      (a!53 (< (+ tk_bv_s0 26)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!54 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 26))))
      (a!55 (< (+ tk_bv_s0 27)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!56 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 27))))
      (a!57 (< (+ tk_bv_s0 28)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!58 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 28))))
      (a!59 (< (+ tk_bv_s0 29)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!60 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 29))))
      (a!61 (< (+ tk_bv_s0 30)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!62 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 30))))
      (a!63 (< (+ tk_bv_s0 31)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!64 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 31))))
      (a!65 (< (+ tk_bv_s0 32)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!66 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 32))))
      (a!67 (< (+ tk_bv_s0 33)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!68 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 33))))
      (a!69 (< (+ tk_bv_s0 34)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!70 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 34))))
      (a!71 (< (+ tk_bv_s0 35)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!72 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 35))))
      (a!73 (< (+ tk_bv_s0 36)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!74 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 36))))
      (a!75 (< (+ tk_bv_s0 37)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!76 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 37))))
      (a!77 (< (+ tk_bv_s0 38)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!78 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 38))))
      (a!79 (< (+ tk_bv_s0 39)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!80 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 39))))
      (a!81 (< (+ tk_bv_s0 40)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!82 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 40))))
      (a!83 (< (+ tk_bv_s0 41)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!84 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 41))))
      (a!85 (< (+ tk_bv_s0 42)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!86 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 42))))
      (a!87 (< (+ tk_bv_s0 43)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!88 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 43))))
      (a!89 (< (+ tk_bv_s0 44)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!90 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 44))))
      (a!91 (< (+ tk_bv_s0 45)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!92 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 45))))
      (a!93 (< (+ tk_bv_s0 46)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!94 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 46))))
      (a!95 (< (+ tk_bv_s0 47)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!96 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 47))))
      (a!97 (< (+ tk_bv_s0 48)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!98 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (and tk_bound_reg tk_bound_hl)
                                       tk_code
                                       "")
                                  (+ tk_bv_s0 48))))
      (a!99 (< (+ tk_bv_s0 49)
               (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!100 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 49))))
      (a!101 (< (+ tk_bv_s0 50)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!102 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 50))))
      (a!103 (< (+ tk_bv_s0 51)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!104 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 51))))
      (a!105 (< (+ tk_bv_s0 52)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!106 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 52))))
      (a!107 (< (+ tk_bv_s0 53)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!108 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 53))))
      (a!109 (< (+ tk_bv_s0 54)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!110 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 54))))
      (a!111 (< (+ tk_bv_s0 55)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!112 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 55))))
      (a!113 (< (+ tk_bv_s0 56)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!114 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 56))))
      (a!115 (< (+ tk_bv_s0 57)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!116 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 57))))
      (a!117 (< (+ tk_bv_s0 58)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!118 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 58))))
      (a!119 (< (+ tk_bv_s0 59)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!120 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 59))))
      (a!121 (< (+ tk_bv_s0 60)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!122 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 60))))
      (a!123 (< (+ tk_bv_s0 61)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!124 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 61))))
      (a!125 (< (+ tk_bv_s0 62)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!126 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 62))))
      (a!127 (< (+ tk_bv_s0 63)
                (str.len (ite (and tk_bound_reg tk_bound_hl) tk_code ""))))
      (a!128 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (and tk_bound_reg tk_bound_hl)
                                        tk_code
                                        "")
                                   (+ tk_bv_s0 63)))))
(let ((a!129 (ite (not (and a!125 a!126))
                  (+ tk_bv_s0 62)
                  (ite (not (and a!127 a!128)) (+ tk_bv_s0 63) (+ tk_bv_s0 64)))))
(let ((a!130 (ite (not (and a!121 a!122))
                  (+ tk_bv_s0 60)
                  (ite (not (and a!123 a!124)) (+ tk_bv_s0 61) a!129))))
(let ((a!131 (ite (not (and a!117 a!118))
                  (+ tk_bv_s0 58)
                  (ite (not (and a!119 a!120)) (+ tk_bv_s0 59) a!130))))
(let ((a!132 (ite (not (and a!113 a!114))
                  (+ tk_bv_s0 56)
                  (ite (not (and a!115 a!116)) (+ tk_bv_s0 57) a!131))))
(let ((a!133 (ite (not (and a!109 a!110))
                  (+ tk_bv_s0 54)
                  (ite (not (and a!111 a!112)) (+ tk_bv_s0 55) a!132))))
(let ((a!134 (ite (not (and a!105 a!106))
                  (+ tk_bv_s0 52)
                  (ite (not (and a!107 a!108)) (+ tk_bv_s0 53) a!133))))
(let ((a!135 (ite (not (and a!101 a!102))
                  (+ tk_bv_s0 50)
                  (ite (not (and a!103 a!104)) (+ tk_bv_s0 51) a!134))))
(let ((a!136 (ite (not (and a!97 a!98))
                  (+ tk_bv_s0 48)
                  (ite (not (and a!99 a!100)) (+ tk_bv_s0 49) a!135))))
(let ((a!137 (ite (not (and a!93 a!94))
                  (+ tk_bv_s0 46)
                  (ite (not (and a!95 a!96)) (+ tk_bv_s0 47) a!136))))
(let ((a!138 (ite (not (and a!89 a!90))
                  (+ tk_bv_s0 44)
                  (ite (not (and a!91 a!92)) (+ tk_bv_s0 45) a!137))))
(let ((a!139 (ite (not (and a!85 a!86))
                  (+ tk_bv_s0 42)
                  (ite (not (and a!87 a!88)) (+ tk_bv_s0 43) a!138))))
(let ((a!140 (ite (not (and a!81 a!82))
                  (+ tk_bv_s0 40)
                  (ite (not (and a!83 a!84)) (+ tk_bv_s0 41) a!139))))
(let ((a!141 (ite (not (and a!77 a!78))
                  (+ tk_bv_s0 38)
                  (ite (not (and a!79 a!80)) (+ tk_bv_s0 39) a!140))))
(let ((a!142 (ite (not (and a!73 a!74))
                  (+ tk_bv_s0 36)
                  (ite (not (and a!75 a!76)) (+ tk_bv_s0 37) a!141))))
(let ((a!143 (ite (not (and a!69 a!70))
                  (+ tk_bv_s0 34)
                  (ite (not (and a!71 a!72)) (+ tk_bv_s0 35) a!142))))
(let ((a!144 (ite (not (and a!65 a!66))
                  (+ tk_bv_s0 32)
                  (ite (not (and a!67 a!68)) (+ tk_bv_s0 33) a!143))))
(let ((a!145 (ite (not (and a!61 a!62))
                  (+ tk_bv_s0 30)
                  (ite (not (and a!63 a!64)) (+ tk_bv_s0 31) a!144))))
(let ((a!146 (ite (not (and a!57 a!58))
                  (+ tk_bv_s0 28)
                  (ite (not (and a!59 a!60)) (+ tk_bv_s0 29) a!145))))
(let ((a!147 (ite (not (and a!53 a!54))
                  (+ tk_bv_s0 26)
                  (ite (not (and a!55 a!56)) (+ tk_bv_s0 27) a!146))))
(let ((a!148 (ite (not (and a!49 a!50))
                  (+ tk_bv_s0 24)
                  (ite (not (and a!51 a!52)) (+ tk_bv_s0 25) a!147))))
(let ((a!149 (ite (not (and a!45 a!46))
                  (+ tk_bv_s0 22)
                  (ite (not (and a!47 a!48)) (+ tk_bv_s0 23) a!148))))
(let ((a!150 (ite (not (and a!41 a!42))
                  (+ tk_bv_s0 20)
                  (ite (not (and a!43 a!44)) (+ tk_bv_s0 21) a!149))))
(let ((a!151 (ite (not (and a!37 a!38))
                  (+ tk_bv_s0 18)
                  (ite (not (and a!39 a!40)) (+ tk_bv_s0 19) a!150))))
(let ((a!152 (ite (not (and a!33 a!34))
                  (+ tk_bv_s0 16)
                  (ite (not (and a!35 a!36)) (+ tk_bv_s0 17) a!151))))
(let ((a!153 (ite (not (and a!29 a!30))
                  (+ tk_bv_s0 14)
                  (ite (not (and a!31 a!32)) (+ tk_bv_s0 15) a!152))))
(let ((a!154 (ite (not (and a!25 a!26))
                  (+ tk_bv_s0 12)
                  (ite (not (and a!27 a!28)) (+ tk_bv_s0 13) a!153))))
(let ((a!155 (ite (not (and a!21 a!22))
                  (+ tk_bv_s0 10)
                  (ite (not (and a!23 a!24)) (+ tk_bv_s0 11) a!154))))
(let ((a!156 (ite (not (and a!17 a!18))
                  (+ tk_bv_s0 8)
                  (ite (not (and a!19 a!20)) (+ tk_bv_s0 9) a!155))))
(let ((a!157 (ite (not (and a!13 a!14))
                  (+ tk_bv_s0 6)
                  (ite (not (and a!15 a!16)) (+ tk_bv_s0 7) a!156))))
(let ((a!158 (ite (not (and a!9 a!10))
                  (+ tk_bv_s0 4)
                  (ite (not (and a!11 a!12)) (+ tk_bv_s0 5) a!157))))
(let ((a!159 (ite (not (and a!5 a!6))
                  (+ tk_bv_s0 2)
                  (ite (not (and a!7 a!8)) (+ tk_bv_s0 3) a!158))))
(let ((a!160 (ite (not (and a!1 a!2))
                  tk_bv_s0
                  (ite (not (and a!3 a!4)) (+ tk_bv_s0 1) a!159))))
  (= tk_bv_e0 a!160)))))))))))))))))))))))))))))))))))
(assert (= tk_bound_n
   (ite (> tk_bv_e0 tk_bv_s0)
        (str.substr tk_code tk_bv_s0 (- tk_bv_e0 tk_bv_s0))
        "")))
(assert (= tk_rewrite_bound (and tk_bound_reg tk_bound_hl)))
(assert (= tk_default
   (and tk_src
        (not tk_is_top)
        (not tk_enter_loop)
        (not tk_enter_dual)
        (not tk_drop_bound)
        (not tk_rewrite_bound))))
(assert (= tk_loop_run (and (= tk_ph 4) (< _emit_k _emit_n))))
(assert (= tk_loop_done (and (= tk_ph 4) (>= _emit_k _emit_n))))
(assert (let ((a!1 (ite (= (- _emit_k 1) 13)
                LsCommaPos__cp13__call10
                (ite (= (- _emit_k 1) 14)
                     LsCommaPos__cp14__call10
                     LsCommaPos__cp15__call10))))
(let ((a!2 (ite (= (- _emit_k 1) 11)
                LsCommaPos__cp11__call10
                (ite (= (- _emit_k 1) 12) LsCommaPos__cp12__call10 a!1))))
(let ((a!3 (ite (= (- _emit_k 1) 9)
                LsCommaPos__cp9__call10
                (ite (= (- _emit_k 1) 10) LsCommaPos__cp10__call10 a!2))))
(let ((a!4 (ite (= (- _emit_k 1) 7)
                LsCommaPos__cp7__call10
                (ite (= (- _emit_k 1) 8) LsCommaPos__cp8__call10 a!3))))
(let ((a!5 (ite (= (- _emit_k 1) 5)
                LsCommaPos__cp5__call10
                (ite (= (- _emit_k 1) 6) LsCommaPos__cp6__call10 a!4))))
(let ((a!6 (ite (= (- _emit_k 1) 3)
                LsCommaPos__cp3__call10
                (ite (= (- _emit_k 1) 4) LsCommaPos__cp4__call10 a!5))))
(let ((a!7 (ite (= (- _emit_k 1) 1)
                LsCommaPos__cp1__call10
                (ite (= (- _emit_k 1) 2) LsCommaPos__cp2__call10 a!6))))
(let ((a!8 (ite (< (- _emit_k 1) 0)
                (- 0 1)
                (ite (= (- _emit_k 1) 0) LsCommaPos__cp0__call10 a!7))))
  (= LsNthElem__ne_pstart__call9 a!8))))))))))
(assert (= LsCommaPos__cp0__call10
   (ite (< (str.indexof _emit_inside "," 0) 0)
        (str.len _emit_inside)
        (str.indexof _emit_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp0__call10 1))
                  0))))
  (= LsCommaPos__cp1__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp0__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp1__call10 1))
                  0))))
  (= LsCommaPos__cp2__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp1__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp2__call10 1))
                  0))))
  (= LsCommaPos__cp3__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp2__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp3__call10 1))
                  0))))
  (= LsCommaPos__cp4__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp3__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp4__call10 1))
                  0))))
  (= LsCommaPos__cp5__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp4__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp5__call10 1))
                  0))))
  (= LsCommaPos__cp6__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp5__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp6__call10 1))
                  0))))
  (= LsCommaPos__cp7__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp6__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp7__call10 1))
                  0))))
  (= LsCommaPos__cp8__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp7__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp8__call10 1))
                  0))))
  (= LsCommaPos__cp9__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp8__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp9__call10 1))
                  0))))
  (= LsCommaPos__cp10__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp9__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp10__call10 1))
                  0))))
  (= LsCommaPos__cp11__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp10__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp11__call10 1))
                  0))))
  (= LsCommaPos__cp12__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp11__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp12__call10 1))
                  0))))
  (= LsCommaPos__cp13__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp12__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp13__call10 1))
                  0))))
  (= LsCommaPos__cp14__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp13__call10 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call10 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp14__call10 1))
                  0))))
  (= LsCommaPos__cp15__call10
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp14__call10 1))))))
(assert (let ((a!1 (ite (= _emit_k 12)
                LsCommaPos__cp12__call11
                (ite (= _emit_k 13)
                     LsCommaPos__cp13__call11
                     (ite (= _emit_k 14)
                          LsCommaPos__cp14__call11
                          LsCommaPos__cp15__call11)))))
(let ((a!2 (ite (= _emit_k 9)
                LsCommaPos__cp9__call11
                (ite (= _emit_k 10)
                     LsCommaPos__cp10__call11
                     (ite (= _emit_k 11) LsCommaPos__cp11__call11 a!1)))))
(let ((a!3 (ite (= _emit_k 6)
                LsCommaPos__cp6__call11
                (ite (= _emit_k 7)
                     LsCommaPos__cp7__call11
                     (ite (= _emit_k 8) LsCommaPos__cp8__call11 a!2)))))
(let ((a!4 (ite (= _emit_k 3)
                LsCommaPos__cp3__call11
                (ite (= _emit_k 4)
                     LsCommaPos__cp4__call11
                     (ite (= _emit_k 5) LsCommaPos__cp5__call11 a!3)))))
(let ((a!5 (ite (= _emit_k 0)
                LsCommaPos__cp0__call11
                (ite (= _emit_k 1)
                     LsCommaPos__cp1__call11
                     (ite (= _emit_k 2) LsCommaPos__cp2__call11 a!4)))))
  (= LsNthElem__ne_pend__call9 (ite (< _emit_k 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call11
   (ite (< (str.indexof _emit_inside "," 0) 0)
        (str.len _emit_inside)
        (str.indexof _emit_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp0__call11 1))
                  0))))
  (= LsCommaPos__cp1__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp0__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp1__call11 1))
                  0))))
  (= LsCommaPos__cp2__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp1__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp2__call11 1))
                  0))))
  (= LsCommaPos__cp3__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp2__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp3__call11 1))
                  0))))
  (= LsCommaPos__cp4__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp3__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp4__call11 1))
                  0))))
  (= LsCommaPos__cp5__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp4__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp5__call11 1))
                  0))))
  (= LsCommaPos__cp6__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp5__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp6__call11 1))
                  0))))
  (= LsCommaPos__cp7__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp6__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp7__call11 1))
                  0))))
  (= LsCommaPos__cp8__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp7__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp8__call11 1))
                  0))))
  (= LsCommaPos__cp9__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp8__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp9__call11 1))
                  0))))
  (= LsCommaPos__cp10__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp9__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp10__call11 1))
                  0))))
  (= LsCommaPos__cp11__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp10__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp11__call11 1))
                  0))))
  (= LsCommaPos__cp12__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp11__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp12__call11 1))
                  0))))
  (= LsCommaPos__cp13__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp12__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp13__call11 1))
                  0))))
  (= LsCommaPos__cp14__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp13__call11 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call11 (str.len _emit_inside))
               (< (str.indexof _emit_inside "," (+ LsCommaPos__cp14__call11 1))
                  0))))
  (= LsCommaPos__cp15__call11
     (ite a!1
          (str.len _emit_inside)
          (str.indexof _emit_inside "," (+ LsCommaPos__cp14__call11 1))))))
(assert (= LsNthElem__ne_raw_s__call9
   (ite (= _emit_k 0) 0 (+ LsNthElem__ne_pstart__call9 1))))
(assert (= LsNthElem__ne_raw_e__call9
   (ite (>= LsNthElem__ne_pend__call9 (str.len _emit_inside))
        (str.len _emit_inside)
        LsNthElem__ne_pend__call9)))
(assert (let ((a!1 (and (< LsNthElem__ne_raw_s__call9 LsNthElem__ne_raw_e__call9)
                (or (= (str.at _emit_inside LsNthElem__ne_raw_s__call9) " ")
                    (= (str.at _emit_inside LsNthElem__ne_raw_s__call9) "\u{9}"))))
      (a!2 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 1)) " ")
               (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 1))
                  "\u{9}")))
      (a!4 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 2)) " ")
               (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 2))
                  "\u{9}")))
      (a!6 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 3)) " ")
               (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 3))
                  "\u{9}")))
      (a!8 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 4)) " ")
               (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 4))
                  "\u{9}")))
      (a!10 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 5)) " ")
                (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 5))
                   "\u{9}")))
      (a!12 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 6)) " ")
                (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 6))
                   "\u{9}")))
      (a!14 (or (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 7)) " ")
                (= (str.at _emit_inside (+ LsNthElem__ne_raw_s__call9 7))
                   "\u{9}"))))
(let ((a!3 (not (and (< (+ LsNthElem__ne_raw_s__call9 1)
                        LsNthElem__ne_raw_e__call9)
                     a!2)))
      (a!5 (not (and (< (+ LsNthElem__ne_raw_s__call9 2)
                        LsNthElem__ne_raw_e__call9)
                     a!4)))
      (a!7 (not (and (< (+ LsNthElem__ne_raw_s__call9 3)
                        LsNthElem__ne_raw_e__call9)
                     a!6)))
      (a!9 (not (and (< (+ LsNthElem__ne_raw_s__call9 4)
                        LsNthElem__ne_raw_e__call9)
                     a!8)))
      (a!11 (not (and (< (+ LsNthElem__ne_raw_s__call9 5)
                         LsNthElem__ne_raw_e__call9)
                      a!10)))
      (a!13 (not (and (< (+ LsNthElem__ne_raw_s__call9 6)
                         LsNthElem__ne_raw_e__call9)
                      a!12)))
      (a!15 (not (and (< (+ LsNthElem__ne_raw_s__call9 7)
                         LsNthElem__ne_raw_e__call9)
                      a!14))))
(let ((a!16 (ite a!11
                 (+ LsNthElem__ne_raw_s__call9 5)
                 (ite a!13
                      (+ LsNthElem__ne_raw_s__call9 6)
                      (ite a!15
                           (+ LsNthElem__ne_raw_s__call9 7)
                           (+ LsNthElem__ne_raw_s__call9 8))))))
(let ((a!17 (ite a!5
                 (+ LsNthElem__ne_raw_s__call9 2)
                 (ite a!7
                      (+ LsNthElem__ne_raw_s__call9 3)
                      (ite a!9 (+ LsNthElem__ne_raw_s__call9 4) a!16)))))
  (= LsNthElem__ne_ts__call9
     (ite (not a!1)
          LsNthElem__ne_raw_s__call9
          (ite a!3 (+ LsNthElem__ne_raw_s__call9 1) a!17))))))))
(assert (let ((a!1 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 1)) " ")
               (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 1))
                  "\u{9}")))
      (a!3 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 2)) " ")
               (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 2))
                  "\u{9}")))
      (a!5 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 3)) " ")
               (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 3))
                  "\u{9}")))
      (a!7 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 4)) " ")
               (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 4))
                  "\u{9}")))
      (a!9 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 5)) " ")
               (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 5))
                  "\u{9}")))
      (a!11 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 6)) " ")
                (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 6))
                   "\u{9}")))
      (a!13 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 7)) " ")
                (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 7))
                   "\u{9}")))
      (a!15 (or (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 8)) " ")
                (= (str.at _emit_inside (- LsNthElem__ne_raw_e__call9 8))
                   "\u{9}"))))
(let ((a!2 (not (and (>= (- LsNthElem__ne_raw_e__call9 1)
                         LsNthElem__ne_ts__call9)
                     a!1)))
      (a!4 (not (and (>= (- LsNthElem__ne_raw_e__call9 2)
                         LsNthElem__ne_ts__call9)
                     a!3)))
      (a!6 (not (and (>= (- LsNthElem__ne_raw_e__call9 3)
                         LsNthElem__ne_ts__call9)
                     a!5)))
      (a!8 (not (and (>= (- LsNthElem__ne_raw_e__call9 4)
                         LsNthElem__ne_ts__call9)
                     a!7)))
      (a!10 (not (and (>= (- LsNthElem__ne_raw_e__call9 5)
                          LsNthElem__ne_ts__call9)
                      a!9)))
      (a!12 (not (and (>= (- LsNthElem__ne_raw_e__call9 6)
                          LsNthElem__ne_ts__call9)
                      a!11)))
      (a!14 (not (and (>= (- LsNthElem__ne_raw_e__call9 7)
                          LsNthElem__ne_ts__call9)
                      a!13)))
      (a!16 (not (and (>= (- LsNthElem__ne_raw_e__call9 8)
                          LsNthElem__ne_ts__call9)
                      a!15))))
(let ((a!17 (ite a!12
                 (- LsNthElem__ne_raw_e__call9 5)
                 (ite a!14
                      (- LsNthElem__ne_raw_e__call9 6)
                      (ite a!16
                           (- LsNthElem__ne_raw_e__call9 7)
                           (- LsNthElem__ne_raw_e__call9 8))))))
(let ((a!18 (ite a!6
                 (- LsNthElem__ne_raw_e__call9 2)
                 (ite a!8
                      (- LsNthElem__ne_raw_e__call9 3)
                      (ite a!10 (- LsNthElem__ne_raw_e__call9 4) a!17)))))
  (= LsNthElem__ne_te__call9
     (ite a!2
          LsNthElem__ne_raw_e__call9
          (ite a!4 (- LsNthElem__ne_raw_e__call9 1) a!18))))))))
(assert (= tk_el
   (ite (> LsNthElem__ne_te__call9 LsNthElem__ne_ts__call9)
        (str.substr _emit_inside
                    LsNthElem__ne_ts__call9
                    (- LsNthElem__ne_te__call9 LsNthElem__ne_ts__call9))
        "")))
(assert (= tk_zdef
   (ite (= _emit_base "String") """""" (ite (= _emit_base "Bool") "false" "0"))))
(assert (let ((a!1 (ite (>= _emit_k 0)
                (str.from_int _emit_k)
                (str.++ "-" (str.from_int (- 0 _emit_k))))))
  (= tk_slot_pfx
     (str.++ (ite (= _emit_kind 2) (str.++ "_" _emit_nm) _emit_nm) "_" a!1))))
(assert (let ((a!1 (ite (>= _emit_k 0)
                (str.from_int _emit_k)
                (str.++ "-" (str.from_int (- 0 _emit_k))))))
(let ((a!2 (ite (= _emit_kind 3)
                (str.++ (str.++ tk_slot_pfx " \u{2208} " _emit_base)
                        " = "
                        (ite (< _emit_k _emit_ne) tk_el tk_zdef))
                (str.++ tk_slot_pfx
                        " = (is_first_tick ? "
                        tk_zdef
                        " : _"
                        _emit_nm
                        "_"
                        a!1
                        ")"))))
  (= tk_slot_line
     (ite (= _emit_kind 1)
          (str.++ tk_slot_pfx " \u{2208} " _emit_base)
          (ite (= _emit_kind 2)
               (str.++ tk_slot_pfx " \u{2208} " _emit_base)
               a!2))))))
(assert (let ((a!1 (ite (>= _emit_ne 0)
                (str.from_int _emit_ne)
                (str.++ "-" (str.from_int (- 0 _emit_ne)))))
      (a!2 (ite (= _emit_kind 4)
                (str.++ _emit_nm
                        "_len = (is_first_tick ? 0 : _"
                        _emit_nm
                        "_len)")
                (ite _emit_haslen
                     (ite (= _emit_kind 2)
                          (str.++ "_" _emit_nm "_len \u{2208} Int")
                          (str.++ _emit_nm
                                  "_len \u{2208} Int"
                                  "\u{a}"
                                  _indent
                                  "0 \u{2264} "
                                  _emit_nm
                                  "_len"))
                     ""))))
  (= tk_len_lines
     (ite (= _emit_kind 3) (str.++ _emit_nm "_len \u{2208} Int = " a!1) a!2))))
(assert (= tk_has_len_lines (and tk_loop_done (not (= tk_len_lines "")))))
(assert (= tk_needs_walk
   (and tk_default (or (str.contains tk_rline "#") (str.contains tk_rline "[")))))
(assert (= tk_default_plain (and tk_default (not tk_needs_walk))))
(assert (= w_src _sub_src))
(assert (= w_p _sub_pos))
(assert (= w_ch (str.at w_src w_p)))
(assert (= w_is_hash (and (= tk_ph 5) (= w_ch "#"))))
(assert (let ((a!1 (and (< (+ w_p 1) (str.len (ite w_is_hash w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_is_hash w_src "") (+ w_p 1)))))
      (a!2 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 1))))
      (a!4 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 2))))
      (a!6 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 3))))
      (a!8 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 4))))
      (a!10 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 5))))
      (a!12 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 6))))
      (a!14 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 7))))
      (a!16 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 8))))
      (a!18 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 9))))
      (a!20 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 10))))
      (a!22 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 11))))
      (a!24 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 12))))
      (a!26 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 13))))
      (a!28 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 14))))
      (a!30 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 15))))
      (a!32 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 16))))
      (a!34 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 17))))
      (a!36 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 18))))
      (a!38 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 19))))
      (a!40 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 20))))
      (a!42 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 21))))
      (a!44 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 22))))
      (a!46 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 23))))
      (a!48 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 24))))
      (a!50 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 25))))
      (a!52 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 26))))
      (a!54 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 27))))
      (a!56 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 28))))
      (a!58 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 29))))
      (a!60 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 30))))
      (a!62 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 31))))
      (a!64 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 32))))
      (a!66 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 33))))
      (a!68 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 34))))
      (a!70 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 35))))
      (a!72 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 36))))
      (a!74 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 37))))
      (a!76 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 38))))
      (a!78 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 39))))
      (a!80 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 40))))
      (a!82 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 41))))
      (a!84 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 42))))
      (a!86 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 43))))
      (a!88 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 44))))
      (a!90 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 45))))
      (a!92 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 46))))
      (a!94 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 47))))
      (a!96 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 48))))
      (a!98 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 49))))
      (a!100 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 50))))
      (a!102 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 51))))
      (a!104 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 52))))
      (a!106 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 53))))
      (a!108 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 54))))
      (a!110 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 55))))
      (a!112 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 56))))
      (a!114 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 57))))
      (a!116 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 58))))
      (a!118 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 59))))
      (a!120 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 60))))
      (a!122 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 61))))
      (a!124 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 62))))
      (a!126 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite w_is_hash w_src "") (+ (+ w_p 1) 63)))))
(let ((a!3 (and (< (+ (+ w_p 1) 1) (str.len (ite w_is_hash w_src ""))) a!2))
      (a!5 (and (< (+ (+ w_p 1) 2) (str.len (ite w_is_hash w_src ""))) a!4))
      (a!7 (and (< (+ (+ w_p 1) 3) (str.len (ite w_is_hash w_src ""))) a!6))
      (a!9 (and (< (+ (+ w_p 1) 4) (str.len (ite w_is_hash w_src ""))) a!8))
      (a!11 (and (< (+ (+ w_p 1) 5) (str.len (ite w_is_hash w_src ""))) a!10))
      (a!13 (and (< (+ (+ w_p 1) 6) (str.len (ite w_is_hash w_src ""))) a!12))
      (a!15 (and (< (+ (+ w_p 1) 7) (str.len (ite w_is_hash w_src ""))) a!14))
      (a!17 (and (< (+ (+ w_p 1) 8) (str.len (ite w_is_hash w_src ""))) a!16))
      (a!19 (and (< (+ (+ w_p 1) 9) (str.len (ite w_is_hash w_src ""))) a!18))
      (a!21 (and (< (+ (+ w_p 1) 10) (str.len (ite w_is_hash w_src ""))) a!20))
      (a!23 (and (< (+ (+ w_p 1) 11) (str.len (ite w_is_hash w_src ""))) a!22))
      (a!25 (and (< (+ (+ w_p 1) 12) (str.len (ite w_is_hash w_src ""))) a!24))
      (a!27 (and (< (+ (+ w_p 1) 13) (str.len (ite w_is_hash w_src ""))) a!26))
      (a!29 (and (< (+ (+ w_p 1) 14) (str.len (ite w_is_hash w_src ""))) a!28))
      (a!31 (and (< (+ (+ w_p 1) 15) (str.len (ite w_is_hash w_src ""))) a!30))
      (a!33 (and (< (+ (+ w_p 1) 16) (str.len (ite w_is_hash w_src ""))) a!32))
      (a!35 (and (< (+ (+ w_p 1) 17) (str.len (ite w_is_hash w_src ""))) a!34))
      (a!37 (and (< (+ (+ w_p 1) 18) (str.len (ite w_is_hash w_src ""))) a!36))
      (a!39 (and (< (+ (+ w_p 1) 19) (str.len (ite w_is_hash w_src ""))) a!38))
      (a!41 (and (< (+ (+ w_p 1) 20) (str.len (ite w_is_hash w_src ""))) a!40))
      (a!43 (and (< (+ (+ w_p 1) 21) (str.len (ite w_is_hash w_src ""))) a!42))
      (a!45 (and (< (+ (+ w_p 1) 22) (str.len (ite w_is_hash w_src ""))) a!44))
      (a!47 (and (< (+ (+ w_p 1) 23) (str.len (ite w_is_hash w_src ""))) a!46))
      (a!49 (and (< (+ (+ w_p 1) 24) (str.len (ite w_is_hash w_src ""))) a!48))
      (a!51 (and (< (+ (+ w_p 1) 25) (str.len (ite w_is_hash w_src ""))) a!50))
      (a!53 (and (< (+ (+ w_p 1) 26) (str.len (ite w_is_hash w_src ""))) a!52))
      (a!55 (and (< (+ (+ w_p 1) 27) (str.len (ite w_is_hash w_src ""))) a!54))
      (a!57 (and (< (+ (+ w_p 1) 28) (str.len (ite w_is_hash w_src ""))) a!56))
      (a!59 (and (< (+ (+ w_p 1) 29) (str.len (ite w_is_hash w_src ""))) a!58))
      (a!61 (and (< (+ (+ w_p 1) 30) (str.len (ite w_is_hash w_src ""))) a!60))
      (a!63 (and (< (+ (+ w_p 1) 31) (str.len (ite w_is_hash w_src ""))) a!62))
      (a!65 (and (< (+ (+ w_p 1) 32) (str.len (ite w_is_hash w_src ""))) a!64))
      (a!67 (and (< (+ (+ w_p 1) 33) (str.len (ite w_is_hash w_src ""))) a!66))
      (a!69 (and (< (+ (+ w_p 1) 34) (str.len (ite w_is_hash w_src ""))) a!68))
      (a!71 (and (< (+ (+ w_p 1) 35) (str.len (ite w_is_hash w_src ""))) a!70))
      (a!73 (and (< (+ (+ w_p 1) 36) (str.len (ite w_is_hash w_src ""))) a!72))
      (a!75 (and (< (+ (+ w_p 1) 37) (str.len (ite w_is_hash w_src ""))) a!74))
      (a!77 (and (< (+ (+ w_p 1) 38) (str.len (ite w_is_hash w_src ""))) a!76))
      (a!79 (and (< (+ (+ w_p 1) 39) (str.len (ite w_is_hash w_src ""))) a!78))
      (a!81 (and (< (+ (+ w_p 1) 40) (str.len (ite w_is_hash w_src ""))) a!80))
      (a!83 (and (< (+ (+ w_p 1) 41) (str.len (ite w_is_hash w_src ""))) a!82))
      (a!85 (and (< (+ (+ w_p 1) 42) (str.len (ite w_is_hash w_src ""))) a!84))
      (a!87 (and (< (+ (+ w_p 1) 43) (str.len (ite w_is_hash w_src ""))) a!86))
      (a!89 (and (< (+ (+ w_p 1) 44) (str.len (ite w_is_hash w_src ""))) a!88))
      (a!91 (and (< (+ (+ w_p 1) 45) (str.len (ite w_is_hash w_src ""))) a!90))
      (a!93 (and (< (+ (+ w_p 1) 46) (str.len (ite w_is_hash w_src ""))) a!92))
      (a!95 (and (< (+ (+ w_p 1) 47) (str.len (ite w_is_hash w_src ""))) a!94))
      (a!97 (and (< (+ (+ w_p 1) 48) (str.len (ite w_is_hash w_src ""))) a!96))
      (a!99 (and (< (+ (+ w_p 1) 49) (str.len (ite w_is_hash w_src ""))) a!98))
      (a!101 (and (< (+ (+ w_p 1) 50) (str.len (ite w_is_hash w_src ""))) a!100))
      (a!103 (and (< (+ (+ w_p 1) 51) (str.len (ite w_is_hash w_src ""))) a!102))
      (a!105 (and (< (+ (+ w_p 1) 52) (str.len (ite w_is_hash w_src ""))) a!104))
      (a!107 (and (< (+ (+ w_p 1) 53) (str.len (ite w_is_hash w_src ""))) a!106))
      (a!109 (and (< (+ (+ w_p 1) 54) (str.len (ite w_is_hash w_src ""))) a!108))
      (a!111 (and (< (+ (+ w_p 1) 55) (str.len (ite w_is_hash w_src ""))) a!110))
      (a!113 (and (< (+ (+ w_p 1) 56) (str.len (ite w_is_hash w_src ""))) a!112))
      (a!115 (and (< (+ (+ w_p 1) 57) (str.len (ite w_is_hash w_src ""))) a!114))
      (a!117 (and (< (+ (+ w_p 1) 58) (str.len (ite w_is_hash w_src ""))) a!116))
      (a!119 (and (< (+ (+ w_p 1) 59) (str.len (ite w_is_hash w_src ""))) a!118))
      (a!121 (and (< (+ (+ w_p 1) 60) (str.len (ite w_is_hash w_src ""))) a!120))
      (a!123 (and (< (+ (+ w_p 1) 61) (str.len (ite w_is_hash w_src ""))) a!122))
      (a!125 (and (< (+ (+ w_p 1) 62) (str.len (ite w_is_hash w_src ""))) a!124))
      (a!127 (and (< (+ (+ w_p 1) 63) (str.len (ite w_is_hash w_src ""))) a!126)))
(let ((a!128 (ite (not a!125)
                  (+ (+ w_p 1) 62)
                  (ite (not a!127) (+ (+ w_p 1) 63) (+ (+ w_p 1) 64)))))
(let ((a!129 (ite (not a!121)
                  (+ (+ w_p 1) 60)
                  (ite (not a!123) (+ (+ w_p 1) 61) a!128))))
(let ((a!130 (ite (not a!117)
                  (+ (+ w_p 1) 58)
                  (ite (not a!119) (+ (+ w_p 1) 59) a!129))))
(let ((a!131 (ite (not a!113)
                  (+ (+ w_p 1) 56)
                  (ite (not a!115) (+ (+ w_p 1) 57) a!130))))
(let ((a!132 (ite (not a!109)
                  (+ (+ w_p 1) 54)
                  (ite (not a!111) (+ (+ w_p 1) 55) a!131))))
(let ((a!133 (ite (not a!105)
                  (+ (+ w_p 1) 52)
                  (ite (not a!107) (+ (+ w_p 1) 53) a!132))))
(let ((a!134 (ite (not a!101)
                  (+ (+ w_p 1) 50)
                  (ite (not a!103) (+ (+ w_p 1) 51) a!133))))
(let ((a!135 (ite (not a!97)
                  (+ (+ w_p 1) 48)
                  (ite (not a!99) (+ (+ w_p 1) 49) a!134))))
(let ((a!136 (ite (not a!93)
                  (+ (+ w_p 1) 46)
                  (ite (not a!95) (+ (+ w_p 1) 47) a!135))))
(let ((a!137 (ite (not a!89)
                  (+ (+ w_p 1) 44)
                  (ite (not a!91) (+ (+ w_p 1) 45) a!136))))
(let ((a!138 (ite (not a!85)
                  (+ (+ w_p 1) 42)
                  (ite (not a!87) (+ (+ w_p 1) 43) a!137))))
(let ((a!139 (ite (not a!81)
                  (+ (+ w_p 1) 40)
                  (ite (not a!83) (+ (+ w_p 1) 41) a!138))))
(let ((a!140 (ite (not a!77)
                  (+ (+ w_p 1) 38)
                  (ite (not a!79) (+ (+ w_p 1) 39) a!139))))
(let ((a!141 (ite (not a!73)
                  (+ (+ w_p 1) 36)
                  (ite (not a!75) (+ (+ w_p 1) 37) a!140))))
(let ((a!142 (ite (not a!69)
                  (+ (+ w_p 1) 34)
                  (ite (not a!71) (+ (+ w_p 1) 35) a!141))))
(let ((a!143 (ite (not a!65)
                  (+ (+ w_p 1) 32)
                  (ite (not a!67) (+ (+ w_p 1) 33) a!142))))
(let ((a!144 (ite (not a!61)
                  (+ (+ w_p 1) 30)
                  (ite (not a!63) (+ (+ w_p 1) 31) a!143))))
(let ((a!145 (ite (not a!57)
                  (+ (+ w_p 1) 28)
                  (ite (not a!59) (+ (+ w_p 1) 29) a!144))))
(let ((a!146 (ite (not a!53)
                  (+ (+ w_p 1) 26)
                  (ite (not a!55) (+ (+ w_p 1) 27) a!145))))
(let ((a!147 (ite (not a!49)
                  (+ (+ w_p 1) 24)
                  (ite (not a!51) (+ (+ w_p 1) 25) a!146))))
(let ((a!148 (ite (not a!45)
                  (+ (+ w_p 1) 22)
                  (ite (not a!47) (+ (+ w_p 1) 23) a!147))))
(let ((a!149 (ite (not a!41)
                  (+ (+ w_p 1) 20)
                  (ite (not a!43) (+ (+ w_p 1) 21) a!148))))
(let ((a!150 (ite (not a!37)
                  (+ (+ w_p 1) 18)
                  (ite (not a!39) (+ (+ w_p 1) 19) a!149))))
(let ((a!151 (ite (not a!33)
                  (+ (+ w_p 1) 16)
                  (ite (not a!35) (+ (+ w_p 1) 17) a!150))))
(let ((a!152 (ite (not a!29)
                  (+ (+ w_p 1) 14)
                  (ite (not a!31) (+ (+ w_p 1) 15) a!151))))
(let ((a!153 (ite (not a!25)
                  (+ (+ w_p 1) 12)
                  (ite (not a!27) (+ (+ w_p 1) 13) a!152))))
(let ((a!154 (ite (not a!21)
                  (+ (+ w_p 1) 10)
                  (ite (not a!23) (+ (+ w_p 1) 11) a!153))))
(let ((a!155 (ite (not a!17)
                  (+ (+ w_p 1) 8)
                  (ite (not a!19) (+ (+ w_p 1) 9) a!154))))
(let ((a!156 (ite (not a!13)
                  (+ (+ w_p 1) 6)
                  (ite (not a!15) (+ (+ w_p 1) 7) a!155))))
(let ((a!157 (ite (not a!9)
                  (+ (+ w_p 1) 4)
                  (ite (not a!11) (+ (+ w_p 1) 5) a!156))))
(let ((a!158 (ite (not a!5)
                  (+ (+ w_p 1) 2)
                  (ite (not a!7) (+ (+ w_p 1) 3) a!157))))
(let ((a!159 (ite (not a!1) (+ w_p 1) (ite (not a!3) (+ (+ w_p 1) 1) a!158))))
  (= w_he a!159))))))))))))))))))))))))))))))))))))
(assert (let ((a!1 (ite (> w_he (+ w_p 1))
                (str.substr w_src (+ w_p 1) (- (- w_he w_p) 1))
                "")))
  (= w_word a!1)))
(assert (let ((a!1 (and w_is_hash
                (> w_he (+ w_p 1))
                (>= (str.indexof _reg (str.++ "\u{27e6}" w_word "\u{27e7}") 0)
                    0))))
  (= w_word_reg a!1)))
(assert (let ((a!1 (< w_p (str.len (ite (= tk_ph 5) w_src ""))))
      (a!2 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (= tk_ph 5) w_src "") w_p)))
      (a!3 (< (+ w_p 1) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!4 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (= tk_ph 5) w_src "") (+ w_p 1))))
      (a!5 (< (+ w_p 2) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!6 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (= tk_ph 5) w_src "") (+ w_p 2))))
      (a!7 (< (+ w_p 3) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!8 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                         (str.at (ite (= tk_ph 5) w_src "") (+ w_p 3))))
      (a!9 (< (+ w_p 4) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!10 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 4))))
      (a!11 (< (+ w_p 5) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!12 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 5))))
      (a!13 (< (+ w_p 6) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!14 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 6))))
      (a!15 (< (+ w_p 7) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!16 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 7))))
      (a!17 (< (+ w_p 8) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!18 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 8))))
      (a!19 (< (+ w_p 9) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!20 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 9))))
      (a!21 (< (+ w_p 10) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!22 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 10))))
      (a!23 (< (+ w_p 11) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!24 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 11))))
      (a!25 (< (+ w_p 12) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!26 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 12))))
      (a!27 (< (+ w_p 13) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!28 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 13))))
      (a!29 (< (+ w_p 14) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!30 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 14))))
      (a!31 (< (+ w_p 15) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!32 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 15))))
      (a!33 (< (+ w_p 16) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!34 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 16))))
      (a!35 (< (+ w_p 17) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!36 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 17))))
      (a!37 (< (+ w_p 18) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!38 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 18))))
      (a!39 (< (+ w_p 19) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!40 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 19))))
      (a!41 (< (+ w_p 20) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!42 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 20))))
      (a!43 (< (+ w_p 21) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!44 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 21))))
      (a!45 (< (+ w_p 22) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!46 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 22))))
      (a!47 (< (+ w_p 23) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!48 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 23))))
      (a!49 (< (+ w_p 24) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!50 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 24))))
      (a!51 (< (+ w_p 25) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!52 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 25))))
      (a!53 (< (+ w_p 26) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!54 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 26))))
      (a!55 (< (+ w_p 27) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!56 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 27))))
      (a!57 (< (+ w_p 28) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!58 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 28))))
      (a!59 (< (+ w_p 29) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!60 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 29))))
      (a!61 (< (+ w_p 30) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!62 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 30))))
      (a!63 (< (+ w_p 31) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!64 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 31))))
      (a!65 (< (+ w_p 32) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!66 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 32))))
      (a!67 (< (+ w_p 33) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!68 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 33))))
      (a!69 (< (+ w_p 34) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!70 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 34))))
      (a!71 (< (+ w_p 35) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!72 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 35))))
      (a!73 (< (+ w_p 36) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!74 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 36))))
      (a!75 (< (+ w_p 37) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!76 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 37))))
      (a!77 (< (+ w_p 38) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!78 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 38))))
      (a!79 (< (+ w_p 39) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!80 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 39))))
      (a!81 (< (+ w_p 40) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!82 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 40))))
      (a!83 (< (+ w_p 41) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!84 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 41))))
      (a!85 (< (+ w_p 42) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!86 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 42))))
      (a!87 (< (+ w_p 43) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!88 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 43))))
      (a!89 (< (+ w_p 44) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!90 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 44))))
      (a!91 (< (+ w_p 45) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!92 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 45))))
      (a!93 (< (+ w_p 46) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!94 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 46))))
      (a!95 (< (+ w_p 47) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!96 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 47))))
      (a!97 (< (+ w_p 48) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!98 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                          (str.at (ite (= tk_ph 5) w_src "") (+ w_p 48))))
      (a!99 (< (+ w_p 49) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!100 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 49))))
      (a!101 (< (+ w_p 50) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!102 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 50))))
      (a!103 (< (+ w_p 51) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!104 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 51))))
      (a!105 (< (+ w_p 52) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!106 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 52))))
      (a!107 (< (+ w_p 53) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!108 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 53))))
      (a!109 (< (+ w_p 54) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!110 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 54))))
      (a!111 (< (+ w_p 55) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!112 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 55))))
      (a!113 (< (+ w_p 56) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!114 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 56))))
      (a!115 (< (+ w_p 57) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!116 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 57))))
      (a!117 (< (+ w_p 58) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!118 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 58))))
      (a!119 (< (+ w_p 59) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!120 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 59))))
      (a!121 (< (+ w_p 60) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!122 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 60))))
      (a!123 (< (+ w_p 61) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!124 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 61))))
      (a!125 (< (+ w_p 62) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!126 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 62))))
      (a!127 (< (+ w_p 63) (str.len (ite (= tk_ph 5) w_src ""))))
      (a!128 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                           (str.at (ite (= tk_ph 5) w_src "") (+ w_p 63)))))
(let ((a!129 (ite (not (and a!125 a!126))
                  (+ w_p 62)
                  (ite (not (and a!127 a!128)) (+ w_p 63) (+ w_p 64)))))
(let ((a!130 (ite (not (and a!121 a!122))
                  (+ w_p 60)
                  (ite (not (and a!123 a!124)) (+ w_p 61) a!129))))
(let ((a!131 (ite (not (and a!117 a!118))
                  (+ w_p 58)
                  (ite (not (and a!119 a!120)) (+ w_p 59) a!130))))
(let ((a!132 (ite (not (and a!113 a!114))
                  (+ w_p 56)
                  (ite (not (and a!115 a!116)) (+ w_p 57) a!131))))
(let ((a!133 (ite (not (and a!109 a!110))
                  (+ w_p 54)
                  (ite (not (and a!111 a!112)) (+ w_p 55) a!132))))
(let ((a!134 (ite (not (and a!105 a!106))
                  (+ w_p 52)
                  (ite (not (and a!107 a!108)) (+ w_p 53) a!133))))
(let ((a!135 (ite (not (and a!101 a!102))
                  (+ w_p 50)
                  (ite (not (and a!103 a!104)) (+ w_p 51) a!134))))
(let ((a!136 (ite (not (and a!97 a!98))
                  (+ w_p 48)
                  (ite (not (and a!99 a!100)) (+ w_p 49) a!135))))
(let ((a!137 (ite (not (and a!93 a!94))
                  (+ w_p 46)
                  (ite (not (and a!95 a!96)) (+ w_p 47) a!136))))
(let ((a!138 (ite (not (and a!89 a!90))
                  (+ w_p 44)
                  (ite (not (and a!91 a!92)) (+ w_p 45) a!137))))
(let ((a!139 (ite (not (and a!85 a!86))
                  (+ w_p 42)
                  (ite (not (and a!87 a!88)) (+ w_p 43) a!138))))
(let ((a!140 (ite (not (and a!81 a!82))
                  (+ w_p 40)
                  (ite (not (and a!83 a!84)) (+ w_p 41) a!139))))
(let ((a!141 (ite (not (and a!77 a!78))
                  (+ w_p 38)
                  (ite (not (and a!79 a!80)) (+ w_p 39) a!140))))
(let ((a!142 (ite (not (and a!73 a!74))
                  (+ w_p 36)
                  (ite (not (and a!75 a!76)) (+ w_p 37) a!141))))
(let ((a!143 (ite (not (and a!69 a!70))
                  (+ w_p 34)
                  (ite (not (and a!71 a!72)) (+ w_p 35) a!142))))
(let ((a!144 (ite (not (and a!65 a!66))
                  (+ w_p 32)
                  (ite (not (and a!67 a!68)) (+ w_p 33) a!143))))
(let ((a!145 (ite (not (and a!61 a!62))
                  (+ w_p 30)
                  (ite (not (and a!63 a!64)) (+ w_p 31) a!144))))
(let ((a!146 (ite (not (and a!57 a!58))
                  (+ w_p 28)
                  (ite (not (and a!59 a!60)) (+ w_p 29) a!145))))
(let ((a!147 (ite (not (and a!53 a!54))
                  (+ w_p 26)
                  (ite (not (and a!55 a!56)) (+ w_p 27) a!146))))
(let ((a!148 (ite (not (and a!49 a!50))
                  (+ w_p 24)
                  (ite (not (and a!51 a!52)) (+ w_p 25) a!147))))
(let ((a!149 (ite (not (and a!45 a!46))
                  (+ w_p 22)
                  (ite (not (and a!47 a!48)) (+ w_p 23) a!148))))
(let ((a!150 (ite (not (and a!41 a!42))
                  (+ w_p 20)
                  (ite (not (and a!43 a!44)) (+ w_p 21) a!149))))
(let ((a!151 (ite (not (and a!37 a!38))
                  (+ w_p 18)
                  (ite (not (and a!39 a!40)) (+ w_p 19) a!150))))
(let ((a!152 (ite (not (and a!33 a!34))
                  (+ w_p 16)
                  (ite (not (and a!35 a!36)) (+ w_p 17) a!151))))
(let ((a!153 (ite (not (and a!29 a!30))
                  (+ w_p 14)
                  (ite (not (and a!31 a!32)) (+ w_p 15) a!152))))
(let ((a!154 (ite (not (and a!25 a!26))
                  (+ w_p 12)
                  (ite (not (and a!27 a!28)) (+ w_p 13) a!153))))
(let ((a!155 (ite (not (and a!21 a!22))
                  (+ w_p 10)
                  (ite (not (and a!23 a!24)) (+ w_p 11) a!154))))
(let ((a!156 (ite (not (and a!17 a!18))
                  (+ w_p 8)
                  (ite (not (and a!19 a!20)) (+ w_p 9) a!155))))
(let ((a!157 (ite (not (and a!13 a!14))
                  (+ w_p 6)
                  (ite (not (and a!15 a!16)) (+ w_p 7) a!156))))
(let ((a!158 (ite (not (and a!9 a!10))
                  (+ w_p 4)
                  (ite (not (and a!11 a!12)) (+ w_p 5) a!157))))
(let ((a!159 (ite (not (and a!5 a!6))
                  (+ w_p 2)
                  (ite (not (and a!7 a!8)) (+ w_p 3) a!158))))
(let ((a!160 (ite (not (and a!1 a!2))
                  w_p
                  (ite (not (and a!3 a!4)) (+ w_p 1) a!159))))
  (= w_we a!160)))))))))))))))))))))))))))))))))))
(assert (= w_is_ident (and (= tk_ph 5) (> w_we w_p))))
(assert (= w_tok (ite w_is_ident (str.substr w_src w_p (- w_we w_p)) "")))
(assert (= w_followed_br (and w_is_ident (= (str.at w_src w_we) "["))))
(assert (let ((a!1 (ite (= (str.at w_tok 0) "_")
                (str.substr w_tok 1 (- (str.len w_tok) 1))
                w_tok)))
  (= w_base a!1)))
(assert (let ((a!1 (and w_followed_br
                (>= (str.indexof _reg (str.++ "\u{27e6}" w_base "\u{27e7}") 0)
                    0))))
  (= w_base_reg a!1)))
(assert (= w_cb (ite w_base_reg (str.indexof w_src "]" (+ w_we 1)) (- 0 1))))
(assert (let ((a!1 (ite (> w_cb w_we)
                (str.substr w_src (+ w_we 1) (- (- w_cb w_we) 1))
                "")))
  (= w_inner a!1)))
(assert (= LsIdxEval__ie_t__call16 LsStripWs__sw24__call17))
(assert (= LsStripWs__sw24__call17
   (str.++ LsStripWs__sw_keep0__call17
           LsStripWs__sw_keep1__call17
           LsStripWs__sw_keep2__call17
           LsStripWs__sw_keep3__call17
           LsStripWs__sw_keep4__call17
           LsStripWs__sw_keep5__call17
           LsStripWs__sw_keep6__call17
           LsStripWs__sw_keep7__call17
           LsStripWs__sw_keep8__call17
           LsStripWs__sw_keep9__call17
           LsStripWs__sw_keep10__call17
           LsStripWs__sw_keep11__call17
           LsStripWs__sw_keep12__call17
           LsStripWs__sw_keep13__call17
           LsStripWs__sw_keep14__call17
           LsStripWs__sw_keep15__call17
           LsStripWs__sw_keep16__call17
           LsStripWs__sw_keep17__call17
           LsStripWs__sw_keep18__call17
           LsStripWs__sw_keep19__call17
           LsStripWs__sw_keep20__call17
           LsStripWs__sw_keep21__call17
           LsStripWs__sw_keep22__call17
           LsStripWs__sw_keep23__call17)))
(assert (let ((a!1 (not (or (= (str.at w_inner 0) " ") (= (str.at w_inner 0) "\u{9}")))))
(let ((a!2 (ite (and (< 0 (str.len w_inner)) a!1) (str.at w_inner 0) "")))
  (= LsStripWs__sw_keep0__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 1) " ") (= (str.at w_inner 1) "\u{9}")))))
(let ((a!2 (ite (and (< 1 (str.len w_inner)) a!1) (str.at w_inner 1) "")))
  (= LsStripWs__sw_keep1__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 2) " ") (= (str.at w_inner 2) "\u{9}")))))
(let ((a!2 (ite (and (< 2 (str.len w_inner)) a!1) (str.at w_inner 2) "")))
  (= LsStripWs__sw_keep2__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 3) " ") (= (str.at w_inner 3) "\u{9}")))))
(let ((a!2 (ite (and (< 3 (str.len w_inner)) a!1) (str.at w_inner 3) "")))
  (= LsStripWs__sw_keep3__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 4) " ") (= (str.at w_inner 4) "\u{9}")))))
(let ((a!2 (ite (and (< 4 (str.len w_inner)) a!1) (str.at w_inner 4) "")))
  (= LsStripWs__sw_keep4__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 5) " ") (= (str.at w_inner 5) "\u{9}")))))
(let ((a!2 (ite (and (< 5 (str.len w_inner)) a!1) (str.at w_inner 5) "")))
  (= LsStripWs__sw_keep5__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 6) " ") (= (str.at w_inner 6) "\u{9}")))))
(let ((a!2 (ite (and (< 6 (str.len w_inner)) a!1) (str.at w_inner 6) "")))
  (= LsStripWs__sw_keep6__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 7) " ") (= (str.at w_inner 7) "\u{9}")))))
(let ((a!2 (ite (and (< 7 (str.len w_inner)) a!1) (str.at w_inner 7) "")))
  (= LsStripWs__sw_keep7__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 8) " ") (= (str.at w_inner 8) "\u{9}")))))
(let ((a!2 (ite (and (< 8 (str.len w_inner)) a!1) (str.at w_inner 8) "")))
  (= LsStripWs__sw_keep8__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 9) " ") (= (str.at w_inner 9) "\u{9}")))))
(let ((a!2 (ite (and (< 9 (str.len w_inner)) a!1) (str.at w_inner 9) "")))
  (= LsStripWs__sw_keep9__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 10) " ") (= (str.at w_inner 10) "\u{9}")))))
(let ((a!2 (ite (and (< 10 (str.len w_inner)) a!1) (str.at w_inner 10) "")))
  (= LsStripWs__sw_keep10__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 11) " ") (= (str.at w_inner 11) "\u{9}")))))
(let ((a!2 (ite (and (< 11 (str.len w_inner)) a!1) (str.at w_inner 11) "")))
  (= LsStripWs__sw_keep11__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 12) " ") (= (str.at w_inner 12) "\u{9}")))))
(let ((a!2 (ite (and (< 12 (str.len w_inner)) a!1) (str.at w_inner 12) "")))
  (= LsStripWs__sw_keep12__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 13) " ") (= (str.at w_inner 13) "\u{9}")))))
(let ((a!2 (ite (and (< 13 (str.len w_inner)) a!1) (str.at w_inner 13) "")))
  (= LsStripWs__sw_keep13__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 14) " ") (= (str.at w_inner 14) "\u{9}")))))
(let ((a!2 (ite (and (< 14 (str.len w_inner)) a!1) (str.at w_inner 14) "")))
  (= LsStripWs__sw_keep14__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 15) " ") (= (str.at w_inner 15) "\u{9}")))))
(let ((a!2 (ite (and (< 15 (str.len w_inner)) a!1) (str.at w_inner 15) "")))
  (= LsStripWs__sw_keep15__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 16) " ") (= (str.at w_inner 16) "\u{9}")))))
(let ((a!2 (ite (and (< 16 (str.len w_inner)) a!1) (str.at w_inner 16) "")))
  (= LsStripWs__sw_keep16__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 17) " ") (= (str.at w_inner 17) "\u{9}")))))
(let ((a!2 (ite (and (< 17 (str.len w_inner)) a!1) (str.at w_inner 17) "")))
  (= LsStripWs__sw_keep17__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 18) " ") (= (str.at w_inner 18) "\u{9}")))))
(let ((a!2 (ite (and (< 18 (str.len w_inner)) a!1) (str.at w_inner 18) "")))
  (= LsStripWs__sw_keep18__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 19) " ") (= (str.at w_inner 19) "\u{9}")))))
(let ((a!2 (ite (and (< 19 (str.len w_inner)) a!1) (str.at w_inner 19) "")))
  (= LsStripWs__sw_keep19__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 20) " ") (= (str.at w_inner 20) "\u{9}")))))
(let ((a!2 (ite (and (< 20 (str.len w_inner)) a!1) (str.at w_inner 20) "")))
  (= LsStripWs__sw_keep20__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 21) " ") (= (str.at w_inner 21) "\u{9}")))))
(let ((a!2 (ite (and (< 21 (str.len w_inner)) a!1) (str.at w_inner 21) "")))
  (= LsStripWs__sw_keep21__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 22) " ") (= (str.at w_inner 22) "\u{9}")))))
(let ((a!2 (ite (and (< 22 (str.len w_inner)) a!1) (str.at w_inner 22) "")))
  (= LsStripWs__sw_keep22__call17 a!2))))
(assert (let ((a!1 (not (or (= (str.at w_inner 23) " ") (= (str.at w_inner 23) "\u{9}")))))
(let ((a!2 (ite (and (< 23 (str.len w_inner)) a!1) (str.at w_inner 23) "")))
  (= LsStripWs__sw_keep23__call17 a!2))))
(assert (= LsIdxEval__ie_valid_chars__call16 (< LsOnlyIdxChars__oic_bad__call18 0)))
(assert (let ((a!1 (ite LsOnlyIdxChars__oic_b21__call18
                21
                (ite LsOnlyIdxChars__oic_b22__call18
                     22
                     (ite LsOnlyIdxChars__oic_b23__call18 23 (- 0 1))))))
(let ((a!2 (ite LsOnlyIdxChars__oic_b17__call18
                17
                (ite LsOnlyIdxChars__oic_b18__call18
                     18
                     (ite LsOnlyIdxChars__oic_b19__call18
                          19
                          (ite LsOnlyIdxChars__oic_b20__call18 20 a!1))))))
(let ((a!3 (ite LsOnlyIdxChars__oic_b13__call18
                13
                (ite LsOnlyIdxChars__oic_b14__call18
                     14
                     (ite LsOnlyIdxChars__oic_b15__call18
                          15
                          (ite LsOnlyIdxChars__oic_b16__call18 16 a!2))))))
(let ((a!4 (ite LsOnlyIdxChars__oic_b9__call18
                9
                (ite LsOnlyIdxChars__oic_b10__call18
                     10
                     (ite LsOnlyIdxChars__oic_b11__call18
                          11
                          (ite LsOnlyIdxChars__oic_b12__call18 12 a!3))))))
(let ((a!5 (ite LsOnlyIdxChars__oic_b5__call18
                5
                (ite LsOnlyIdxChars__oic_b6__call18
                     6
                     (ite LsOnlyIdxChars__oic_b7__call18
                          7
                          (ite LsOnlyIdxChars__oic_b8__call18 8 a!4))))))
(let ((a!6 (ite LsOnlyIdxChars__oic_b1__call18
                1
                (ite LsOnlyIdxChars__oic_b2__call18
                     2
                     (ite LsOnlyIdxChars__oic_b3__call18
                          3
                          (ite LsOnlyIdxChars__oic_b4__call18 4 a!5))))))
  (= LsOnlyIdxChars__oic_bad__call18 (ite LsOnlyIdxChars__oic_b0__call18 0 a!6)))))))))
(assert (let ((a!1 (and (< 0 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 0))))))
  (= LsOnlyIdxChars__oic_b0__call18 a!1)))
(assert (let ((a!1 (and (< 1 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 1))))))
  (= LsOnlyIdxChars__oic_b1__call18 a!1)))
(assert (let ((a!1 (and (< 2 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 2))))))
  (= LsOnlyIdxChars__oic_b2__call18 a!1)))
(assert (let ((a!1 (and (< 3 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 3))))))
  (= LsOnlyIdxChars__oic_b3__call18 a!1)))
(assert (let ((a!1 (and (< 4 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 4))))))
  (= LsOnlyIdxChars__oic_b4__call18 a!1)))
(assert (let ((a!1 (and (< 5 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 5))))))
  (= LsOnlyIdxChars__oic_b5__call18 a!1)))
(assert (let ((a!1 (and (< 6 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 6))))))
  (= LsOnlyIdxChars__oic_b6__call18 a!1)))
(assert (let ((a!1 (and (< 7 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 7))))))
  (= LsOnlyIdxChars__oic_b7__call18 a!1)))
(assert (let ((a!1 (and (< 8 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 8))))))
  (= LsOnlyIdxChars__oic_b8__call18 a!1)))
(assert (let ((a!1 (and (< 9 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 9))))))
  (= LsOnlyIdxChars__oic_b9__call18 a!1)))
(assert (let ((a!1 (and (< 10 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 10))))))
  (= LsOnlyIdxChars__oic_b10__call18 a!1)))
(assert (let ((a!1 (and (< 11 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 11))))))
  (= LsOnlyIdxChars__oic_b11__call18 a!1)))
(assert (let ((a!1 (and (< 12 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 12))))))
  (= LsOnlyIdxChars__oic_b12__call18 a!1)))
(assert (let ((a!1 (and (< 13 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 13))))))
  (= LsOnlyIdxChars__oic_b13__call18 a!1)))
(assert (let ((a!1 (and (< 14 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 14))))))
  (= LsOnlyIdxChars__oic_b14__call18 a!1)))
(assert (let ((a!1 (and (< 15 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 15))))))
  (= LsOnlyIdxChars__oic_b15__call18 a!1)))
(assert (let ((a!1 (and (< 16 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 16))))))
  (= LsOnlyIdxChars__oic_b16__call18 a!1)))
(assert (let ((a!1 (and (< 17 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 17))))))
  (= LsOnlyIdxChars__oic_b17__call18 a!1)))
(assert (let ((a!1 (and (< 18 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 18))))))
  (= LsOnlyIdxChars__oic_b18__call18 a!1)))
(assert (let ((a!1 (and (< 19 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 19))))))
  (= LsOnlyIdxChars__oic_b19__call18 a!1)))
(assert (let ((a!1 (and (< 20 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 20))))))
  (= LsOnlyIdxChars__oic_b20__call18 a!1)))
(assert (let ((a!1 (and (< 21 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 21))))))
  (= LsOnlyIdxChars__oic_b21__call18 a!1)))
(assert (let ((a!1 (and (< 22 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 22))))))
  (= LsOnlyIdxChars__oic_b22__call18 a!1)))
(assert (let ((a!1 (and (< 23 (str.len LsIdxEval__ie_t__call16))
                (not (str.contains "0123456789+*-"
                                   (str.at LsIdxEval__ie_t__call16 23))))))
  (= LsOnlyIdxChars__oic_b23__call18 a!1)))
(assert (= LsIdxEval__ie_starts_digit__call16
   (and (>= (str.len LsIdxEval__ie_t__call16) 1)
        (str.contains "0123456789" (str.at LsIdxEval__ie_t__call16 0)))))
(assert (let ((a!1 (not (and (< 0 (str.len LsIdxEval__ie_t__call16))
                     (str.contains "0123456789"
                                   (str.at LsIdxEval__ie_t__call16 0)))))
      (a!2 (and (< (+ 0 1) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 1)))))
      (a!3 (and (< (+ 0 2) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 2)))))
      (a!4 (and (< (+ 0 3) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 3)))))
      (a!5 (and (< (+ 0 4) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 4)))))
      (a!6 (and (< (+ 0 5) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 5)))))
      (a!7 (and (< (+ 0 6) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 6)))))
      (a!8 (and (< (+ 0 7) (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 7))))))
(let ((a!9 (ite (not a!6)
                (+ 0 5)
                (ite (not a!7) (+ 0 6) (ite (not a!8) (+ 0 7) (+ 0 8))))))
(let ((a!10 (ite (not a!3)
                 (+ 0 2)
                 (ite (not a!4) (+ 0 3) (ite (not a!5) (+ 0 4) a!9)))))
  (= LsIdxEval__ie_ne0__call16 (ite a!1 0 (ite (not a!2) (+ 0 1) a!10)))))))
(assert (= LsIdxEval__ie_op0__call16
   (ite (< LsIdxEval__ie_ne0__call16 (str.len LsIdxEval__ie_t__call16))
        (str.at LsIdxEval__ie_t__call16 LsIdxEval__ie_ne0__call16)
        "")))
(assert (= LsIdxEval__ie_s1__call16 (+ LsIdxEval__ie_ne0__call16 1)))
(assert (let ((a!1 (not (and (< LsIdxEval__ie_s1__call16
                        (str.len LsIdxEval__ie_t__call16))
                     (str.contains "0123456789"
                                   (str.at LsIdxEval__ie_t__call16
                                           LsIdxEval__ie_s1__call16)))))
      (a!2 (and (< (+ LsIdxEval__ie_s1__call16 1)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 1)))))
      (a!3 (and (< (+ LsIdxEval__ie_s1__call16 2)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 2)))))
      (a!4 (and (< (+ LsIdxEval__ie_s1__call16 3)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 3)))))
      (a!5 (and (< (+ LsIdxEval__ie_s1__call16 4)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 4)))))
      (a!6 (and (< (+ LsIdxEval__ie_s1__call16 5)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 5)))))
      (a!7 (and (< (+ LsIdxEval__ie_s1__call16 6)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 6)))))
      (a!8 (and (< (+ LsIdxEval__ie_s1__call16 7)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 7))))))
(let ((a!9 (ite (not a!6)
                (+ LsIdxEval__ie_s1__call16 5)
                (ite (not a!7)
                     (+ LsIdxEval__ie_s1__call16 6)
                     (ite (not a!8)
                          (+ LsIdxEval__ie_s1__call16 7)
                          (+ LsIdxEval__ie_s1__call16 8))))))
(let ((a!10 (ite (not a!3)
                 (+ LsIdxEval__ie_s1__call16 2)
                 (ite (not a!4)
                      (+ LsIdxEval__ie_s1__call16 3)
                      (ite (not a!5) (+ LsIdxEval__ie_s1__call16 4) a!9)))))
  (= LsIdxEval__ie_ne1__call16
     (ite a!1
          LsIdxEval__ie_s1__call16
          (ite (not a!2) (+ LsIdxEval__ie_s1__call16 1) a!10)))))))
(assert (= LsIdxEval__ie_op1__call16
   (ite (< LsIdxEval__ie_ne1__call16 (str.len LsIdxEval__ie_t__call16))
        (str.at LsIdxEval__ie_t__call16 LsIdxEval__ie_ne1__call16)
        "")))
(assert (= LsIdxEval__ie_s2__call16 (+ LsIdxEval__ie_ne1__call16 1)))
(assert (let ((a!1 (not (and (< LsIdxEval__ie_s2__call16
                        (str.len LsIdxEval__ie_t__call16))
                     (str.contains "0123456789"
                                   (str.at LsIdxEval__ie_t__call16
                                           LsIdxEval__ie_s2__call16)))))
      (a!2 (and (< (+ LsIdxEval__ie_s2__call16 1)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 1)))))
      (a!3 (and (< (+ LsIdxEval__ie_s2__call16 2)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 2)))))
      (a!4 (and (< (+ LsIdxEval__ie_s2__call16 3)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 3)))))
      (a!5 (and (< (+ LsIdxEval__ie_s2__call16 4)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 4)))))
      (a!6 (and (< (+ LsIdxEval__ie_s2__call16 5)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 5)))))
      (a!7 (and (< (+ LsIdxEval__ie_s2__call16 6)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 6)))))
      (a!8 (and (< (+ LsIdxEval__ie_s2__call16 7)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 7))))))
(let ((a!9 (ite (not a!6)
                (+ LsIdxEval__ie_s2__call16 5)
                (ite (not a!7)
                     (+ LsIdxEval__ie_s2__call16 6)
                     (ite (not a!8)
                          (+ LsIdxEval__ie_s2__call16 7)
                          (+ LsIdxEval__ie_s2__call16 8))))))
(let ((a!10 (ite (not a!3)
                 (+ LsIdxEval__ie_s2__call16 2)
                 (ite (not a!4)
                      (+ LsIdxEval__ie_s2__call16 3)
                      (ite (not a!5) (+ LsIdxEval__ie_s2__call16 4) a!9)))))
  (= LsIdxEval__ie_ne2__call16
     (ite a!1
          LsIdxEval__ie_s2__call16
          (ite (not a!2) (+ LsIdxEval__ie_s2__call16 1) a!10)))))))
(assert (= LsIdxEval__ie_op2__call16
   (ite (< LsIdxEval__ie_ne2__call16 (str.len LsIdxEval__ie_t__call16))
        (str.at LsIdxEval__ie_t__call16 LsIdxEval__ie_ne2__call16)
        "")))
(assert (= LsIdxEval__ie_s3__call16 (+ LsIdxEval__ie_ne2__call16 1)))
(assert (let ((a!1 (not (and (< LsIdxEval__ie_s3__call16
                        (str.len LsIdxEval__ie_t__call16))
                     (str.contains "0123456789"
                                   (str.at LsIdxEval__ie_t__call16
                                           LsIdxEval__ie_s3__call16)))))
      (a!2 (and (< (+ LsIdxEval__ie_s3__call16 1)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 1)))))
      (a!3 (and (< (+ LsIdxEval__ie_s3__call16 2)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 2)))))
      (a!4 (and (< (+ LsIdxEval__ie_s3__call16 3)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 3)))))
      (a!5 (and (< (+ LsIdxEval__ie_s3__call16 4)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 4)))))
      (a!6 (and (< (+ LsIdxEval__ie_s3__call16 5)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 5)))))
      (a!7 (and (< (+ LsIdxEval__ie_s3__call16 6)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 6)))))
      (a!8 (and (< (+ LsIdxEval__ie_s3__call16 7)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 7))))))
(let ((a!9 (ite (not a!6)
                (+ LsIdxEval__ie_s3__call16 5)
                (ite (not a!7)
                     (+ LsIdxEval__ie_s3__call16 6)
                     (ite (not a!8)
                          (+ LsIdxEval__ie_s3__call16 7)
                          (+ LsIdxEval__ie_s3__call16 8))))))
(let ((a!10 (ite (not a!3)
                 (+ LsIdxEval__ie_s3__call16 2)
                 (ite (not a!4)
                      (+ LsIdxEval__ie_s3__call16 3)
                      (ite (not a!5) (+ LsIdxEval__ie_s3__call16 4) a!9)))))
  (= LsIdxEval__ie_ne3__call16
     (ite a!1
          LsIdxEval__ie_s3__call16
          (ite (not a!2) (+ LsIdxEval__ie_s3__call16 1) a!10)))))))
(assert (= LsIdxEval__ie_op3__call16
   (ite (< LsIdxEval__ie_ne3__call16 (str.len LsIdxEval__ie_t__call16))
        (str.at LsIdxEval__ie_t__call16 LsIdxEval__ie_ne3__call16)
        "")))
(assert (= LsIdxEval__ie_s4__call16 (+ LsIdxEval__ie_ne3__call16 1)))
(assert (let ((a!1 (not (and (< LsIdxEval__ie_s4__call16
                        (str.len LsIdxEval__ie_t__call16))
                     (str.contains "0123456789"
                                   (str.at LsIdxEval__ie_t__call16
                                           LsIdxEval__ie_s4__call16)))))
      (a!2 (and (< (+ LsIdxEval__ie_s4__call16 1)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 1)))))
      (a!3 (and (< (+ LsIdxEval__ie_s4__call16 2)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 2)))))
      (a!4 (and (< (+ LsIdxEval__ie_s4__call16 3)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 3)))))
      (a!5 (and (< (+ LsIdxEval__ie_s4__call16 4)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 4)))))
      (a!6 (and (< (+ LsIdxEval__ie_s4__call16 5)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 5)))))
      (a!7 (and (< (+ LsIdxEval__ie_s4__call16 6)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 6)))))
      (a!8 (and (< (+ LsIdxEval__ie_s4__call16 7)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 7))))))
(let ((a!9 (ite (not a!6)
                (+ LsIdxEval__ie_s4__call16 5)
                (ite (not a!7)
                     (+ LsIdxEval__ie_s4__call16 6)
                     (ite (not a!8)
                          (+ LsIdxEval__ie_s4__call16 7)
                          (+ LsIdxEval__ie_s4__call16 8))))))
(let ((a!10 (ite (not a!3)
                 (+ LsIdxEval__ie_s4__call16 2)
                 (ite (not a!4)
                      (+ LsIdxEval__ie_s4__call16 3)
                      (ite (not a!5) (+ LsIdxEval__ie_s4__call16 4) a!9)))))
  (= LsIdxEval__ie_ne4__call16
     (ite a!1
          LsIdxEval__ie_s4__call16
          (ite (not a!2) (+ LsIdxEval__ie_s4__call16 1) a!10)))))))
(assert (= LsIdxEval__ie_op4__call16
   (ite (< LsIdxEval__ie_ne4__call16 (str.len LsIdxEval__ie_t__call16))
        (str.at LsIdxEval__ie_t__call16 LsIdxEval__ie_ne4__call16)
        "")))
(assert (= LsIdxEval__ie_s5__call16 (+ LsIdxEval__ie_ne4__call16 1)))
(assert (let ((a!1 (not (and (< LsIdxEval__ie_s5__call16
                        (str.len LsIdxEval__ie_t__call16))
                     (str.contains "0123456789"
                                   (str.at LsIdxEval__ie_t__call16
                                           LsIdxEval__ie_s5__call16)))))
      (a!2 (and (< (+ LsIdxEval__ie_s5__call16 1)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 1)))))
      (a!3 (and (< (+ LsIdxEval__ie_s5__call16 2)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 2)))))
      (a!4 (and (< (+ LsIdxEval__ie_s5__call16 3)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 3)))))
      (a!5 (and (< (+ LsIdxEval__ie_s5__call16 4)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 4)))))
      (a!6 (and (< (+ LsIdxEval__ie_s5__call16 5)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 5)))))
      (a!7 (and (< (+ LsIdxEval__ie_s5__call16 6)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 6)))))
      (a!8 (and (< (+ LsIdxEval__ie_s5__call16 7)
                   (str.len LsIdxEval__ie_t__call16))
                (str.contains "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 7))))))
(let ((a!9 (ite (not a!6)
                (+ LsIdxEval__ie_s5__call16 5)
                (ite (not a!7)
                     (+ LsIdxEval__ie_s5__call16 6)
                     (ite (not a!8)
                          (+ LsIdxEval__ie_s5__call16 7)
                          (+ LsIdxEval__ie_s5__call16 8))))))
(let ((a!10 (ite (not a!3)
                 (+ LsIdxEval__ie_s5__call16 2)
                 (ite (not a!4)
                      (+ LsIdxEval__ie_s5__call16 3)
                      (ite (not a!5) (+ LsIdxEval__ie_s5__call16 4) a!9)))))
  (= LsIdxEval__ie_ne5__call16
     (ite a!1
          LsIdxEval__ie_s5__call16
          (ite (not a!2) (+ LsIdxEval__ie_s5__call16 1) a!10)))))))
(assert (let ((a!1 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                           0)
              10))
      (a!3 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                           0)
              100))
      (a!4 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 1))
                           0)
              10))
      (a!6 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                           0)
              1000))
      (a!7 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 1))
                           0)
              100))
      (a!8 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 2))
                           0)
              10))
      (a!10 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                            0)
               10000))
      (a!11 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 1))
                            0)
               1000))
      (a!12 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 2))
                            0)
               100))
      (a!13 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 3))
                            0)
               10))
      (a!15 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                            0)
               100000))
      (a!16 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 1))
                            0)
               10000))
      (a!17 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 2))
                            0)
               1000))
      (a!18 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 3))
                            0)
               100))
      (a!19 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 4))
                            0)
               10))
      (a!21 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                            0)
               1000000))
      (a!22 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 1))
                            0)
               100000))
      (a!23 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 2))
                            0)
               10000))
      (a!24 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 3))
                            0)
               1000))
      (a!25 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 4))
                            0)
               100))
      (a!26 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 5))
                            0)
               10)))
(let ((a!2 (+ a!1
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 1))
                           0)))
      (a!5 (+ a!3
              a!4
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 2))
                           0)))
      (a!9 (+ a!6
              a!7
              a!8
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16 (+ 0 3))
                           0)))
      (a!14 (+ a!10
               a!11
               a!12
               a!13
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 4))
                            0)))
      (a!20 (+ a!15
               a!16
               a!17
               a!18
               a!19
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 5))
                            0)))
      (a!27 (+ a!21
               a!22
               a!23
               a!24
               a!25
               a!26
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16 (+ 0 6))
                            0))))
(let ((a!28 (ite (= LsIdxEval__ie_ne0__call16 5)
                 a!14
                 (ite (= LsIdxEval__ie_ne0__call16 6)
                      a!20
                      (ite (= LsIdxEval__ie_ne0__call16 7) a!27 (- 0 1))))))
(let ((a!29 (ite (= LsIdxEval__ie_ne0__call16 2)
                 a!2
                 (ite (= LsIdxEval__ie_ne0__call16 3)
                      a!5
                      (ite (= LsIdxEval__ie_ne0__call16 4) a!9 a!28)))))
(let ((a!30 (ite (= LsIdxEval__ie_ne0__call16 1)
                 (str.indexof "0123456789"
                              (str.at LsIdxEval__ie_t__call16 (+ 0 0))
                              0)
                 a!29)))
  (= LsIdxEval__ie_n0__call16 a!30)))))))
(assert (let ((a!1 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 0))
                           0)
              10))
      (a!3 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 0))
                           0)
              100))
      (a!4 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 1))
                           0)
              10))
      (a!6 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 0))
                           0)
              1000))
      (a!7 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 1))
                           0)
              100))
      (a!8 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 2))
                           0)
              10))
      (a!10 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 0))
                            0)
               10000))
      (a!11 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 1))
                            0)
               1000))
      (a!12 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 2))
                            0)
               100))
      (a!13 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 3))
                            0)
               10))
      (a!15 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 0))
                            0)
               100000))
      (a!16 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 1))
                            0)
               10000))
      (a!17 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 2))
                            0)
               1000))
      (a!18 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 3))
                            0)
               100))
      (a!19 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 4))
                            0)
               10))
      (a!21 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 0))
                            0)
               1000000))
      (a!22 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 1))
                            0)
               100000))
      (a!23 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 2))
                            0)
               10000))
      (a!24 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 3))
                            0)
               1000))
      (a!25 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 4))
                            0)
               100))
      (a!26 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 5))
                            0)
               10)))
(let ((a!2 (+ a!1
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 1))
                           0)))
      (a!5 (+ a!3
              a!4
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 2))
                           0)))
      (a!9 (+ a!6
              a!7
              a!8
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s1__call16 3))
                           0)))
      (a!14 (+ a!10
               a!11
               a!12
               a!13
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 4))
                            0)))
      (a!20 (+ a!15
               a!16
               a!17
               a!18
               a!19
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 5))
                            0)))
      (a!27 (+ a!21
               a!22
               a!23
               a!24
               a!25
               a!26
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s1__call16 6))
                            0))))
(let ((a!28 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16) 6)
                 a!20
                 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16)
                         7)
                      a!27
                      (- 0 1)))))
(let ((a!29 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16) 4)
                 a!9
                 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16)
                         5)
                      a!14
                      a!28))))
(let ((a!30 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16) 2)
                 a!2
                 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16)
                         3)
                      a!5
                      a!29))))
(let ((a!31 (ite (= (- LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16) 1)
                 (str.indexof "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s1__call16 0))
                              0)
                 a!30)))
  (= LsIdxEval__ie_n1__call16 a!31))))))))
(assert (let ((a!1 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 0))
                           0)
              10))
      (a!3 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 0))
                           0)
              100))
      (a!4 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 1))
                           0)
              10))
      (a!6 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 0))
                           0)
              1000))
      (a!7 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 1))
                           0)
              100))
      (a!8 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 2))
                           0)
              10))
      (a!10 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 0))
                            0)
               10000))
      (a!11 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 1))
                            0)
               1000))
      (a!12 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 2))
                            0)
               100))
      (a!13 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 3))
                            0)
               10))
      (a!15 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 0))
                            0)
               100000))
      (a!16 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 1))
                            0)
               10000))
      (a!17 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 2))
                            0)
               1000))
      (a!18 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 3))
                            0)
               100))
      (a!19 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 4))
                            0)
               10))
      (a!21 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 0))
                            0)
               1000000))
      (a!22 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 1))
                            0)
               100000))
      (a!23 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 2))
                            0)
               10000))
      (a!24 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 3))
                            0)
               1000))
      (a!25 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 4))
                            0)
               100))
      (a!26 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 5))
                            0)
               10)))
(let ((a!2 (+ a!1
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 1))
                           0)))
      (a!5 (+ a!3
              a!4
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 2))
                           0)))
      (a!9 (+ a!6
              a!7
              a!8
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s2__call16 3))
                           0)))
      (a!14 (+ a!10
               a!11
               a!12
               a!13
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 4))
                            0)))
      (a!20 (+ a!15
               a!16
               a!17
               a!18
               a!19
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 5))
                            0)))
      (a!27 (+ a!21
               a!22
               a!23
               a!24
               a!25
               a!26
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s2__call16 6))
                            0))))
(let ((a!28 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16) 6)
                 a!20
                 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16)
                         7)
                      a!27
                      (- 0 1)))))
(let ((a!29 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16) 4)
                 a!9
                 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16)
                         5)
                      a!14
                      a!28))))
(let ((a!30 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16) 2)
                 a!2
                 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16)
                         3)
                      a!5
                      a!29))))
(let ((a!31 (ite (= (- LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16) 1)
                 (str.indexof "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s2__call16 0))
                              0)
                 a!30)))
  (= LsIdxEval__ie_n2__call16 a!31))))))))
(assert (let ((a!1 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 0))
                           0)
              10))
      (a!3 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 0))
                           0)
              100))
      (a!4 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 1))
                           0)
              10))
      (a!6 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 0))
                           0)
              1000))
      (a!7 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 1))
                           0)
              100))
      (a!8 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 2))
                           0)
              10))
      (a!10 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 0))
                            0)
               10000))
      (a!11 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 1))
                            0)
               1000))
      (a!12 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 2))
                            0)
               100))
      (a!13 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 3))
                            0)
               10))
      (a!15 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 0))
                            0)
               100000))
      (a!16 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 1))
                            0)
               10000))
      (a!17 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 2))
                            0)
               1000))
      (a!18 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 3))
                            0)
               100))
      (a!19 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 4))
                            0)
               10))
      (a!21 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 0))
                            0)
               1000000))
      (a!22 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 1))
                            0)
               100000))
      (a!23 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 2))
                            0)
               10000))
      (a!24 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 3))
                            0)
               1000))
      (a!25 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 4))
                            0)
               100))
      (a!26 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 5))
                            0)
               10)))
(let ((a!2 (+ a!1
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 1))
                           0)))
      (a!5 (+ a!3
              a!4
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 2))
                           0)))
      (a!9 (+ a!6
              a!7
              a!8
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s3__call16 3))
                           0)))
      (a!14 (+ a!10
               a!11
               a!12
               a!13
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 4))
                            0)))
      (a!20 (+ a!15
               a!16
               a!17
               a!18
               a!19
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 5))
                            0)))
      (a!27 (+ a!21
               a!22
               a!23
               a!24
               a!25
               a!26
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s3__call16 6))
                            0))))
(let ((a!28 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16) 6)
                 a!20
                 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16)
                         7)
                      a!27
                      (- 0 1)))))
(let ((a!29 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16) 4)
                 a!9
                 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16)
                         5)
                      a!14
                      a!28))))
(let ((a!30 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16) 2)
                 a!2
                 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16)
                         3)
                      a!5
                      a!29))))
(let ((a!31 (ite (= (- LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16) 1)
                 (str.indexof "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s3__call16 0))
                              0)
                 a!30)))
  (= LsIdxEval__ie_n3__call16 a!31))))))))
(assert (let ((a!1 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 0))
                           0)
              10))
      (a!3 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 0))
                           0)
              100))
      (a!4 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 1))
                           0)
              10))
      (a!6 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 0))
                           0)
              1000))
      (a!7 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 1))
                           0)
              100))
      (a!8 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 2))
                           0)
              10))
      (a!10 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 0))
                            0)
               10000))
      (a!11 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 1))
                            0)
               1000))
      (a!12 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 2))
                            0)
               100))
      (a!13 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 3))
                            0)
               10))
      (a!15 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 0))
                            0)
               100000))
      (a!16 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 1))
                            0)
               10000))
      (a!17 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 2))
                            0)
               1000))
      (a!18 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 3))
                            0)
               100))
      (a!19 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 4))
                            0)
               10))
      (a!21 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 0))
                            0)
               1000000))
      (a!22 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 1))
                            0)
               100000))
      (a!23 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 2))
                            0)
               10000))
      (a!24 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 3))
                            0)
               1000))
      (a!25 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 4))
                            0)
               100))
      (a!26 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 5))
                            0)
               10)))
(let ((a!2 (+ a!1
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 1))
                           0)))
      (a!5 (+ a!3
              a!4
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 2))
                           0)))
      (a!9 (+ a!6
              a!7
              a!8
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s4__call16 3))
                           0)))
      (a!14 (+ a!10
               a!11
               a!12
               a!13
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 4))
                            0)))
      (a!20 (+ a!15
               a!16
               a!17
               a!18
               a!19
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 5))
                            0)))
      (a!27 (+ a!21
               a!22
               a!23
               a!24
               a!25
               a!26
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s4__call16 6))
                            0))))
(let ((a!28 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16) 6)
                 a!20
                 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16)
                         7)
                      a!27
                      (- 0 1)))))
(let ((a!29 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16) 4)
                 a!9
                 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16)
                         5)
                      a!14
                      a!28))))
(let ((a!30 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16) 2)
                 a!2
                 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16)
                         3)
                      a!5
                      a!29))))
(let ((a!31 (ite (= (- LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16) 1)
                 (str.indexof "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s4__call16 0))
                              0)
                 a!30)))
  (= LsIdxEval__ie_n4__call16 a!31))))))))
(assert (let ((a!1 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 0))
                           0)
              10))
      (a!3 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 0))
                           0)
              100))
      (a!4 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 1))
                           0)
              10))
      (a!6 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 0))
                           0)
              1000))
      (a!7 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 1))
                           0)
              100))
      (a!8 (* (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 2))
                           0)
              10))
      (a!10 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 0))
                            0)
               10000))
      (a!11 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 1))
                            0)
               1000))
      (a!12 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 2))
                            0)
               100))
      (a!13 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 3))
                            0)
               10))
      (a!15 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 0))
                            0)
               100000))
      (a!16 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 1))
                            0)
               10000))
      (a!17 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 2))
                            0)
               1000))
      (a!18 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 3))
                            0)
               100))
      (a!19 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 4))
                            0)
               10))
      (a!21 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 0))
                            0)
               1000000))
      (a!22 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 1))
                            0)
               100000))
      (a!23 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 2))
                            0)
               10000))
      (a!24 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 3))
                            0)
               1000))
      (a!25 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 4))
                            0)
               100))
      (a!26 (* (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 5))
                            0)
               10)))
(let ((a!2 (+ a!1
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 1))
                           0)))
      (a!5 (+ a!3
              a!4
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 2))
                           0)))
      (a!9 (+ a!6
              a!7
              a!8
              (str.indexof "0123456789"
                           (str.at LsIdxEval__ie_t__call16
                                   (+ LsIdxEval__ie_s5__call16 3))
                           0)))
      (a!14 (+ a!10
               a!11
               a!12
               a!13
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 4))
                            0)))
      (a!20 (+ a!15
               a!16
               a!17
               a!18
               a!19
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 5))
                            0)))
      (a!27 (+ a!21
               a!22
               a!23
               a!24
               a!25
               a!26
               (str.indexof "0123456789"
                            (str.at LsIdxEval__ie_t__call16
                                    (+ LsIdxEval__ie_s5__call16 6))
                            0))))
(let ((a!28 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16) 6)
                 a!20
                 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16)
                         7)
                      a!27
                      (- 0 1)))))
(let ((a!29 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16) 4)
                 a!9
                 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16)
                         5)
                      a!14
                      a!28))))
(let ((a!30 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16) 2)
                 a!2
                 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16)
                         3)
                      a!5
                      a!29))))
(let ((a!31 (ite (= (- LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16) 1)
                 (str.indexof "0123456789"
                              (str.at LsIdxEval__ie_t__call16
                                      (+ LsIdxEval__ie_s5__call16 0))
                              0)
                 a!30)))
  (= LsIdxEval__ie_n5__call16 a!31))))))))
(assert (let ((a!1 (ite (= LsIdxEval__ie_op2__call16 "")
                3
                (ite (= LsIdxEval__ie_op3__call16 "")
                     4
                     (ite (= LsIdxEval__ie_op4__call16 "") 5 6)))))
  (= LsIdxEval__ie_cnt__call16
     (ite (= LsIdxEval__ie_op0__call16 "")
          1
          (ite (= LsIdxEval__ie_op1__call16 "") 2 a!1)))))
(assert (let ((a!1 (and LsIdxEval__ie_starts_digit__call16
                LsIdxEval__ie_valid_chars__call16
                (> LsIdxEval__ie_ne0__call16 0)
                (or (< LsIdxEval__ie_cnt__call16 2)
                    (> LsIdxEval__ie_ne1__call16 LsIdxEval__ie_s1__call16))
                (or (< LsIdxEval__ie_cnt__call16 3)
                    (> LsIdxEval__ie_ne2__call16 LsIdxEval__ie_s2__call16))
                (or (< LsIdxEval__ie_cnt__call16 4)
                    (> LsIdxEval__ie_ne3__call16 LsIdxEval__ie_s3__call16))
                (or (< LsIdxEval__ie_cnt__call16 5)
                    (> LsIdxEval__ie_ne4__call16 LsIdxEval__ie_s4__call16))
                (or (< LsIdxEval__ie_cnt__call16 6)
                    (> LsIdxEval__ie_ne5__call16 LsIdxEval__ie_s5__call16))
                (ite (>= LsIdxEval__ie_cnt__call16 6)
                     (>= LsIdxEval__ie_ne5__call16
                         (str.len LsIdxEval__ie_t__call16))
                     true))))
  (= LsIdxEval__ie_shape_ok__call16 a!1)))
(assert (= LsIdxEval__ie_g0__call16 LsIdxEval__ie_n0__call16))
(assert (= LsIdxEval__ie_t0__call16 0))
(assert (= LsIdxEval__ie_sg0__call16 1))
(assert (= LsIdxEval__ie_g1__call16
   (ite (= LsIdxEval__ie_op0__call16 "*")
        (* LsIdxEval__ie_g0__call16 LsIdxEval__ie_n1__call16)
        LsIdxEval__ie_n1__call16)))
(assert (= LsIdxEval__ie_t1__call16
   (ite (= LsIdxEval__ie_op0__call16 "*")
        LsIdxEval__ie_t0__call16
        (+ LsIdxEval__ie_t0__call16
           (* LsIdxEval__ie_sg0__call16 LsIdxEval__ie_g0__call16)))))
(assert (= LsIdxEval__ie_sg1__call16
   (ite (= LsIdxEval__ie_op0__call16 "*")
        LsIdxEval__ie_sg0__call16
        (ite (= LsIdxEval__ie_op0__call16 "-") (- 0 1) 1))))
(assert (= LsIdxEval__ie_g2__call16
   (ite (= LsIdxEval__ie_op1__call16 "*")
        (* LsIdxEval__ie_g1__call16 LsIdxEval__ie_n2__call16)
        LsIdxEval__ie_n2__call16)))
(assert (= LsIdxEval__ie_t2__call16
   (ite (= LsIdxEval__ie_op1__call16 "*")
        LsIdxEval__ie_t1__call16
        (+ LsIdxEval__ie_t1__call16
           (* LsIdxEval__ie_sg1__call16 LsIdxEval__ie_g1__call16)))))
(assert (= LsIdxEval__ie_sg2__call16
   (ite (= LsIdxEval__ie_op1__call16 "*")
        LsIdxEval__ie_sg1__call16
        (ite (= LsIdxEval__ie_op1__call16 "-") (- 0 1) 1))))
(assert (= LsIdxEval__ie_g3__call16
   (ite (= LsIdxEval__ie_op2__call16 "*")
        (* LsIdxEval__ie_g2__call16 LsIdxEval__ie_n3__call16)
        LsIdxEval__ie_n3__call16)))
(assert (= LsIdxEval__ie_t3__call16
   (ite (= LsIdxEval__ie_op2__call16 "*")
        LsIdxEval__ie_t2__call16
        (+ LsIdxEval__ie_t2__call16
           (* LsIdxEval__ie_sg2__call16 LsIdxEval__ie_g2__call16)))))
(assert (= LsIdxEval__ie_sg3__call16
   (ite (= LsIdxEval__ie_op2__call16 "*")
        LsIdxEval__ie_sg2__call16
        (ite (= LsIdxEval__ie_op2__call16 "-") (- 0 1) 1))))
(assert (= LsIdxEval__ie_g4__call16
   (ite (= LsIdxEval__ie_op3__call16 "*")
        (* LsIdxEval__ie_g3__call16 LsIdxEval__ie_n4__call16)
        LsIdxEval__ie_n4__call16)))
(assert (= LsIdxEval__ie_t4__call16
   (ite (= LsIdxEval__ie_op3__call16 "*")
        LsIdxEval__ie_t3__call16
        (+ LsIdxEval__ie_t3__call16
           (* LsIdxEval__ie_sg3__call16 LsIdxEval__ie_g3__call16)))))
(assert (= LsIdxEval__ie_sg4__call16
   (ite (= LsIdxEval__ie_op3__call16 "*")
        LsIdxEval__ie_sg3__call16
        (ite (= LsIdxEval__ie_op3__call16 "-") (- 0 1) 1))))
(assert (= LsIdxEval__ie_g5__call16
   (ite (= LsIdxEval__ie_op4__call16 "*")
        (* LsIdxEval__ie_g4__call16 LsIdxEval__ie_n5__call16)
        LsIdxEval__ie_n5__call16)))
(assert (= LsIdxEval__ie_t5__call16
   (ite (= LsIdxEval__ie_op4__call16 "*")
        LsIdxEval__ie_t4__call16
        (+ LsIdxEval__ie_t4__call16
           (* LsIdxEval__ie_sg4__call16 LsIdxEval__ie_g4__call16)))))
(assert (= LsIdxEval__ie_sg5__call16
   (ite (= LsIdxEval__ie_op4__call16 "*")
        LsIdxEval__ie_sg4__call16
        (ite (= LsIdxEval__ie_op4__call16 "-") (- 0 1) 1))))
(assert (let ((a!1 (ite (= LsIdxEval__ie_cnt__call16 4)
                (+ LsIdxEval__ie_t3__call16
                   (* LsIdxEval__ie_sg3__call16 LsIdxEval__ie_g3__call16))
                (ite (= LsIdxEval__ie_cnt__call16 5)
                     (+ LsIdxEval__ie_t4__call16
                        (* LsIdxEval__ie_sg4__call16 LsIdxEval__ie_g4__call16))
                     (+ LsIdxEval__ie_t5__call16
                        (* LsIdxEval__ie_sg5__call16 LsIdxEval__ie_g5__call16))))))
(let ((a!2 (ite (= LsIdxEval__ie_cnt__call16 2)
                (+ LsIdxEval__ie_t1__call16
                   (* LsIdxEval__ie_sg1__call16 LsIdxEval__ie_g1__call16))
                (ite (= LsIdxEval__ie_cnt__call16 3)
                     (+ LsIdxEval__ie_t2__call16
                        (* LsIdxEval__ie_sg2__call16 LsIdxEval__ie_g2__call16))
                     a!1))))
  (= LsIdxEval__ie_total__call16
     (ite (= LsIdxEval__ie_cnt__call16 1)
          (+ LsIdxEval__ie_t0__call16
             (* LsIdxEval__ie_sg0__call16 LsIdxEval__ie_g0__call16))
          a!2)))))
(assert (= w_idx_ok
   (and LsIdxEval__ie_shape_ok__call16 (>= LsIdxEval__ie_total__call16 0))))
(assert (let ((a!1 (ite (>= LsIdxEval__ie_total__call16 0)
                (str.from_int LsIdxEval__ie_total__call16)
                (str.++ "-" (str.from_int (- 0 LsIdxEval__ie_total__call16))))))
  (= w_idx (ite w_idx_ok a!1 ""))))
(assert (= w_do_index (and w_base_reg (> w_cb w_we) w_idx_ok)))
(assert (let ((a!1 (and w_do_index (= (str.at w_src (+ w_cb 1)) "."))))
  (= w_dot a!1)))
(assert (= w_fs (+ w_cb 2)))
(assert (let ((a!1 (and (< w_fs (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") w_fs))))
      (a!2 (and (< (+ w_fs 1) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 1)))))
      (a!3 (and (< (+ w_fs 2) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 2)))))
      (a!4 (and (< (+ w_fs 3) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 3)))))
      (a!5 (and (< (+ w_fs 4) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 4)))))
      (a!6 (and (< (+ w_fs 5) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 5)))))
      (a!7 (and (< (+ w_fs 6) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 6)))))
      (a!8 (and (< (+ w_fs 7) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 7)))))
      (a!9 (and (< (+ w_fs 8) (str.len (ite w_dot w_src "")))
                (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                              (str.at (ite w_dot w_src "") (+ w_fs 8)))))
      (a!10 (and (< (+ w_fs 9) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 9)))))
      (a!11 (and (< (+ w_fs 10) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 10)))))
      (a!12 (and (< (+ w_fs 11) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 11)))))
      (a!13 (and (< (+ w_fs 12) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 12)))))
      (a!14 (and (< (+ w_fs 13) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 13)))))
      (a!15 (and (< (+ w_fs 14) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 14)))))
      (a!16 (and (< (+ w_fs 15) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 15)))))
      (a!17 (and (< (+ w_fs 16) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 16)))))
      (a!18 (and (< (+ w_fs 17) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 17)))))
      (a!19 (and (< (+ w_fs 18) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 18)))))
      (a!20 (and (< (+ w_fs 19) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 19)))))
      (a!21 (and (< (+ w_fs 20) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 20)))))
      (a!22 (and (< (+ w_fs 21) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 21)))))
      (a!23 (and (< (+ w_fs 22) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 22)))))
      (a!24 (and (< (+ w_fs 23) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 23)))))
      (a!25 (and (< (+ w_fs 24) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 24)))))
      (a!26 (and (< (+ w_fs 25) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 25)))))
      (a!27 (and (< (+ w_fs 26) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 26)))))
      (a!28 (and (< (+ w_fs 27) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 27)))))
      (a!29 (and (< (+ w_fs 28) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 28)))))
      (a!30 (and (< (+ w_fs 29) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 29)))))
      (a!31 (and (< (+ w_fs 30) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 30)))))
      (a!32 (and (< (+ w_fs 31) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 31)))))
      (a!33 (and (< (+ w_fs 32) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 32)))))
      (a!34 (and (< (+ w_fs 33) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 33)))))
      (a!35 (and (< (+ w_fs 34) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 34)))))
      (a!36 (and (< (+ w_fs 35) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 35)))))
      (a!37 (and (< (+ w_fs 36) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 36)))))
      (a!38 (and (< (+ w_fs 37) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 37)))))
      (a!39 (and (< (+ w_fs 38) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 38)))))
      (a!40 (and (< (+ w_fs 39) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 39)))))
      (a!41 (and (< (+ w_fs 40) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 40)))))
      (a!42 (and (< (+ w_fs 41) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 41)))))
      (a!43 (and (< (+ w_fs 42) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 42)))))
      (a!44 (and (< (+ w_fs 43) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 43)))))
      (a!45 (and (< (+ w_fs 44) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 44)))))
      (a!46 (and (< (+ w_fs 45) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 45)))))
      (a!47 (and (< (+ w_fs 46) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 46)))))
      (a!48 (and (< (+ w_fs 47) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 47)))))
      (a!49 (and (< (+ w_fs 48) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 48)))))
      (a!50 (and (< (+ w_fs 49) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 49)))))
      (a!51 (and (< (+ w_fs 50) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 50)))))
      (a!52 (and (< (+ w_fs 51) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 51)))))
      (a!53 (and (< (+ w_fs 52) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 52)))))
      (a!54 (and (< (+ w_fs 53) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 53)))))
      (a!55 (and (< (+ w_fs 54) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 54)))))
      (a!56 (and (< (+ w_fs 55) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 55)))))
      (a!57 (and (< (+ w_fs 56) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 56)))))
      (a!58 (and (< (+ w_fs 57) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 57)))))
      (a!59 (and (< (+ w_fs 58) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 58)))))
      (a!60 (and (< (+ w_fs 59) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 59)))))
      (a!61 (and (< (+ w_fs 60) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 60)))))
      (a!62 (and (< (+ w_fs 61) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 61)))))
      (a!63 (and (< (+ w_fs 62) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 62)))))
      (a!64 (and (< (+ w_fs 63) (str.len (ite w_dot w_src "")))
                 (str.contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                               (str.at (ite w_dot w_src "") (+ w_fs 63))))))
(let ((a!65 (ite (not a!62)
                 (+ w_fs 61)
                 (ite (not a!63)
                      (+ w_fs 62)
                      (ite (not a!64) (+ w_fs 63) (+ w_fs 64))))))
(let ((a!66 (ite (not a!59)
                 (+ w_fs 58)
                 (ite (not a!60) (+ w_fs 59) (ite (not a!61) (+ w_fs 60) a!65)))))
(let ((a!67 (ite (not a!56)
                 (+ w_fs 55)
                 (ite (not a!57) (+ w_fs 56) (ite (not a!58) (+ w_fs 57) a!66)))))
(let ((a!68 (ite (not a!53)
                 (+ w_fs 52)
                 (ite (not a!54) (+ w_fs 53) (ite (not a!55) (+ w_fs 54) a!67)))))
(let ((a!69 (ite (not a!50)
                 (+ w_fs 49)
                 (ite (not a!51) (+ w_fs 50) (ite (not a!52) (+ w_fs 51) a!68)))))
(let ((a!70 (ite (not a!47)
                 (+ w_fs 46)
                 (ite (not a!48) (+ w_fs 47) (ite (not a!49) (+ w_fs 48) a!69)))))
(let ((a!71 (ite (not a!44)
                 (+ w_fs 43)
                 (ite (not a!45) (+ w_fs 44) (ite (not a!46) (+ w_fs 45) a!70)))))
(let ((a!72 (ite (not a!41)
                 (+ w_fs 40)
                 (ite (not a!42) (+ w_fs 41) (ite (not a!43) (+ w_fs 42) a!71)))))
(let ((a!73 (ite (not a!38)
                 (+ w_fs 37)
                 (ite (not a!39) (+ w_fs 38) (ite (not a!40) (+ w_fs 39) a!72)))))
(let ((a!74 (ite (not a!35)
                 (+ w_fs 34)
                 (ite (not a!36) (+ w_fs 35) (ite (not a!37) (+ w_fs 36) a!73)))))
(let ((a!75 (ite (not a!32)
                 (+ w_fs 31)
                 (ite (not a!33) (+ w_fs 32) (ite (not a!34) (+ w_fs 33) a!74)))))
(let ((a!76 (ite (not a!29)
                 (+ w_fs 28)
                 (ite (not a!30) (+ w_fs 29) (ite (not a!31) (+ w_fs 30) a!75)))))
(let ((a!77 (ite (not a!26)
                 (+ w_fs 25)
                 (ite (not a!27) (+ w_fs 26) (ite (not a!28) (+ w_fs 27) a!76)))))
(let ((a!78 (ite (not a!23)
                 (+ w_fs 22)
                 (ite (not a!24) (+ w_fs 23) (ite (not a!25) (+ w_fs 24) a!77)))))
(let ((a!79 (ite (not a!20)
                 (+ w_fs 19)
                 (ite (not a!21) (+ w_fs 20) (ite (not a!22) (+ w_fs 21) a!78)))))
(let ((a!80 (ite (not a!17)
                 (+ w_fs 16)
                 (ite (not a!18) (+ w_fs 17) (ite (not a!19) (+ w_fs 18) a!79)))))
(let ((a!81 (ite (not a!14)
                 (+ w_fs 13)
                 (ite (not a!15) (+ w_fs 14) (ite (not a!16) (+ w_fs 15) a!80)))))
(let ((a!82 (ite (not a!11)
                 (+ w_fs 10)
                 (ite (not a!12) (+ w_fs 11) (ite (not a!13) (+ w_fs 12) a!81)))))
(let ((a!83 (ite (not a!8)
                 (+ w_fs 7)
                 (ite (not a!9) (+ w_fs 8) (ite (not a!10) (+ w_fs 9) a!82)))))
(let ((a!84 (ite (not a!5)
                 (+ w_fs 4)
                 (ite (not a!6) (+ w_fs 5) (ite (not a!7) (+ w_fs 6) a!83)))))
(let ((a!85 (ite (not a!2)
                 (+ w_fs 1)
                 (ite (not a!3) (+ w_fs 2) (ite (not a!4) (+ w_fs 3) a!84)))))
  (= w_fe (ite (not a!1) w_fs a!85)))))))))))))))))))))))))
(assert (= w_has_field (and w_dot (> w_fe w_fs))))
(assert (= w_field (ite w_has_field (str.substr w_src w_fs (- w_fe w_fs)) "")))
(assert (= w_sub_br (and w_has_field (= (str.at w_src w_fe) "["))))
(assert (= w_scb (ite w_sub_br (str.indexof w_src "]" (+ w_fe 1)) (- 0 1))))
(assert (let ((a!1 (ite (> w_scb w_fe)
                (str.substr w_src (+ w_fe 1) (- (- w_scb w_fe) 1))
                "")))
  (= w_sinner a!1)))
(assert (let ((a!1 (ite (= (str.len w_sinner) 0)
                false
                (ite (> (str.len w_sinner) 16) false LsAllDigits__ad_ok__call32))))
  (= w_sidx_ok a!1)))
(assert (= LsAllDigits__ad_ok__call32
   (>= LsAllDigits__ad_first__call32 (str.len w_sinner))))
(assert (let ((a!1 (ite (not LsAllDigits__ad_d13__call32)
                13
                (ite (not LsAllDigits__ad_d14__call32)
                     14
                     (ite (not LsAllDigits__ad_d15__call32) 15 16)))))
(let ((a!2 (ite (not LsAllDigits__ad_d10__call32)
                10
                (ite (not LsAllDigits__ad_d11__call32)
                     11
                     (ite (not LsAllDigits__ad_d12__call32) 12 a!1)))))
(let ((a!3 (ite (not LsAllDigits__ad_d7__call32)
                7
                (ite (not LsAllDigits__ad_d8__call32)
                     8
                     (ite (not LsAllDigits__ad_d9__call32) 9 a!2)))))
(let ((a!4 (ite (not LsAllDigits__ad_d4__call32)
                4
                (ite (not LsAllDigits__ad_d5__call32)
                     5
                     (ite (not LsAllDigits__ad_d6__call32) 6 a!3)))))
(let ((a!5 (ite (not LsAllDigits__ad_d1__call32)
                1
                (ite (not LsAllDigits__ad_d2__call32)
                     2
                     (ite (not LsAllDigits__ad_d3__call32) 3 a!4)))))
  (= LsAllDigits__ad_first__call32 (ite (not LsAllDigits__ad_d0__call32) 0 a!5))))))))
(assert (= LsAllDigits__ad_d0__call32
   (and (< 0 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 0)))))
(assert (= LsAllDigits__ad_d1__call32
   (and (< 1 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 1)))))
(assert (= LsAllDigits__ad_d2__call32
   (and (< 2 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 2)))))
(assert (= LsAllDigits__ad_d3__call32
   (and (< 3 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 3)))))
(assert (= LsAllDigits__ad_d4__call32
   (and (< 4 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 4)))))
(assert (= LsAllDigits__ad_d5__call32
   (and (< 5 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 5)))))
(assert (= LsAllDigits__ad_d6__call32
   (and (< 6 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 6)))))
(assert (= LsAllDigits__ad_d7__call32
   (and (< 7 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 7)))))
(assert (= LsAllDigits__ad_d8__call32
   (and (< 8 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 8)))))
(assert (= LsAllDigits__ad_d9__call32
   (and (< 9 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 9)))))
(assert (= LsAllDigits__ad_d10__call32
   (and (< 10 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 10)))))
(assert (= LsAllDigits__ad_d11__call32
   (and (< 11 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 11)))))
(assert (= LsAllDigits__ad_d12__call32
   (and (< 12 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 12)))))
(assert (= LsAllDigits__ad_d13__call32
   (and (< 13 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 13)))))
(assert (= LsAllDigits__ad_d14__call32
   (and (< 14 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 14)))))
(assert (= LsAllDigits__ad_d15__call32
   (and (< 15 (str.len w_sinner))
        (str.contains "0123456789" (str.at w_sinner 15)))))
(assert (= w_has_sub (and w_sub_br (> w_scb w_fe) w_sidx_ok (>= (str.len w_sinner) 1))))
(assert (= w_index_out
   (str.++ w_tok
           "_"
           w_idx
           (ite w_has_field (str.++ "_" w_field) "")
           (ite w_has_sub (str.++ "_" w_sinner) ""))))
(assert (= w_index_end (ite w_has_sub (+ w_scb 1) (ite w_has_field w_fe (+ w_cb 1)))))
(assert (= w_unit
   (ite w_word_reg
        (str.++ w_word "_len")
        (ite w_do_index w_index_out (ite w_is_ident w_tok w_ch)))))
(assert (let ((a!1 (ite w_word_reg
                w_he
                (ite w_do_index w_index_end (ite w_is_ident w_we (+ w_p 1))))))
  (= w_next a!1)))
(assert (= tk_walk_run (and (= tk_ph 5) (< w_p (str.len w_src)))))
(assert (= tk_walk_done (and (= tk_ph 5) (>= w_p (str.len w_src)))))
(assert (let ((a!1 (ite tk_walk_done
                _sub_acc
                (ite tk_loop_run
                     (str.++ _indent tk_slot_line)
                     (ite tk_has_len_lines (str.++ _indent tk_len_lines) "")))))
  (= tk_print_now
     (ite (and tk_src tk_is_top)
          tk_rline
          (ite tk_rewrite_bound
               (str.++ tk_ind tk_hash_after "_len \u{2264} " tk_bound_n)
               (ite tk_default_plain tk_rline a!1))))))
(assert (= tk_emit
   (or (and tk_src (or tk_is_top tk_default_plain tk_rewrite_bound))
       tk_walk_done
       tk_loop_run
       tk_has_len_lines)))
(assert (let ((a!1 (ite (= tk_ph 4)
                (ite tk_loop_done 3 4)
                (ite (= tk_ph 5)
                     (ite tk_walk_done 3 5)
                     (ite (= tk_ph 3) 3 tk_ph)))))
(let ((a!2 (ite (= tk_ph 2)
                3
                (ite tk_eof_now
                     13
                     (ite (or tk_enter_loop tk_enter_dual)
                          4
                          (ite tk_needs_walk 5 a!1))))))
  (= phase (ite is_first_tick 2 a!2)))))
(assert (= reg (ite is_first_tick "" (ite (= tk_ph 2) tk_rline _reg))))
(assert (= line (ite is_first_tick "" (ite tk_src tk_rline _line))))
(assert (= code (ite is_first_tick "" (ite tk_src tk_code _code))))
(assert (= indent
   (ite is_first_tick "" (ite (or tk_enter_loop tk_enter_dual) tk_ind _indent))))
(assert (= sub_src (ite is_first_tick "" (ite tk_needs_walk tk_rline _sub_src))))
(assert (= sub_pos
   (ite is_first_tick 0 (ite tk_needs_walk 0 (ite tk_walk_run w_next _sub_pos)))))
(assert (let ((a!1 (ite is_first_tick
                ""
                (ite tk_needs_walk
                     ""
                     (ite tk_walk_run (str.++ _sub_acc w_unit) _sub_acc)))))
  (= sub_acc a!1)))
(assert (let ((a!1 (ite tk_enter_dual
                2
                (ite tk_enter_loop
                     (ite (or tk_decl_lit tk_is_litassign)
                          3
                          (ite tk_is_hold 4 1))
                     _emit_kind))))
  (= emit_kind (ite is_first_tick 0 a!1))))
(assert (= emit_nm
   (ite is_first_tick
        ""
        (ite (or tk_enter_loop tk_enter_dual) tk_lead_base _emit_nm))))
(assert (= emit_base
   (ite is_first_tick
        ""
        (ite (or tk_enter_loop tk_enter_dual) tk_rbase _emit_base))))
(assert (= emit_n
   (ite is_first_tick 0 (ite (or tk_enter_loop tk_enter_dual) tk_rn _emit_n))))
(assert (= emit_haslen
   (ite is_first_tick
        false
        (ite (or tk_enter_loop tk_enter_dual) tk_rhaslen _emit_haslen))))
(assert (let ((a!1 (ite is_first_tick
                0
                (ite (or tk_enter_loop tk_enter_dual)
                     0
                     (ite tk_loop_run (+ _emit_k 1) _emit_k)))))
  (= emit_k a!1)))
(assert (= emit_inside
   (ite is_first_tick "" (ite tk_enter_loop tk_inside _emit_inside))))
(assert (let ((a!1 (and (< 0 (str.len tk_inside))
                (or (= (str.at tk_inside 0) " ")
                    (= (str.at tk_inside 0) "\u{9}"))))
      (a!2 (or (= (str.at tk_inside (+ 0 1)) " ")
               (= (str.at tk_inside (+ 0 1)) "\u{9}")))
      (a!4 (or (= (str.at tk_inside (+ 0 2)) " ")
               (= (str.at tk_inside (+ 0 2)) "\u{9}")))
      (a!6 (or (= (str.at tk_inside (+ 0 3)) " ")
               (= (str.at tk_inside (+ 0 3)) "\u{9}")))
      (a!8 (or (= (str.at tk_inside (+ 0 4)) " ")
               (= (str.at tk_inside (+ 0 4)) "\u{9}")))
      (a!10 (or (= (str.at tk_inside (+ 0 5)) " ")
                (= (str.at tk_inside (+ 0 5)) "\u{9}")))
      (a!12 (or (= (str.at tk_inside (+ 0 6)) " ")
                (= (str.at tk_inside (+ 0 6)) "\u{9}")))
      (a!14 (or (= (str.at tk_inside (+ 0 7)) " ")
                (= (str.at tk_inside (+ 0 7)) "\u{9}"))))
(let ((a!3 (not (and (< (+ 0 1) (str.len tk_inside)) a!2)))
      (a!5 (not (and (< (+ 0 2) (str.len tk_inside)) a!4)))
      (a!7 (not (and (< (+ 0 3) (str.len tk_inside)) a!6)))
      (a!9 (not (and (< (+ 0 4) (str.len tk_inside)) a!8)))
      (a!11 (not (and (< (+ 0 5) (str.len tk_inside)) a!10)))
      (a!13 (not (and (< (+ 0 6) (str.len tk_inside)) a!12)))
      (a!15 (not (and (< (+ 0 7) (str.len tk_inside)) a!14))))
(let ((a!16 (ite a!11 (+ 0 5) (ite a!13 (+ 0 6) (ite a!15 (+ 0 7) (+ 0 8))))))
(let ((a!17 (ite a!5 (+ 0 2) (ite a!7 (+ 0 3) (ite a!9 (+ 0 4) a!16)))))
  (= tk_inside_tl (ite (not a!1) 0 (ite a!3 (+ 0 1) a!17))))))))
(assert (let ((a!1 (ite tk_enter_loop
                (ite (>= tk_inside_tl (str.len tk_inside)) 0 tk_count_el)
                _emit_ne)))
  (= emit_ne (ite is_first_tick 0 a!1))))
(assert (= tk_count_el (ite (= (str.len tk_inside) 0) 0 LsCountElem__ce_n__call34)))
(assert (= LsCountElem__ce_n__call34
   (ite (< (str.indexof tk_inside "," 0) 0) 1 LsCountElem__ce_scan__call34)))
(assert (= LsCountElem__ce_scan__call34 (+ LsCountElem__ce_count__call34 1)))
(assert (let ((a!1 (ite (>= LsCountElem__cP14__call34 (str.len tk_inside))
                14
                (ite (>= LsCountElem__cP15__call34 (str.len tk_inside)) 15 16))))
(let ((a!2 (ite (>= LsCountElem__cP12__call34 (str.len tk_inside))
                12
                (ite (>= LsCountElem__cP13__call34 (str.len tk_inside)) 13 a!1))))
(let ((a!3 (ite (>= LsCountElem__cP10__call34 (str.len tk_inside))
                10
                (ite (>= LsCountElem__cP11__call34 (str.len tk_inside)) 11 a!2))))
(let ((a!4 (ite (>= LsCountElem__cP8__call34 (str.len tk_inside))
                8
                (ite (>= LsCountElem__cP9__call34 (str.len tk_inside)) 9 a!3))))
(let ((a!5 (ite (>= LsCountElem__cP6__call34 (str.len tk_inside))
                6
                (ite (>= LsCountElem__cP7__call34 (str.len tk_inside)) 7 a!4))))
(let ((a!6 (ite (>= LsCountElem__cP4__call34 (str.len tk_inside))
                4
                (ite (>= LsCountElem__cP5__call34 (str.len tk_inside)) 5 a!5))))
(let ((a!7 (ite (>= LsCountElem__cP2__call34 (str.len tk_inside))
                2
                (ite (>= LsCountElem__cP3__call34 (str.len tk_inside)) 3 a!6))))
(let ((a!8 (ite (>= LsCountElem__cP0__call34 (str.len tk_inside))
                0
                (ite (>= LsCountElem__cP1__call34 (str.len tk_inside)) 1 a!7))))
  (= LsCountElem__ce_count__call34 a!8))))))))))
(assert (let ((a!1 (ite (= 0 12)
                LsCommaPos__cp12__call35
                (ite (= 0 13)
                     LsCommaPos__cp13__call35
                     (ite (= 0 14)
                          LsCommaPos__cp14__call35
                          LsCommaPos__cp15__call35)))))
(let ((a!2 (ite (= 0 9)
                LsCommaPos__cp9__call35
                (ite (= 0 10)
                     LsCommaPos__cp10__call35
                     (ite (= 0 11) LsCommaPos__cp11__call35 a!1)))))
(let ((a!3 (ite (= 0 6)
                LsCommaPos__cp6__call35
                (ite (= 0 7)
                     LsCommaPos__cp7__call35
                     (ite (= 0 8) LsCommaPos__cp8__call35 a!2)))))
(let ((a!4 (ite (= 0 3)
                LsCommaPos__cp3__call35
                (ite (= 0 4)
                     LsCommaPos__cp4__call35
                     (ite (= 0 5) LsCommaPos__cp5__call35 a!3)))))
(let ((a!5 (ite (= 0 0)
                LsCommaPos__cp0__call35
                (ite (= 0 1)
                     LsCommaPos__cp1__call35
                     (ite (= 0 2) LsCommaPos__cp2__call35 a!4)))))
  (= LsCountElem__cP0__call34 (ite (< 0 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call35
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call35 1)) 0))))
  (= LsCommaPos__cp1__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call35 1)) 0))))
  (= LsCommaPos__cp2__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call35 1)) 0))))
  (= LsCommaPos__cp3__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call35 1)) 0))))
  (= LsCommaPos__cp4__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call35 1)) 0))))
  (= LsCommaPos__cp5__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call35 1)) 0))))
  (= LsCommaPos__cp6__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call35 1)) 0))))
  (= LsCommaPos__cp7__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call35 1)) 0))))
  (= LsCommaPos__cp8__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call35 1)) 0))))
  (= LsCommaPos__cp9__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call35 1)) 0))))
  (= LsCommaPos__cp10__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call35 1)) 0))))
  (= LsCommaPos__cp11__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call35 1)) 0))))
  (= LsCommaPos__cp12__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call35 1)) 0))))
  (= LsCommaPos__cp13__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call35 1)) 0))))
  (= LsCommaPos__cp14__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call35 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call35 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call35 1)) 0))))
  (= LsCommaPos__cp15__call35
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call35 1))))))
(assert (let ((a!1 (ite (= 1 12)
                LsCommaPos__cp12__call36
                (ite (= 1 13)
                     LsCommaPos__cp13__call36
                     (ite (= 1 14)
                          LsCommaPos__cp14__call36
                          LsCommaPos__cp15__call36)))))
(let ((a!2 (ite (= 1 9)
                LsCommaPos__cp9__call36
                (ite (= 1 10)
                     LsCommaPos__cp10__call36
                     (ite (= 1 11) LsCommaPos__cp11__call36 a!1)))))
(let ((a!3 (ite (= 1 6)
                LsCommaPos__cp6__call36
                (ite (= 1 7)
                     LsCommaPos__cp7__call36
                     (ite (= 1 8) LsCommaPos__cp8__call36 a!2)))))
(let ((a!4 (ite (= 1 3)
                LsCommaPos__cp3__call36
                (ite (= 1 4)
                     LsCommaPos__cp4__call36
                     (ite (= 1 5) LsCommaPos__cp5__call36 a!3)))))
(let ((a!5 (ite (= 1 0)
                LsCommaPos__cp0__call36
                (ite (= 1 1)
                     LsCommaPos__cp1__call36
                     (ite (= 1 2) LsCommaPos__cp2__call36 a!4)))))
  (= LsCountElem__cP1__call34 (ite (< 1 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call36
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call36 1)) 0))))
  (= LsCommaPos__cp1__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call36 1)) 0))))
  (= LsCommaPos__cp2__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call36 1)) 0))))
  (= LsCommaPos__cp3__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call36 1)) 0))))
  (= LsCommaPos__cp4__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call36 1)) 0))))
  (= LsCommaPos__cp5__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call36 1)) 0))))
  (= LsCommaPos__cp6__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call36 1)) 0))))
  (= LsCommaPos__cp7__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call36 1)) 0))))
  (= LsCommaPos__cp8__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call36 1)) 0))))
  (= LsCommaPos__cp9__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call36 1)) 0))))
  (= LsCommaPos__cp10__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call36 1)) 0))))
  (= LsCommaPos__cp11__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call36 1)) 0))))
  (= LsCommaPos__cp12__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call36 1)) 0))))
  (= LsCommaPos__cp13__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call36 1)) 0))))
  (= LsCommaPos__cp14__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call36 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call36 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call36 1)) 0))))
  (= LsCommaPos__cp15__call36
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call36 1))))))
(assert (let ((a!1 (ite (= 2 12)
                LsCommaPos__cp12__call37
                (ite (= 2 13)
                     LsCommaPos__cp13__call37
                     (ite (= 2 14)
                          LsCommaPos__cp14__call37
                          LsCommaPos__cp15__call37)))))
(let ((a!2 (ite (= 2 9)
                LsCommaPos__cp9__call37
                (ite (= 2 10)
                     LsCommaPos__cp10__call37
                     (ite (= 2 11) LsCommaPos__cp11__call37 a!1)))))
(let ((a!3 (ite (= 2 6)
                LsCommaPos__cp6__call37
                (ite (= 2 7)
                     LsCommaPos__cp7__call37
                     (ite (= 2 8) LsCommaPos__cp8__call37 a!2)))))
(let ((a!4 (ite (= 2 3)
                LsCommaPos__cp3__call37
                (ite (= 2 4)
                     LsCommaPos__cp4__call37
                     (ite (= 2 5) LsCommaPos__cp5__call37 a!3)))))
(let ((a!5 (ite (= 2 0)
                LsCommaPos__cp0__call37
                (ite (= 2 1)
                     LsCommaPos__cp1__call37
                     (ite (= 2 2) LsCommaPos__cp2__call37 a!4)))))
  (= LsCountElem__cP2__call34 (ite (< 2 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call37
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call37 1)) 0))))
  (= LsCommaPos__cp1__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call37 1)) 0))))
  (= LsCommaPos__cp2__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call37 1)) 0))))
  (= LsCommaPos__cp3__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call37 1)) 0))))
  (= LsCommaPos__cp4__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call37 1)) 0))))
  (= LsCommaPos__cp5__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call37 1)) 0))))
  (= LsCommaPos__cp6__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call37 1)) 0))))
  (= LsCommaPos__cp7__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call37 1)) 0))))
  (= LsCommaPos__cp8__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call37 1)) 0))))
  (= LsCommaPos__cp9__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call37 1)) 0))))
  (= LsCommaPos__cp10__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call37 1)) 0))))
  (= LsCommaPos__cp11__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call37 1)) 0))))
  (= LsCommaPos__cp12__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call37 1)) 0))))
  (= LsCommaPos__cp13__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call37 1)) 0))))
  (= LsCommaPos__cp14__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call37 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call37 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call37 1)) 0))))
  (= LsCommaPos__cp15__call37
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call37 1))))))
(assert (let ((a!1 (ite (= 3 12)
                LsCommaPos__cp12__call38
                (ite (= 3 13)
                     LsCommaPos__cp13__call38
                     (ite (= 3 14)
                          LsCommaPos__cp14__call38
                          LsCommaPos__cp15__call38)))))
(let ((a!2 (ite (= 3 9)
                LsCommaPos__cp9__call38
                (ite (= 3 10)
                     LsCommaPos__cp10__call38
                     (ite (= 3 11) LsCommaPos__cp11__call38 a!1)))))
(let ((a!3 (ite (= 3 6)
                LsCommaPos__cp6__call38
                (ite (= 3 7)
                     LsCommaPos__cp7__call38
                     (ite (= 3 8) LsCommaPos__cp8__call38 a!2)))))
(let ((a!4 (ite (= 3 3)
                LsCommaPos__cp3__call38
                (ite (= 3 4)
                     LsCommaPos__cp4__call38
                     (ite (= 3 5) LsCommaPos__cp5__call38 a!3)))))
(let ((a!5 (ite (= 3 0)
                LsCommaPos__cp0__call38
                (ite (= 3 1)
                     LsCommaPos__cp1__call38
                     (ite (= 3 2) LsCommaPos__cp2__call38 a!4)))))
  (= LsCountElem__cP3__call34 (ite (< 3 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call38
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call38 1)) 0))))
  (= LsCommaPos__cp1__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call38 1)) 0))))
  (= LsCommaPos__cp2__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call38 1)) 0))))
  (= LsCommaPos__cp3__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call38 1)) 0))))
  (= LsCommaPos__cp4__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call38 1)) 0))))
  (= LsCommaPos__cp5__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call38 1)) 0))))
  (= LsCommaPos__cp6__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call38 1)) 0))))
  (= LsCommaPos__cp7__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call38 1)) 0))))
  (= LsCommaPos__cp8__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call38 1)) 0))))
  (= LsCommaPos__cp9__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call38 1)) 0))))
  (= LsCommaPos__cp10__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call38 1)) 0))))
  (= LsCommaPos__cp11__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call38 1)) 0))))
  (= LsCommaPos__cp12__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call38 1)) 0))))
  (= LsCommaPos__cp13__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call38 1)) 0))))
  (= LsCommaPos__cp14__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call38 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call38 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call38 1)) 0))))
  (= LsCommaPos__cp15__call38
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call38 1))))))
(assert (let ((a!1 (ite (= 4 12)
                LsCommaPos__cp12__call39
                (ite (= 4 13)
                     LsCommaPos__cp13__call39
                     (ite (= 4 14)
                          LsCommaPos__cp14__call39
                          LsCommaPos__cp15__call39)))))
(let ((a!2 (ite (= 4 9)
                LsCommaPos__cp9__call39
                (ite (= 4 10)
                     LsCommaPos__cp10__call39
                     (ite (= 4 11) LsCommaPos__cp11__call39 a!1)))))
(let ((a!3 (ite (= 4 6)
                LsCommaPos__cp6__call39
                (ite (= 4 7)
                     LsCommaPos__cp7__call39
                     (ite (= 4 8) LsCommaPos__cp8__call39 a!2)))))
(let ((a!4 (ite (= 4 3)
                LsCommaPos__cp3__call39
                (ite (= 4 4)
                     LsCommaPos__cp4__call39
                     (ite (= 4 5) LsCommaPos__cp5__call39 a!3)))))
(let ((a!5 (ite (= 4 0)
                LsCommaPos__cp0__call39
                (ite (= 4 1)
                     LsCommaPos__cp1__call39
                     (ite (= 4 2) LsCommaPos__cp2__call39 a!4)))))
  (= LsCountElem__cP4__call34 (ite (< 4 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call39
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call39 1)) 0))))
  (= LsCommaPos__cp1__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call39 1)) 0))))
  (= LsCommaPos__cp2__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call39 1)) 0))))
  (= LsCommaPos__cp3__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call39 1)) 0))))
  (= LsCommaPos__cp4__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call39 1)) 0))))
  (= LsCommaPos__cp5__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call39 1)) 0))))
  (= LsCommaPos__cp6__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call39 1)) 0))))
  (= LsCommaPos__cp7__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call39 1)) 0))))
  (= LsCommaPos__cp8__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call39 1)) 0))))
  (= LsCommaPos__cp9__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call39 1)) 0))))
  (= LsCommaPos__cp10__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call39 1)) 0))))
  (= LsCommaPos__cp11__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call39 1)) 0))))
  (= LsCommaPos__cp12__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call39 1)) 0))))
  (= LsCommaPos__cp13__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call39 1)) 0))))
  (= LsCommaPos__cp14__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call39 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call39 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call39 1)) 0))))
  (= LsCommaPos__cp15__call39
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call39 1))))))
(assert (let ((a!1 (ite (= 5 12)
                LsCommaPos__cp12__call40
                (ite (= 5 13)
                     LsCommaPos__cp13__call40
                     (ite (= 5 14)
                          LsCommaPos__cp14__call40
                          LsCommaPos__cp15__call40)))))
(let ((a!2 (ite (= 5 9)
                LsCommaPos__cp9__call40
                (ite (= 5 10)
                     LsCommaPos__cp10__call40
                     (ite (= 5 11) LsCommaPos__cp11__call40 a!1)))))
(let ((a!3 (ite (= 5 6)
                LsCommaPos__cp6__call40
                (ite (= 5 7)
                     LsCommaPos__cp7__call40
                     (ite (= 5 8) LsCommaPos__cp8__call40 a!2)))))
(let ((a!4 (ite (= 5 3)
                LsCommaPos__cp3__call40
                (ite (= 5 4)
                     LsCommaPos__cp4__call40
                     (ite (= 5 5) LsCommaPos__cp5__call40 a!3)))))
(let ((a!5 (ite (= 5 0)
                LsCommaPos__cp0__call40
                (ite (= 5 1)
                     LsCommaPos__cp1__call40
                     (ite (= 5 2) LsCommaPos__cp2__call40 a!4)))))
  (= LsCountElem__cP5__call34 (ite (< 5 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call40
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call40 1)) 0))))
  (= LsCommaPos__cp1__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call40 1)) 0))))
  (= LsCommaPos__cp2__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call40 1)) 0))))
  (= LsCommaPos__cp3__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call40 1)) 0))))
  (= LsCommaPos__cp4__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call40 1)) 0))))
  (= LsCommaPos__cp5__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call40 1)) 0))))
  (= LsCommaPos__cp6__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call40 1)) 0))))
  (= LsCommaPos__cp7__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call40 1)) 0))))
  (= LsCommaPos__cp8__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call40 1)) 0))))
  (= LsCommaPos__cp9__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call40 1)) 0))))
  (= LsCommaPos__cp10__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call40 1)) 0))))
  (= LsCommaPos__cp11__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call40 1)) 0))))
  (= LsCommaPos__cp12__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call40 1)) 0))))
  (= LsCommaPos__cp13__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call40 1)) 0))))
  (= LsCommaPos__cp14__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call40 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call40 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call40 1)) 0))))
  (= LsCommaPos__cp15__call40
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call40 1))))))
(assert (let ((a!1 (ite (= 6 12)
                LsCommaPos__cp12__call41
                (ite (= 6 13)
                     LsCommaPos__cp13__call41
                     (ite (= 6 14)
                          LsCommaPos__cp14__call41
                          LsCommaPos__cp15__call41)))))
(let ((a!2 (ite (= 6 9)
                LsCommaPos__cp9__call41
                (ite (= 6 10)
                     LsCommaPos__cp10__call41
                     (ite (= 6 11) LsCommaPos__cp11__call41 a!1)))))
(let ((a!3 (ite (= 6 6)
                LsCommaPos__cp6__call41
                (ite (= 6 7)
                     LsCommaPos__cp7__call41
                     (ite (= 6 8) LsCommaPos__cp8__call41 a!2)))))
(let ((a!4 (ite (= 6 3)
                LsCommaPos__cp3__call41
                (ite (= 6 4)
                     LsCommaPos__cp4__call41
                     (ite (= 6 5) LsCommaPos__cp5__call41 a!3)))))
(let ((a!5 (ite (= 6 0)
                LsCommaPos__cp0__call41
                (ite (= 6 1)
                     LsCommaPos__cp1__call41
                     (ite (= 6 2) LsCommaPos__cp2__call41 a!4)))))
  (= LsCountElem__cP6__call34 (ite (< 6 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call41
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call41 1)) 0))))
  (= LsCommaPos__cp1__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call41 1)) 0))))
  (= LsCommaPos__cp2__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call41 1)) 0))))
  (= LsCommaPos__cp3__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call41 1)) 0))))
  (= LsCommaPos__cp4__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call41 1)) 0))))
  (= LsCommaPos__cp5__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call41 1)) 0))))
  (= LsCommaPos__cp6__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call41 1)) 0))))
  (= LsCommaPos__cp7__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call41 1)) 0))))
  (= LsCommaPos__cp8__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call41 1)) 0))))
  (= LsCommaPos__cp9__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call41 1)) 0))))
  (= LsCommaPos__cp10__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call41 1)) 0))))
  (= LsCommaPos__cp11__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call41 1)) 0))))
  (= LsCommaPos__cp12__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call41 1)) 0))))
  (= LsCommaPos__cp13__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call41 1)) 0))))
  (= LsCommaPos__cp14__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call41 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call41 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call41 1)) 0))))
  (= LsCommaPos__cp15__call41
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call41 1))))))
(assert (let ((a!1 (ite (= 7 12)
                LsCommaPos__cp12__call42
                (ite (= 7 13)
                     LsCommaPos__cp13__call42
                     (ite (= 7 14)
                          LsCommaPos__cp14__call42
                          LsCommaPos__cp15__call42)))))
(let ((a!2 (ite (= 7 9)
                LsCommaPos__cp9__call42
                (ite (= 7 10)
                     LsCommaPos__cp10__call42
                     (ite (= 7 11) LsCommaPos__cp11__call42 a!1)))))
(let ((a!3 (ite (= 7 6)
                LsCommaPos__cp6__call42
                (ite (= 7 7)
                     LsCommaPos__cp7__call42
                     (ite (= 7 8) LsCommaPos__cp8__call42 a!2)))))
(let ((a!4 (ite (= 7 3)
                LsCommaPos__cp3__call42
                (ite (= 7 4)
                     LsCommaPos__cp4__call42
                     (ite (= 7 5) LsCommaPos__cp5__call42 a!3)))))
(let ((a!5 (ite (= 7 0)
                LsCommaPos__cp0__call42
                (ite (= 7 1)
                     LsCommaPos__cp1__call42
                     (ite (= 7 2) LsCommaPos__cp2__call42 a!4)))))
  (= LsCountElem__cP7__call34 (ite (< 7 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call42
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call42 1)) 0))))
  (= LsCommaPos__cp1__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call42 1)) 0))))
  (= LsCommaPos__cp2__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call42 1)) 0))))
  (= LsCommaPos__cp3__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call42 1)) 0))))
  (= LsCommaPos__cp4__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call42 1)) 0))))
  (= LsCommaPos__cp5__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call42 1)) 0))))
  (= LsCommaPos__cp6__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call42 1)) 0))))
  (= LsCommaPos__cp7__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call42 1)) 0))))
  (= LsCommaPos__cp8__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call42 1)) 0))))
  (= LsCommaPos__cp9__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call42 1)) 0))))
  (= LsCommaPos__cp10__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call42 1)) 0))))
  (= LsCommaPos__cp11__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call42 1)) 0))))
  (= LsCommaPos__cp12__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call42 1)) 0))))
  (= LsCommaPos__cp13__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call42 1)) 0))))
  (= LsCommaPos__cp14__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call42 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call42 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call42 1)) 0))))
  (= LsCommaPos__cp15__call42
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call42 1))))))
(assert (let ((a!1 (ite (= 8 12)
                LsCommaPos__cp12__call43
                (ite (= 8 13)
                     LsCommaPos__cp13__call43
                     (ite (= 8 14)
                          LsCommaPos__cp14__call43
                          LsCommaPos__cp15__call43)))))
(let ((a!2 (ite (= 8 9)
                LsCommaPos__cp9__call43
                (ite (= 8 10)
                     LsCommaPos__cp10__call43
                     (ite (= 8 11) LsCommaPos__cp11__call43 a!1)))))
(let ((a!3 (ite (= 8 6)
                LsCommaPos__cp6__call43
                (ite (= 8 7)
                     LsCommaPos__cp7__call43
                     (ite (= 8 8) LsCommaPos__cp8__call43 a!2)))))
(let ((a!4 (ite (= 8 3)
                LsCommaPos__cp3__call43
                (ite (= 8 4)
                     LsCommaPos__cp4__call43
                     (ite (= 8 5) LsCommaPos__cp5__call43 a!3)))))
(let ((a!5 (ite (= 8 0)
                LsCommaPos__cp0__call43
                (ite (= 8 1)
                     LsCommaPos__cp1__call43
                     (ite (= 8 2) LsCommaPos__cp2__call43 a!4)))))
  (= LsCountElem__cP8__call34 (ite (< 8 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call43
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call43 1)) 0))))
  (= LsCommaPos__cp1__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call43 1)) 0))))
  (= LsCommaPos__cp2__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call43 1)) 0))))
  (= LsCommaPos__cp3__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call43 1)) 0))))
  (= LsCommaPos__cp4__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call43 1)) 0))))
  (= LsCommaPos__cp5__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call43 1)) 0))))
  (= LsCommaPos__cp6__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call43 1)) 0))))
  (= LsCommaPos__cp7__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call43 1)) 0))))
  (= LsCommaPos__cp8__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call43 1)) 0))))
  (= LsCommaPos__cp9__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call43 1)) 0))))
  (= LsCommaPos__cp10__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call43 1)) 0))))
  (= LsCommaPos__cp11__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call43 1)) 0))))
  (= LsCommaPos__cp12__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call43 1)) 0))))
  (= LsCommaPos__cp13__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call43 1)) 0))))
  (= LsCommaPos__cp14__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call43 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call43 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call43 1)) 0))))
  (= LsCommaPos__cp15__call43
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call43 1))))))
(assert (let ((a!1 (ite (= 9 12)
                LsCommaPos__cp12__call44
                (ite (= 9 13)
                     LsCommaPos__cp13__call44
                     (ite (= 9 14)
                          LsCommaPos__cp14__call44
                          LsCommaPos__cp15__call44)))))
(let ((a!2 (ite (= 9 9)
                LsCommaPos__cp9__call44
                (ite (= 9 10)
                     LsCommaPos__cp10__call44
                     (ite (= 9 11) LsCommaPos__cp11__call44 a!1)))))
(let ((a!3 (ite (= 9 6)
                LsCommaPos__cp6__call44
                (ite (= 9 7)
                     LsCommaPos__cp7__call44
                     (ite (= 9 8) LsCommaPos__cp8__call44 a!2)))))
(let ((a!4 (ite (= 9 3)
                LsCommaPos__cp3__call44
                (ite (= 9 4)
                     LsCommaPos__cp4__call44
                     (ite (= 9 5) LsCommaPos__cp5__call44 a!3)))))
(let ((a!5 (ite (= 9 0)
                LsCommaPos__cp0__call44
                (ite (= 9 1)
                     LsCommaPos__cp1__call44
                     (ite (= 9 2) LsCommaPos__cp2__call44 a!4)))))
  (= LsCountElem__cP9__call34 (ite (< 9 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call44
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call44 1)) 0))))
  (= LsCommaPos__cp1__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call44 1)) 0))))
  (= LsCommaPos__cp2__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call44 1)) 0))))
  (= LsCommaPos__cp3__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call44 1)) 0))))
  (= LsCommaPos__cp4__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call44 1)) 0))))
  (= LsCommaPos__cp5__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call44 1)) 0))))
  (= LsCommaPos__cp6__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call44 1)) 0))))
  (= LsCommaPos__cp7__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call44 1)) 0))))
  (= LsCommaPos__cp8__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call44 1)) 0))))
  (= LsCommaPos__cp9__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call44 1)) 0))))
  (= LsCommaPos__cp10__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call44 1)) 0))))
  (= LsCommaPos__cp11__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call44 1)) 0))))
  (= LsCommaPos__cp12__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call44 1)) 0))))
  (= LsCommaPos__cp13__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call44 1)) 0))))
  (= LsCommaPos__cp14__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call44 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call44 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call44 1)) 0))))
  (= LsCommaPos__cp15__call44
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call44 1))))))
(assert (let ((a!1 (ite (= 10 12)
                LsCommaPos__cp12__call45
                (ite (= 10 13)
                     LsCommaPos__cp13__call45
                     (ite (= 10 14)
                          LsCommaPos__cp14__call45
                          LsCommaPos__cp15__call45)))))
(let ((a!2 (ite (= 10 9)
                LsCommaPos__cp9__call45
                (ite (= 10 10)
                     LsCommaPos__cp10__call45
                     (ite (= 10 11) LsCommaPos__cp11__call45 a!1)))))
(let ((a!3 (ite (= 10 6)
                LsCommaPos__cp6__call45
                (ite (= 10 7)
                     LsCommaPos__cp7__call45
                     (ite (= 10 8) LsCommaPos__cp8__call45 a!2)))))
(let ((a!4 (ite (= 10 3)
                LsCommaPos__cp3__call45
                (ite (= 10 4)
                     LsCommaPos__cp4__call45
                     (ite (= 10 5) LsCommaPos__cp5__call45 a!3)))))
(let ((a!5 (ite (= 10 0)
                LsCommaPos__cp0__call45
                (ite (= 10 1)
                     LsCommaPos__cp1__call45
                     (ite (= 10 2) LsCommaPos__cp2__call45 a!4)))))
  (= LsCountElem__cP10__call34 (ite (< 10 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call45
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call45 1)) 0))))
  (= LsCommaPos__cp1__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call45 1)) 0))))
  (= LsCommaPos__cp2__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call45 1)) 0))))
  (= LsCommaPos__cp3__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call45 1)) 0))))
  (= LsCommaPos__cp4__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call45 1)) 0))))
  (= LsCommaPos__cp5__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call45 1)) 0))))
  (= LsCommaPos__cp6__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call45 1)) 0))))
  (= LsCommaPos__cp7__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call45 1)) 0))))
  (= LsCommaPos__cp8__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call45 1)) 0))))
  (= LsCommaPos__cp9__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call45 1)) 0))))
  (= LsCommaPos__cp10__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call45 1)) 0))))
  (= LsCommaPos__cp11__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call45 1)) 0))))
  (= LsCommaPos__cp12__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call45 1)) 0))))
  (= LsCommaPos__cp13__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call45 1)) 0))))
  (= LsCommaPos__cp14__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call45 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call45 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call45 1)) 0))))
  (= LsCommaPos__cp15__call45
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call45 1))))))
(assert (let ((a!1 (ite (= 11 12)
                LsCommaPos__cp12__call46
                (ite (= 11 13)
                     LsCommaPos__cp13__call46
                     (ite (= 11 14)
                          LsCommaPos__cp14__call46
                          LsCommaPos__cp15__call46)))))
(let ((a!2 (ite (= 11 9)
                LsCommaPos__cp9__call46
                (ite (= 11 10)
                     LsCommaPos__cp10__call46
                     (ite (= 11 11) LsCommaPos__cp11__call46 a!1)))))
(let ((a!3 (ite (= 11 6)
                LsCommaPos__cp6__call46
                (ite (= 11 7)
                     LsCommaPos__cp7__call46
                     (ite (= 11 8) LsCommaPos__cp8__call46 a!2)))))
(let ((a!4 (ite (= 11 3)
                LsCommaPos__cp3__call46
                (ite (= 11 4)
                     LsCommaPos__cp4__call46
                     (ite (= 11 5) LsCommaPos__cp5__call46 a!3)))))
(let ((a!5 (ite (= 11 0)
                LsCommaPos__cp0__call46
                (ite (= 11 1)
                     LsCommaPos__cp1__call46
                     (ite (= 11 2) LsCommaPos__cp2__call46 a!4)))))
  (= LsCountElem__cP11__call34 (ite (< 11 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call46
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call46 1)) 0))))
  (= LsCommaPos__cp1__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call46 1)) 0))))
  (= LsCommaPos__cp2__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call46 1)) 0))))
  (= LsCommaPos__cp3__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call46 1)) 0))))
  (= LsCommaPos__cp4__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call46 1)) 0))))
  (= LsCommaPos__cp5__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call46 1)) 0))))
  (= LsCommaPos__cp6__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call46 1)) 0))))
  (= LsCommaPos__cp7__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call46 1)) 0))))
  (= LsCommaPos__cp8__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call46 1)) 0))))
  (= LsCommaPos__cp9__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call46 1)) 0))))
  (= LsCommaPos__cp10__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call46 1)) 0))))
  (= LsCommaPos__cp11__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call46 1)) 0))))
  (= LsCommaPos__cp12__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call46 1)) 0))))
  (= LsCommaPos__cp13__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call46 1)) 0))))
  (= LsCommaPos__cp14__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call46 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call46 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call46 1)) 0))))
  (= LsCommaPos__cp15__call46
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call46 1))))))
(assert (let ((a!1 (ite (= 12 12)
                LsCommaPos__cp12__call47
                (ite (= 12 13)
                     LsCommaPos__cp13__call47
                     (ite (= 12 14)
                          LsCommaPos__cp14__call47
                          LsCommaPos__cp15__call47)))))
(let ((a!2 (ite (= 12 9)
                LsCommaPos__cp9__call47
                (ite (= 12 10)
                     LsCommaPos__cp10__call47
                     (ite (= 12 11) LsCommaPos__cp11__call47 a!1)))))
(let ((a!3 (ite (= 12 6)
                LsCommaPos__cp6__call47
                (ite (= 12 7)
                     LsCommaPos__cp7__call47
                     (ite (= 12 8) LsCommaPos__cp8__call47 a!2)))))
(let ((a!4 (ite (= 12 3)
                LsCommaPos__cp3__call47
                (ite (= 12 4)
                     LsCommaPos__cp4__call47
                     (ite (= 12 5) LsCommaPos__cp5__call47 a!3)))))
(let ((a!5 (ite (= 12 0)
                LsCommaPos__cp0__call47
                (ite (= 12 1)
                     LsCommaPos__cp1__call47
                     (ite (= 12 2) LsCommaPos__cp2__call47 a!4)))))
  (= LsCountElem__cP12__call34 (ite (< 12 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call47
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call47 1)) 0))))
  (= LsCommaPos__cp1__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call47 1)) 0))))
  (= LsCommaPos__cp2__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call47 1)) 0))))
  (= LsCommaPos__cp3__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call47 1)) 0))))
  (= LsCommaPos__cp4__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call47 1)) 0))))
  (= LsCommaPos__cp5__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call47 1)) 0))))
  (= LsCommaPos__cp6__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call47 1)) 0))))
  (= LsCommaPos__cp7__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call47 1)) 0))))
  (= LsCommaPos__cp8__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call47 1)) 0))))
  (= LsCommaPos__cp9__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call47 1)) 0))))
  (= LsCommaPos__cp10__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call47 1)) 0))))
  (= LsCommaPos__cp11__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call47 1)) 0))))
  (= LsCommaPos__cp12__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call47 1)) 0))))
  (= LsCommaPos__cp13__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call47 1)) 0))))
  (= LsCommaPos__cp14__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call47 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call47 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call47 1)) 0))))
  (= LsCommaPos__cp15__call47
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call47 1))))))
(assert (let ((a!1 (ite (= 13 12)
                LsCommaPos__cp12__call48
                (ite (= 13 13)
                     LsCommaPos__cp13__call48
                     (ite (= 13 14)
                          LsCommaPos__cp14__call48
                          LsCommaPos__cp15__call48)))))
(let ((a!2 (ite (= 13 9)
                LsCommaPos__cp9__call48
                (ite (= 13 10)
                     LsCommaPos__cp10__call48
                     (ite (= 13 11) LsCommaPos__cp11__call48 a!1)))))
(let ((a!3 (ite (= 13 6)
                LsCommaPos__cp6__call48
                (ite (= 13 7)
                     LsCommaPos__cp7__call48
                     (ite (= 13 8) LsCommaPos__cp8__call48 a!2)))))
(let ((a!4 (ite (= 13 3)
                LsCommaPos__cp3__call48
                (ite (= 13 4)
                     LsCommaPos__cp4__call48
                     (ite (= 13 5) LsCommaPos__cp5__call48 a!3)))))
(let ((a!5 (ite (= 13 0)
                LsCommaPos__cp0__call48
                (ite (= 13 1)
                     LsCommaPos__cp1__call48
                     (ite (= 13 2) LsCommaPos__cp2__call48 a!4)))))
  (= LsCountElem__cP13__call34 (ite (< 13 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call48
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call48 1)) 0))))
  (= LsCommaPos__cp1__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call48 1)) 0))))
  (= LsCommaPos__cp2__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call48 1)) 0))))
  (= LsCommaPos__cp3__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call48 1)) 0))))
  (= LsCommaPos__cp4__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call48 1)) 0))))
  (= LsCommaPos__cp5__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call48 1)) 0))))
  (= LsCommaPos__cp6__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call48 1)) 0))))
  (= LsCommaPos__cp7__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call48 1)) 0))))
  (= LsCommaPos__cp8__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call48 1)) 0))))
  (= LsCommaPos__cp9__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call48 1)) 0))))
  (= LsCommaPos__cp10__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call48 1)) 0))))
  (= LsCommaPos__cp11__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call48 1)) 0))))
  (= LsCommaPos__cp12__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call48 1)) 0))))
  (= LsCommaPos__cp13__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call48 1)) 0))))
  (= LsCommaPos__cp14__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call48 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call48 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call48 1)) 0))))
  (= LsCommaPos__cp15__call48
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call48 1))))))
(assert (let ((a!1 (ite (= 14 12)
                LsCommaPos__cp12__call49
                (ite (= 14 13)
                     LsCommaPos__cp13__call49
                     (ite (= 14 14)
                          LsCommaPos__cp14__call49
                          LsCommaPos__cp15__call49)))))
(let ((a!2 (ite (= 14 9)
                LsCommaPos__cp9__call49
                (ite (= 14 10)
                     LsCommaPos__cp10__call49
                     (ite (= 14 11) LsCommaPos__cp11__call49 a!1)))))
(let ((a!3 (ite (= 14 6)
                LsCommaPos__cp6__call49
                (ite (= 14 7)
                     LsCommaPos__cp7__call49
                     (ite (= 14 8) LsCommaPos__cp8__call49 a!2)))))
(let ((a!4 (ite (= 14 3)
                LsCommaPos__cp3__call49
                (ite (= 14 4)
                     LsCommaPos__cp4__call49
                     (ite (= 14 5) LsCommaPos__cp5__call49 a!3)))))
(let ((a!5 (ite (= 14 0)
                LsCommaPos__cp0__call49
                (ite (= 14 1)
                     LsCommaPos__cp1__call49
                     (ite (= 14 2) LsCommaPos__cp2__call49 a!4)))))
  (= LsCountElem__cP14__call34 (ite (< 14 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call49
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call49 1)) 0))))
  (= LsCommaPos__cp1__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call49 1)) 0))))
  (= LsCommaPos__cp2__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call49 1)) 0))))
  (= LsCommaPos__cp3__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call49 1)) 0))))
  (= LsCommaPos__cp4__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call49 1)) 0))))
  (= LsCommaPos__cp5__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call49 1)) 0))))
  (= LsCommaPos__cp6__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call49 1)) 0))))
  (= LsCommaPos__cp7__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call49 1)) 0))))
  (= LsCommaPos__cp8__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call49 1)) 0))))
  (= LsCommaPos__cp9__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call49 1)) 0))))
  (= LsCommaPos__cp10__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call49 1)) 0))))
  (= LsCommaPos__cp11__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call49 1)) 0))))
  (= LsCommaPos__cp12__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call49 1)) 0))))
  (= LsCommaPos__cp13__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call49 1)) 0))))
  (= LsCommaPos__cp14__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call49 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call49 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call49 1)) 0))))
  (= LsCommaPos__cp15__call49
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call49 1))))))
(assert (let ((a!1 (ite (= 15 12)
                LsCommaPos__cp12__call50
                (ite (= 15 13)
                     LsCommaPos__cp13__call50
                     (ite (= 15 14)
                          LsCommaPos__cp14__call50
                          LsCommaPos__cp15__call50)))))
(let ((a!2 (ite (= 15 9)
                LsCommaPos__cp9__call50
                (ite (= 15 10)
                     LsCommaPos__cp10__call50
                     (ite (= 15 11) LsCommaPos__cp11__call50 a!1)))))
(let ((a!3 (ite (= 15 6)
                LsCommaPos__cp6__call50
                (ite (= 15 7)
                     LsCommaPos__cp7__call50
                     (ite (= 15 8) LsCommaPos__cp8__call50 a!2)))))
(let ((a!4 (ite (= 15 3)
                LsCommaPos__cp3__call50
                (ite (= 15 4)
                     LsCommaPos__cp4__call50
                     (ite (= 15 5) LsCommaPos__cp5__call50 a!3)))))
(let ((a!5 (ite (= 15 0)
                LsCommaPos__cp0__call50
                (ite (= 15 1)
                     LsCommaPos__cp1__call50
                     (ite (= 15 2) LsCommaPos__cp2__call50 a!4)))))
  (= LsCountElem__cP15__call34 (ite (< 15 0) (- 0 1) a!5))))))))
(assert (= LsCommaPos__cp0__call50
   (ite (< (str.indexof tk_inside "," 0) 0)
        (str.len tk_inside)
        (str.indexof tk_inside "," 0))))
(assert (let ((a!1 (or (>= LsCommaPos__cp0__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp0__call50 1)) 0))))
  (= LsCommaPos__cp1__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp0__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp1__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp1__call50 1)) 0))))
  (= LsCommaPos__cp2__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp1__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp2__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp2__call50 1)) 0))))
  (= LsCommaPos__cp3__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp2__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp3__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp3__call50 1)) 0))))
  (= LsCommaPos__cp4__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp3__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp4__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp4__call50 1)) 0))))
  (= LsCommaPos__cp5__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp4__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp5__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp5__call50 1)) 0))))
  (= LsCommaPos__cp6__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp5__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp6__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp6__call50 1)) 0))))
  (= LsCommaPos__cp7__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp6__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp7__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp7__call50 1)) 0))))
  (= LsCommaPos__cp8__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp7__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp8__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp8__call50 1)) 0))))
  (= LsCommaPos__cp9__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp8__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp9__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp9__call50 1)) 0))))
  (= LsCommaPos__cp10__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp9__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp10__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp10__call50 1)) 0))))
  (= LsCommaPos__cp11__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp10__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp11__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp11__call50 1)) 0))))
  (= LsCommaPos__cp12__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp11__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp12__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp12__call50 1)) 0))))
  (= LsCommaPos__cp13__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp12__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp13__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp13__call50 1)) 0))))
  (= LsCommaPos__cp14__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp13__call50 1))))))
(assert (let ((a!1 (or (>= LsCommaPos__cp14__call50 (str.len tk_inside))
               (< (str.indexof tk_inside "," (+ LsCommaPos__cp14__call50 1)) 0))))
  (= LsCommaPos__cp15__call50
     (ite a!1
          (str.len tk_inside)
          (str.indexof tk_inside "," (+ LsCommaPos__cp14__call50 1))))))
(assert (= eff_nop (LibCall "libc" "getpid" __Empty_LibArg)))
(assert (= eff_out
   (LibCall "libc" "puts" (__Cell_LibArg (ArgStr tk_print_now) __Empty_LibArg))))
(assert (= tk_read_go
   (or (= tk_ph 2)
       (and tk_src
            (or tk_is_top tk_default_plain tk_drop_bound tk_rewrite_bound))
       tk_loop_done
       tk_walk_done)))
(assert (let ((a!1 (and (= effects__len 2)
                (= (select effects 0) eff_nop)
                (= (select effects 1) ReadLine)))
      (a!2 (=> tk_eof_now
               (and (= effects__len 2)
                    (= (select effects 0) (Exit 0))
                    (= (select effects 1) eff_nop))))
      (a!3 (=> (and tk_emit tk_read_go)
               (and (= effects__len 2)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) ReadLine))))
      (a!4 (=> tk_emit
               (and (= effects__len 2)
                    (= (select effects 0) eff_out)
                    (= (select effects 1) eff_nop))))
      (a!5 (=> (not tk_read_go)
               (and (= effects__len 2)
                    (= (select effects 0) eff_nop)
                    (= (select effects 1) eff_nop)))))
(let ((a!6 (and a!4 (=> (not tk_emit) (and (=> tk_read_go a!1) a!5)))))
(let ((a!7 (and a!3 (=> (not (and tk_emit tk_read_go)) a!6))))
(let ((a!8 (=> (not is_first_tick) (and a!2 (=> (not tk_eof_now) a!7)))))
  (and (=> is_first_tick a!1) a!8))))))
(declare-fun _eff_nop () Effect)
(declare-fun _eff_out () Effect)
(declare-fun _tk_after_lead () Int)
(declare-fun _tk_at () Int)
(declare-fun _tk_bkey () String)
(declare-fun _tk_bound_d1 () Int)
(declare-fun _tk_bound_d2 () Int)
(declare-fun _tk_bound_hl () Bool)
(declare-fun _tk_bound_hl_at () Int)
(declare-fun _tk_bound_n () String)
(declare-fun _tk_bound_reg () Bool)
(declare-fun _tk_bv_e0 () Int)
(declare-fun _tk_bv_s0 () Int)
(declare-fun _tk_code () String)
(declare-fun _tk_count_el () Int)
(declare-fun _tk_d0 () Int)
(declare-fun _tk_d1 () Int)
(declare-fun _tk_d2 () Int)
(declare-fun _tk_decl_eq () Int)
(declare-fun _tk_decl_lit () Bool)
(declare-fun _tk_default () Bool)
(declare-fun _tk_default_plain () Bool)
(declare-fun _tk_drop_bound () Bool)
(declare-fun _tk_el () String)
(declare-fun _tk_emit () Bool)
(declare-fun _tk_enter_dual () Bool)
(declare-fun _tk_enter_loop () Bool)
(declare-fun _tk_eof_now () Bool)
(declare-fun _tk_glyph () String)
(declare-fun _tk_has_len_lines () Bool)
(declare-fun _tk_hash_after () String)
(declare-fun _tk_hash_aws () Int)
(declare-fun _tk_hh_e () Int)
(declare-fun _tk_hh_s () Int)
(declare-fun _tk_ie () Int)
(declare-fun _tk_ind () String)
(declare-fun _tk_inside () String)
(declare-fun _tk_inside_tl () Int)
(declare-fun _tk_is_assign () Bool)
(declare-fun _tk_is_bound_line () Bool)
(declare-fun _tk_is_decl () Bool)
(declare-fun _tk_is_hold () Bool)
(declare-fun _tk_is_litassign () Bool)
(declare-fun _tk_is_top () Bool)
(declare-fun _tk_key () String)
(declare-fun _tk_lead () String)
(declare-fun _tk_lead_base () String)
(declare-fun _tk_lead_is_dual () Bool)
(declare-fun _tk_len_lines () String)
(declare-fun _tk_loop_done () Bool)
(declare-fun _tk_loop_run () Bool)
(declare-fun _tk_lt () Int)
(declare-fun _tk_needs_walk () Bool)
(declare-fun _tk_ph () Int)
(declare-fun _tk_print_now () String)
(declare-fun _tk_rbase () String)
(declare-fun _tk_read_go () Bool)
(declare-fun _tk_reg_hit () Bool)
(declare-fun _tk_reg_line () Bool)
(declare-fun _tk_reof () Bool)
(declare-fun _tk_rewrite_bound () Bool)
(declare-fun _tk_rhaslen () Bool)
(declare-fun _tk_rhs () String)
(declare-fun _tk_rhs_s () Int)
(declare-fun _tk_rline () String)
(declare-fun _tk_rn () Int)
(declare-fun _tk_rt () Int)
(declare-fun _tk_slot_line () String)
(declare-fun _tk_slot_pfx () String)
(declare-fun _tk_src () Bool)
(declare-fun _tk_vs () Int)
(declare-fun _tk_walk_done () Bool)
(declare-fun _tk_walk_run () Bool)
(declare-fun _tk_ws () Int)
(declare-fun _tk_zdef () String)
(declare-fun _w_base () String)
(declare-fun _w_base_reg () Bool)
(declare-fun _w_cb () Int)
(declare-fun _w_ch () String)
(declare-fun _w_do_index () Bool)
(declare-fun _w_dot () Bool)
(declare-fun _w_fe () Int)
(declare-fun _w_field () String)
(declare-fun _w_followed_br () Bool)
(declare-fun _w_fs () Int)
(declare-fun _w_has_field () Bool)
(declare-fun _w_has_sub () Bool)
(declare-fun _w_he () Int)
(declare-fun _w_idx () String)
(declare-fun _w_idx_ok () Bool)
(declare-fun _w_index_end () Int)
(declare-fun _w_index_out () String)
(declare-fun _w_inner () String)
(declare-fun _w_is_hash () Bool)
(declare-fun _w_is_ident () Bool)
(declare-fun _w_next () Int)
(declare-fun _w_p () Int)
(declare-fun _w_scb () Int)
(declare-fun _w_sidx_ok () Bool)
(declare-fun _w_sinner () String)
(declare-fun _w_src () String)
(declare-fun _w_sub_br () Bool)
(declare-fun _w_tok () String)
(declare-fun _w_unit () String)
(declare-fun _w_we () Int)
(declare-fun _w_word () String)
(declare-fun _w_word_reg () Bool)
