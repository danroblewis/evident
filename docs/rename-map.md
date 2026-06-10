# compiler2 rename map

Old → new for every renamed variable in `compiler2/*.ev` (applied repo-wide
to `compiler2/` + `tests/compiler2_units/`; names compose by names-match, so
old names must have zero survivors). Names NOT in this table were kept
deliberately: kernel-reserved names (`effects`, `last_results`,
`is_first_tick`), claim/type/enum names and wire-format slot names
(`RtRecName`/`RtIdxOf`/`RtFieldAcc`/`VariantFieldType` slots, `C2*` items,
`Op*`, `Build*`), already-readable names (`pos`, `input`, `target`,
`lex_done`, `zstep`, `phase`, `tok_ready`, `fetch_go`, the `translate2_*`
claim interfaces), and claim-local helpers scoped to a single pure claim
(`ps_*` in C2PrattStep, `c2to_*`/`c2a_*`/`pc_*`, `rfa_*`, `vfc_*`/`vft_*`).

No `?? UNSURE` entries remain: every meaning below was decoded from the
module contract headers and usage.

## driver_window.ev + driver_lex.ev + lex_fti.ev — token window, fetch burst, lexer

| old | new | meaning |
| --- | --- | ------- |
| `tcur` | `tok_cur` | absolute index of next unconsumed token in the FTI buffer |
| `wend` | `fetch_end` | one past the fetched window coverage |
| `fmode` | `fetch_mode` | fetch state machine (0 idle, 1 read burst landed, 2 rebuild) |
| `wtoks` | `win_toks` | the decoded 8-token window |
| `w_rem` | `win_avail` | tokens remaining in the fetched window |
| `w_need` | `win_need` | lookahead need of the active parse mode |
| `f_cap` | `fetch_cap` | this is the capture tick of a read burst |
| `f_built` | `win_rebuilt` | the freshly decoded 8-token window |
| `f_a` | `fetch_addr` | byte address of the window head in the token buffer |
| `wt0` | `lat_tag0` | latched token tag register, slot 0 |
| `wq0` | `lat_pay0` | latched token payload register, slot 0 |
| `f_s0` | `is_str0` | slot 0 holds a string-carrying token (Ident/StringLit) |
| `cp0` | `copy_eff0` | cstr-copy effect for slot 0 string payload |
| `f_sr0` | `res_str0` | last_results[0] decoded as String |
| `f_tok0` | `dec_tok0` | decoded Token for window slot 0 |
| `rd_t0` | `read_tag0` | read-burst effect: tag of token 0 |
| `rd_q0` | `read_pay0` | read-burst effect: payload of token 0 |
| `ww_t0` | `tok0` | decoded window token 0 (head views) |
| `wt1` | `lat_tag1` | latched token tag register, slot 1 |
| `wq1` | `lat_pay1` | latched token payload register, slot 1 |
| `f_s1` | `is_str1` | slot 1 holds a string-carrying token (Ident/StringLit) |
| `cp1` | `copy_eff1` | cstr-copy effect for slot 1 string payload |
| `f_sr1` | `res_str1` | last_results[1] decoded as String |
| `f_tok1` | `dec_tok1` | decoded Token for window slot 1 |
| `rd_t1` | `read_tag1` | read-burst effect: tag of token 1 |
| `rd_q1` | `read_pay1` | read-burst effect: payload of token 1 |
| `ww_t1` | `tok1` | decoded window token 1 (head views) |
| `wt2` | `lat_tag2` | latched token tag register, slot 2 |
| `wq2` | `lat_pay2` | latched token payload register, slot 2 |
| `f_s2` | `is_str2` | slot 2 holds a string-carrying token (Ident/StringLit) |
| `cp2` | `copy_eff2` | cstr-copy effect for slot 2 string payload |
| `f_sr2` | `res_str2` | last_results[2] decoded as String |
| `f_tok2` | `dec_tok2` | decoded Token for window slot 2 |
| `rd_t2` | `read_tag2` | read-burst effect: tag of token 2 |
| `rd_q2` | `read_pay2` | read-burst effect: payload of token 2 |
| `ww_t2` | `tok2` | decoded window token 2 (head views) |
| `wt3` | `lat_tag3` | latched token tag register, slot 3 |
| `wq3` | `lat_pay3` | latched token payload register, slot 3 |
| `f_s3` | `is_str3` | slot 3 holds a string-carrying token (Ident/StringLit) |
| `cp3` | `copy_eff3` | cstr-copy effect for slot 3 string payload |
| `f_sr3` | `res_str3` | last_results[3] decoded as String |
| `f_tok3` | `dec_tok3` | decoded Token for window slot 3 |
| `rd_t3` | `read_tag3` | read-burst effect: tag of token 3 |
| `rd_q3` | `read_pay3` | read-burst effect: payload of token 3 |
| `ww_t3` | `tok3` | decoded window token 3 (head views) |
| `wt4` | `lat_tag4` | latched token tag register, slot 4 |
| `wq4` | `lat_pay4` | latched token payload register, slot 4 |
| `f_s4` | `is_str4` | slot 4 holds a string-carrying token (Ident/StringLit) |
| `cp4` | `copy_eff4` | cstr-copy effect for slot 4 string payload |
| `f_sr4` | `res_str4` | last_results[4] decoded as String |
| `f_tok4` | `dec_tok4` | decoded Token for window slot 4 |
| `rd_t4` | `read_tag4` | read-burst effect: tag of token 4 |
| `rd_q4` | `read_pay4` | read-burst effect: payload of token 4 |
| `ww_t4` | `tok4` | decoded window token 4 (head views) |
| `wt5` | `lat_tag5` | latched token tag register, slot 5 |
| `wq5` | `lat_pay5` | latched token payload register, slot 5 |
| `f_s5` | `is_str5` | slot 5 holds a string-carrying token (Ident/StringLit) |
| `cp5` | `copy_eff5` | cstr-copy effect for slot 5 string payload |
| `f_sr5` | `res_str5` | last_results[5] decoded as String |
| `f_tok5` | `dec_tok5` | decoded Token for window slot 5 |
| `rd_t5` | `read_tag5` | read-burst effect: tag of token 5 |
| `rd_q5` | `read_pay5` | read-burst effect: payload of token 5 |
| `ww_t5` | `tok5` | decoded window token 5 (head views) |
| `wt6` | `lat_tag6` | latched token tag register, slot 6 |
| `wq6` | `lat_pay6` | latched token payload register, slot 6 |
| `f_s6` | `is_str6` | slot 6 holds a string-carrying token (Ident/StringLit) |
| `cp6` | `copy_eff6` | cstr-copy effect for slot 6 string payload |
| `f_sr6` | `res_str6` | last_results[6] decoded as String |
| `f_tok6` | `dec_tok6` | decoded Token for window slot 6 |
| `rd_t6` | `read_tag6` | read-burst effect: tag of token 6 |
| `rd_q6` | `read_pay6` | read-burst effect: payload of token 6 |
| `ww_t6` | `tok6` | decoded window token 6 (head views) |
| `wt7` | `lat_tag7` | latched token tag register, slot 7 |
| `wq7` | `lat_pay7` | latched token payload register, slot 7 |
| `f_s7` | `is_str7` | slot 7 holds a string-carrying token (Ident/StringLit) |
| `cp7` | `copy_eff7` | cstr-copy effect for slot 7 string payload |
| `f_sr7` | `res_str7` | last_results[7] decoded as String |
| `f_tok7` | `dec_tok7` | decoded Token for window slot 7 |
| `rd_t7` | `read_tag7` | read-burst effect: tag of token 7 |
| `rd_q7` | `read_pay7` | read-burst effect: payload of token 7 |
| `ww_t7` | `tok7` | decoded window token 7 (head views) |
| `f_ir0` | `res_int0` | last_results[0] decoded as Int |
| `f_ir1` | `res_int1` | last_results[1] decoded as Int |
| `f_ir2` | `res_int2` | last_results[2] decoded as Int |
| `f_ir3` | `res_int3` | last_results[3] decoded as Int |
| `f_ir4` | `res_int4` | last_results[4] decoded as Int |
| `f_ir5` | `res_int5` | last_results[5] decoded as Int |
| `f_ir6` | `res_int6` | last_results[6] decoded as Int |
| `f_ir7` | `res_int7` | last_results[7] decoded as Int |
| `f_ir8` | `res_int8` | last_results[8] decoded as Int |
| `f_ir9` | `res_int9` | last_results[9] decoded as Int |
| `f_ir10` | `res_int10` | last_results[10] decoded as Int |
| `f_ir11` | `res_int11` | last_results[11] decoded as Int |
| `f_ir12` | `res_int12` | last_results[12] decoded as Int |
| `f_ir13` | `res_int13` | last_results[13] decoded as Int |
| `f_ir14` | `res_int14` | last_results[14] decoded as Int |
| `f_ir15` | `res_int15` | last_results[15] decoded as Int |
| `ww_l1` | `win_rest1` | window tail after consuming 1 tokens |
| `ww_l2` | `win_rest2` | window tail after consuming 2 tokens |
| `ww_l3` | `win_rest3` | window tail after consuming 3 tokens |
| `ww_l4` | `win_rest4` | window tail after consuming 4 tokens |
| `ww_l5` | `win_rest5` | window tail after consuming 5 tokens |
| `ww_l6` | `win_rest6` | window tail after consuming 6 tokens |
| `ww_l7` | `win_rest7` | window tail after consuming 7 tokens |
| `cc_tag` | `char_tag` | LexCharTag tag of the current char |
| `is_bs` | `is_backslash` | current char is a backslash |
| `esc_pend` | `esc_pending` | escape pending inside a string literal |
| `esc_tr` | `esc_trans` | EscapeChar translation of the current char |
| `nx_dig` | `next_is_digit` | the next char is a digit |
| `lx_fr` | `in_fraction` | collecting the fraction digits of a float |
| `fl_whole` | `float_whole` | latched whole part of the float being lexed |
| `fr_digs` | `frac_digits` | count of fraction digits collected |
| `fl_pow` | `frac_pow` | 10^frac_digits scale factor |
| `fl_p0` | `float_payload` | packed float payload (scaled*8 + digits) |
| `w_ftag` | `write_float_tag` | write effect: float token tag |
| `w_fp0` | `write_float_pay` | write effect: float token payload |
| `w_fop` | `write_float_op` | write effect: op token finishing the float |
| `float_fin` | `float_finish` | the float token finishes this tick |
| `float_op` | `float_with_op` | the float finishes at an operator char |
| `lx_last` | `lex_last_tag` | cached tag of the last written token |
| `lx_prev` | `lex_prev_tag` | cached tag of the token before last |
| `lx_pend` | `pend_str_addr` | payload address awaiting a strdup pointer (-1 none) |
| `lx_have_pend` | `have_pend_str` | a strdup pointer write is pending |
| `k_kind` | `plan_kind` | LexFtiPlan: token-push shape for this tick |
| `k_tag0` | `plan_tag0` | LexFtiPlan: first token tag |
| `k_p0` | `plan_pay0` | LexFtiPlan: first token int payload |
| `k_str0` | `plan_str0` | LexFtiPlan: string payload to strdup |
| `k_tag1` | `plan_tag1` | LexFtiPlan: second token tag |
| `k_atag0` | `plan_addr_tag0` | LexFtiPlan: address of first tag slot |
| `k_ap0` | `plan_addr_pay0` | LexFtiPlan: address of first payload slot |
| `k_atag1` | `plan_addr_tag1` | LexFtiPlan: address of second tag slot |
| `k_cn` | `plan_count_n` | LexFtiPlan: next token count |
| `k_ln` | `plan_last_n` | LexFtiPlan: next last-tag cache |
| `k_pn` | `plan_prev_n` | LexFtiPlan: next prev-tag cache |
| `lxp_fi_op` | `plan_int_op` | int finishing at an op (float ticks suppressed) |
| `lxp_fi_only` | `plan_int_only` | int finishing alone (float ticks suppressed) |
| `lx_pp` | `at_plusplus` | the two-char ++ token starts here |
| `k_tag0pp` | `plan_tag0_pp` | first tag with the ++ override applied |
| `k_tag1pp` | `plan_tag1_pp` | second tag with the ++ override applied |
| `w_tag0e` | `write_tag0` | write effect: first token tag |
| `w_p0e` | `write_pay0` | write effect: first token payload |
| `w_tag1e` | `write_tag1` | write effect: second token tag |
| `e_dup` | `eff_strdup` | strdup effect for a string payload |
| `w_pend` | `write_pend_str` | write effect: pending strdup pointer |
| `w_eof` | `write_eof` | write effect: EofTok sentinel |
| `lx_next_char` | `next_char` | the char after the cursor |
| `lx_collecting` | `collecting` | an ident or int is being collected |
| `lx_is_dash` | `at_dash` | current char is a dash |
| `lx_next_dash` | `next_dash` | next char is a dash |
| `lx_comment` | `at_comment` | a -- comment starts here (also the LexFtiPlan slot) |
| `lx_nl` | `comment_nl` | position of the newline ending the comment |
| `lx_comment_to` | `comment_end` | scan position the comment skips to |
| `lx_skip_ws` | `skip_ws` | whitespace bulk-skip fires this tick |
| `lx_ws_adv` | `ws_run_len` | length of the whitespace run |
| `lf_ctag` | `cur_char_tag` | LexFtiPlan local: char tag of cur_char |
| `lf_crec` | `cur_char_isop` | LexFtiPlan local: cur_char is an operator char |
| `lf_ktag` | `keyword_tag` | LexFtiPlan local: keyword tag of the finished ident |
| `lf_kw_ident` | `is_plain_ident` | LexFtiPlan local: finished ident is not a keyword |
| `lf_prev_atom` | `prev_is_atom` | LexFtiPlan local: token before last is atom-like |
| `lf_fold` | `neg_fold` | LexFtiPlan local: negative-literal fold fires |
| `lf_e0` | `entry_idx` | LexFtiPlan local: buffer entry index to write |

