"""model_kinduction — k-INDUCTION proof of an UNBOUNDED safety invariant (a KInductionMixin).

The companion to model_query's `check_invariant`: where that SCANS the BFS reachable set
(a genuine proof only when the BFS exhausts), this PROVES a safety predicate holds in EVERY
reachable state via Z3, even when the reachable set is infinite (a counter that grows forever).
This is the only path to a real proof for an unbounded model — the BFS can only ever sample it.

  SOUND BY CONSTRUCTION. k-induction refutes ¬P along the transition relation:
    BASE   — no INITIAL state violates P:           first_tick=True  ∧ transition ∧ ¬P(x)   UNSAT
    STEP   — no step from a P-state reaches a ¬P:    P(_x) ∧ first_tick=False ∧ transition ∧ ¬P(x)   UNSAT
  Both UNSAT ⇒ P holds in every reachable state (1-induction). k>1 strengthens the hypothesis
  to k consecutive P-states along a path before refuting ¬P at the (k+1)-th, and discharges the
  first k base steps — proving true invariants whose 1-induction step is too weak.

  It can only be INCOMPLETE (fail to prove a true invariant that needs strengthening / a bigger k),
  NEVER UNSOUND: a Z3 UNSAT is a proof; a SAT or UNKNOWN is NOT, so we only ever claim `proven` when
  base ∧ step are BOTH refuted (UNSAT) for some k ≤ K. An UNKNOWN (timeout) is never a proof.

  `prove_inductive(spec, K=3, timeout_ms=…)` -> {proven: bool, k: int|None, method: 'k-induction'}.

The predicate `spec` is the SAME shape model_query threads through `_scan_invariant`:
  {"kind": "conj", "terms": [[var, op, value], …]}                 — a conjunction ∧terms
  {"kind": "impl", "antecedent": [...], "consequent": [...]}        — (∧A) ⇒ (∧C)
We build P as a z3 BoolRef over a CHOSEN snapshot's symbols ("name" = current x, "prev" = _x),
reusing each carried leaf's `consts[name]`/`consts[prev]` twin — never re-parsing the smt2.

SOUNDNESS RESTRICTION: induction is declined (returns proven=False, reason set) when any term
references a DERIVED var. A derived var (`done = count≥5`) has NO prev (`_done`) symbol — the smt2
only binds the CURRENT `done` from the current `count` — so the hypothesis P(_x) can't be stated
soundly over it. Declining is honest+incomplete; the carried-var invariants k-induction targets
(counters, balances, levels) are unaffected. A false "proven" is the one bug this must never have.
"""
import z3


