"""Evident AST → SMT-LIB text.

Updated 2026-06-01 alongside the parser rewrite. Reads the AST shape
the new parser produces (per docs/evident-language-spec.md, Appendix A).

Conventions baked in:
  - `is_init` is always declared. Each tick the runtime pins it.
  - The Effect datatype + FFIArg + Result datatypes are auto-declared
    at the top. Users don't redeclare them.
  - For `fsm` schemas, each first-line parameter `name ∈ S` produces
    a state pair: `_name` (previous tick) and `name` (this tick).
    The body refers to `_name` for prev, `name` for new.
  - For `claim` / `type` / `schema` (non-fsm) schemas, no state pairs;
    bindings are plain consts.
  - `effects ∈ Seq(Effect)` channel: declared in FSMs; if the body
    doesn't assign it, the runtime sees an empty Seq (its sort default).

Composition mechanisms (spec §7):
  END-TO-END (parser → SMT-LIB → runtime):
    - membership `x ∈ T` — declares a typed const + asserts membership.
    - constraint (any expression).
    - FTI inlining (the bootstrap extension): `x ∈ FtiName(T)` inside an
      fsm body inlines the FTI's vars/constraints under a namespace.
    - subclaim (registered as a top-level claim, then called by name).

  PARSED but NOT YET LOWERED (transpiler raises a clear error):
    - `..ClaimName` passthrough.
    - `ClaimName(slot ↦ val, …)` explicit-mapping claim call.
    - `(args) ∈ ClaimName` tuple-in dispatch.
    - `cond ⇒ ClaimName` guarded invocation (parses as binop ⇒; the
      lowering of the consequent as a *claim reference* is not yet
      implemented — we lower it as a Bool right-hand side, which
      produces a sort error for any consequent that is a bare claim
      identifier, surfaced by Z3 at runtime).
    - `recv.subclaim_name(args)` receiver-prefix dispatch.

  PARSED but only USEFUL inside a multi-FSM scheduler the bootstrap
  doesn't have:
    - `import "path"`.
    - `enum X = A | B(T)` payload variants.
"""

# ── Built-in declarations every program gets ────────────────────────

PRELUDE = """
;; Effect / FFIArg / Result datatypes baked into every Evident program.
;; Users can emit LibCall effects against this Effect type without
;; redeclaring them. New top-level effect kinds can be added by the
;; runtime; library-level effects compose via LibCall.

(declare-datatypes ((FFIArg 0))
  (((ArgInt (ArgInt_0 Int))
    (ArgStr (ArgStr_0 String)))))

(declare-datatypes ((Effect 0))
  (((LibCall (lc_lib  String) (lc_sym  String) (lc_sig  String)
             (lc_args (Seq FFIArg))
             (lc_ok   String) (lc_err  String))
    (Println (println_0 String))
    (Exit    (exit_0 Int))
    (IntToStr (int_to_str_0 Int)))))

(declare-datatypes ((Result 0))
  (((NoResult)
    (StringResult (string_result_0 String))
    (IntResult    (int_result_0 Int)))))

(declare-const is_init Bool)
""".strip()


# ── Built-in set names → Z3 sort SMT-LIB representation ──────────────

BUILTIN_SORTS = {
    "Int":    "Int",
    "Bool":   "Bool",
    "Real":   "Real",
    "String": "String",
    "Nat":    "Int",          # surface alias for non-negative Int
}


# ── Operator translation tables ──────────────────────────────────────

BINOP_SMT = {
    "+":  "+",   "-":  "-",   "*":  "*",   "/":  "div",
    "++": "seq.++",
    "=":  "=",   "≠":  "distinct",
    "<":  "<",   "≤":  "<=",  ">":  ">",   "≥":  ">=",
    "∧":  "and", "∨":  "or",
    "⇒":  "=>",
    # `∈` is handled specially (set / range / type-name membership).
}

UNOP_SMT = {
    "-":  "-",
    "¬":  "not",
}


# ── FTI registry ────────────────────────────────────────────────────

FTI_REGISTRY = {}


def register_ftis(decls):
    for d in decls:
        if d["kind"] == "fti":
            FTI_REGISTRY[d["name"]] = d


# ── Schema (claim) registry — used for plain-identifier claim references
# inside bodies. Names-match composition isn't implemented end-to-end yet,
# but storing the table lets future transpiler work pick it up without
# touching the parser.
SCHEMA_REGISTRY = {}