## driver_zinit.ev + driver_buildeff.ev — Z3 lifecycle latch bank + effect constructors

| old | new | meaning |
| --- | --- | ------- |
| `ed_hold` | `enum_hold` | hold zstep at 9 while the enum machine runs |
| `z_arrsort` | `z_eff_arrsort` | (Array Int Effect) sort handle |
| `z_sym_effs` | `z_effects_sym` | symbol handle for "effects" |
| `z_effs` | `z_effects` | build-context const for effects |
| `z_sym_elen` | `z_efflen_sym` | symbol handle for "effects__len" |
| `z_elen` | `z_efflen` | build-context const for effects__len |
| `tbuf` | `tok_buf` | FTI token buffer (base/count/cap) |
| `stbuf` | `sym_buf` | FTI symbol-handle buffer (base/count/cap) |
| `cibuf` | `claimidx_buf` | claim-index cursor buffer (base/count/cap) |
| `z_lrsort` | `z_lastres_sort` | (Array Int Result) sort handle |
| `z_sym_lr` | `z_lastres_sym` | symbol handle for "last_results" |
| `z_sym_lrl` | `z_lastres_len_sym` | symbol handle for "last_results__len" |
| `z_lrlen` | `z_lastres_len` | build-context const for last_results__len |
| `z_sym_ift` | `z_first_tick_sym` | symbol handle for "is_first_tick" |
| `z_ift` | `z_first_tick` | build-context const for is_first_tick |
| `z_lrge` | `z_lastres_ge` | AST handle of (>= last_results__len 0) |
| `z_iarr` | `z_intarr_sort` | (Array Int Int) sort handle |
| `z_ege` | `z_efflen_ge` | AST handle of (>= effects__len 0) |
| `ed_fn_p` | `enum_fnames_p` | arena ptr: variant field-name syms |
| `ed_fsort_p` | `enum_fsorts_p` | arena ptr: variant field sorts |
| `ed_sr_p` | `enum_srefs_p` | arena ptr: packed sort_refs |
| `ed_ctors_p` | `enum_ctors_p` | arena ptr: ctor handles array |
| `ed_names_p` | `enum_sortnames_p` | arena ptr: sort-name syms array |
| `ed_souts_p` | `enum_sortsout_p` | arena ptr: mk_datatypes sorts-out array |
| `ed_clists_p` | `enum_clists_p` | arena ptr: constructor-list array |
| `ed_qout_p` | `enum_query_p` | arena ptr: query_constructor out block |
| `ed_f0u` | `enum_fsym0_now` | field-0 sym handle (capture-or-carry view) |
| `ed_f1u` | `enum_fsym1_now` | field-1 sym handle (capture-or-carry view) |
| `ed_f2u` | `enum_fsym2_now` | field-2 sym handle (capture-or-carry view) |
| `ede_nsym` | `enum_namesym` | effect: mk variant-name symbol |
| `ede_rsym` | `enum_recogsym` | effect: mk recognizer symbol |
| `ede_fsym0` | `enum_fieldsym0` | effect: mk field-0 symbol |
| `ede_fsym1` | `enum_fieldsym1` | effect: mk field-1 symbol |
| `ede_fsym2` | `enum_fieldsym2` | effect: mk field-2 symbol |
| `edw_fn0` | `enum_write_fn0` | effect: write field-0 name sym to arena |
| `edw_fn1` | `enum_write_fn1` | effect: write field-1 name sym to arena |
| `edw_fn2` | `enum_write_fn2` | effect: write field-2 name sym to arena |
| `edw_fs0` | `enum_write_fs0` | effect: write field-0 sort to arena |
| `edw_fs1` | `enum_write_fs1` | effect: write field-1 sort to arena |
| `edw_fs2` | `enum_write_fs2` | effect: write field-2 sort to arena |
| `edw_sr01` | `enum_write_refs01` | effect: write packed sort_refs 0/1 |
| `edw_sr2` | `enum_write_refs2` | effect: write sort_ref 2 |
| `ede_mkctor` | `enum_mk_ctor` | effect: Z3_mk_constructor for the variant |
| `edw_ctor` | `enum_write_ctor` | effect: write ctor handle to ctors array |
| `ed_sortsym_decl` | `enum_sortsym_decl` | zero-variant EnumDeclAst for the sort sym |
| `ede_ssym` | `enum_sortsym` | effect: mk enum-sort symbol |
| `ede_clist` | `enum_mk_clist` | effect: Z3_mk_constructor_list |
| `edw_nm` | `enum_write_name` | effect: write sort-name sym to arena |
| `edw_cl` | `enum_write_clist` | effect: write ctor-list handle to arena |
| `ede_mkdt` | `enum_mk_datatype` | effect: Z3_mk_datatypes |
| `ede_rdsort` | `enum_read_sort` | effect: read the created sort handle |
| `ede_rdctor` | `enum_read_ctor` | effect: read a ctor handle back |
| `ede_query` | `enum_query` | effect: Z3_query_constructor |
| `ede_rddecl` | `enum_read_decl` | effect: read the ctor func_decl |
| `ede_app0` | `enum_app0` | effect: mk_app of a nullary ctor |
| `ede_rdtest` | `enum_read_tester` | effect: read the tester decl |
| `ede_rdacc` | `enum_read_acc` | effect: read the field-0 accessor decl |
| `ede_filler` | `enum_filler` | no-op filler effect |
| `ed_eff1` | `enum_step_eff` | the one enum-machine effect for this tick |
| `ed_w5` | `enum_wbatch5` | act-1 step-5 array-write batch tick |
| `ed_w2` | `enum_wbatch2` | act-2 step-2 write batch tick |
| `ed_w3u` | `enum_wbatch3u` | user nullary app+tester batch tick |
| `ze_cfg` | `zinit_cfg` | effect: Z3_mk_config |
| `ze_ctx` | `zinit_ctx` | effect: Z3_mk_context |
| `ze_sol` | `zinit_sol` | effect: Z3_mk_solver |
| `ze_sinc` | `zinit_sol_inc` | effect: solver inc_ref |
| `ze_isort` | `zinit_isort` | effect: mk Int sort |
| `ze_bsort` | `zinit_bsort` | effect: mk Bool sort |
| `ze_ssort` | `zinit_ssort` | effect: mk String sort |
| `ze_rsort` | `zinit_rsort` | effect: mk Real sort |
| `ze_arena` | `zinit_arena` | effect: malloc the arena |
| `ze_arrsort` | `zinit_arrsort` | effect: mk (Array Int Effect) sort |
| `ze_sym_effs` | `zinit_effects_sym` | effect: mk "effects" symbol |
| `ze_effs` | `zinit_effects` | effect: mk effects const |
| `ze_sym_elen` | `zinit_efflen_sym` | effect: mk "effects__len" symbol |
| `ze_elen` | `zinit_efflen` | effect: mk effects__len const |
| `ze_zero` | `zinit_zero` | effect: mk numeral 0 |
| `ze_one` | `zinit_one` | effect: mk numeral 1 |
| `ze_two` | `zinit_two` | effect: mk numeral 2 |
| `ze_three` | `zinit_three` | effect: mk numeral 3 |
| `ze_four` | `zinit_four` | effect: mk numeral 4 |
| `ze_true` | `zinit_true` | effect: mk true |
| `ze_false` | `zinit_false` | effect: mk false |
| `ze_tbuf` | `zinit_tok_buf` | effect: calloc the token buffer |
| `ze_sbuf` | `zinit_sym_buf` | effect: calloc the symbol buffer |
| `ze_cibuf` | `zinit_claimidx_buf` | effect: calloc the claim-index buffer |
| `ze_lrarr` | `zinit_lastres_arr` | effect: mk (Array Int Result) sort |
| `ze_sym_lr` | `zinit_lastres_sym` | effect: mk "last_results" symbol |
| `ze_lastres` | `zinit_lastres` | effect: mk last_results const |
| `ze_sym_lrl` | `zinit_lastres_len_sym` | effect: mk "last_results__len" symbol |
| `ze_lrlen` | `zinit_lastres_len` | effect: mk last_results__len const |
| `ze_sym_ift` | `zinit_first_tick_sym` | effect: mk "is_first_tick" symbol |
| `ze_ift` | `zinit_first_tick` | effect: mk is_first_tick const |
| `ze_lrge` | `zinit_lastres_ge` | effect: build (>= lr__len 0) |
| `ze_lrge_ok` | `zinit_lastres_ge_ok` | BoolCmp ok out for the lr floor |
| `ze_lrge_nn` | `zinit_lastres_ge_nn` | BoolCmp needs_not out for the lr floor |
| `ze_lrassert` | `zinit_lastres_assert` | effect: assert the lr floor |
| `ze_seed0` | `zinit_seed0` | effect: seed symtab slot 0 (last_results) |
| `ze_seed1` | `zinit_seed1` | effect: seed symtab slot 1 (is_first_tick) |
| `ze_iarr` | `zinit_intarr` | effect: mk (Array Int Int) sort |
| `ze_ege` | `zinit_efflen_ge` | effect: build (>= effects__len 0) |
| `ze_ege_ok` | `zinit_efflen_ge_ok` | BoolCmp ok out for the effects floor |
| `ze_ege_nn` | `zinit_efflen_ge_nn` | BoolCmp needs_not out for the effects floor |
| `ze_eassert` | `zinit_efflen_assert` | effect: assert the effects floor |

## driver_enum.ev — enum-declaration machine + user-variant registries

