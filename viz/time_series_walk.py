"""time_series_walk — successor-chain trajectory primitives for the time-series views.

The renderer (render_time_series) and the ensemble seeder (time_series_ensemble) both walk
the model's EXISTING successor relation; these are the shared step / chain / projection
helpers, kept in one module so neither imports the other (the ensemble's `step_trajectory`
reuses `_advance` from here, not from the renderer).

  * `_advance` — one step, preferring a state-CHANGING successor on a discrete graph.
  * `walk`     — follow one chain from a seed to a fixed point / revisit.
  * `pick_seed` / `to_ordinal` / `_flatten_seqs` — seed, categorical→y projection, and Seq
    expansion (a Seq plots as one track per element).
"""


def pick_seed(m):
    """The trajectory starts at the program's ACTUAL initial_state — the only faithful seed, so
    every plotted value is genuinely reachable.

    An earlier version nudged the first numeric var by +2000 to escape a flat fixed-point origin,
    but that seeded an UNREACHABLE state and could plot a variable far outside its proven bound
    (vending init has a self-loop under one nondeterministic choice, so it was misread as a fixed
    point and balance was seeded to 2000 ∉ [0,5] — Marek #183, a faithfulness violation). A
    genuinely-flat trajectory from a true fixed-point init is HONEST; the off-origin limit-cycle
    dynamics of a continuous system are shown by phase_portrait, which probes within the reachable
    set — never by fabricating an out-of-domain start here."""
    return m.initial_state()


def excited_seed(m):
    """An off-fixed-point seed for an UNBOUNDED OSCILLATOR whose initial_state is the origin fixed
    point — so a single-run view (time_series single-run / orbit_scatter fallback) traces the real
    limit cycle instead of falsely reporting 'no dynamics'. Returns a seed state dict, or None when
    perturbing would be a lie or isn't needed.

    The guard is what keeps this faithful (it's the inverse of the Marek #183 violation, where
    nudging a BOUNDED var to 2000 plotted a value outside its proven range): we ONLY perturb when
      (1) the initial state IS a fixed point (successor == itself — otherwise pick_seed's honest
          init already moves), AND
      (2) the model has ≥2 numeric carried vars (a true 2-D oscillator shape — pendulum/vanderpol),
          AND
      (3) the perturbed seed has a real successor (the transition admits it).
    Caller contract: only reach for this on the UNBOUNDED path (no finite ensemble box) — an
    unbounded var has no proven bound to violate, so an off-origin start is honest, not fabricated.
    On a bounded model the ensemble already excites the dynamics and this must not be used."""
    init = m.initial_state()
    if init is None:
        return None
    nxt = m.successor(init)
    if nxt is not None and m._key(nxt) != m._key(init):
        return None                                 # init already moves — pick_seed's honest seed wins
    numeric = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    if len(numeric) < 2:
        return None                                 # not an oscillator — a flat fixed point is honest
    # Perturb the FIRST numeric var off the origin (the limit-cycle-from-origin case phase_portrait
    # already probes); leave the rest at their init so the seed stays minimal.
    seed = dict(init)
    v0 = numeric[0]
    seed[v0["name"]] = 28 if v0["kind"] == "int" else 2.8
    return seed if m.successor(seed) is not None else None


def _advance(m, cur, prefer_change, visited):
    """One step of the walk. For DISCRETE programs (prefer_change), pick a
    successor that actually CHANGES the state — and, when possible, one not yet
    visited — so the trajectory explores the program rather than parking on a
    self-loop. This mirrors render_timing_diagram._advance: on a discrete graph
    the lone successor() can sit on a legal self-edge (dungeon's Entrance->Entrance
    is satisfiable, and z3 may pick it), which would report a genuinely-dynamic
    program as static. Falls back to the lone successor() for non-discrete
    (driven difference-equation) systems."""
    if not prefer_change:
        return m.successor(cur)
    succ = m.successors(cur, limit=32)
    if not succ:
        return None
    changed = [s for s in succ if m._key(s) != m._key(cur)]
    pool = changed or succ
    fresh = [s for s in pool if m._key(s) not in visited]
    return (fresh or pool)[0]


def walk(m, seed, steps):
    """Follow one successor chain from `seed`, stopping at a fixed point / revisit.

    For DRIVEN difference equations (numeric / mixed: brackets streams
    ⟨LParen,LBrack,…,BEnd⟩) the next state is DETERMINISTIC given the full previous
    state, so we follow the lone successor() — picking a 'fresh' state out of an
    out-of-bounds fan would fabricate a trace that never occurs on the declared run.

    For DISCRETE programs (all-categorical interface — an adjacency graph like
    dungeon) the lone successor() can park on a legal self-edge: Entrance->Entrance
    is satisfiable and z3 may pick it, which would make a genuinely-dynamic program
    look static. There we prefer a STATE-CHANGING, not-yet-visited successor — exactly
    what render_timing_diagram already does — so the trajectory walks
    Entrance->Hall->Gate instead of stalling at the seed."""
    prefer_change = m.is_discrete()
    cur = seed
    path = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        nxt = _advance(m, cur, prefer_change, seen)
        if nxt is None:
            break
        path.append(nxt)
        k = m._key(nxt)
        if k in seen:        # fixed point / revisit -> stop
            break
        seen.add(k)
        cur = nxt
    return path


def to_ordinal(m, var, value):
    """Map a non-numeric value to a y-coordinate + its label."""
    k = var["kind"]
    if k == "bool":
        return (1 if value else 0), str(bool(value)).lower()
    if k == "enum":
        variants = m.enum_variants.get(var["name"], [])
        idx = variants.index(value) if value in variants else 0
        return idx, str(value)
    # string
    return 0, str(value)


def _flatten_seqs(state_vars, traj):
    """Expand each Seq var into per-element scalar pseudo-vars (`xs[0]`, `xs[1]`, …) and
    mirror their values into every trajectory state dict, so the row loop plots each Seq
    element as its own line (a Seq is a vector — a single flat row hides its dynamics).
    Returns (vars2, traj2): `vars2` has each seq var replaced IN PLACE by its element
    pseudo-vars (preserving rank order); `traj2` is the states with `xs[i]` keys added."""
    traj2 = [dict(s) for s in traj]
    vars2 = []
    for v in state_vars:
        if v["kind"] == "seq":
            elem = v.get("elem", "int")
            for i in range(v.get("len", 0)):
                pseudo = f"{v['name']}[{i}]"
                vars2.append({"name": pseudo, "kind": elem, "role": v.get("role")})
                for s in traj2:
                    s[pseudo] = s[v["name"]][i]
        else:
            vars2.append(v)
    return vars2, traj2