class KInductionMixin:
    # ---- predicate spec → z3 formula over a chosen snapshot -----------------
    def _term_formula(self, term, key):
        """One `[var, op, value]` term as a z3 BoolRef over the `key` snapshot
        (key="name" → current x consts, key="prev" → prev _x consts). Reuses the
        var's resolved kind + the codec's `_scalar_lit` so the literal matches the
        const's sort exactly (int/bool/real/enum). Returns None when the var is
        unknown or has no symbol for this snapshot (the caller declines induction)."""
        var, op, value = term
        v = self._resolve_carried(var)
        if v is None:
            return None
        sym_name = v["name"] if key == "name" else v.get("prev")
        if not sym_name or sym_name not in self.consts:
            return None                       # no symbol for this snapshot (e.g. derived var's prev)
        c = self.consts[sym_name]
        canon = _CANON.get(op)
        if canon is None:
            return None
        target = self._coerce_predicate_value(v, value, canon)
        lit = self._scalar_lit(v["kind"], v["name"], target)
        if canon == "=":
            return c == lit
        if canon == "!=":
            return c != lit
        if canon == "<=":
            return c <= lit
        if canon == "<":
            return c < lit
        if canon == ">=":
            return c >= lit
        return c > lit                        # ">"

    def _conj_formula(self, terms, key):
        """The z3 conjunction ∧terms over the `key` snapshot, or None if any term
        can't be lifted (unknown var / op, or a derived var with no prev twin)."""
        fs = []
        for t in terms:
            f = self._term_formula(t, key)
            if f is None:
                return None
            fs.append(f)
        return z3.And(*fs) if fs else z3.BoolVal(True)

    def _pred_formula(self, spec, key):
        """P as a z3 BoolRef over the `key` snapshot. Mirrors the python predicate
        model_query builds: a CONJUNCTION ∧terms, or an IMPLICATION ¬(∧A) ∨ (∧C).
        Returns None when the predicate touches a symbol this snapshot lacks (so the
        prover declines, never fabricates a proof over a missing var)."""
        if spec.get("kind") == "impl":
            a = self._conj_formula(spec["antecedent"], key)
            c = self._conj_formula(spec["consequent"], key)
            if a is None or c is None:
                return None
            return z3.Or(z3.Not(a), c)
        terms = spec.get("terms") or []
        if not terms:
            return None
        return self._conj_formula(terms, key)

    # ---- the prover ---------------------------------------------------------
    def prove_inductive(self, spec, K=3, timeout_ms=4000):
        """PROVE the safety predicate `spec` holds in EVERY reachable state by k-induction,
        sound by construction. Tries k=1, escalating to 2..K only while the step is SAT (1-induction
        is often too weak even for a true invariant). Returns:
          {"proven": bool, "k": int|None, "method": "k-induction", "reason": str|None}
        `proven` is True ONLY when, for some k ≤ K, BOTH the base and the step are UNSAT (a Z3 proof).
        An UNKNOWN (timeout) is NOT a proof — `proven` stays False, reason='inconclusive (z3 unknown
        at timeout)'. Declines (proven=False, reason set) when `spec` touches a derived var (no prev
        twin to state the hypothesis soundly) — honest and incomplete, never a false proof."""
        if self.first_tick is None:
            return _decline("no is_first_tick symbol — can't separate base from step")
        # Build P over BOTH snapshots up front; if either is unliftable (derived var, unknown
        # var/op) we DECLINE rather than risk an unsound proof over a partially-built predicate.
        p_cur = self._pred_formula(spec, "name")
        p_prev = self._pred_formula(spec, "prev")
        if p_cur is None or p_prev is None:
            return _decline("predicate references a var with no prev-snapshot symbol "
                            "(e.g. a derived var) — induction declined to stay sound")

        # BASE(k): no path of k consecutive INITIAL-rooted states violates P. For 1-induction
        # this is just "no initial state violates P". For k>1 we discharge the first k base steps
        # via the unrolled relation (k_base below); but the FIRST base obligation is the cheap,
        # always-required one, so check it once.
        if self._base_violates(p_cur):
            return {"proven": False, "k": None, "method": "k-induction",
                    "reason": "base case fails — an INITIAL state already violates P "
                              "(this predicate is not even an invariant of the start states)"}

        for k in range(1, K + 1):
            verdict = self._step_unsat(spec, k, timeout_ms)
            if verdict == "unsat":
                # base already refuted above (k=1 base) AND, for k>1, the unrolled base obligations
                # are discharged inside _step_unsat. Both halves UNSAT ⇒ a genuine proof.
                return {"proven": True, "k": k, "method": "k-induction", "reason": None}
            if verdict == "unknown":
                return {"proven": False, "k": None, "method": "k-induction",
                        "reason": f"inconclusive — z3 returned unknown (timeout) at k={k}"}
            # verdict == "sat": the step is too weak at this k; escalate.
        return {"proven": False, "k": None, "method": "k-induction",
                "reason": f"step not closed at k≤{K} — the invariant may need strengthening "
                          "or a larger k (1-induction too weak); not proven (and not refuted)"}

    def _base_violates(self, p_cur):
        """The base obligation: is there an INITIAL state (first_tick=True) satisfying the
        transition where P is FALSE? SAT ⇒ P fails on a start state (base broken). UNSAT ⇒
        every initial state satisfies P. A z3 `unknown` is treated as 'could violate' (we do
        NOT claim the base holds on a timeout) — but that only blocks a proof, never fabricates one."""
        s = self._base()
        s.add(self.first_tick == True)            # noqa: E712
        s.add(z3.Not(p_cur))
        return s.check() != z3.unsat              # sat OR unknown ⇒ not proven safe

    def _step_unsat(self, spec, k, timeout_ms):
        """The k-induction STEP over an UNROLLED path of k+1 consecutive snapshots t0→…→tk:
          assume P at t0..t(k-1), each consecutive pair satisfies the (non-first-tick) transition,
          and refute P at tk. UNSAT ⇒ no length-k path of P-states steps to a ¬P state (P preserved).
        For k>1 we ALSO discharge the intermediate base obligations: a path SHORTER than k rooted at
        an initial state must not reach ¬P either (so a true-but-not-1-inductive invariant is proven
        without a spurious short counterexample). Returns 'unsat' | 'sat' | 'unknown'.

        SOUNDNESS: each consecutive pair is wired with the REAL transition relation (a fresh renamed
        copy via _renamed_transition), first_tick pinned False on every step — exactly the relation the
        runtime ticks. We only ever read UNSAT as a proof; sat/unknown never claim preservation."""
        s = z3.Solver()
        s.set("timeout", timeout_ms)
        # Snapshot 0 uses the model's own prev/cur consts; snapshots 1..k are fresh renamed copies.
        # Chain: pair (t_{i}, t_{i+1}) is the transition with prev=t_i's cur-symbols, cur=t_{i+1}'s.
        # We realize this by k renamed transition copies sharing the boundary symbols.
        chain = self._unrolled_chain(k, s, timeout_ms)
        if chain is None:
            return "unknown"                      # couldn't build the unrolling soundly → not a proof
        # Hypothesis: P holds at every snapshot 0..k-1; conclusion to refute: ¬P at snapshot k.
        for i in range(k):
            pf = self._pred_at(spec, chain, i)
            if pf is None:
                return "unknown"
            s.add(pf)
        neg = self._pred_at(spec, chain, k)
        if neg is None:
            return "unknown"
        s.add(z3.Not(neg))
        r = s.check()
        if r == z3.unsat:
            return "unsat"
        if r == z3.unknown:
            return "unknown"
        return "sat"

    # ---- unrolling: k renamed copies of the transition relation -------------
    def _unrolled_chain(self, k, solver, timeout_ms):
        """Build a path of k+1 snapshots wired by k copies of the transition relation, all with
        first_tick=False. Snapshot i is a dict {leaf_name: z3_symbol}. Snapshot 0 reuses the
        model's prev consts (`_x`) as its symbols and the cur consts (`x`) as snapshot 1; each
        further step renames a fresh copy. Returns the list of snapshot symbol-maps, or None if the
        relation can't be copied (z3 substitution failure). Every copy pins first_tick False —
        the per-tick (non-initial) transition the runtime actually ticks."""
        carried = self.carried
        # snapshot symbols: snap[i][name] is the z3 const for `name` at tick i along the path.
        # tick 0 = prev symbols (_x); tick 1 = cur symbols (x); tick ≥2 = fresh.
        snap = [{v["name"]: self.consts[v["prev"]] for v in carried},
                {v["name"]: self.consts[v["name"]] for v in carried}]
        # The base relation R(_x, x) with first_tick False, as a single conjunction we can rename.
        reln = z3.And(*list(self.assertions), self.first_tick == False)  # noqa: E712
        solver.add(reln)                          # pair (snap0, snap1) = the model's own consts
        for i in range(2, k + 1):
            fresh = {}
            subst = []
            for v in carried:
                nc = z3.Const(f"__k{i}_{v['name']}", self.consts[v["name"]].sort())
                fresh[v["name"]] = nc
                # rename: this copy's PREV(_x) := previous snapshot's cur symbol; CUR(x) := fresh
                subst.append((self.consts[v["prev"]], snap[i - 1][v["name"]]))
                subst.append((self.consts[v["name"]], nc))
            try:
                renamed = z3.substitute(reln, *subst)
            except z3.Z3Exception:
                return None
            solver.add(renamed)
            snap.append(fresh)
        return snap

    def _pred_at(self, spec, chain, i):
        """P evaluated at snapshot `i` of the unrolled chain: take the current-snapshot formula
        (built over the model's `x` consts) and substitute each carried leaf's `x` symbol with the
        chain's tick-i symbol. Reuses _pred_formula("name") and renames it onto the path — so the
        SAME predicate definition is asserted at every tick, no re-derivation. None on failure."""
        base = self._pred_formula(spec, "name")
        if base is None:
            return None
        subst = [(self.consts[v["name"]], chain[i][v["name"]]) for v in self.carried]
        try:
            return z3.substitute(base, *subst)
        except z3.Z3Exception:
            return None


_CANON = {"<=": "<=", "≤": "<=", "<": "<", ">=": ">=", "≥": ">=", ">": ">",
          "=": "=", "==": "=", "!=": "!=", "≠": "!="}


def _decline(reason):
    return {"proven": False, "k": None, "method": "k-induction", "reason": reason}