| old | new | meaning |
| --- | --- | ------- |
| `ed_act` | `enum_act` | enum machine: current act (1 declare, 2 finalize, 3 harvest) |
| `ed_step` | `enum_step` | enum machine: step within the act |
| `ed_src` | `enum_src` | which floor enum is running (0..3, then user) |
| `ed_user` | `enum_is_user` | the user enum (not a floor enum) is running |
| `ed_name` | `enum_decl_name` | name of the enum being declared |
| `ed_all` | `enum_variants_all` | full variant list of the enum |
| `ed_vs` | `enum_variants` | variant list still to walk |
| `ed_vidx` | `enum_vidx` | index of the current variant |
| `ed_nv` | `enum_n_variants` | total variant count |
| `ed_v` | `enum_variant` | the current variant decl |
| `ed_vrest` | `enum_vrest` | variants after the current one |
| `ed_vs_nonempty` | `enum_vs_nonempty` | the walk list still has a head |
| `ed_vs_eff` | `enum_vs_now` | variant list view on the start tick |
| `ed_n` | `enum_field_n` | field count of the current variant |
| `ed_vname` | `enum_vname` | name of the current variant |
| `ed_nullary` | `enum_nullary` | current variant has no payload |
| `ed_h_name` | `enum_h_name` | captured variant-name sym handle |
| `ed_h_rec` | `enum_h_recog` | captured recognizer sym handle |
| `ed_h_f0` | `enum_h_field0` | captured field-0 sym handle |
| `ed_h_f1` | `enum_h_field1` | captured field-1 sym handle |
| `ed_h_f2` | `enum_h_field2` | captured field-2 sym handle |
| `d_la_decl` | `libarg_decl` | LibArg floor enum decl AST |
| `d_sq_decl` | `seqarg_decl` | __SeqOf_LibArg floor enum decl AST |
| `d_eff_decl` | `effect_decl` | Effect floor enum decl AST |
| `d_res_decl` | `result_decl` | Result floor enum decl AST |
| `d_la_vs` | `libarg_variants` | LibArg variant list |
| `d_sq_vs` | `seqarg_variants` | __SeqOf_LibArg variant list |
| `d_eff_vs` | `effect_variants` | Effect variant list |
| `d_res_vs` | `result_variants` | Result variant list |
| `ed_go_floor` | `enum_go_floor` | a floor-enum run starts this tick |
| `ed_go` | `enum_go` | an enum run starts this tick |
| `ed_go_vs` | `enum_go_variants` | variant list for the starting run |
| `ed_go_name` | `enum_go_name` | enum name for the starting run |
| `efs` | `field_slots` | per-field type/sort/self-ref slots of the variant |
| `ed_fr0` | `field_ref0` | sort_ref slot for field 0 |
| `ed_fr1` | `field_ref1` | sort_ref slot for field 1 |
| `ed_fr2` | `field_ref2` | sort_ref slot for field 2 |
| `ed_slot0` | `field_sort0` | resolved sort handle for field 0 |
| `ed_slot1` | `field_sort1` | resolved sort handle for field 1 |
| `ed_slot2` | `field_sort2` | resolved sort handle for field 2 |
| `ed_pack01` | `refs_packed01` | sort_refs 0 and 1 packed into one i64 |
| `ed_cur_act` | `enum_act_now` | act chosen for this tick |
| `ed_cur_step` | `enum_step_now` | step chosen for this tick |
| `ed_running` | `enum_running` | the enum machine is active |
| `ed_done` | `enum_done` | the enum run finished this tick |
| `ed_done_user` | `enum_done_user` | the user-enum run finished this tick |
| `ed_adv` | `enum_advance` | the variant walk advances this tick |
| `ed_sort_cap` | `enum_sort_cap` | this tick captures the created sort |
| `ed_decl_cap` | `enum_decl_cap` | this tick captures a ctor func_decl |
| `ed_val_cap` | `enum_val_cap` | this tick captures a nullary value |
| `la_sort` | `libarg_sort` | LibArg sort handle |
| `sq_sort` | `seqarg_sort` | Seq(LibArg) sort handle |
| `z_effsort` | `z_effect_sort` | Effect sort handle |
| `z_ressort` | `z_result_sort` | Result sort handle |
| `ue_sort` | `user_enum_sort` | user enum sort handle |
| `z_exitdecl` | `z_exit_decl` | Exit ctor func_decl handle |
| `z_lc_decl` | `z_libcall_decl` | LibCall ctor func_decl handle |
| `ed_res_run` | `result_run` | the Result floor run is active |
| `ed_res_harvest` | `result_harvest` | Result payload variant: harvest tester+acc |
| `z_irtest` | `z_intres_test` | IntResult tester decl handle |
| `z_srtest` | `z_strres_test` | StringResult tester decl handle |
| `res_acc_pend` | `result_acc_pend` | which Result accessor the landing value belongs to |
| `z_iracc` | `z_intres_acc` | IntResult accessor decl handle |
| `z_sracc` | `z_strres_acc` | StringResult accessor decl handle |
| `evt` | `enum_values` | user-enum nullary value table (name, const handle) |
| `evt_add` | `enum_values_add` | append to the value table this tick |
| `uev` | `user_variants` | user-variant registry (ctor/tester/accessor decls) |
| `uev_cap_d` | `variant_cap_decl` | this tick captures a user ctor decl |
| `uev_cap_t` | `variant_cap_tester` | this tick captures a user tester decl |
| `uev_t_val` | `variant_tester_val` | tester handle (nullary reads lr[1]) |
| `uev_acc_pend` | `variant_acc_pend` | vidx+1 of the accessor landing next tick |
| `uev_cap_a` | `variant_cap_acc` | this tick captures a user accessor decl |

## driver_record.ev + driver_recval.ev + driver_ir.ev fields — record registry/machines

| old | new | meaning |
| --- | --- | ------- |
| `rt_cnt` | `rec_count` | number of registered record types |
| `rt_e0` | `rec0` | record-type registry slot 0 |
| `rt_e1` | `rec1` | record-type registry slot 1 |
| `rt_e2` | `rec2` | record-type registry slot 2 |
| `rc_on` | `rec_collect_on` | skip-pass record field collection active |
| `rc_ok` | `rec_collect_ok` | collection still well-formed |
| `rc_np` | `rec_param_n` | pending comma-group param count |
| `rc_p` | `rec_params` | pending param names of the current field group |
| `rck` | `rec_slot` | registry slot of the type being collected |
| `rc_name` | `rec_cur_name` | current type name |
| `rc_f` | `rec_cur_fnames` | current type field-name rows |
| `rc_t` | `rec_cur_ftypes` | current type field-type rows |
| `rc_nf` | `rec_cur_nf` | current type field count |
| `rc_sort` | `rec_cur_sort` | current type sort handle |
| `rc_start` | `rec_start` | a type header starts collection |
| `rc_skip` | `rec_collecting` | skip tick inside an active collection |
| `rcf_t0nm` | `field_name_tok` | ident at window slot 0 (field name) |
| `rcf_t2nm` | `field_ty_tok` | ident at window slot 2 (field type) |
| `rcf_t4nm` | `field_ty_arg` | ident at slot 4 (Seq element type) |
| `rcf_pend` | `field_pending` | a name-comma pair joins the group |
| `rcf_take` | `field_take` | a field membership line lands |
| `rcf_seq` | `field_is_seq` | the field is Seq(Int) |
| `rcf_prim` | `field_is_prim` | the field type is primitive |
| `rcf_reck` | `field_rec_slot` | registry slot of a record-typed field |
| `rcf_cnt` | `field_add_n` | fields this line adds (incl. __len) |
| `rc_nf_prev` | `rec_nf_prev` | field count before this line |
| `rcf_bad` | `field_bad` | the field line is malformed/overflows |
| `rc_padv` | `rec_pad31` | 31-space padding for fixed-width rows |
| `rcf_ty_norm` | `field_ty_norm` | normalized field type string |
| `rcf_rec_n` | `field_row_name` | fixed-width row for the field name |
| `rcf_rec_p0` | `field_row_p0` | fixed-width row for group param 0 |
| `rcf_rec_p1` | `field_row_p1` | fixed-width row for group param 1 |
| `rcf_rec_p2` | `field_row_p2` | fixed-width row for group param 2 |
| `rcf_trec` | `field_row_ty` | fixed-width row for the field type |
| `rcf_lenrec` | `field_row_len` | fixed-width row for the __len companion |
| `rcf_irec` | `field_row_int` | fixed-width row for "Int" |
| `rcf_app_f` | `field_rows_names` | name rows this line appends |
| `rcf_app_t` | `field_rows_types` | type rows this line appends |
| `rcf_ok` | `field_ok` | the field line is accepted |
| `rd_st` | `recdecl_st` | record-declare machine state |
| `rd_fi` | `recdecl_field_i` | field index in the declare walk |
| `rd_hn` | `recdecl_h_name` | captured type-name sym handle |
| `rd_hf` | `recdecl_h_fields` | captured field sym handles |
| `rd_hfu` | `recdecl_h_fields_now` | field sym handles (capture-or-carry view) |
| `rd_go` | `recdecl_go` | the declare machine starts this tick |
| `rd_prev` | `recdecl_prev` | previous tick was in the declare machine |
| `rd_cur_st` | `recdecl_st_now` | state chosen for this tick |
| `rd_cur_fi` | `recdecl_fi_now` | field index chosen for this tick |
| `rd_fin` | `recdecl_fin` | the declare machine finished |
| `rd_act_now` | `recdecl_active` | the declare machine emits this tick |
| `rd_cap` | `recdecl_cap` | captures from the declare machine land |
| `rd_sort_cap` | `recdecl_sort_cap` | this tick captures the tuple sort |
| `rd_ctor_cap` | `recdecl_ctor_cap` | this tick captures the ctor decl |
| `rd_acc_cap` | `recdecl_acc_cap` | this tick captures an accessor decl |
| `rd_as_cap` | `recdecl_asort_cap` | this tick captures the array sort |
| `rd_ss_cap` | `recdecl_ssort_cap` | this tick captures the set sort |
| `rd_fn_p` | `recdecl_fnames_p` | arena ptr: field-name syms |
| `rd_fs_p` | `recdecl_fsorts_p` | arena ptr: field sorts |
| `rd_ct_p` | `recdecl_ctor_p` | arena ptr: ctor decl out |
| `rd_ac_p` | `recdecl_accs_p` | arena ptr: accessor decls out |
| `rd_eff_st` | `recdecl_eff_st` | effect-select state for this tick |
| `rd_eff_fi` | `recdecl_eff_fi` | effect-select field index |
| `rdf_name` | `recdecl_fname` | current field name (from the rows) |
| `rdt_0` | `recdecl_ty0` | field-0 type name |
| `rdt_1` | `recdecl_ty1` | field-1 type name |
| `rdt_2` | `recdecl_ty2` | field-2 type name |
| `rdt_3` | `recdecl_ty3` | field-3 type name |
| `rdt_4` | `recdecl_ty4` | field-4 type name |
| `rdt_5` | `recdecl_ty5` | field-5 type name |
| `rds_0` | `recdecl_sort0` | field-0 resolved sort |
| `rds_1` | `recdecl_sort1` | field-1 resolved sort |
| `rds_2` | `recdecl_sort2` | field-2 resolved sort |
| `rds_3` | `recdecl_sort3` | field-3 resolved sort |
| `rds_4` | `recdecl_sort4` | field-4 resolved sort |
| `rds_5` | `recdecl_sort5` | field-5 resolved sort |
| `rde_nsym` | `recdecl_namesym` | effect: mk type-name symbol |
| `rde_fsym` | `recdecl_fieldsym` | effect: mk field symbol |
| `rde_wfn0` | `recdecl_write_fn0` | effect: write field-0 name sym |
| `rde_wfn1` | `recdecl_write_fn1` | effect: write field-1 name sym |
| `rde_wfn2` | `recdecl_write_fn2` | effect: write field-2 name sym |
| `rde_wfn3` | `recdecl_write_fn3` | effect: write field-3 name sym |
| `rde_wfn4` | `recdecl_write_fn4` | effect: write field-4 name sym |
| `rde_wfn5` | `recdecl_write_fn5` | effect: write field-5 name sym |
| `rde_wfs0` | `recdecl_write_fs0` | effect: write field-0 sort |
| `rde_wfs1` | `recdecl_write_fs1` | effect: write field-1 sort |
| `rde_wfs2` | `recdecl_write_fs2` | effect: write field-2 sort |
| `rde_wfs3` | `recdecl_write_fs3` | effect: write field-3 sort |
| `rde_wfs4` | `recdecl_write_fs4` | effect: write field-4 sort |
| `rde_wfs5` | `recdecl_write_fs5` | effect: write field-5 sort |
| `rde_tup` | `recdecl_mk_tuple` | effect: Z3_mk_tuple_sort |
| `rde_rdct` | `recdecl_read_ctor` | effect: read the ctor decl |
| `rde_rdacc` | `recdecl_read_acc` | effect: read an accessor decl |
| `rde_asort` | `recdecl_mk_asort` | effect: mk (Array Int T) sort |
| `rde_ssort` | `recdecl_mk_ssort` | effect: mk (Set T) sort |
| `rd_eff1` | `recdecl_step_eff` | the one declare-machine effect this tick |
| `d_rv_f` | `recval_fnames` | field-name rows of the instance type |
| `d_rv_t` | `recval_ftypes` | field-type rows of the instance type |
| `d_rv_n` | `recval_nf` | field count of the instance type |
| `d_rv_ctor` | `recval_ctor` | ctor decl of the instance type |
| `rv_fields` | `recval_fields` | decoded per-field name/type/slot rows |
| `rv_seg0` | `recval_seg0` | field-0 value item (recurse or dotted const) |
| `rv_seg1` | `recval_seg1` | field-1 value item |
| `rv_seg2` | `recval_seg2` | field-2 value item |
| `rv_seg3` | `recval_seg3` | field-3 value item |
| `rv_seg4` | `recval_seg4` | field-4 value item |
| `rv_seg5` | `recval_seg5` | field-5 value item |
| `rv_tail` | `recval_tail` | ctor application tail of the expansion |
| `d_rv_items` | `recval_items` | C2RecVal expansion item run |
| `rdc_sc` | `recval_sortcodes` | per-field sort codes for the decl expansion |
| `rdc_t1` | `recval_dtail1` | decl-expansion tail from field 1 |
| `rdc_t2` | `recval_dtail2` | decl-expansion tail from field 2 |
| `rdc_t3` | `recval_dtail3` | decl-expansion tail from field 3 |
| `rdc_t4` | `recval_dtail4` | decl-expansion tail from field 4 |
| `rdc_t5` | `recval_dtail5` | decl-expansion tail from field 5 |
| `d_rdc_items` | `recval_decl_items` | C2RecDecl expansion item run |
| `d_rv_nm` | `recval_name` | instance name of the record work item |
| `d_rv_k` | `recval_slot` | registry slot of the record work item |
| `nf` | `n_fields` | RecTypeEntry field: number of fields |
| `a0` | `acc0` | RecTypeEntry field: accessor decl 0 |
| `a1` | `acc1` | RecTypeEntry field: accessor decl 1 |
| `a2` | `acc2` | RecTypeEntry field: accessor decl 2 |
| `a3` | `acc3` | RecTypeEntry field: accessor decl 3 |
| `a4` | `acc4` | RecTypeEntry field: accessor decl 4 |
| `a5` | `acc5` | RecTypeEntry field: accessor decl 5 |

