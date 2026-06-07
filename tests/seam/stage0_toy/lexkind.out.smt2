;; manifest: state-fields = lk_cls:Result lk_ka:Int lk_kb:Int lk_kind:Int lk_sentinel:Int
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 1
(declare-datatypes ((Result 0)) (((NoResult) (IntResult (IntResult__f0 Int)) (StringResult (StringResult__f0 String)) (RealResult (RealResult__f0 Real)) (EofResult) (ErrorResult (ErrorResult__f0 String)))))
(declare-fun last_results () (Array Int Result))
(declare-fun last_results__len () Int)
(assert (>= last_results__len 0))
(declare-datatypes ((LibArg 0) (__SeqOf_LibArg 0) (Effect 0) ) (((ArgInt (ArgInt__f0 Int)) (ArgStr (ArgStr__f0 String)) (ArgReal (ArgReal__f0 Real))) ((__Empty_LibArg) (__Cell_LibArg (head LibArg) (tail __SeqOf_LibArg))) ((ReadLine) (ReadFile (ReadFile__f0 String)) (WriteFile (WriteFile__f0 String) (WriteFile__f1 String)) (LibCall (LibCall__f0 String) (LibCall__f1 String) (LibCall__f2 __SeqOf_LibArg)) (Exit (Exit__f0 Int))) ))
(declare-fun is_first_tick () Bool)
(declare-fun lk_cls () Result)
(declare-fun lk_ka () Int)
(assert (= lk_ka (ite ((_ is IntResult) lk_cls) 2 0)))
(declare-fun lk_kb () Int)
(assert (= lk_kb (ite ((_ is RealResult) lk_cls) 3 lk_ka)))
(declare-fun lk_kind () Int)
(assert (= lk_kind (ite ((_ is StringResult) lk_cls) 1 lk_kb)))
(declare-fun lk_sentinel () Int)
(assert (= lk_sentinel 42))
(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (= effects__len 1))
(assert (= (select effects 0) (Exit 0)))