def register_schemas(decls):
    for d in decls:
        if d["kind"] == "schema":
            SCHEMA_REGISTRY[d["name"]] = d
        elif d["kind"] == "subclaim":
            SCHEMA_REGISTRY[d["decl"]["name"]] = d["decl"]


# ── Sort + membership helpers ───────────────────────────────────────

def set_sort(set_expr):
    """Return the Z3 sort sexpr for the host of this set."""
    k = set_expr["kind"]
    if k == "set_named":
        name = set_expr["name"]
        if name in BUILTIN_SORTS:
            return BUILTIN_SORTS[name]
        if set_expr.get("param") is not None:
            inner = set_sort(set_expr["param"])
            if name == "Seq":
                return f"(Seq {inner})"
            if name == "Set":
                return f"(Array {inner} Bool)"
            return f"({name} {inner})"
        return name
    if k in ("set_range", "set_enum"):
        return "Int"
    raise ValueError(f"unknown set_expr kind: {k}")


def set_membership(name, set_expr):
    """Return SMT-LIB Bool expression asserting `name ∈ set_expr`, or
    None when membership is implied by the sort decl alone."""
    k = set_expr["kind"]
    if k == "set_named":
        nm = set_expr["name"]
        if nm == "Nat":
            return f"(>= {name} 0)"
        return None
    if k == "set_range":
        lo = transpile_expr(set_expr["lo"])
        hi = transpile_expr(set_expr["hi"])
        return f"(and (>= {name} {lo}) (<= {name} {hi}))"
    if k == "set_enum":
        items = [transpile_expr(e) for e in set_expr["items"]]
        if not items:
            return "false"
        return "(or " + " ".join(f"(= {name} {it})" for it in items) + ")"
    raise ValueError(f"unknown set_expr kind: {k}")


# ── Expression walker ───────────────────────────────────────────────

def transpile_expr(expr, hint_sort=None, declared_sorts=None):
    k = expr["kind"]

    if k == "int":  return str(expr["value"])
    if k == "real": return repr(expr["value"])
    if k == "bool": return "true" if expr["value"] else "false"
    if k == "str":
        escaped = expr["value"].replace("\\", "\\\\").replace('"', '""')
        return f'"{escaped}"'
    if k == "ident": return expr["name"]
    if k == "qualified":
        return "__".join(expr["parts"])
    if k == "field":
        return f"({expr['name']} {transpile_expr(expr['recv'], declared_sorts=declared_sorts)})"
    if k == "index":
        return f"(seq.nth {transpile_expr(expr['recv'], declared_sorts=declared_sorts)} {transpile_expr(expr['idx'], declared_sorts=declared_sorts)})"
    if k == "cardinality":
        return f"(seq.len {transpile_expr(expr['x'], declared_sorts=declared_sorts)})"

    if k == "binop":
        op = expr["op"]
        if op == "∈":
            return transpile_in(expr["l"], expr["r"], declared_sorts)
        smt = BINOP_SMT.get(op)
        if smt is None:
            raise ValueError(f"unknown binop: {op}")
        rhs_hint = None
        if op == "=" and declared_sorts is not None \
                and expr["l"]["kind"] == "ident":
            rhs_hint = declared_sorts.get(expr["l"]["name"])
        l = transpile_expr(expr["l"], hint_sort=None,
                           declared_sorts=declared_sorts)
        r = transpile_expr(expr["r"], hint_sort=rhs_hint,
                           declared_sorts=declared_sorts)
        return f"({smt} {l} {r})"

    if k == "unop":
        op = UNOP_SMT.get(expr["op"])
        if op is None:
            raise ValueError(f"unknown unop: {expr['op']}")
        x = transpile_expr(expr["x"], hint_sort=hint_sort,
                           declared_sorts=declared_sorts)
        return f"({op} {x})"

    if k == "ternary":
        c = transpile_expr(expr["cond"], declared_sorts=declared_sorts)
        t = transpile_expr(expr["t"], hint_sort=hint_sort,
                           declared_sorts=declared_sorts)
        f = transpile_expr(expr["f"], hint_sort=hint_sort,
                           declared_sorts=declared_sorts)
        return f"(ite {c} {t} {f})"

    if k == "call":
        return transpile_call(expr, declared_sorts=declared_sorts)

    if k == "seq":
        return transpile_seq_literal(expr, hint_sort, declared_sorts)

    if k == "set_range":
        # A bare set_range can't be lowered to an SMT-LIB value; it only
        # makes sense as the RHS of `∈`.
        raise ValueError("range literal {lo..hi} can only appear as RHS of ∈")

    if k == "set_enum":
        raise ValueError("set literal {a,b,…} can only appear as RHS of ∈")

    if k == "tuple":
        raise ValueError(
            "tuple expression unsupported in the bootstrap transpiler "
            "(used only by the tuple-in composition form, which is "
            "parsed but not yet lowered)")

    if k == "match":
        return transpile_match(expr, hint_sort=hint_sort,
                               declared_sorts=declared_sorts)

    if k == "matches":
        return transpile_matches(expr, declared_sorts=declared_sorts)

    if k == "quantifier":
        return transpile_quantifier(expr, declared_sorts=declared_sorts)

    raise ValueError(f"unknown expr kind: {k}")