## driver_symtab.ev + driver_symlookup.ev + driver_exprdecomp.ev + driver_calllower.ev — work-item decode, name resolution, call lowering

| old | new | meaning |
| --- | --- | ------- |
| `st_names` | `sym_names` | symbol-table name rows (fixed-width 32-byte) |
| `st_entry` | `sym_entry` | fixed-width row for the name being declared |
| `d_st_add` | `sym_add` | a decl appends to the symbol table this tick |
| `d_seed_names` | `sym_seed_names` | pre-seeded rows (last_results, is_first_tick) |
| `saw_lr` | `saw_lastres` | a constraint mentioned last_results |
| `saw_ift` | `saw_first_tick` | a constraint mentioned is_first_tick |
| `d_h_top` | `h_top` | handle stack: top |
| `d_h_2nd` | `h_2nd` | handle stack: second |
| `d_h_3rd` | `h_3rd` | handle stack: third |
| `d_h_4th` | `h_4th` | handle stack: fourth |
| `d_h_5th` | `h_5th` | handle stack: fifth |
| `d_h_6th` | `h_6th` | handle stack: sixth |
| `d_h_t1` | `h_tail1` | handle stack minus 1 |
| `d_h_t2` | `h_tail2` | handle stack minus 2 |
| `d_h_t3` | `h_tail3` | handle stack minus 3 |
| `d_h_t4` | `h_tail4` | handle stack minus 4 |
| `d_h_t5` | `h_tail5` | handle stack minus 5 |
| `d_h_t6` | `h_tail6` | handle stack minus 6 |
| `d_witems_nil` | `work_nil` | the work-item list is empty |
| `d_whead` | `work_head` | current work item |
| `d_wtail` | `work_tail` | work items after the current one |
| `d_classify` | `classify_now` | this tick classifies a new line |
| `d_processing` | `processing` | this tick runs one work-item micro-step |
| `d_it_proc` | `it_process` | work item is C2Process |
| `d_it_op` | `it_op` | work item is C2Op |
| `d_it_ite` | `it_ite` | work item is C2Ite |
| `d_it_not` | `it_not` | work item is C2Not |
| `d_it_decl` | `it_decl` | work item is C2DeclConst |
| `d_it_nat` | `it_natbound` | work item is C2NatBound |
| `d_it_pineq` | `it_pin_eq` | work item is C2PinEq |
| `d_it_assert` | `it_assert` | work item is C2AssertTop |
| `d_it_drop` | `it_drop` | work item is C2Drop |
| `d_it_pushh` | `it_push_h` | work item is C2PushH |
| `d_it_app` | `it_app` | work item is C2App |
| `d_it_seleq` | `it_select_eq` | work item is C2SelectEq |
| `d_it_leneq` | `it_len_eq` | work item is C2LenEq |
| `d_it_strop` | `it_strop` | work item is C2StrOp |
| `d_it_selh` | `it_sel_h` | work item is C2SelH |
| `d_it_seleqh` | `it_sel_eq_h` | work item is C2SelEqH |
| `d_it_leneqh` | `it_len_eq_h` | work item is C2LenEqH |
| `d_it_dup3` | `it_dup3` | work item is C2Dup3 |
| `d_it_swap` | `it_swap` | work item is C2Swap |
| `d_it_rot3` | `it_rot3` | work item is C2Rot3 |
| `d_it_bsc` | `it_bind_scope` | work item is C2BindScope |
| `d_it_bend` | `it_bind_end` | work item is C2BindEnd |
| `d_it_real` | `it_real` | work item is C2Real |
| `d_it_mes` | `it_empty_set` | work item is C2MkEmptySet |
| `d_it_sadd` | `it_set_add` | work item is C2SetAdd |
| `d_it_recv` | `it_rec_val` | work item is C2RecVal |
| `d_it_recd` | `it_rec_decl` | work item is C2RecDecl |
| `d_pe` | `cur_expr` | Expr payload of the current C2Process item |
| `d_op` | `cur_op` | Op payload of the current C2Op item |
| `d_dc_name` | `decl_const_name` | C2DeclConst: name to declare |
| `d_dc_sc` | `decl_const_sort` | C2DeclConst: sort code |
| `d_ph_h` | `push_h_val` | C2PushH: handle to push |
| `d_app_d` | `app_decl` | C2App: func_decl handle |
| `d_app_n` | `app_argc` | C2App: argument count |
| `d_sel_i` | `sel_idx` | C2SelectEq/C2SelEqH: select index |
| `d_len_n` | `len_lit` | C2LenEq/C2LenEqH: length literal |
| `d_bs_n` | `bind_name` | C2BindScope: bound name |
| `d_bs_a` | `bind_acc` | C2BindScope: accessor decl handle |
| `d_so_nm` | `strop_name` | C2StrOp: operation name |
| `d_so_n` | `strop_argc` | C2StrOp: operand count |
| `d_re_s` | `real_scaled` | C2Real: scaled value |
| `d_re_d` | `real_digits` | C2Real: fraction digit count |
| `d_mes_k` | `empty_set_slot` | C2MkEmptySet: registry slot |
| `d_lk_key` | `lookup_key` | fixed-width row key for the lookup name |
| `d_lk_pos` | `lookup_pos` | position of the name in sym_names |
| `d_lk_found` | `lookup_found` | plain-table lookup hit |
| `d_lk_special` | `lookup_special` | name is true/false |
| `d_ilb_found` | `bound_found` | name hits a frame slot-bind |
| `d_ilb_h` | `bound_handle` | handle of the frame slot-bind hit |
| `d_lk_pfx_key` | `lookup_pfx_key` | prefix-scoped row key |
| `d_lk_pfx_pos` | `lookup_pfx_pos` | prefix-scoped position |
| `d_lk_pfx_on` | `lookup_pfx_hit` | prefix-scoped lookup hit |
| `d_lk_pos_eff` | `lookup_pos_now` | effective position (prefix wins) |
| `d_lk_found2` | `lookup_found2` | any table lookup hit |
| `d_lk_pure` | `lookup_pure` | resolves without a memory read |
| `d_lk` | `lookup_handle` | pure-resolved handle (true/false/variant) |
| `d_eff_stread` | `eff_sym_read` | effect: read the handle from the symtab buffer |
| `d_lkname` | `lookup_name` | ident name of the current C2Process leaf |
| `d_pe_is_int` | `expr_is_int` | current expr is EInt |
| `d_pe_is_id` | `expr_is_ident` | current expr is EIdent |
| `d_pe_is_str` | `expr_is_str` | current expr is EStr |
| `d_pe_str` | `expr_str_val` | EStr payload |
| `d_pe_op` | `expr_op` | EBinOp op |
| `d_pe_l` | `expr_lhs` | EBinOp lhs |
| `d_pe_r` | `expr_rhs` | EBinOp rhs |
| `d_pe_is_bin` | `expr_is_binop` | current expr is EBinOp |
| `d_pe_c` | `expr_cond` | ETernary cond |
| `d_pe_t` | `expr_then` | ETernary then |
| `d_pe_e` | `expr_else` | ETernary else |
| `d_pe_is_tern` | `expr_is_ternary` | current expr is ETernary |
| `d_pe_ni` | `expr_not_inner` | ENot inner expr |
| `d_pe_is_not` | `expr_is_not` | current expr is ENot |
| `d_pe_op_ne` | `expr_op_is_neq` | EBinOp op is OpNeq |
| `d_pe_is_c1` | `expr_is_call1` | current expr is ECall1 |
| `d_pe_is_c2` | `expr_is_call2` | current expr is ECall2 |
| `d_pe_is_c3` | `expr_is_call3` | current expr is ECall3 |
| `d_cnm` | `call_name` | callee name of the call expr |
| `d_ca0` | `call_arg0` | call argument 0 |
| `d_ca1` | `call_arg1` | call argument 1 |
| `d_ca2` | `call_arg2` | call argument 2 |
| `d_ctor_d` | `ctor_decl` | user ctor decl matching the callee |
| `d_pe_is_m` | `expr_is_matches` | current expr is EMatches |
| `d_m_nm` | `matches_name` | EMatches variant name |
| `d_m_in` | `matches_scrut` | EMatches scrutinee expr |
| `d_m_t` | `matches_tester` | resolved tester decl |
| `d_m_items` | `matches_items` | lowering of the matches expr |
| `d_sfi_items` | `str_from_int_items` | negative-safe str_from_int expansion |
| `d_ca0_nm` | `call_arg0_name` | arg-0 ident name |
| `d_sl_seq` | `len_of_seqvar` | str_len arg is the registered seq var |
| `d_sl_setk` | `len_set_slot` | str_len arg hits a set-var slot |
| `d_slc_e` | `card_elems` | element rows of the set for cardinality |
| `d_slc_c` | `card_count` | element count of the set |
| `d_slc_k` | `card_slot` | registry slot of the set elements |
| `d_slc_e0` | `card_elt0` | set element-0 name |
| `d_slc_e1` | `card_elt1` | set element-1 name |
| `d_card_items` | `card_items` | set-cardinality lowering |
| `d_c1_items` | `call1_items` | 1-arg call lowering |
| `d_rl_s` | `real_arg_scaled` | __real scaled argument |
| `d_rl_d` | `real_arg_digits` | __real digits argument |
| `d_fld_nm` | `field_arg_name` | __field field-name argument |
| `d_fld_acc` | `field_arg_acc` | resolved field accessor decl |
| `d_c2_items` | `call2_items` | 2-arg call lowering |
| `d_c3_items` | `call3_items` | 3-arg call lowering |

## driver_claimidx.ev — claim index, user-enum collection, effects-literal walk

