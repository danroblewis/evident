"""model_query — the SAFETY / existential verification layer over a loaded `Model` (a QueryMixin).

Split out of `evident_viz.py`; the LIVENESS side (◇/□◇/⤳) lives in `model_temporal.py`.
This module answers the SAFETY (□) and EXISTENTIAL (∃) model-checker questions over the
reachable graph, plus the predicate/reachability helpers both sides share:

  - `check_invariant` — does `var op value` hold on EVERY reachable state? (safety □)
  - `query` — is there a reachable state satisfying a conjunction? (∃ / ◇), all matches
  - `explore` — forward image + init→here trace from a clicked state
  - shared helpers: `reachable_from`, `_trace_to`, `_bfs_indices`, `_predicate`,
    `_conj_predicate`, `_resolve_carried`, `_coerce_predicate_value`, `_fmt_val`

Provided as a MIXIN class `Model` inherits; bodies moved VERBATIM (still `self`-based) —
a behavior-preserving relocation — so every `m.check_invariant(...)` / `m.query(...)`
call contract stays byte-identical. TemporalMixin reuses `_bfs_indices`/`_conj_predicate`
from here via `self`.
"""


class QueryMixin:
    def reachable_from(self, start_state, limit=400):
        """Forward BFS from an ARBITRARY state — "assume the machine is HERE,
        what's reachable forward?" Identical to `reachable()`'s single-tick BFS
        (same `successors` fan, dedup-by-`_key`, edge collection, cap) but seeded
        from `start_state` instead of the initial state, which lands at index 0.
        Returns (states, edges) with edges as (from_index, to_index).

        `reachable_from(self.initial_state())` equals `reachable()` for a
        single-tick model. For a two-tick (ΔΔ) model the transition depends on a
        prior snapshot the clicked state doesn't carry; `successors` pins only
        `_x = start` (no `__x`), so the forward image is the bootstrap fan and may
        differ from the pair-graph `reachable()` builds — fine for the explore
        view, which wants "what follows from here", and exact for the common
        discrete single-tick case."""
        if start_state is None:
            return [], []
        states = [dict(start_state)]
        index = {self._key(start_state): 0}
        edges = []
        frontier = [0]
        while frontier and len(states) < limit:
            i = frontier.pop()
            for nxt in self.successors(states[i]):
                k = self._key(nxt)
                if k not in index:
                    index[k] = len(states)
                    states.append(nxt)
                    frontier.append(index[k])
                edges.append((i, index[k]))
        return states, edges

    def check_invariant(self, var, op, value, limit=400):
        """VERIFY a safety property over the reachable set: does `var op value` hold
        on EVERY reachable state? If not, return the first reachable counterexample.

        This is the *verification* counterpart to the dynamics queries above: instead
        of watching the orbit, it asks "is this predicate an invariant of the whole
        reachable set?" — the question a model-checker answers.

        Predicate spec — a single comparison over ONE carried var (by short or full
        name, e.g. "balance" or "state.balance"):
          op ∈ {"<=", "<", ">=", ">", "=", "!="}  (aliases: "==", "≤", "≥", "≠")
          value: a python int/float for numeric vars, a bool for bool vars
                 (true/false, "true"/"false", 1/0), a variant NAME (str) for enums.
        Examples: check_invariant("count", "<=", 5), ("balance", ">=", 0),
                  ("done", "=", True), ("mode", "!=", "Vending").

        SOUNDNESS: this checks the predicate on the BFS-enumerated reachable set
        (`reachable(limit)`). For a finite reachable set the BFS exhausts
        (`exhaustive=True`), HOLDS is a genuine proof — every reachable state was
        tested. When the BFS hit `limit` (`exhaustive=False`), a 'holds' is only
        "holds on the states explored so far"; a counterexample is ALWAYS real (the
        violating state IS reachable). For enum/bool/`=`/`!=` predicates the
        reachable set is the exact thing to check; for unbounded numeric dynamics
        raise `limit` or read HOLDS as bounded.

        DETERMINISM: states are tested in BFS-discovery order (the order `reachable`
        appends them), so the returned counterexample is stable run to run — the
        first reachable violator, not an arbitrary z3 model.

        Returns:
          {
            "holds": bool,                       # predicate true on every checked state
            "checked": int,                      # number of reachable states tested
            "exhaustive": bool,                  # did the BFS exhaust the reachable set?
            "counterexample": {var: value, ...}  # first violating FULL state, or None
                              | None,
            "violating_value": <value> | None,   # the var's value in the counterexample
            "predicate": "balance ≥ 0",          # human-readable, with the short var name
          }
        """
        OPS = {"<=": "<=", "≤": "<=", "<": "<", ">=": ">=", "≥": ">=", ">": ">",
               "=": "=", "==": "=", "!=": "!=", "≠": "!="}
        canon = OPS.get(op)
        if canon is None:
            raise ValueError(f"unknown op {op!r}; use one of <= < >= > = !=")

        # Resolve the var by full name ("state.balance") or short name ("balance").
        v = self._resolve_carried(var)
        if v is None:
            known = ", ".join(sorted({w["name"] for w in self.carried}))
            raise ValueError(f"unknown carried var {var!r}; known: {known}")
        name = v["name"]

        target = self._coerce_predicate_value(v, value, canon)
        pretty = {"<=": "≤", "<": "<", ">=": "≥", ">": ">", "=": "=", "!=": "≠"}[canon]
        predicate = f"{name.split('.')[-1]} {pretty} {self._fmt_val(target)}"

        def ok(sv):
            # A state that doesn't carry this leaf can't be judged — treat as
            # vacuously satisfying so a partial state never fabricates a violation.
            if name not in sv:
                return True
            x = sv[name]
            if canon == "=":
                return x == target
            if canon == "!=":
                return x != target
            if canon == "<=":
                return x <= target
            if canon == "<":
                return x < target
            if canon == ">=":
                return x >= target
            return x > target  # ">"

        states, edges = self.reachable(limit=limit)
        exhaustive = len(states) < limit          # BFS stopped on its own, not capped
        checked = 0
        for idx, sv in enumerate(states):          # BFS-discovery order = deterministic
            checked += 1
            if not ok(sv):
                return {"holds": False, "checked": checked, "exhaustive": exhaustive,
                        "counterexample": dict(sv), "violating_value": sv.get(name),
                        "trace": self._trace_to(idx, edges, states),  # the path init→violation
                        "predicate": predicate}
        return {"holds": True, "checked": checked, "exhaustive": exhaustive,
                "counterexample": None, "violating_value": None, "trace": None,
                "predicate": predicate}

    def _trace_to(self, target, edges, states):
        """The shortest path of STATES from the initial state (index 0) to `target`, via BFS over
        the reachable graph — so a counterexample comes with the trajectory that reaches it
        (Ana #173/#175), not just the offending state."""
        path = self._bfs_indices(0, target, edges, restrict=None)
        if path is None:
            return [dict(states[target])]
        return [dict(states[k]) for k in path]

    @staticmethod
    def _bfs_indices(source, target, edges, restrict=None):
        """Shortest path of node INDICES from `source` to `target` via BFS over `edges`.
        `restrict`, if given, is a node set the path must stay inside (every visited node
        must be in it) — used to keep the stem inside the ¬Q subgraph so it genuinely
        dodges Q. Returns the index list (inclusive of both ends) or None if unreachable."""
        import collections
        if restrict is not None and (source not in restrict or target not in restrict):
            return None
        if source == target:
            return [source]
        adj = collections.defaultdict(list)
        for a, b in edges:
            if restrict is None or (a in restrict and b in restrict):
                adj[a].append(b)
        prev = {source: None}
        q = collections.deque([source])
        while q:
            u = q.popleft()
            if u == target:
                break
            for w in adj[u]:
                if w not in prev:
                    prev[w] = u
                    q.append(w)
        if target not in prev:
            return None
        path, u = [], target
        while u is not None:
            path.append(u)
            u = prev[u]
        return list(reversed(path))

    def _predicate(self, var, op, value):
        """Build (full_name, short_pretty_predicate, fn) for a `var op value` comparison —
        the shared core of check_invariant/check_temporal. fn(state)→bool, vacuously true on a
        state that doesn't carry the leaf."""
        OPS = {"<=": "<=", "≤": "<=", "<": "<", ">=": ">=", "≥": ">=", ">": ">",
               "=": "=", "==": "=", "!=": "!=", "≠": "!="}
        canon = OPS.get(op)
        if canon is None:
            raise ValueError(f"unknown op {op!r}; use one of <= < >= > = !=")
        v = self._resolve_carried(var)
        if v is None:
            known = ", ".join(sorted({w["name"] for w in self.carried}))
            raise ValueError(f"unknown carried var {var!r}; known: {known}")
        name = v["name"]
        target = self._coerce_predicate_value(v, value, canon)
        pretty = {"<=": "≤", "<": "<", ">=": "≥", ">": ">", "=": "=", "!=": "≠"}[canon]
        cmp = {"=": lambda x: x == target, "!=": lambda x: x != target,
               "<=": lambda x: x <= target, "<": lambda x: x < target,
               ">=": lambda x: x >= target, ">": lambda x: x > target}[canon]
        def fn(sv):
            return True if name not in sv else cmp(sv[name])
        return name, f"{name.split('.')[-1]} {pretty} {self._fmt_val(target)}", fn

    def _conj_predicate(self, terms):
        """Build (pretty, fn) for the CONJUNCTION of `terms` (each [var, op, value]) — lets a temporal
        check take a COMPOUND property like ◇(timer = 0 ∧ light = Red), not just one var op value
        (Ana #258). fn(state) holds iff every term holds; per-term vacuity is handled in _predicate."""
        built = [self._predicate(v, o, val) for (v, o, val) in terms]
        fns = [f for (_, _, f) in built]
        return " ∧ ".join(p for (_, p, _) in built), (lambda sv: all(f(sv) for f in fns))

    def query(self, terms, limit=400):
        """SOLVE an existential query over the reachable set: is there a reachable state
        satisfying the CONJUNCTION of `terms`? This is the dual of check_invariant —
        instead of "does P hold on EVERY reachable state (□)", it asks "does ANY reachable
        state satisfy P₁ ∧ P₂ ∧ … (◇/∃)" — the Z3/Alloy `(assert)(check-sat)` move, run
        against the loaded model without editing the source.

        `terms` is a list of `(var, op, value)` triples (each in the same spec as
        check_invariant); the query is their conjunction. An empty `terms` matches every
        reachable state (so `count` == #reachable).

        Returns:
          {
            "satisfiable": bool,                 # some reachable state satisfies all terms
            "witness": {var: value, ...} | None, # first such FULL state (BFS order), or None
            "count": int,                        # how many reachable states satisfy it
            "checked": int,                      # number of reachable states scanned
            "exhaustive": bool,                  # did the BFS exhaust the reachable set?
            "trace": [state, ...] | None,        # path init→witness, or None
            "predicate": "light = Green ∧ timer = 2",  # human-readable conjunction
          }
        """
        # Build a (full_name, pretty, fn) per term, reusing _predicate (no duplication).
        built = [self._predicate(var, op, value) for (var, op, value) in terms]
        predicate = " ∧ ".join(p for (_, p, _) in built) if built else "true"

        def sat(sv):
            return all(fn(sv) for (_, _, fn) in built)

        states, edges = self.reachable(limit=limit)
        exhaustive = len(states) < limit          # BFS stopped on its own, not capped
        match_idxs = [idx for idx, sv in enumerate(states) if sat(sv)]   # BFS order = deterministic
        count = len(match_idxs)
        if not match_idxs:
            return {"satisfiable": False, "witness": None, "count": 0, "matches": [],
                    "checked": len(states), "exhaustive": exhaustive,
                    "trace": None, "predicate": predicate}
        first_idx = match_idxs[0]
        # Enumerate ALL matching reachable states so the caller can WALK them (the SAT dual of the
        # all-cores enumeration — Alloy's "every instance", Ana #241). Capped; `matches_capped` flags more.
        MATCH_CAP = 40
        matches = [dict(states[i]) for i in match_idxs[:MATCH_CAP]]
        return {"satisfiable": True, "witness": dict(states[first_idx]), "count": count,
                "matches": matches, "matches_capped": count > MATCH_CAP,
                "checked": len(states), "exhaustive": exhaustive,
                "trace": self._trace_to(first_idx, edges, states),  # path init→witness
                "predicate": predicate}

    def explore(self, state, limit=400):
        """EXPLORE from a clicked state — "assume the machine is HERE": answer the
        two reachability questions at once. Forward = `reachable_from(state)` (what
        runs follow from here); backward = `_trace_to` over the WHOLE reachable graph
        (a run init→state, "what leads here").

        `state` is a (possibly partial) carried-state dict — the clicked diagram
        point. We locate it in the global reachable set by `state_key` to get the
        init→state trace and to tell whether it's the initial state. `reaches_init`
        asks: from here, is the initial state forward-reachable (i.e. this state sits
        on a cycle back through init)?

        Returns:
          {
            "forward_count": int,                # distinct states reachable forward
            "forward_capped": bool,              # did the forward BFS hit `limit`?
            "forward": [state, ...],             # ≤40 sample of the forward set
            "trace_to": [state, ...] | None,     # init→state, or None if state IS init
            "reaches_init": bool,                # is init forward-reachable from here?
            "is_initial": bool,                  # is the clicked state the initial state?
          }
        """
        states, edges = self.reachable(limit=limit)
        init_key = self._key(states[0]) if states else None
        target_key = self._key(state)
        idx = next((i for i, s in enumerate(states)
                    if self._key(s) == target_key), None)

        fwd_states, _ = self.reachable_from(state, limit=limit)
        is_initial = (target_key == init_key)
        reaches_init = (init_key is not None
                        and any(self._key(s) == init_key for s in fwd_states))
        trace_to = (None if is_initial or idx is None
                    else self._trace_to(idx, edges, states))
        return {
            "forward_count": len(fwd_states),
            "forward_capped": len(fwd_states) >= limit,
            "forward": [dict(s) for s in fwd_states[:40]],
            "trace_to": trace_to,
            "reaches_init": reaches_init,
            "is_initial": is_initial,
        }

    def _resolve_carried(self, var):
        for w in self.carried:                     # exact full-name match first
            if w["name"] == var:
                return w
        for w in self.carried:                     # then short-name (leaf) match
            if w["name"].split(".")[-1] == var:
                return w
        return None

    def _coerce_predicate_value(self, v, value, canon):
        """Turn the caller's predicate `value` into the same python type `_read`
        decodes this var into, so == / <= compare like-with-like."""
        kind = v["kind"]
        if kind == "int":
            return int(value)
        if kind == "real":
            return float(value)
        if kind == "bool":
            if isinstance(value, str):
                s = value.strip().lower()
                if s in ("true", "1", "yes"):
                    return True
                if s in ("false", "0", "no"):
                    return False
                raise ValueError(f"bad bool literal {value!r}")
            return bool(value)
        if kind == "enum":
            variants = self.enum_variants.get(v["name"], [])
            val = str(value)
            if variants and val not in variants:
                raise ValueError(f"{val!r} is not a variant of {v['name']} "
                                 f"({', '.join(variants)})")
            if canon in ("<=", "<", ">=", ">"):
                raise ValueError(f"ordered op {canon!r} is undefined on enum "
                                 f"{v['name'].split('.')[-1]}; use = or !=")
            return val
        # string / fallback
        return str(value) if kind == "string" else value

    @staticmethod
    def _fmt_val(x):
        if isinstance(x, bool):
            return "true" if x else "false"
        return str(x)