def transpile_in(lhs, rhs, declared_sorts):
    """Lower `lhs ∈ rhs`. Cases:
      - rhs is a set_range → (and (>= lhs lo) (<= lhs hi))
      - rhs is a set_enum  → (or (= lhs e1) …)
      - rhs is anything else (Seq-valued expression, etc.) → seq.contains
    """
    if rhs.get("kind") == "set_range":
        l_text = transpile_expr(lhs, declared_sorts=declared_sorts)
        lo = transpile_expr(rhs["lo"], declared_sorts=declared_sorts)
        hi = transpile_expr(rhs["hi"], declared_sorts=declared_sorts)
        return f"(and (>= {l_text} {lo}) (<= {l_text} {hi}))"
    if rhs.get("kind") == "set_enum":
        l_text = transpile_expr(lhs, declared_sorts=declared_sorts)
        items = [transpile_expr(e, declared_sorts=declared_sorts)
                 for e in rhs["items"]]
        if not items:
            return "false"
        return "(or " + " ".join(f"(= {l_text} {it})" for it in items) + ")"
    # Otherwise treat RHS as a Seq-typed value.
    l_text = transpile_expr(lhs, declared_sorts=declared_sorts)
    r_text = transpile_expr(rhs, declared_sorts=declared_sorts)
    return f"(seq.contains {r_text} (seq.unit {l_text}))"


def transpile_seq_literal(expr, hint_sort, declared_sorts):
    if not expr["items"]:
        if hint_sort is None:
            raise ValueError(
                "empty seq literal ⟨⟩ / [] has no inferable element sort; "
                "use `empty(T)` to specify it explicitly")
        return f"(as seq.empty {hint_sort})"
    units = [f"(seq.unit {transpile_expr(it, declared_sorts=declared_sorts)})"
             for it in expr["items"]]
    result = units[-1]
    for u in reversed(units[:-1]):
        result = f"(seq.++ {u} {result})"
    return result


_SEQ_IDIOMS = {"head", "last", "len", "init", "tail", "unit", "empty"}


def transpile_call(expr, declared_sorts=None):
    name = expr["name"]
    args = expr["args"]
    if name in _SEQ_IDIOMS:
        if name == "empty":
            if len(args) != 1 or args[0]["kind"] != "ident":
                raise ValueError("empty(T) takes one sort name (IDENT)")
            sort = BUILTIN_SORTS.get(args[0]["name"], args[0]["name"])
            return f"(as seq.empty (Seq {sort}))"
        if len(args) != 1:
            raise ValueError(f"{name} takes exactly one argument")
        s = transpile_expr(args[0], declared_sorts=declared_sorts)
        if name == "head":  return f"(seq.nth {s} 0)"
        if name == "last":  return f"(seq.nth {s} (- (seq.len {s}) 1))"
        if name == "len":   return f"(seq.len {s})"
        if name == "init":  return f"(seq.extract {s} 0 (- (seq.len {s}) 1))"
        if name == "tail":  return f"(seq.extract {s} 1 (- (seq.len {s}) 1))"
        if name == "unit":  return f"(seq.unit {s})"
    arg_texts = " ".join(
        transpile_expr(a, declared_sorts=declared_sorts) for a in args)
    if args:
        return f"({name} {arg_texts})"
    return f"({name})"