| old | new | meaning |
| --- | --- | ------- |
| `ci_names` | `claimidx_names` | claim/type index name rows |
| `ci_add` | `claimidx_add` | a skipped claim/type is indexed this tick |
| `ci_entry` | `claimidx_entry` | fixed-width row for the indexed name |
| `e_ciw` | `eff_claimidx_write` | effect: write the body cursor to the index buffer |
| `ec_active` | `variant_walk_on` | pmode-4 user-enum variant collection tick |
| `ec_vname` | `variant_name` | variant name at the window head |
| `ec_v_ok` | `variant_name_ok` | head token is an ident |
| `ec_more` | `variant_more` | a pipe follows (more variants) |
| `ec_payload` | `variant_payload` | the variant has a payload paren |
| `ec_ty0` | `variant_ty0` | payload field-0 type name |
| `ec_ty1` | `variant_ty1` | payload field-1 type name |
| `ec_ty0_ok` | `variant_ty0_ok` | field-0 type is supported |
| `ec_ty1_ok` | `variant_ty1_ok` | field-1 type is supported |
| `ec_p1` | `variant_pay1` | 1-field payload shape matches |
| `ec_p2` | `variant_pay2` | 2-field payload shape matches |
| `ec_p_ok` | `variant_pay_ok` | payload shape is well-formed |
| `ec_more_p` | `variant_more_pay` | pipe after the payload close |
| `ec_more_u` | `variant_more_any` | more variants follow (either shape) |
| `ec_bail` | `variant_bail` | malformed variant; bail to skip |
| `ec_take` | `variant_take` | collect this variant |
| `ec_start` | `enum_start` | last variant collected; start the enum machine |
| `ec_fields` | `variant_fields` | payload field list for the variant |
| `ec_v` | `variant_decl` | collected variant decl |
| `ec_cons` | `variant_consume` | tokens this variant consumes |
| `ec_list` | `variant_list` | collected variant list (prepend order) |
| `ec_list_n` | `variant_list_now` | variant list including this tick's take |
| `el_cnt` | `efflit_count` | effects-literal element count so far |
| `el_lc` | `efflit_libcall` | inside a LibCall element (two-bite shape) |
| `el_lib` | `efflit_lib` | LibCall library string |
| `el_fn` | `efflit_fn` | LibCall function string |
| `el_st` | `efflit_st` | effects-literal walk state |
| `ch_on` | `chain_on` | conditional-effects ternary chain active |
| `ch_pd` | `chain_pdepth` | paren depth within the chain |
| `ch_n` | `chain_n` | chain guard count |
| `d_in_el` | `in_efflit` | pmode-5 effects-literal tick |
| `el_w_t2s` | `efflit_t2_str` | string literal at window slot 2 |
| `el_w_t4s` | `efflit_t4_str` | string literal at window slot 4 |
| `el_head_name` | `head_ident` | ident name at the window head |
| `el_g_enter` | `efflit_guard_enter` | chain guard expression starts |
| `el_open` | `efflit_open` | chain branch literal opens |
| `el_nest` | `efflit_nest` | nested chain in else position |
| `el_st2` | `efflit_st2` | post-branch state (colon or rparen) |
| `el_colon` | `efflit_colon` | chain colon after a branch |
| `el_rp` | `efflit_rparen` | chain rparen (chain may fire) |
| `ch_fire` | `chain_fire` | the chain fold fires |
| `el_st2_bad` | `efflit_st2_bad` | unexpected token after a branch |
| `el_exit_enter` | `efflit_exit_enter` | Exit(...) element starts |
| `el_lc_enter` | `efflit_libcall_enter` | LibCall(...) element starts |
| `el_comma` | `efflit_comma` | element separator |
| `el_close` | `efflit_close` | the literal closes |
| `el_arg_name` | `efflit_arg_name` | LibCall arg ctor name (ArgInt/ArgStr) |
| `el_arg_e` | `efflit_arg_expr` | LibCall arg atom expr |
| `el_arg_ok` | `efflit_arg_ok` | LibCall arg atom is valid |
| `el_b2_shape` | `efflit_arg_shape` | LibCall second-bite shape (one arg) |
| `el_b2e_shape` | `efflit_noargs_shape` | LibCall second-bite shape (no args) |
| `el_b2` | `efflit_argbite` | LibCall arg bite fires |
| `el_b2e` | `efflit_noargs` | LibCall no-args bite fires |
| `el_b2_bad` | `efflit_argbite_bad` | malformed LibCall second bite |
| `el_bad` | `efflit_bad` | malformed effects-literal element |
| `el_argdecl` | `efflit_arg_decl` | ArgInt/ArgStr ctor decl for the arg |
| `el_seleq_item` | `efflit_sel_eq` | select-eq item (chain vs plain form) |
| `el_lc_items` | `efflit_libcall_items` | LibCall element lowering |
| `el_lce_items` | `efflit_libcall0_items` | no-args LibCall element lowering |
| `eg_done` | `guard_done` | a chain guard expression finished |
| `el_cj1` | `conj_items1` | 1 trailing conjunction item |
| `el_cj2` | `conj_items2` | 2 trailing conjunction items |
| `el_cj3` | `conj_items3` | 3 trailing conjunction items |
| `el_cj4` | `conj_items4` | 4 trailing conjunction items |
| `el_bfold` | `efflit_branch_fold` | fold a chain branch into a conjunction |
| `ch_l0` | `chain_lvl0` | chain fold: innermost tail |
| `ch_l1` | `chain_lvl1` | chain fold level 1 |
| `ch_l2` | `chain_lvl2` | chain fold level 2 |
| `ch_l3` | `chain_lvl3` | chain fold level 3 |
| `ch_l4` | `chain_lvl4` | chain fold level 4 |
| `ch_fold_items` | `chain_fold_items` | the guard-tree fold item run |

## driver_compose.ev + driver_guard.ev — inline frames, slot calls, guarded splices

| old | new | meaning |
| --- | --- | ------- |
| `il_frames` | `frames` | inline-frame stack (return cursor, prefix, binds) |
| `il_binds` | `binds` | current frame's slot-bind table |
| `il_pfx` | `scope_prefix` | current alpha-rename prefix |
| `il_depth` | `frame_depth` | inline-frame depth |
| `il_cnt` | `call_count` | slot-call counter (prefix numbering) |
| `il_tgt` | `jump_target` | captured callee body cursor |
| `il_ps` | `param_skip` | param-skip substate (post-jump) |
| `il_pd` | `param_depth` | paren depth in the param skip |
| `cw_st` | `callw_st` | composition-call walk state |
| `cw_bare` | `callw_bare` | bare/.. splice (no slot binds) |
| `cw_ty` | `callw_is_type` | the splice target is a record type |
| `ts_pfx` | `type_prefix` | dotted prefix for a type splice |
| `cw_k` | `slot_count` | slot-bind count collected |
| `cs` | `slot_names` | slot names of the pending call |
| `ilb_n0` | `bind_n0` | frame bind 0: name |
| `ilb_n1` | `bind_n1` | frame bind 1: name |
| `ilb_n2` | `bind_n2` | frame bind 2: name |
| `ilb_n3` | `bind_n3` | frame bind 3: name |
| `ilb_n4` | `bind_n4` | frame bind 4: name |
| `ilb_n5` | `bind_n5` | frame bind 5: name |
| `ilb_h0` | `bind_h0` | frame bind 0: handle |
| `ilb_h1` | `bind_h1` | frame bind 1: handle |
| `ilb_h2` | `bind_h2` | frame bind 2: handle |
| `ilb_h3` | `bind_h3` | frame bind 3: handle |
| `ilb_h4` | `bind_h4` | frame bind 4: handle |
| `ilb_h5` | `bind_h5` | frame bind 5: handle |
| `ilb_t0` | `bind_tail0` | bind list minus 1 |
| `ilb_t1` | `bind_tail1` | bind list minus 2 |
| `ilb_t2` | `bind_tail2` | bind list minus 3 |
| `ilb_t3` | `bind_tail3` | bind list minus 4 |
| `ilb_t4` | `bind_tail4` | bind list minus 5 |
| `fr_ret` | `frame_ret` | top frame: return cursor |
| `fr_pfx` | `frame_prefix` | top frame: saved prefix |
| `fr_bnd` | `frame_binds` | top frame: saved binds |
| `fr_tl` | `frames_tail` | frames below the top |
| `il_rdcur` | `read_callee_cursor` | read the callee cursor this tick |
| `e_cir` | `eff_claimidx_read` | effect: read the callee cursor |
| `d_in_cw0` | `in_callw0` | call-walk state 0 tick (cursor capture) |
| `cw_cap` | `callw_cap` | capture the callee cursor |
| `cw_bjump` | `callw_bare_jump` | bare splice jumps this tick |
| `d_in_cw1` | `in_callw1` | call-walk state 1 tick (slot list) |
| `cw_slot` | `callw_slot` | a slot-bind starts |
| `cw_comma` | `callw_comma` | slot separator |
| `cw_fire` | `callw_fire` | the call fires (jump + frame push) |
| `cw_bail` | `callw_bail` | malformed slot list |
| `cw_vdone` | `slot_val_done` | a slot value expression finished |
| `cw_slot_nm` | `slot_name_tok` | slot name at the window head |
| `b_h0` | `slot_h0` | slot value handle 0 |
| `b_h1` | `slot_h1` | slot value handle 1 |
| `b_h2` | `slot_h2` | slot value handle 2 |
| `b_h3` | `slot_h3` | slot value handle 3 |
| `b_h4` | `slot_h4` | slot value handle 4 |
| `b_h5` | `slot_h5` | slot value handle 5 |
| `il_binds_new` | `binds_new` | bind table built from the slots |
| `cw_pop_stk` | `callw_pop_stack` | handle stack after popping the slot values |
| `il_pfx_new` | `prefix_new` | fresh __cN_ prefix for the call |
| `tyc_g5` | `type_pin_g5` | type-splice pin items from slot 5 |
| `tyc_g4` | `type_pin_g4` | type-splice pin items from slot 4 |
| `tyc_g3` | `type_pin_g3` | type-splice pin items from slot 3 |
| `tyc_g2` | `type_pin_g2` | type-splice pin items from slot 2 |
| `tyc_g1` | `type_pin_g1` | type-splice pin items from slot 1 |
| `tyc_items` | `type_pin_items` | type-splice field pin item run |
| `tsb_items` | `type_recdecl_items` | bare type splice C2RecDecl item run |
| `ts_k` | `type_slot` | registry slot of the spliced type |
| `ts_nm` | `type_inst_name` | instance name of the spliced type |
| `c_ty_rtk` | `type_rec_slot` | registry slot of the membership type |
| `il_jump` | `frame_jump` | a jump fires this tick |
| `il_tgt_v` | `jump_target_now` | jump target (capture or carry) |
| `il_ret_v` | `frame_ret_now` | return cursor to save |
| `ips_act` | `pskip_act` | param-skip tick |
| `ips_chk` | `pskip_check` | param-skip: check for an open paren |
| `ips_open` | `pskip_open` | param list opens |
| `ips_off` | `pskip_off` | no param list; skip ends |
| `ips_skp` | `pskip_run` | skipping inside the param list |
| `ips_done` | `pskip_done` | param list close found |
| `cw_reset` | `callw_reset` | claim entry resets the call walk |
| `gc_op` | `guard_op` | parsed root: binop op |
| `gc_l` | `guard_lhs` | parsed root: lhs (the guard) |
| `gc_r` | `guard_rhs` | parsed root: rhs (the callee) |
| `gc_bin` | `guard_is_binop` | parsed root is a binop |
| `gc_rnm` | `guard_callee` | callee name right of the implies |
| `gc_key` | `guard_key` | fixed-width row key for the callee |
| `gc_pos` | `guard_pos` | callee position in the claim index |
| `gc_hit` | `guard_hit` | a guarded-splice line is recognized |
| `e_gcr` | `eff_guard_read` | effect: read the guarded callee cursor |
| `gj_cap` | `guard_jump_cap` | capture the guarded callee cursor |
| `gj_fire` | `guard_jump_fire` | the guarded splice jumps |
| `il_guard` | `guard_handle` | active guard AST handle (0 = off) |
| `il_gd` | `guard_depth` | frame depth the guard arms at |
| `gj_unguard` | `guard_off` | the guard disarms (matching pop) |
| `d_guard_on` | `guard_on` | a guard is active |

## driver_classify.ev + driver_litmem.ev + driver_setvar.ev + driver_quant.ev + driver_broadcast.ev — line classification, collections, quantifiers, broadcast

