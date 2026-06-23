"""Value-symmetry folding for witness enumeration (Ana #271/#16/#122/#257).

The witness walker (`solve._enumerate`) emits EVERY distinct assignment, including ones that
differ only by permuting INTERCHANGEABLE enum values — a 3-colouring has 3! = 6 copies that just
relabel the colours. This module collapses each such symmetry orbit to ONE canonical witness plus
a count, SOUNDLY: it folds only when the symmetry is *provable* from the source, and shows the raw
witness untouched otherwise.

SOUNDNESS is the whole game (Ana's bar). Folding two genuinely-distinct witnesses is a correctness
bug far worse than no folding, so the interchangeability test is a CONSERVATIVE under-approximation:

  The values of an enum are interchangeable IFF, parsing the source conservatively, BOTH hold —
    (a) NO value of the enum is mentioned BY NAME anywhere outside its own `enum …` decl line
        (so `light = Red`, `match … Red ⇒ …`, `x ≠ Blue` all DISQUALIFY — naming a value
        distinguishes it; naming the *type* in `x ∈ Hue` is fine), AND
    (b) NO variable of that enum type is ever an operand of an ORDERING operator (`< > ≤ ≥ ⩽ ⩾`)
        — equality/inequality/`match` only. (Enum values have no intrinsic order; an ordering
        comparison would impose one, breaking interchangeability.)
  Additionally we require every variant be NULLARY — a payload variant (`Ok(Int)`) carries data,
  not a bare interchangeable label, so such enums are never folded.

If either test is uncertain we DO NOT fold (treat the values as fixed). Over-disqualifying only
costs us a missed fold; under-disqualifying would be unsound, so every ambiguity resolves to "fixed".

The canonical form (orbit key) relabels each interchangeable value to its FIRST-OCCURRENCE index
under a FIXED witness traversal — variable names sorted, then sequence index — so two witnesses in
the same orbit map to the identical key. Non-interchangeable values are never relabeled, so
distinct solutions stay distinct.
"""
import json
import re

# enum decl:  enum Name = A | B | C(Int) | …   (capture the RHS variant list)
_ENUM_DECL = re.compile(r'^\s*enum\s+([A-Za-z_]\w*)\s*=\s*(.+?)\s*$')
_ORDERING = ("<", ">", "≤", "≥", "⩽", "⩾")
_IDENT = re.compile(r'[A-Za-z_]\w*')


def _strip_comment(line):
    """Drop an Evident `--` line comment (best-effort: `--` not inside a string)."""
    in_str = False
    for i, ch in enumerate(line):
        if ch == '"':
            in_str = not in_str
        elif not in_str and ch == "-" and i + 1 < len(line) and line[i + 1] == "-":
            return line[:i]
    return line


def _parse_enums(source):
    """{enum_name: (variant_names, all_nullary, decl_line_index)} for every `enum` decl.

    `variant_names` is the list of bare variant identifiers; `all_nullary` is False if ANY variant
    carries a payload (`Ok(Int)`). One decl per source line is assumed (the language's enum form)."""
    enums = {}
    for idx, raw in enumerate(source.split("\n")):
        m = _ENUM_DECL.match(_strip_comment(raw))
        if not m:
            continue
        name, rhs = m.group(1), m.group(2)
        variants, all_nullary = [], True
        for part in rhs.split("|"):
            part = part.strip()
            if not part:
                continue
            vm = re.match(r'^([A-Za-z_]\w*)\s*(\(.*\))?$', part)
            if not vm:                       # unparseable variant → be safe, mark non-nullary
                all_nullary = False
                continue
            variants.append(vm.group(1))
            if vm.group(2):                  # has a `(...)` payload
                all_nullary = False
        if variants:
            enums[name] = (variants, all_nullary, idx)
    return enums


def _enum_typed_vars(source, enum_name):
    """Variable names declared with type `enum_name`: `x ∈ Hue`, `a, b ∈ Hue`, `s ∈ Seq(Hue)`,
    and first-line claim params. Used to attribute ordering comparisons to the enum (test b).

    Conservative: matches the type name as a whole word appearing after `∈`; the captured LHS
    identifiers become the var set. Over-capturing only widens the ordering check (sound)."""
    vars_ = set()
    pat = re.compile(r'([A-Za-z_][\w,\s]*?)\s*∈\s*[^,\n]*\b' + re.escape(enum_name) + r'\b')
    for raw in source.split("\n"):
        line = _strip_comment(raw)
        for m in pat.finditer(line):
            for nm in m.group(1).split(","):
                nm = nm.strip()
                if _IDENT.fullmatch(nm):
                    vars_.add(nm)
    return vars_