def transpile_match(expr, hint_sort=None, declared_sorts=None):
    scrut = transpile_expr(expr["scrutinee"], declared_sorts=declared_sorts)
    arms = expr["arms"]

    def pat_test(pat, scrut_text):
        pk = pat["kind"]
        if pk == "pat_wildcard": return "true"
        if pk == "pat_int":  return f"(= {scrut_text} {pat['value']})"
        if pk == "pat_bool":
            return f"(= {scrut_text} {'true' if pat['value'] else 'false'})"
        if pk == "pat_str":
            v = pat["value"].replace("\\", "\\\\").replace('"', '""')
            return f'(= {scrut_text} "{v}")'
        if pk == "pat_bind":
            # No binding semantics yet.
            return "true"
        if pk == "pat_ctor":
            # Test the variant tag.
            return f"((_ is {pat['name']}) {scrut_text})"
        raise ValueError(f"unknown pattern kind: {pk}")

    def transpile_arm_body(arm):
        # Pattern bindings: for `Ctor(b)` arms, the body may reference `b`.
        # We lower by substituting `b` → `(Ctor_i scrut)` (the field accessor).
        # We rebuild the expression with these substitutions.
        pat = arm["pattern"]
        body = arm["body"]
        subs = {}
        if pat["kind"] == "pat_ctor":
            for i, sub in enumerate(pat["args"]):
                if sub["kind"] == "pat_bind":
                    # Use the accessor naming convention from the enum decl:
                    # field `f0`, `f1`, … — but the user picks names.
                    # For the runtime enums (Effect/Result), the accessors
                    # are listed in the PRELUDE; for user enums, we follow
                    # the spec convention of `f0 / f1 / …` (Appendix A).
                    field = f"{pat['name']}_{i}"
                    # Special-case the prelude accessors so common code
                    # (test_02_counter etc.) works without name guessing.
                    field = _resolve_accessor(pat["name"], i, sub["name"]) or field
                    subs[sub["name"]] = f"({field} {scrut})"
        if subs:
            body = _subst_idents(body, subs)
        return transpile_expr(body, hint_sort=hint_sort,
                              declared_sorts=declared_sorts)

    result = transpile_arm_body(arms[-1])
    for arm in reversed(arms[:-1]):
        test = pat_test(arm["pattern"], scrut)
        b = transpile_arm_body(arm)
        result = f"(ite {test} {b} {result})"
    return result


# Accessor names for the prelude enums. Other enum payloads default to f0/f1/…
_PRELUDE_ACCESSORS = {
    "ArgInt":       ["ArgInt_0"],
    "ArgStr":       ["ArgStr_0"],
    "LibCall":      ["lc_lib", "lc_sym", "lc_sig", "lc_args", "lc_ok", "lc_err"],
    "Println":      ["println_0"],
    "Exit":         ["exit_0"],
    "IntToStr":     ["int_to_str_0"],
    "StringResult": ["string_result_0"],
    "IntResult":    ["int_result_0"],
}


def _resolve_accessor(ctor_name, idx, _bind_name):
    accessors = _PRELUDE_ACCESSORS.get(ctor_name)
    if accessors and idx < len(accessors):
        return accessors[idx]
    return None


def _subst_idents(expr, subs):
    if expr is None: return None
    k = expr["kind"]
    if k in ("int", "real", "bool", "str"): return expr
    if k == "ident":
        if expr["name"] in subs:
            return {"kind": "raw_smt", "text": subs[expr["name"]]}
        return expr
    if k == "qualified": return expr
    if k == "field":
        return {"kind": "field", "recv": _subst_idents(expr["recv"], subs),
                "name": expr["name"]}
    if k == "index":
        return {"kind": "index",
                "recv": _subst_idents(expr["recv"], subs),
                "idx": _subst_idents(expr["idx"], subs)}
    if k == "cardinality":
        return {"kind": "cardinality", "x": _subst_idents(expr["x"], subs)}
    if k == "binop":
        return {"kind": "binop", "op": expr["op"],
                "l": _subst_idents(expr["l"], subs),
                "r": _subst_idents(expr["r"], subs)}
    if k == "unop":
        return {"kind": "unop", "op": expr["op"],
                "x": _subst_idents(expr["x"], subs)}
    if k == "ternary":
        return {"kind": "ternary",
                "cond": _subst_idents(expr["cond"], subs),
                "t": _subst_idents(expr["t"], subs),
                "f": _subst_idents(expr["f"], subs)}
    if k == "call":
        return {"kind": "call", "name": expr["name"],
                "generics": expr.get("generics", []),
                "args": [_subst_idents(a, subs) for a in expr["args"]]}
    if k == "seq":
        return {"kind": "seq",
                "items": [_subst_idents(i, subs) for i in expr["items"]]}
    if k == "match":
        return {"kind": "match",
                "scrutinee": _subst_idents(expr["scrutinee"], subs),
                "arms": [{"kind": "arm", "pattern": a["pattern"],
                          "body": _subst_idents(a["body"], subs)}
                         for a in expr["arms"]]}
    if k == "matches":
        return {"kind": "matches",
                "expr": _subst_idents(expr["expr"], subs),
                "pattern": expr["pattern"]}
    if k == "quantifier":
        # Don't substitute under a quantifier that re-binds the name.
        bound = set(expr["vars"])
        live = {n: v for n, v in subs.items() if n not in bound}
        return {"kind": "quantifier", "q": expr["q"], "vars": expr["vars"],
                "range": _subst_idents(expr["range"], live),
                "body": _subst_idents(expr["body"], live)}
    if k == "tuple":
        return {"kind": "tuple",
                "items": [_subst_idents(i, subs) for i in expr["items"]]}
    if k == "raw_smt":
        return expr
    raise ValueError(f"unknown expr kind in subst: {k}")


