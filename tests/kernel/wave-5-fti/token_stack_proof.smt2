;; Wave 5+ FTI proof: TokenStack via libc malloc + __mem.write/read_long.
;;
;; Hand-written sibling of tests/kernel/test_token_stack_fti.ev. Same FSM
;; the Evident source models, but emitted directly so we can test the
;; runtime correctness of the FTI Stack pattern without going through
;; the seam (which currently can't compile real-size programs without
;; OOMing — the very problem this pattern is supposed to solve).
;;
;; Schedule:
;;   tick 0  libc.malloc(1024)              → base captured next tick
;;   tick 1  __mem.write_long(base+0, 8)    Plus  pushed at depth 0
;;   tick 2  __mem.write_long(base+32, 7)   Comma pushed at depth 1
;;   tick 3  __mem.write_long(base+64, 5)   LParen pushed at depth 2
;;   tick 4  __mem.read_long(base+64)       → read top tag (depth 2)
;;   tick 5  capture read value → top_tag
;;   tick 6  libc.free(base) + Exit(0 iff top_tag == 5)
;;
;; Expected: kernel exits 0 (LParen tag = 5 round-trips through libc).
;;
;; What this proves: an FSM can carry an "unbounded stack" of tokens
;; using only TWO Ints in Z3 state (base + depth). The contents live
;; in libc memory and never appear in the per-tick pin string. That's
;; the memory bound the lexer/parser pivot will deliver.

;; manifest: state-fields = phase:Int base:Int top_tag:Int
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

(declare-fun base () Int) (declare-fun _base () Int)
(assert (= base (ite is_first_tick 0 (ite (= phase 1) (ir_at 0) _base))))

(declare-fun top_tag () Int) (declare-fun _top_tag () Int)
(assert (= top_tag (ite is_first_tick 0 (ite (= phase 5) (ir_at 0) _top_tag))))

(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (>= effects__len 0))

(assert
  (ite (= phase 0)
    (and (= effects__len 1)
         (= (select effects 0) (LibCall "libc" "malloc" (__Cell_LibArg (ArgInt 1024) __Empty_LibArg))))
  (ite (= phase 1)
    (and (= effects__len 1)
         (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt base) (__Cell_LibArg (ArgInt 8) __Empty_LibArg)))))
  (ite (= phase 2)
    (and (= effects__len 1)
         (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ base 32)) (__Cell_LibArg (ArgInt 7) __Empty_LibArg)))))
  (ite (= phase 3)
    (and (= effects__len 1)
         (= (select effects 0)
            (LibCall "__mem" "write_long"
              (__Cell_LibArg (ArgInt (+ base 64)) (__Cell_LibArg (ArgInt 5) __Empty_LibArg)))))
  (ite (= phase 4)
    (and (= effects__len 1)
         (= (select effects 0)
            (LibCall "__mem" "read_long"
              (__Cell_LibArg (ArgInt (+ base 64)) __Empty_LibArg))))
  (ite (= phase 5)
    (and (= effects__len 1)
         (= (select effects 0)
            (LibCall "libc" "free" (__Cell_LibArg (ArgInt base) __Empty_LibArg))))
    (and (= effects__len 1)
         (= (select effects 0) (Exit (ite (= top_tag 5) 0 1)))))))))))