| old | new | meaning |
| --- | --- | ------- |
| `c_rem_nil` | `line_at_eof` | window head is the EofTok sentinel |
| `c_t0` | `line_t0` | line token 0 |
| `c_t1` | `line_t1` | line token 1 |
| `c_t2` | `line_t2` | line token 2 |
| `c_t3` | `line_t3` | line token 3 |
| `c_t4` | `line_t4` | line token 4 |
| `c_t0_is_ident` | `line_t0_ident` | token 0 is an ident |
| `c_t0_is_tlkw` | `line_t0_toplevel` | token 0 is a top-level keyword |
| `c_name` | `line_name` | ident name at token 0 |
| `c_is_mem` | `line_is_mem` | token 1 is the membership op |
| `c_is_nmem` | `line_is_notmem` | token 1 is the not-in op |
| `c_is_carry` | `line_is_carry` | the name is a _carry mention |
| `c_mn_line` | `multiname_line` | comma-group multi-name line |
| `c_eff_line` | `effects_line` | effects = ... line |
| `c_type_str` | `line_ty_name` | type name at token 2 |
| `c_is_nat` | `line_is_nat` | the membership type is Nat |
| `c_sc` | `line_sort` | sort code of the membership type |
| `c_sq_ty` | `seq_elt_ty` | Seq(...) element type name |
| `c_sq_rt` | `seq_elt_slot` | registry slot of the Seq element type |
| `c_seq_line` | `seqmem_line` | Seq(...) membership line |
| `c_setmem_line` | `setmem_decl_line` | Set(T) membership (declaration) line |
| `c_seqlit_line` | `seqlit_line` | registered-seq literal assignment line |
| `c_eff_cond` | `effects_cond` | conditional-effects chain shape |
| `c_pinned` | `line_pinned` | membership carries an = pin |
| `c_set_line` | `intset_line` | x in {literal int set} line |
| `c_match_line` | `match_line` | match-pin line |
| `c_bnd_op` | `bound_op` | bound comparison operator |
| `c_t3_is_op` | `line_t3_is_op` | token 3 is a comparison op |
| `c_bound` | `line_bounded` | membership carries a bound |
| `c_bnd_e` | `bound_expr` | bound atom expr |
| `c_bnd_ok` | `bound_ok` | bound atom is valid |
| `c_can_expr` | `line_can_expr` | token 0 can start an expression |
| `c_dd_shape` | `dotdot_shape` | ..Name splice shape |
| `c_dd_nm` | `dotdot_name` | name after the .. |
| `c_comp_nm` | `comp_name` | composition callee name |
| `c_comp_key` | `comp_key` | fixed-width row key for the callee |
| `c_comp_pos` | `comp_pos` | callee position in the claim index |
| `c_comp_found` | `comp_found` | callee found in the claim index |
| `c_dd_head` | `dotdot_head` | ..Name splice recognized |
| `d_line_end` | `line_end` | the line cannot start here (end/pop) |
| `d_claim_end` | `claim_end` | the target claim body ends |
| `il_pop` | `frame_pop` | an inline frame pops this tick |
| `c_comp_call` | `comp_call` | slot-bound composition call line |
| `c_comp_bare` | `comp_bare` | bare composition splice line |
| `c_comp_dd` | `comp_dotdot` | ..Name composition splice line |
| `c_comp_line` | `comp_line` | any composition line |
| `d_enter_el` | `enter_efflit` | enter the effects-literal walk |
| `d_enter_set` | `enter_intset` | enter the int-set membership walk |
| `d_enter_match` | `enter_match` | enter the match-pin walk |
| `d_seq_mem` | `enter_seqmem` | take a Seq membership line |
| `d_enter_sl` | `enter_seqlit` | enter the seq-literal walk |
| `d_enter_mn` | `enter_multiname` | enter the multi-name group walk |
| `d_enter_pratt0` | `enter_pratt_line` | a plain line enters the Pratt FSM |
| `d_enter_pratt` | `enter_pratt` | any consumer enters the Pratt FSM |
| `d_pratt_cons0` | `pratt_enter_consume` | tokens consumed on Pratt entry |
| `d_pratt_kind0` | `pratt_enter_kind` | Pratt entry kind for this consumer |
| `pq_qstop` | `pratt_qstop` | top-level ? terminates the expression |
| `c_seg_bound` | `bound_seg` | bound assert tail items |
| `c_seg_drop` | `drop_seg` | plain drop tail item |
| `c_tailseg` | `tail_seg` | membership tail (bound or drop) |
| `c_natseg` | `nat_tail_seg` | Nat floor + tail items |
| `c_chk_name` | `scoped_name` | prefix-scoped name of the line |
| `c_dname` | `line_dname` | name the line declares (scoped) |
| `c_chk_key` | `scoped_key` | fixed-width row key for the scoped name |
| `c_dup` | `name_dup` | the name is already in the symtab |
| `c_bnd` | `name_bound` | the name hits a frame slot-bind |
| `c_nodecl` | `no_decl` | resolve instead of redeclaring |
| `c_pn` | `resolved_name` | name to resolve when not declaring |
| `c_mem_head` | `mem_head_item` | declare-or-resolve head item |
| `c_mem_items` | `mem_items` | plain membership item run |
| `c_strin_line` | `strin_line` | "lit" in name (infix contains) line |
| `c_si_lit` | `strin_lit` | the contained string literal |
| `c_si_items` | `strin_items` | infix-contains lowering |
| `d_enter_si` | `enter_strin` | take an infix-contains line |
| `c_ty_key` | `tymem_key` | fixed-width row key for the membership type |
| `c_ty_pos` | `tymem_pos` | membership type position in the claim index |
| `c_ty_found` | `tymem_found` | membership type is an indexed record type |
| `c_ty_call` | `tymem_call` | type-use pin with slot binds |
| `c_ty_bare` | `tymem_bare` | bare record-type membership |
| `c_ty_line` | `tymem_line` | any record-type membership line |
| `c_in_ty` | `in_type_frame` | current frame is a type splice |
| `c_cur_pos` | `callee_pos` | index position of the line's callee |
| `c_seqmem_items` | `seq_mem_items` | Seq membership item run |
| `c_setmem_items` | `set_mem_items` | Set(T) declaration item run |
| `c_sq_eltnm` | `seq_elt_tyname` | Seq element type name (from the registry) |
| `c_tyname` | `mem_tyname` | manifest type name of the membership |
| `c_field` | `mem_field` | manifest state-field entry |
| `c_cdecl` | `mem_carry_decl` | textual _name carry declare line(s) |
| `ms_name` | `intset_name` | int-set membership: bound name |
| `ms_neg` | `intset_neg` | negated (not-in) membership |
| `ms_e` | `intset_expr` | or-folded element equation |
| `s_act` | `intset_act` | pmode-7 int-set walk tick |
| `ms_elem` | `intset_elem` | an int element lands |
| `ms_close` | `intset_close` | the set literal closes |
| `ms_bad` | `intset_bad` | malformed set element |
| `ms_v` | `intset_val` | the int element value |
| `ms_eq` | `intset_eq` | name = element equation |
| `ms_e_n` | `intset_expr_now` | or-fold including this element |
| `ms_body` | `intset_body` | final folded expr (false when empty) |
| `ms_items` | `intset_items` | int-set membership assert run |
| `sq_set` | `seqvar_set` | a seq var is registered |
| `sq_name` | `seqvar_name` | registered seq var name |
| `sq_rt` | `seqvar_slot` | registered seq element registry slot |
| `stv_cnt` | `setvar_count` | registered Set(T) var count |
| `stv` | `set_vars` | Set(T) variable registry |
| `sv_cur` | `setvar_cur` | set-literal walk: active registry slot |
| `d_setmem` | `enter_setdecl` | take a Set(T) declaration line |
| `c_stv_idx` | `setvar_idx` | set-var slot matching the line name |
| `c_setlit_line` | `setlit_line` | set-literal assignment line |
| `d_enter_stl` | `enter_setlit` | enter the set-literal walk |
| `sv_k` | `setvar_slot` | element-type slot of the active set var |
| `stl_entry_items` | `setlit_entry_items` | set-literal entry item run |
| `sv_act` | `setlit_act` | pmode-14 set-literal walk tick |
| `stl_elem` | `setlit_elem` | a set element lands |
| `stl_comma` | `setlit_comma` | set element separator |
| `stl_close` | `setlit_close` | the set literal closes |
| `stl_bad` | `setlit_bad` | malformed set element |
| `stl_enm` | `setlit_elt_name` | element ident name |
| `stl_elem_items` | `setlit_elem_items` | set-add item run for the element |
| `stl_close_items` | `setlit_close_items` | set pin item run at close |
| `stl_rec` | `setlit_row` | fixed-width row for the element name |
| `c_smem_idx` | `setmem_idx` | set-var slot matching the membership type |
| `c_smem_line` | `setmem_line` | membership-in-set-var line |
| `d_enter_smem` | `enter_setmem` | take a membership-in-set-var line |
| `c_smem_set` | `setmem_set` | the set var being joined |
| `c_smem_k` | `setmem_slot` | element-type slot of that set |
| `c_smem_items` | `setmem_items` | set-membership assert run |
| `qset_b_var` | `qset_bind_var` | bound var name in the quantifier body |
| `qset_b_tgtnm` | `qset_tgt_name` | target set name in the body |
| `d_enter_qset` | `enter_qset` | take a quantifier-over-set line |
| `qset_e` | `qset_elems` | source-set element rows |
| `qset_c` | `qset_count` | source-set element count |
| `qset_k` | `qset_slot` | element-type slot |
| `qset_tname` | `qset_tgt_set` | target set name |
| `qset_enm` | `qset_names` | unpacked element names of the source set |
| `sl_cnt` | `seqlit_count` | seq-literal element count so far |
| `d_in_sl` | `in_seqlit` | pmode-8 seq-literal walk tick |
| `sl_close` | `seqlit_close` | the seq literal closes |
| `sl_comma` | `seqlit_comma` | seq element separator |
| `sl_e_enter` | `seqlit_elem_enter` | a seq element expression starts |
| `sl_done_inc` | `seqlit_elem_done` | a seq element expression finished |
| `sl_e_nm` | `seqlit_elt_name` | element ident name (record element) |
| `sl_e_item` | `seqlit_elt_item` | element item (RecVal or Process) |
| `sl_elem_items` | `seqlit_elem_items` | per-element select-pin run |
| `sl_close_items` | `seqlit_close_items` | length pin run at close |
| `d_skipl_nil` | `skip_at_eof` | skip pass reached the sentinel |
| `d_skip_at_tlkw` | `skip_at_kw` | skip pass reached a top-level keyword |
| `d_skip_stop` | `skip_stop` | the skip pass stops here |
| `c_q_ex` | `q_is_exists` | quantifier head is exists |
| `c_q_hd` | `q_head` | token 0 is a quantifier |
| `c_q_line` | `q_line` | quantifier line shape |
| `c_q_var` | `q_var` | quantifier bound-variable name |
| `c_q_rng` | `q_range` | integer-range quantifier shape |
| `c_q_lo` | `q_lo` | range low literal |
| `c_q_hi` | `q_hi` | range high literal |
| `c_q_t3nm` | `q_t3_name` | ident at token 3 (seq/set name) |
| `c_q_t5nm` | `q_t5_name` | ident at window slot 5 (dotted part) |
| `c_q_s1` | `q_seq_plain` | over-seq shape, plain name |
| `c_q_s2` | `q_seq_dotted` | over-seq shape, dotted name |
| `c_q_seq` | `q_over_seq` | any over-seq quantifier shape |
| `c_q_seqnm0` | `q_seq_name0` | raw seq name from the tokens |
| `c_q_seqnm` | `q_seq_name` | seq name resolved against the registry |
| `d_enter_qr` | `enter_qrange` | enter the range-quantifier walk |
| `d_enter_qs` | `enter_qseq` | enter the over-seq quantifier walk |
| `q_act` | `qrange_act` | pmode-11 range-head tick |
| `q_close` | `qrange_close` | the range head closes with } : |
| `q_bail` | `qrange_bail` | malformed range head |
| `fl_on` | `qloop_on` | quantifier re-walk loop armed |
| `fl_ex` | `qloop_exists` | the loop or-folds (exists) |
| `fl_var` | `qloop_var` | loop bound-variable name |
| `fl_kind` | `qloop_kind` | loop kind (0 range, 1 seq) |
| `fl_lo` | `qloop_lo` | loop low bound |
| `fl_hi` | `qloop_hi` | loop high bound |
| `fl_seq` | `qloop_seq` | loop seq name |
| `fl_body` | `qloop_body` | parsed loop body expr |
| `fl_nx` | `qloop_next` | next element index |
| `fl_v` | `qloop_cur` | current element index |
| `fq_done` | `qbody_done` | the loop body parse finished |
| `fx_disp` | `qexp_disp` | loop expansion dispatch tick |
| `fx_more` | `qexp_more` | expand one more element |
| `fx_fin` | `qexp_fin` | the expansion finishes |
| `fx_fold` | `qexp_fold_op` | fold operator (and/or) |
| `fx_items` | `qexp_items` | per-element body item run |
| `fx_fin_items` | `qexp_fin_items` | final assert item run |
| `sv_len` | `seqvar_len` | registered seq length (from its #s = k pin) |
| `svr_l_c1` | `seqlen_lhs_call` | pin lhs is a 1-arg call |
| `svr_nm` | `seqlen_callee` | pin lhs callee name |
| `svr_arg` | `seqlen_arg` | pin lhs call argument |
| `svr_argnm` | `seqlen_arg_name` | pin lhs argument ident name |
| `svr_k` | `seqlen_val` | pinned length literal |
| `svr_hit` | `seqlen_pin_hit` | a #seq = k pin is recognized |
| `rb_on` | `bcast_on` | record-pin broadcast loop armed |
| `rb_k` | `bcast_slot` | registry slot of the pinned type |
| `rb_nm` | `bcast_name` | instance name being broadcast |
| `rb_body` | `bcast_body` | parsed pin body expr |
| `rb_fi` | `bcast_field_i` | next field index |
| `rb_v` | `bcast_cur` | current field index |
| `rq_done` | `recpin_done` | a record-pinned parse finished |
| `rb_nf` | `bcast_nf` | field count of the pinned type |
| `rb_fs` | `bcast_fnames` | field-name rows of the pinned type |
| `rbx_disp` | `bcast_disp` | broadcast dispatch tick |
| `rbx_more` | `bcast_more` | broadcast one more field |
| `rbx_fin` | `bcast_fin` | the broadcast finishes |
| `rb_f` | `bcast_field` | current field name (cur index) |
| `rbx_f` | `bcast_field_now` | current field name (next index) |
| `rb_dname` | `bcast_dname` | dotted const name to declare/pin |
| `rb_dup` | `bcast_dup` | the dotted name already exists |
| `rb_head` | `bcast_head` | declare-or-resolve head item |
| `rbx_items` | `bcast_items` | per-field declare+pin run |
| `rb_q_name` | `bcast_qual_name` | field-qualified name during a re-walk |
| `rb_q_key` | `bcast_qual_key` | fixed-width row key for it |
| `d_rb_hit` | `bcast_hit` | re-walk ident resolves field-qualified |
| `d_rb_items` | `bcast_hit_items` | re-walk qualified-ident item run |
| `rbc_rtk` | `bcast_ctor_slot` | re-walk call hits the registry ctor |
| `d_rbc_hit` | `bcast_ctor_hit` | registry-ctor call during a re-walk |
| `d_rbc_items` | `bcast_ctor_items` | select the field-index argument |

## driver_matchpin.ev + driver_group.ev + driver_posbind.ev + driver_pratt.ev — match-pin, groups, positional binding, Pratt wiring

| old | new | meaning |
| --- | --- | ------- |
| `mp_st` | `match_st` | match-pin walk state |
| `mp_name` | `match_name` | declared name of the match pin |
| `mp_sc` | `match_sort` | sort code of the match pin |
| `mp_lr` | `match_scrut_lr` | scrutinee is last_results[i] |
| `mp_idx` | `match_scrut_idx` | last_results index of the scrutinee |
| `mp_scrut` | `match_scrut` | scrutinee ident name |
| `mp_ac` | `arm_ctor` | latched arm-head ctor name |
| `mp_ab` | `arm_bind` | latched arm-head payload bind |
| `mp_awc` | `arm_is_wc` | latched arm is the wildcard |
| `mp_pc` | `pend_ctor` | pending (untested) arm ctor |
| `mp_pb` | `pend_bind` | pending arm payload bind |
| `mp_pe` | `pend_expr` | pending arm body expr |
| `mp_hasp` | `has_pend_arm` | a pending arm exists |
| `mp_n` | `tested_arms` | promoted (tested) arm count |
| `mp_c1` | `arm1_ctor` | tested arm 1 ctor |
| `mp_bd1` | `arm1_bind` | tested arm 1 payload bind |
| `mp_e1` | `arm1_expr` | tested arm 1 body expr |
| `mp_c2` | `arm2_ctor` | tested arm 2 ctor |
| `mp_bd2` | `arm2_bind` | tested arm 2 payload bind |
| `mp_e2` | `arm2_expr` | tested arm 2 body expr |
| `m_act` | `match_act` | pmode-6 match walk tick |
| `mp_s_lr` | `scrut_lastres` | scrutinee parses as last_results[i] |
| `mp_s_id` | `scrut_ident` | scrutinee parses as a plain ident |
| `mp_s_bad` | `scrut_bad` | malformed scrutinee |
| `mp_idx_v` | `scrut_idx_val` | int literal inside the brackets |
| `mp_w_t2s` | `match_t2_name` | ident at window slot 2 (payload bind) |
| `mp_st1` | `match_arms_st` | arm-walk state tick |
| `mp_arm_pl_h` | `arm_payload_head` | payload arm head shape |
| `mp_arm_nl_h` | `arm_nullary_head` | nullary arm head shape |
| `mp_arm_wc_h` | `arm_wc_head` | wildcard arm head shape |
| `mp_arm_new` | `arm_new` | a tested-arm head lands |
| `mp_overflow` | `arm_overflow` | too many tested arms |
| `mp_take` | `arm_take` | take this arm head |
| `mp_wc_over` | `wc_overflow` | wildcard after too many arms |
| `mp_wc_take` | `wc_take` | take the wildcard head |
| `mp_benter` | `arm_body_enter` | an arm body parse starts |
| `mp_endt` | `match_end_tok` | non-arm head ends the match |
| `mp_fire_end` | `match_fire_end` | the match fires at its end |
| `mp_fire` | `match_fire` | emit the lowered match |
| `mp_bail` | `match_bail` | malformed match; bail |
| `mq_done` | `arm_body_done` | an arm body parse finished |
| `mq_wc` | `wc_body_done` | the wildcard body finished |
| `mq_arm` | `arm_done` | a tested-arm body finished |
| `mq_promote` | `arm_promote` | promote the pending arm |
| `mp_v_n` | `fold_arm_n` | arm count for the fold |
| `mp_v_c1` | `fold_ctor1` | fold view: arm 1 ctor |
| `mp_v_b1` | `fold_bind1` | fold view: arm 1 bind |
| `mp_v_e1` | `fold_expr1` | fold view: arm 1 expr |
| `mp_v_c2` | `fold_ctor2` | fold view: arm 2 ctor |
| `mp_v_b2` | `fold_bind2` | fold view: arm 2 bind |
| `mp_v_e2` | `fold_expr2` | fold view: arm 2 expr |
| `mp_v_def` | `fold_default` | fold view: default expr |
| `mp_v_db` | `fold_def_bind` | fold view: default bind |
| `mp_v_dc` | `fold_def_ctor` | fold view: default ctor |
| `mp_t1` | `fold_tester1` | resolved tester for arm 1 |
| `mp_t2` | `fold_tester2` | resolved tester for arm 2 |
| `mp_acc1` | `fold_acc1` | resolved accessor for arm 1 |
| `mp_acc2` | `fold_acc2` | resolved accessor for arm 2 |
| `mp_dacc` | `fold_def_acc` | resolved accessor for the default |
| `mp_tailE` | `match_tail_eq` | pin+assert tail items |
| `mp_tail_i1` | `match_tail_ite1` | one-ite tail |
| `mp_tail_i2` | `match_tail_ite2` | two-ite tail |
| `mp_dtail` | `match_dtail` | tail sized by arm count |
| `mp_defseg` | `default_seg` | default body item segment |
| `mp_b2seg` | `arm2_body_seg` | arm 2 body segment |
| `mp_c2seg` | `arm2_test_seg` | arm 2 tester segment |
| `mp_a1t` | `arm1_tail` | tail after arm 1 body |
| `mp_b1seg` | `arm1_body_seg` | arm 1 body segment |
| `mp_c1seg` | `arm1_test_seg` | arm 1 tester segment |
| `mp_items` | `match_items` | the lowered match item run |
| `pg_pend` | `group_pending` | pending name tokens of the group |
| `pg_st` | `group_st` | group walk state (collect/drain) |
| `pg_param` | `group_is_param` | the group is a first-line param list |
| `pg_close` | `group_close` | the param list closes on this group |
| `pg_sc` | `group_sort` | latched group sort code |
| `pg_nat` | `group_nat` | latched group Nat flag |
| `pg_ty` | `group_ty` | latched group type name |
| `pg_active` | `group_active` | pmode-9 group walk tick |
| `pg_collect` | `group_collect` | collect substate tick |
| `pg_take2` | `group_take2` | an Ident-comma pair joins the group |
| `pg_ty_tick` | `group_ty_tick` | the Ident-in-Type tick latches the type |
| `pg_bail` | `group_bail` | malformed group line |
| `pg_drain` | `group_drain` | drain substate tick |
| `pg_have_pend` | `group_have_pend` | pending names remain |
| `pg_hd_t` | `group_head_tok` | head pending token |
| `pg_hd_nm` | `group_head_name` | head pending name |
| `pg_tail` | `group_tail` | pending names after the head |
| `pg_last` | `group_last` | this drain is the last name |
| `pg_ty_cons` | `group_ty_consume` | tokens the type tick consumes |
| `pg_ty_close` | `group_ty_close` | rparen follows the group type |
| `pg_group_done_mode` | `group_done_mode` | pmode after the drain finishes |
| `pg_ty_exit_mode` | `group_ty_exit_mode` | pmode after a no-drain type tick |
| `pg_field` | `group_field` | manifest field for the drained name |
| `pg_cdecl` | `group_carry_decl` | textual carry declare for it |
| `pg_drain_items` | `group_drain_items` | declare item run for the drained name |
| `pcl_t2nm` | `stmt_t2_name` | ident at line token 2 |
| `pcl_t4nm` | `stmt_t4_name` | ident at line token 4 |
| `pcl_stmt` | `call_stmt` | Name(...) positional call statement |
| `pcl_m1key` | `method1_key` | row key for the one-dot method callee |
| `pcl_m1` | `method_call1` | recv.claim(...) method call |
| `pcl_m2key` | `method2_key` | row key for the two-dot method callee |
| `pcl_m2` | `method_call2` | recv.field.claim(...) method call |
| `pcl_meth` | `method_call` | any method-call line |
| `pcl_recv_nm` | `recv_name` | receiver name to push first |
| `pcl_tup` | `tuple_line` | (a, b) in Callee tuple line |
| `pcl_line` | `posbind_line` | any positional-binding line |
| `pcl_recv_items` | `recv_items` | receiver push item run |
| `pe_act` | `poselem_act` | pmode-12 element walk tick |
| `pt_comma` | `pos_comma` | element separator |
| `pt_rp` | `pos_rparen` | the element list closes |
| `ptt_e0` | `tuparg_e0` | nested tuple element 0 atom |
| `ptt_ok0` | `tuparg_ok0` | tuple element 0 is a valid atom |
| `ptt_e1` | `tuparg_e1` | nested tuple element 1 atom |
| `ptt_ok1` | `tuparg_ok1` | tuple element 1 is a valid atom |
| `pt_tup_b` | `tuple_bite` | a nested (a, b) tuple element lands |
| `ptt_items` | `tuparg_items` | tuple element item run |
| `pt_tup` | `has_tuple` | a tuple element was taken |
| `pt_ts` | `tuple_pos` | argument position of the tuple |
| `pt_elem` | `pos_elem` | a plain element expression starts |
| `ptc_w2nm` | `close_t2_name` | ident at window slot 2 after the close |
| `ptc_w4nm` | `close_t4_name` | ident at window slot 4 after the close |
| `ptc_n` | `close_named` | close of a named call statement |
| `ptc_tm` | `close_recv_method` | close shaped ) in recv.claim |
| `ptc_t` | `close_membership` | close shaped ) in Callee |
| `ptc_nm` | `close_callee` | callee name at the close |
| `ptc_key` | `close_key` | row key for the callee |
| `ptc_pos` | `close_pos` | callee position in the claim index |
| `ptc_ok` | `close_ok` | the close resolves a callee |
| `pt_close` | `pos_close` | the positional call closes |
| `pt_bail` | `pos_bail` | unresolvable close; bail |
| `e_pcr` | `eff_posbind_read` | effect: read the callee cursor |
| `ptc_recv_items` | `close_recv_items` | receiver push for the recv-method close |
| `ptj_cap` | `posjump_cap` | capture the callee cursor |
| `ptj_fire` | `posjump_fire` | the positional call jumps |
| `pt_st` | `posbind_st` | positional walk state |
| `pt_callee` | `pos_callee` | callee name latched at line start |
| `pi_done` | `poselem_done` | an element expression finished |
| `pc_k` | `pos_argc` | element handle count |
| `pj` | `posjump_armed` | a positional jump is pending bind-zip |
| `pj_recv` | `posjump_recv` | the call has a receiver element |
| `pi_nm` | `elem_name` | element result ident name |
| `pi_skey` | `elem_scoped_key` | scoped row key for the element |
| `pi_pkey` | `elem_plain_key` | plain row key for the element |
| `pi_evt` | `elem_is_variant` | element is an enum-value name |
| `pi_ilb` | `elem_is_bound` | element hits a frame bind |
| `pi_unres` | `elem_unresolved` | element ident resolves nowhere |
| `pi_decl` | `elem_declare` | declare the element (arg inference) |
| `pi_dname` | `elem_dname` | scoped name to declare |
| `pi_field` | `elem_field` | manifest field for it |
| `pi_cdecl` | `elem_carry_decl` | textual carry declare for it |
| `pi_items` | `elem_items` | element item run (declare or process) |
| `pn` | `param_names` | callee first-line param names |
| `pn_k` | `param_count` | collected param count |
| `pn_prev` | `param_expect` | next ident is a param name |
| `pn_take` | `param_take` | collect this param name |
| `pn_nm` | `param_name_tok` | param name at the window head |
| `pty` | `param_types` | callee param type names |
| `pty_take` | `param_ty_take` | collect this param type |
| `pty_nm` | `param_ty_name` | param type name token |
| `pz_act` | `bindzip_act` | bind the handles to the params this tick |
| `pz_v0` | `bindzip_h0` | handle for param 0 |
| `pz_v1` | `bindzip_h1` | handle for param 1 |
| `pz_v2` | `bindzip_h2` | handle for param 2 |
| `pz_v3` | `bindzip_h3` | handle for param 3 |
| `pzt_pty` | `tup_param_ty` | declared type of the tuple param |
| `pzt_k` | `tup_rec_slot` | registry slot of that type |
| `pzt_fs` | `tup_rec_fnames` | field rows of that type |
| `pzt_f0` | `tup_field0` | field-0 name of that type |
| `pzt_f1` | `tup_field1` | field-1 name of that type |
| `pzt_pn` | `tup_param_name` | name of the tuple param |
| `pzt_h0` | `tup_h0` | tuple element handle 0 |
| `pzt_h1` | `tup_h1` | tuple element handle 1 |
| `pzt_tb` | `tup_binds` | field-coerced binds for the tuple |
| `pzt_app1` | `tup_binds_full` | binds including the plain params |
| `pz_binds_nt` | `bindzip_plain` | binds with no tuple element |
| `pz_binds` | `bindzip_binds` | final bind table to install |
| `pz_pop` | `bindzip_pop` | handle stack after the zip pops |
| `d_pk_pinseg` | `pin_seg` | pin+assert tail items |
| `d_pk_natseg` | `nat_seg` | Nat floor + pin tail items |
| `d_pk_head` | `pratt_decl_head` | declare-or-resolve head item |
| `d_pratt_items` | `pratt_done_items` | item run selected on parse completion |
| `d_done_cons` | `pratt_done_consume` | tokens consumed on parse completion |
| `d_in_pratt` | `in_pratt` | pmode-3 Pratt tick |
| `p_out` | `pratt_out` | Pratt output (operand) stack |
| `p_ops` | `pratt_ops` | Pratt operator stack |
| `p_expop` | `pratt_expop` | next token must start an operand |
| `p_pd` | `pratt_pd` | open-paren depth |
| `p_qd` | `pratt_qd` | pending-? depth |
| `p_cd` | `pratt_cd` | open-call depth |
| `pk_kind` | `pratt_kind` | Pratt entry kind (who consumes the result) |
| `pk_name` | `pratt_decl_name` | declared name for a pin parse |
| `pk_sc` | `pratt_sort` | sort code for a pin parse |
| `pk_nat` | `pratt_nat` | Nat flag for a pin parse |
| `pk_nodecl` | `pratt_nodecl` | resolve instead of declare |
| `pk_pn` | `pratt_pin_name` | resolve-name for a no-decl pin |
| `pk_rec` | `pratt_rec_slot` | record slot of a record-pinned parse |
| `d_cb_pad` | `call_pad31` | 31-space padding for the callable rows |
| `d_cb_names` | `callable_names` | whitelisted call-head names (builtins+ctors+types) |
| `pr_ntoks` | `pratt_next_toks` | step out: next token list |
| `pr_nouts` | `pratt_next_outs` | step out: next operand stack |
| `pr_nops` | `pratt_next_ops` | step out: next operator stack |
| `pr_nexpop` | `pratt_next_expop` | step out: next expop |
| `pr_npd` | `pratt_next_pd` | step out: next paren depth |
| `pr_nqd` | `pratt_next_qd` | step out: next ? depth |
| `pr_ncd` | `pratt_next_cd` | step out: next call depth |
| `pr_sdone` | `pratt_step_done` | step out: the expression closed |
| `pr_sres` | `pratt_result` | step out: the finished Expr |
| `pr_scons` | `pratt_consume` | step out: tokens consumed |
| `p_done` | `pratt_done` | the Pratt parse finished this tick |
| `pr_bad` | `pratt_bad` | the parse failed (ENoExpr) |
| `c_pin_rtk` | `pin_rec_slot` | record slot when the pin type is a record |