def transpile_matches(expr, declared_sorts=None):
    """`e matches Pattern` → variant-tag test only."""
    scrut = transpile_expr(expr["expr"], declared_sorts=declared_sorts)
    pat = expr["pattern"]
    if pat["kind"] == "pat_ctor":
        return f"((_ is {pat['name']}) {scrut})"
    if pat["kind"] == "pat_wildcard":
        return "true"
    raise ValueError(
        "`matches` requires a constructor pattern; got "
        f"{pat['kind']}")


def transpile_quantifier(expr, declared_sorts=None):
    raise NotImplementedError(
        "quantifier lowering not implemented in the bootstrap transpiler "
        "(parsed but unused by the existing test corpus). Express the "
        "constraint without ∀/∃ for now.")


# Hook for the raw_smt escape used by _subst_idents.
_orig_transpile_expr = transpile_expr


def transpile_expr(expr, hint_sort=None, declared_sorts=None):  # noqa: F811
    if expr.get("kind") == "raw_smt":
        return expr["text"]
    return _orig_transpile_expr(expr, hint_sort=hint_sort,
                                declared_sorts=declared_sorts)


# ── Declaration walkers ─────────────────────────────────────────────

def transpile_membership(stmt, declared_sorts):
    """`x ∈ T` (possibly multi-name, possibly with pins).

    The new parser produces {names: [...], set: ..., pins: ...}.
    Multi-name is treated as repeating the binding for each name.
    """
    out = []
    set_expr = stmt["set"]
    sort = set_sort(set_expr)
    for name in stmt["names"]:
        if name not in declared_sorts:
            out.append(f"(declare-const {name} {sort})")
            declared_sorts[name] = sort
        m = set_membership(name, set_expr)
        if m is not None:
            out.append(f"(assert {m})")
    # Pin clauses (optional, only for single-name memberships).
    pins = stmt.get("pins")
    if pins and len(stmt["names"]) == 1:
        nm = stmt["names"][0]
        if pins["kind"] == "pins_named":
            for mp in pins["mappings"]:
                slot = mp["slot"]
                val = transpile_expr(mp["value"], declared_sorts=declared_sorts)
                out.append(f"(assert (= ({slot} {nm}) {val}))")
        elif pins["kind"] == "pins_positional":
            # Positional pins require knowing the field order from a
            # user `type`. Without the type table, we can't lower this.
            raise NotImplementedError(
                "positional pins (`x ∈ T(v1, v2)`) require a known field "
                "order; not implemented in the bootstrap transpiler")
    return out


def transpile_chained_mem(stmt, declared_sorts):
    """`expr cmp IDENT (, IDENT)* ∈ T [cmp expr]*` — desugar to membership +
    one assertion per cmp bound."""
    out = transpile_membership({"kind": "membership", "names": stmt["names"],
                                "set": stmt["set"], "pins": None},
                               declared_sorts)
    for nm in stmt["names"]:
        for lo_expr, op in stmt["lows"]:
            lhs = transpile_expr(lo_expr, declared_sorts=declared_sorts)
            rhs = nm
            smt_op = BINOP_SMT[op]
            out.append(f"(assert ({smt_op} {lhs} {rhs}))")
        for op, hi_expr in stmt["highs"]:
            lhs = nm
            rhs = transpile_expr(hi_expr, declared_sorts=declared_sorts)
            smt_op = BINOP_SMT[op]
            out.append(f"(assert ({smt_op} {lhs} {rhs}))")
    return out


def transpile_constraint(stmt, declared_sorts):
    expr = stmt["expr"]
    # A bare identifier whose name matches a registered schema is the
    # names-match composition mode (spec §7.1). Not yet implemented at
    # the transpiler level — surface as a clear error.
    if expr.get("kind") == "ident" and expr["name"] in SCHEMA_REGISTRY:
        raise NotImplementedError(
            f"names-match claim composition (`{expr['name']}` as a bare "
            f"body item) is parsed but not yet lowered by the bootstrap "
            f"transpiler")
    return [f"(assert {transpile_expr(expr, declared_sorts=declared_sorts)})"]