def interchangeable_enums(source):
    """The set of enum names whose values are PROVABLY interchangeable (the sound folding set).

    Returns {enum_name: [variant, …]} for each enum passing BOTH tests (a) no value named outside
    its decl and (b) no enum-typed var in an ordering comparison, with all-nullary variants. An enum
    failing any test — or any uncertainty — is simply omitted (we then show its witnesses unfolded)."""
    enums = _parse_enums(source)
    if not enums:
        return {}
    lines = [_strip_comment(ln) for ln in source.split("\n")]
    out = {}
    for name, (variants, all_nullary, decl_idx) in enums.items():
        if not all_nullary or len(variants) < 2:
            continue                                   # payload variants / trivial — never fold
        vset = set(variants)
        # (a) no VALUE named anywhere outside this enum's own decl line.
        named_outside = False
        for i, line in enumerate(lines):
            if i == decl_idx:
                continue
            for tok in _IDENT.findall(line):
                if tok in vset:
                    named_outside = True
                    break
            if named_outside:
                break
        if named_outside:
            continue
        # (b) no enum-typed var as an operand of an ordering operator. Conservative: if any
        # enum-typed var name appears ON A LINE that also contains an ordering operator, disqualify.
        evars = _enum_typed_vars(source, name)
        ordered = False
        if evars:
            for line in lines:
                if any(op in line for op in _ORDERING) and any(
                        re.search(r'\b' + re.escape(v) + r'\b', line) for v in evars):
                    ordered = True
                    break
        if ordered:
            continue
        out[name] = variants
    return out


def _canonical(bindings, value_to_enum):
    """The orbit key for one witness: every interchangeable value relabeled to its first-occurrence
    index under the FIXED traversal (sorted var names, then in-sequence order). Per-enum counters are
    independent (Red→#0 of Hue, regardless of any other enum). Non-interchangeable scalars pass
    through verbatim, so distinct solutions never collide."""
    counters = {}        # enum_name -> {original_value: canonical_index}
    next_idx = {}        # enum_name -> next free index

    def relabel(v):
        enum = value_to_enum.get(v)
        if enum is None:
            return v
        tbl = counters.setdefault(enum, {})
        if v not in tbl:
            tbl[v] = next_idx.get(enum, 0)
            next_idx[enum] = tbl[v] + 1
        return f"{enum}#{tbl[v]}"

    def walk(val):
        if isinstance(val, str):
            return relabel(val)
        if isinstance(val, list):
            return ["L"] + [walk(e) for e in val]
        if isinstance(val, dict):
            return ["D"] + [[k, walk(val[k])] for k in sorted(val)]
        return val                                       # int/float/bool — never an enum label

    return json.dumps([[k, walk(bindings[k])] for k in sorted(bindings)], sort_keys=True)


def fold_witnesses(source, solutions):
    """Collapse symmetric witnesses. Returns (folded, folded_sets, raw_count).

    `folded` is a list of {bindings, multiplicity} — one canonical representative per symmetry orbit
    (the FIRST raw witness of that orbit, shown verbatim) plus the orbit size. `folded_sets` maps
    each folded enum to its interchangeable variant list (so the UI can SAY what it broke). When no
    enum is provably interchangeable, `folded_sets` is empty and every witness is its own orbit
    (multiplicity 1) — a sound no-op."""
    sets = interchangeable_enums(source)
    raw_count = len(solutions)
    value_to_enum = {v: name for name, variants in sets.items() for v in variants}
    if not value_to_enum:
        return ([{"bindings": s, "multiplicity": 1} for s in solutions], {}, raw_count)
    orbits = {}          # canonical key -> {bindings: first rep, multiplicity: int}
    order = []           # preserve first-seen orbit order
    for s in solutions:
        key = _canonical(s, value_to_enum)
        o = orbits.get(key)
        if o is None:
            orbits[key] = {"bindings": s, "multiplicity": 1}
            order.append(key)
        else:
            o["multiplicity"] += 1
    return ([orbits[k] for k in order], sets, raw_count)