## driver.ev + driver_emit.ev — orchestrator state, per-item effects, EMIT

| old | new | meaning |
| --- | --- | ------- |
| `pmode` | `parse_mode` | parse dispatch mode (0 dispatch, 1 skip, 2 claim, ...) |
| `witems` | `work_items` | current work-item program |
| `hstk` | `handle_stack` | Z3 handle stack |
| `istep` | `item_step` | micro-step within a multi-step item |
| `pend` | `capture_pend` | how next tick's capture applies (1 push, 2 tmp) |
| `tmp_h` | `tmp_handle` | temporary captured handle |
| `fstr` | `manifest_fields` | accumulated manifest state-field entries |
| `cdstr` | `carry_decls` | accumulated textual carry declares |
| `dcons` | `consume_n` | tokens consumed this tick |
| `d_cap_int` | `cap_int` | last_results[0] as Int (per-tick capture) |
| `d_cap_str` | `cap_str` | last_results[0] as String |
| `d_items_nil` | `head_is_eof` | window head is the sentinel |
| `d_head_is_claim` | `head_is_claim` | window head is the claim keyword |
| `d_cl_name` | `decl_name` | ident after the head keyword |
| `d_name_ok` | `is_target` | the claim name matches the target |
| `d_is_param` | `head_has_params` | a param list follows the claim name |
| `d_head_is_enum` | `head_is_enum` | window head is the enum keyword |
| `d_en_reserved` | `enum_reserved` | the enum name is a floor enum |
| `d_in_dispatch` | `in_dispatch` | pmode-0 dispatch tick |
| `d_enter_claim` | `enter_claim` | enter the target claim walk |
| `d_enter_claimp` | `enter_claim_params` | enter via the param-list group walk |
| `d_enter_edecl` | `enter_enum_decl` | enter the user-enum collection |
| `d_enter_skip` | `enter_skip` | skip this top-level item |
| `d_all_done` | `parse_done` | the whole parse finished |
| `d_in_skip` | `in_skip` | pmode-1 skip tick |
| `d_in_claim` | `in_claim` | pmode-2 claim-walk tick |
| `ue_name` | `user_enum_name` | name of the user enum |
| `ue_done` | `user_enum_done` | the user enum is declared |
| `d_hstk_in` | `stack_in` | handle stack after pend application |
| `d_tmp_in` | `tmp_in` | tmp handle after pend application |
| `bsc_on` | `bind_scope_on` | a C2BindScope arm bind is active |
| `bsc_n` | `bind_scope_name` | the active arm-bind name |
| `bsc_acc` | `bind_scope_acc` | the active arm-bind accessor decl |
| `d_bind_hit` | `bind_hit` | processing ident hits the arm bind |
| `d_vb_hit` | `qvar_hit` | processing ident is the quantifier loop var |
| `d_vb_items` | `qvar_items` | loop-var substitution item run |
| `d_vb_pfx` | `qvar_prefix` | loop-var dotted prefix |
| `d_vb_dot` | `qvar_dot` | field access on the loop var |
| `d_vb_fld` | `qvar_field` | field name accessed on the loop var |
| `d_vb_acc` | `qvar_acc` | accessor decl for that field |
| `d_vbd_items` | `qvar_dot_items` | loop-var field-access item run |
| `d_bind_items` | `bind_items` | arm-bind accessor-read item run |
| `d_op_arith_arr` | `op_arith_nary` | op needs the arith array marshal |
| `d_op_bool_arr` | `op_bool_nary` | op is n-ary and/or |
| `d_op_is_div` | `op_is_div` | op is division |
| `d_op_cc` | `op_is_concat` | op is concat |
| `d_op_arr` | `op_is_nary` | op needs the 2-slot array marshal |
| `d_new_items` | `line_items` | item program chosen by the classifier |
| `d_mem_line` | `mem_line` | plain membership line taken |
| `d_mem_cons` | `mem_consume` | tokens a membership line consumes |
| `c_field_add` | `field_add` | append a manifest field this tick |
| `d_carry_strip` | `carry_strip` | first mention of a carry name |
| `cs_pos` | `strip_pos` | position of its duplicate textual declare |
| `cs_nl` | `strip_nl` | end of the duplicate declare line |
| `cs_cut` | `strip_cut` | carry declares with the duplicate spliced out |
| `d_pops` | `stack_pops` | handles this item pops |
| `d_ilb_push` | `push_bound` | push a frame-bind handle |
| `d_push_ident` | `push_ident` | push a pure-resolved ident handle |
| `d_lk_read` | `lookup_read` | read the ident handle from the symtab |
| `d_push_h` | `push_handle` | push a literal handle item |
| `d_push_decl` | `push_decl` | push the declared const handle |
| `d_stk_dup3` | `stack_dup3` | duplicate the third handle |
| `d_stk_swap` | `stack_swap` | swap the top two handles |
| `d_stk_rot3` | `stack_rot3` | rotate the top three handles |
| `d_multi_cont` | `multi_step` | the current item continues next tick |
| `w_after` | `window_after` | window tail after this tick's consumption |
| `d_eff_mkint` | `eff_mkint` | effect: mk int numeral |
| `d_atom_ok` | `atom_ok` | atom build claim ok out |
| `d_eff_cmp` | `eff_cmp` | effect: comparison binop |
| `d_cmp_ok` | `cmp_ok` | comparison claim ok out |
| `d_cmp_nn` | `cmp_needs_not` | comparison needs a not follow-up |
| `d_eff_arith` | `eff_arith` | effect: arithmetic binop |
| `d_arith_ok` | `arith_ok` | arith claim ok out |
| `d_eff_nary` | `eff_nary` | effect: n-ary and/or |
| `d_nary_ok` | `nary_ok` | n-ary claim ok out |
| `d_eff_concat` | `eff_concat` | effect: seq concat |
| `d_eff_ite` | `eff_ite` | effect: mk ite |
| `d_eff_not` | `eff_not` | effect: mk not |
| `d_eff_wl0` | `eff_argw0` | effect: write operand 0 to the args buffer |
| `d_eff_wl1` | `eff_argw1` | effect: write operand 1 to the args buffer |
| `d_eff_mkstring` | `eff_mkstring` | effect: mk string literal |
| `d_so_a` | `strop_a` | string op operand a |
| `d_so_b` | `strop_b` | string op operand b |
| `d_so_c` | `strop_c` | string op operand c |
| `d_eff_slen` | `eff_strlen` | effect: mk str.len |
| `d_eff_strop` | `eff_strop` | effect: string builtin op |
| `d_strop_ok` | `strop_ok` | string op claim ok out |
| `d_eff_sym` | `eff_mksym` | effect: mk decl-name symbol |
| `d_eff_const` | `eff_mkconst` | effect: mk the declared const |
| `d_eff_stwr` | `eff_sym_write` | effect: write the handle into the symtab |
| `d_op_geq` | `op_geq` | pinned OpGeq value |
| `d_op_eq` | `op_eq` | pinned OpEq value |
| `d_op_impl` | `op_impl` | pinned OpImpl value |
| `d_eff_gimpl` | `eff_guard_impl` | effect: wrap the assert in (=> guard ...) |
| `d_gimp_ok` | `guard_impl_ok` | guard implies ok out |
| `d_gimp_nn` | `guard_impl_nn` | guard implies needs_not out |
| `d_eff_natge` | `eff_nat_ge` | effect: build (>= x 0) |
| `d_nat_ok` | `nat_ok` | nat floor ok out |
| `d_nat_nn` | `nat_nn` | nat floor needs_not out |
| `d_eff_pineq` | `eff_pin_eq` | effect: build the pin equality |
| `d_pin_ok2` | `pin_ok` | pin equality ok out |
| `d_pin_nn` | `pin_nn` | pin equality needs_not out |
| `d_eff_assert` | `eff_assert` | effect: assert the top handle |
| `d_eff_asserttmp` | `eff_assert_tmp` | effect: assert the tmp handle |
| `d_app_arg` | `app_arg` | ctor argument for the current write step |
| `d_eff_caw` | `eff_ctor_argw` | effect: write a ctor argument |
| `d_eff_app` | `eff_ctor_app` | effect: mk_app the ctor |
| `d_sel_idx_h` | `sel_idx_handle` | cached numeral for the select index |
| `d_eff_sel` | `eff_select` | effect: select from the effects array |
| `d_eff_selh` | `eff_select_h` | effect: select with handle operands |
| `d_re_den` | `real_denom` | denominator text for the real numeral |
| `d_re_str` | `real_text` | numerator/denominator text |
| `d_eff_real` | `eff_mkreal` | effect: mk real numeral |
| `d_mes_sort` | `empty_set_sort` | element sort for mk_empty_set |
| `d_eff_mes` | `eff_empty_set` | effect: mk empty set |
| `d_eff_sadd` | `eff_set_add` | effect: set add |
| `d_eff_seleq` | `eff_select_eq` | effect: equate the selected element |
| `d_seleq_ok` | `select_eq_ok` | select-eq ok out |
| `d_seleq_nn` | `select_eq_nn` | select-eq needs_not out |
| `d_len_n_h` | `len_lit_handle` | cached numeral for the length literal |
| `d_eff_leneq` | `eff_len_eq` | effect: equate the length |
| `d_leneq_ok` | `len_eq_ok` | len-eq ok out |
| `d_leneq_nn` | `len_eq_nn` | len-eq needs_not out |
| `d_eff_filler` | `eff_filler` | no-op filler effect |
| `d_eff_lib` | `eff_step` | the one work-item effect for this tick |
| `estep` | `emit_step` | EMIT phase program counter |
| `sptr` | `unit_ptr` | captured pointer to the serialized solver text |
| `e_s2s` | `eff_solver_str` | effect: Z3_solver_to_string |
| `e_copy` | `eff_copy_unit` | effect: cstr-copy the serialized text |
| `d_manifest` | `manifest_txt` | the manifest header text |
| `d_prelude` | `prelude_txt` | the Result/last_results prelude text |
| `d_unit` | `unit_txt` | the full translation unit text |
| `e_puts` | `eff_print_unit` | effect: print the unit |
| `e_free_tbuf` | `eff_free_tokbuf` | effect: free the token buffer |
| `e_free_sbuf` | `eff_free_symbuf` | effect: free the symbol buffer |
| `e_free_cibuf` | `eff_free_claimbuf` | effect: free the claim-index buffer |