def transpile_passthrough(stmt, declared_sorts):
    raise NotImplementedError(
        f"`..{stmt['name']}` passthrough composition is parsed but not yet "
        f"lowered by the bootstrap transpiler. Spec §7.4.")


def transpile_claim_call(stmt, declared_sorts):
    raise NotImplementedError(
        f"`{stmt['name']}(slot ↦ …)` explicit-mapping claim call is parsed "
        f"but not yet lowered by the bootstrap transpiler. Spec §7.2.")


def transpile_tuple_in(stmt, declared_sorts):
    raise NotImplementedError(
        f"`(…) ∈ {stmt['name']}` tuple-in dispatch is parsed but not yet "
        f"lowered by the bootstrap transpiler. Spec §7.8.")


def transpile_subclaim_inside_body(stmt, declared_sorts):
    # Top-level subclaim emission happens at the program level; inline
    # subclaims share the parent's vars but aren't called here. The
    # transpiler can register them but emit no SMT-LIB inline.
    decl = stmt["decl"]
    SCHEMA_REGISTRY[decl["name"]] = decl
    return [f";; subclaim {decl['name']} registered (no inline lowering)"]


def transpile_body_stmts(stmts, declared_sorts):
    out = []
    for s in stmts:
        k = s["kind"]
        if k == "membership":
            out.extend(transpile_membership(s, declared_sorts))
        elif k == "chained_mem":
            out.extend(transpile_chained_mem(s, declared_sorts))
        elif k == "constraint":
            out.extend(transpile_constraint(s, declared_sorts))
        elif k == "passthrough":
            out.extend(transpile_passthrough(s, declared_sorts))
        elif k == "claim_call":
            out.extend(transpile_claim_call(s, declared_sorts))
        elif k == "tuple_in":
            out.extend(transpile_tuple_in(s, declared_sorts))
        elif k == "subclaim":
            out.extend(transpile_subclaim_inside_body(s, declared_sorts))
        else:
            raise ValueError(f"unknown body item kind: {k}")
    return out


# ── Schema dispatch ─────────────────────────────────────────────────

def _emit_param_consts(params, declared, is_fsm):
    """Emit declarations for first-line params. FSM params produce state
    pairs (_X and X); claim/type params produce plain consts."""
    out = []
    for p in params:
        sort = set_sort(p["set"])
        if is_fsm:
            prev = f"_{p['name']}"
            if prev not in declared:
                out.append(f"(declare-const {prev} {sort})")
                declared[prev] = sort
                m = set_membership(prev, p["set"])
                if m is not None:
                    out.append(f"(assert {m})")
        if p["name"] not in declared:
            out.append(f"(declare-const {p['name']} {sort})")
            declared[p["name"]] = sort
            m = set_membership(p["name"], p["set"])
            if m is not None:
                out.append(f"(assert {m})")
    return out


def transpile_schema(decl):
    """Dispatch by keyword. For claim/type/schema we treat as plain consts;
    for fsm we use state-pair convention."""
    kw = decl["kw"]
    name = decl["name"]
    is_fsm = (kw == "fsm")
    is_external = decl.get("external", False)
    out = [f";; --- {kw}{' external' if is_external else ''} {name} ---"]
    declared = {}

    out.extend(_emit_param_consts(decl["params"], declared, is_fsm))

    if is_fsm:
        # effects channel — auto-declared for FSMs.
        if "effects" not in declared:
            out.append("(declare-const effects (Seq Effect))")
            declared["effects"] = "(Seq Effect)"
        # last_results — auto-declared so prelude-typed match arms work.
        if "last_results" not in declared:
            out.append("(declare-const last_results (Seq Result))")
            declared["last_results"] = "(Seq Result)"

        # Split body: FTI memberships are inlined; everything else lowers
        # normally.
        rest = []
        for s in decl["body"]:
            if s["kind"] == "membership" and len(s["names"]) == 1 \
                    and _is_fti_set_expr(s["set"]):
                out.extend(transpile_fti_instance(s["names"][0], s["set"],
                                                  declared))
            else:
                rest.append(s)
        out.extend(transpile_body_stmts(rest, declared))
    else:
        out.extend(transpile_body_stmts(decl["body"], declared))

    return "\n".join(out)


