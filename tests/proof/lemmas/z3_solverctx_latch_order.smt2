;; Auxiliary inductive lemma for z3_solverctx_carry's Z3SolverCtx invariant.
;; The cfg/ctx/sol handles latch in step order (cfg@1, ctx@2, sol@3), so on any
;; REACHABLE carry the higher handle is live only once the lower ones already are.
;; `sol > 0 ⇒ ctx > 0 ∧ cfg > 0` is therefore sound but not 1-inductive without
;; this step↔field correspondence. (Itself proven by the same monotone latch.)
(assert (>= _step 0))
(assert (=> (>= _step 1) (> _z3ctx_cfg 0)))
(assert (=> (>= _step 2) (> _z3ctx_ctx 0)))
(assert (=> (>= _step 3) (> _z3ctx_sol 0)))