def transpile_enum(decl):
    """`enum X = A | B(T1, T2)` → (declare-datatypes ...)."""
    name = decl["name"]
    parts = []
    for v in decl["variants"]:
        if v["fields"]:
            fs = " ".join(
                f"(f{i} {set_sort(f)})" for i, f in enumerate(v["fields"]))
            parts.append(f"({v['name']} {fs})")
        else:
            parts.append(f"({v['name']})")
    return (f";; --- enum {name} ---\n"
            f"(declare-datatypes (({name} 0)) (({' '.join(parts)})))")


# ── FTI inlining (the bootstrap extension) ──────────────────────────

def _is_fti_set_expr(set_expr):
    return (set_expr.get("kind") == "set_named"
            and set_expr["name"] in FTI_REGISTRY)


def _subst_set_expr(set_expr, type_subst):
    if set_expr is None: return None
    k = set_expr["kind"]
    if k == "set_named":
        if set_expr.get("param") is None and set_expr["name"] in type_subst:
            return type_subst[set_expr["name"]]
        return {"kind": "set_named", "name": set_expr["name"],
                "param": _subst_set_expr(set_expr.get("param"), type_subst),
                "generics": set_expr.get("generics", [])}
    if k == "set_range":
        return {"kind": "set_range",
                "lo": _subst_expr(set_expr["lo"], type_subst, {}),
                "hi": _subst_expr(set_expr["hi"], type_subst, {})}
    if k == "set_enum":
        return {"kind": "set_enum",
                "items": [_subst_expr(e, type_subst, {})
                          for e in set_expr["items"]]}
    raise ValueError(f"unknown set_expr kind: {k}")


def _ns_ident(name, ns_prefix, fti_locals):
    if name in fti_locals:
        return f"{ns_prefix}__{name}"
    if name.startswith("_") and name[1:] in fti_locals:
        return f"_{ns_prefix}__{name[1:]}"
    return name


def _subst_expr(expr, type_subst, ns):
    if expr is None: return None
    k = expr["kind"]
    if k in ("int", "real", "bool", "str"): return expr
    if k == "ident":
        nm = expr["name"]
        if nm in type_subst:
            ts = type_subst[nm]
            if ts["kind"] == "set_named" and ts.get("param") is None:
                return {"kind": "ident", "name": ts["name"]}
        if ns:
            return {"kind": "ident",
                    "name": _ns_ident(nm, ns["prefix"], ns["locals"])}
        return expr
    if k == "qualified":
        # Namespace the head if applicable.
        if ns:
            parts = list(expr["parts"])
            parts[0] = _ns_ident(parts[0], ns["prefix"], ns["locals"])
            return {"kind": "qualified", "parts": parts}
        return expr
    if k == "field":
        return {"kind": "field",
                "recv": _subst_expr(expr["recv"], type_subst, ns),
                "name": expr["name"]}
    if k == "index":
        return {"kind": "index",
                "recv": _subst_expr(expr["recv"], type_subst, ns),
                "idx": _subst_expr(expr["idx"], type_subst, ns)}
    if k == "cardinality":
        return {"kind": "cardinality",
                "x": _subst_expr(expr["x"], type_subst, ns)}
    if k == "binop":
        return {"kind": "binop", "op": expr["op"],
                "l": _subst_expr(expr["l"], type_subst, ns),
                "r": _subst_expr(expr["r"], type_subst, ns)}
    if k == "unop":
        return {"kind": "unop", "op": expr["op"],
                "x": _subst_expr(expr["x"], type_subst, ns)}
    if k == "ternary":
        return {"kind": "ternary",
                "cond": _subst_expr(expr["cond"], type_subst, ns),
                "t": _subst_expr(expr["t"], type_subst, ns),
                "f": _subst_expr(expr["f"], type_subst, ns)}
    if k == "call":
        new_args = [_subst_expr(a, type_subst, ns) for a in expr["args"]]
        # LibCall(lib, sym, sig, args, ok_dest, err_dest) — when inlined
        # under an FTI namespace, the ok/err dest strings (positions 4,
        # 5) referring to FTI-local consts need rewriting to the
        # namespaced form.
        if expr["name"] == "LibCall" and len(new_args) == 6 and ns:
            for slot in (4, 5):
                arg = new_args[slot]
                if arg.get("kind") == "str" and arg["value"] in ns["locals"]:
                    new_args[slot] = {"kind": "str",
                                      "value": f"{ns['prefix']}__{arg['value']}"}
        return {"kind": "call", "name": expr["name"],
                "generics": expr.get("generics", []), "args": new_args}
    if k == "seq":
        return {"kind": "seq",
                "items": [_subst_expr(i, type_subst, ns) for i in expr["items"]]}
    if k == "match":
        return {"kind": "match",
                "scrutinee": _subst_expr(expr["scrutinee"], type_subst, ns),
                "arms": [{"kind": "arm", "pattern": a["pattern"],
                          "body": _subst_expr(a["body"], type_subst, ns)}
                         for a in expr["arms"]]}
    if k == "matches":
        return {"kind": "matches",
                "expr": _subst_expr(expr["expr"], type_subst, ns),
                "pattern": expr["pattern"]}
    if k == "tuple":
        return {"kind": "tuple",
                "items": [_subst_expr(i, type_subst, ns)
                          for i in expr["items"]]}
    raise ValueError(f"unknown expr kind in subst: {k}")


def _subst_stmt(stmt, type_subst, ns):
    k = stmt["kind"]
    if k == "membership":
        return {"kind": "membership",
                "names": [_ns_ident(n, ns["prefix"], ns["locals"])
                          for n in stmt["names"]],
                "set": _subst_set_expr(stmt["set"], type_subst),
                "pins": stmt.get("pins")}
    if k == "constraint":
        return {"kind": "constraint",
                "expr": _subst_expr(stmt["expr"], type_subst, ns)}
    raise ValueError(f"unknown FTI body stmt kind: {k}")


def _emit_fti_state_pair(name, set_expr, declared):
    out = []
    sort = set_sort(set_expr)
    prev_name = f"_{name}"
    if prev_name not in declared:
        out.append(f"(declare-const {prev_name} {sort})")
        declared[prev_name] = sort
    if name not in declared:
        out.append(f"(declare-const {name} {sort})")
        declared[name] = sort
    m = set_membership(name, set_expr)
    if m is not None: out.append(f"(assert {m})")
    m_prev = set_membership(prev_name, set_expr)
    if m_prev is not None: out.append(f"(assert {m_prev})")
    return out


def transpile_fti_instance(host_var, set_expr, declared):
    fti = FTI_REGISTRY[set_expr["name"]]
    type_subst = {}
    if fti["type_params"]:
        if set_expr.get("param") is None:
            raise ValueError(
                f"FTI {fti['name']} expects a type argument; got "
                f"{host_var} ∈ {fti['name']}")
        type_subst[fti["type_params"][0]] = set_expr["param"]

    fti_locals = set()
    for s in fti["body"]:
        if s["kind"] == "membership":
            for n in s["names"]:
                fti_locals.add(n)

    ns = {"prefix": host_var, "locals": fti_locals}
    out = [f";; --- inlined FTI {fti['name']} as {host_var} ---"]

    # Pass 1: state-pair declarations.
    for s in fti["body"]:
        if s["kind"] == "membership":
            for n in s["names"]:
                ns_name = f"{host_var}__{n}"
                ns_set = _subst_set_expr(s["set"], type_subst)
                out.extend(_emit_fti_state_pair(ns_name, ns_set, declared))

    # Pass 2: body constraints under namespace + type substitution.
    for s in fti["body"]:
        if s["kind"] == "membership":
            continue
        if s["kind"] == "constraint":
            ns_stmt = _subst_stmt(s, type_subst, ns)
            out.append(
                f"(assert {transpile_expr(ns_stmt['expr'], declared_sorts=declared)})")
        elif s["kind"] in ("passthrough", "claim_call", "tuple_in", "subclaim",
                           "chained_mem"):
            raise NotImplementedError(
                f"{s['kind']} inside an FTI body is not yet lowered")

    return out


# ── Top-level driver ────────────────────────────────────────────────

def transpile(program):
    # Registries are global; re-register on every transpile so the same
    # process can handle multiple programs in sequence (tests).
    register_ftis(program["decls"])
    register_schemas(program["decls"])

    out = [PRELUDE]
    for decl in program["decls"]:
        k = decl["kind"]
        if k == "schema":
            kw = decl["kw"]
            if kw in ("claim", "fsm", "type", "schema", "subclaim"):
                out.append(transpile_schema(decl))
            else:
                raise ValueError(f"unknown schema keyword: {kw!r}")
        elif k == "enum":
            out.append(transpile_enum(decl))
        elif k == "fti":
            # Registered above; no top-level emission. FTI bodies are
            # inlined where a host fsm declares a variable of the FTI's
            # type.
            continue
        elif k == "import":
            # Bootstrap: imports are no-ops (no module system yet).
            # main.py loads prelude/ manually; we trust that.
            continue
        else:
            raise ValueError(f"unknown decl kind: {k}")
    return "\n\n".join(out)
