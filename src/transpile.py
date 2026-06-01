"""Evident AST → SMT-LIB text.

The walker has one arm per AST node kind. The output is what Runtime
(src/runtime.py) consumes.

Conventions baked in:
  - `is_init` is always declared. Each tick the runtime pins it.
  - The Effect datatype + FFIArg datatype are auto-declared at the top.
    Users don't redeclare them.
  - For `fsm` decls, each parameter `name ∈ S` produces a state pair:
    `_name` (previous tick) and `name` (this tick). The body refers to
    `_name` for prev, `name` for new.
  - For `claim` decls, no state pairs; bindings are plain consts.
  - `effects ∈ Seq(Effect)` channel: declared in FSMs; if the body
    doesn't assign it, the runtime sees an empty Seq (its sort default).

Future ergonomics (sugar that's NOT here yet, can be added later):
  - `name_next = ...` as a synonym for assigning to `name`.
  - Implicit `effects = []` when not assigned.
  - Library imports.
"""


# ── Built-in declarations every program gets ────────────────────────

PRELUDE = """
; Effect / FFIArg datatypes baked into every Evident program.
; Users can emit LibCall effects against this Effect type without
; redeclaring it. New top-level effect kinds can be added by the
; runtime; library-level effects compose via LibCall.

(declare-datatypes ((FFIArg 0))
  (((ArgInt (ArgInt_0 Int))
    (ArgStr (ArgStr_0 String)))))

(declare-datatypes ((Effect 0))
  (((LibCall (lc_lib  String) (lc_sym  String) (lc_sig  String)
             (lc_args (Seq FFIArg))
             (lc_ok   String) (lc_err  String)))))

(declare-const is_init Bool)
""".strip()


# ── Built-in set names → Z3 sort SMT-LIB representation ──────────────

BUILTIN_SORTS = {
    "Int":    "Int",
    "Bool":   "Bool",
    "Real":   "Real",
    "String": "String",
}


# ── Operator translation tables ──────────────────────────────────────

BINOP_SMT = {
    "+":  "+",   "-":  "-",   "*":  "*",   "/":  "div",  "mod": "mod",
    "=":  "=",   "≠":  "distinct",
    "<":  "<",   "≤":  "<=",  ">":  ">",   "≥":  ">=",
    "∧":  "and", "∨":  "or",
}

UNOP_SMT = {
    "-":  "-",
    "¬":  "not",
}


# ── Set expression handling: sort name + optional membership constraint ─

def set_sort(set_expr):
    """Return the Z3 sort sexpr for the host of this set."""
    if set_expr["kind"] == "set_named":
        name = set_expr["name"]
        if set_expr["param"] is None:
            if name in BUILTIN_SORTS: return BUILTIN_SORTS[name]
            return name  # user-defined type
        # Generic, e.g. Seq(Int)
        inner = set_sort(set_expr["param"])
        if name == "Seq": return f"(Seq {inner})"
        return f"({name} {inner})"
    if set_expr["kind"] in ("set_range", "set_enum"):
        return "Int"  # ranges and enums of values default to Int
    raise ValueError(f"unknown set_expr kind: {set_expr['kind']}")


def set_membership(name, set_expr):
    """Return SMT-LIB Bool expression asserting `name ∈ set_expr`, or None
    when membership is trivially true (host sort)."""
    if set_expr["kind"] == "set_named":
        # No constraint for builtin host sorts (Int, Bool, etc.).
        if set_expr["param"] is None and set_expr["name"] in BUILTIN_SORTS:
            return None
        # Generic (e.g., Seq(Int)) — also no membership predicate; the
        # sort declaration alone asserts membership.
        if set_expr["param"] is not None:
            return None
        return None  # named user type — handled by sort decl
    if set_expr["kind"] == "set_range":
        lo = transpile_expr(set_expr["lo"])
        hi = transpile_expr(set_expr["hi"])
        return f"(and (>= {name} {lo}) (<= {name} {hi}))"
    if set_expr["kind"] == "set_enum":
        items = [transpile_expr(e) for e in set_expr["items"]]
        return "(or " + " ".join(f"(= {name} {item})" for item in items) + ")"
    raise ValueError(f"unknown set_expr kind: {set_expr['kind']}")


# ── Expression walker ───────────────────────────────────────────────
#
# A `hint_sort` argument threads expected-sort context downward. It's
# used for one purpose: when an empty seq literal `[]` is emitted, the
# walker needs to know its element type to produce a valid SMT-LIB
# `(as seq.empty (Seq T))`. The hint is set by binop `=` (LHS sort
# propagated to RHS) and by `match` (arm bodies inherit the match's
# hint).  `declared_sorts` is the name→sort map populated as bindings
# are processed.

def transpile_expr(expr, hint_sort=None, declared_sorts=None):
    k = expr["kind"]

    if k == "int":  return str(expr["value"])
    if k == "bool": return "true" if expr["value"] else "false"
    if k == "str":
        escaped = expr["value"].replace("\\", "\\\\").replace('"', '""')
        return f'"{escaped}"'
    if k == "ident": return expr["name"]

    if k == "binop":
        op = BINOP_SMT.get(expr["op"])
        if op is None:
            raise ValueError(f"unknown binop: {expr['op']}")
        # For `=`, propagate the LHS's declared sort to the RHS so that
        # `effects = [...]` can hand `Seq Effect` to an empty literal.
        rhs_hint = None
        if expr["op"] == "=" and declared_sorts is not None \
           and expr["l"]["kind"] == "ident":
            rhs_hint = declared_sorts.get(expr["l"]["name"])
        l = transpile_expr(expr["l"], hint_sort=None, declared_sorts=declared_sorts)
        r = transpile_expr(expr["r"], hint_sort=rhs_hint, declared_sorts=declared_sorts)
        return f"({op} {l} {r})"

    if k == "unop":
        op = UNOP_SMT.get(expr["op"])
        if op is None:
            raise ValueError(f"unknown unop: {expr['op']}")
        x = transpile_expr(expr["x"], hint_sort=hint_sort, declared_sorts=declared_sorts)
        return f"({op} {x})"

    if k == "call":
        # Function application — emit as positional SMT-LIB. Names that
        # match builtin Z3 constructors (LibCall, ArgInt, ArgStr) just
        # work; user-defined claims-as-functions need declare-fun forms
        # in a future pass.
        args = " ".join(transpile_expr(a, declared_sorts=declared_sorts)
                        for a in expr["args"])
        if expr["args"]:
            return f"({expr['name']} {args})"
        return f"({expr['name']})"

    if k == "seq":
        # Lower [a, b, c] to (seq.++ (seq.unit a) (seq.++ (seq.unit b) (seq.unit c)))
        if not expr["items"]:
            # Empty seq literal — element sort comes from the hint.
            # The hint is the outer sequence sort, e.g. `(Seq Effect)`.
            if hint_sort is None:
                raise ValueError(
                    "empty seq literal `[]` has no inferable element sort; "
                    "use `empty(T)` to specify it explicitly")
            return f"(as seq.empty {hint_sort})"
        units = [f"(seq.unit {transpile_expr(item, declared_sorts=declared_sorts)})"
                 for item in expr["items"]]
        result = units[-1]
        for unit in reversed(units[:-1]):
            result = f"(seq.++ {unit} {result})"
        return result

    if k == "match":
        return transpile_match(expr, hint_sort=hint_sort, declared_sorts=declared_sorts)

    raise ValueError(f"unknown expr kind: {k}")


def transpile_match(expr, hint_sort=None, declared_sorts=None):
    """Lower `match scrut: pat => body; ...` into nested ite."""
    scrut = transpile_expr(expr["scrutinee"], declared_sorts=declared_sorts)
    arms = expr["arms"]

    def pat_test(pat, scrut_text):
        pk = pat["kind"]
        if pk == "pat_wildcard": return "true"
        if pk == "pat_int":  return f"(= {scrut_text} {pat['value']})"
        if pk == "pat_bool": return f"(= {scrut_text} {'true' if pat['value'] else 'false'})"
        if pk == "pat_str":
            v = pat["value"].replace("\\", "\\\\").replace('"', '""')
            return f'(= {scrut_text} "{v}")'
        if pk == "pat_bind":
            # An IDENT pattern: matches anything, binds the name.
            # For now, only handle as wildcard (no binding semantics here);
            # user can refer to the scrutinee directly. Future: introduce
            # let-bindings around the arm body.
            return "true"
        if pk == "pat_ctor":
            # Test via `((_ is CTOR) scrut)`. Field bindings would need
            # let around the body — deferred.
            return f"((_ is {pat['name']}) {scrut_text})"
        raise ValueError(f"unknown pattern kind: {pk}")

    # Build right-to-left: ite(test_0, body_0, ite(test_1, body_1, ...))
    result = transpile_expr(arms[-1]["body"], hint_sort=hint_sort,
                            declared_sorts=declared_sorts)
    for arm in reversed(arms[:-1]):
        test = pat_test(arm["pattern"], scrut)
        body = transpile_expr(arm["body"], hint_sort=hint_sort,
                              declared_sorts=declared_sorts)
        result = f"(ite {test} {body} {result})"
    return result


# ── Declaration walkers ─────────────────────────────────────────────
#
# `declared_sorts` is a dict mapping declared-const name to its SMT-LIB
# sort sexpr (e.g. `Int`, `(Seq Effect)`). It's used by `transpile_expr`
# to thread expected sorts down to empty seq literals.

def transpile_binding(stmt, declared_sorts):
    name = stmt["name"]
    set_expr = stmt["set"]
    sort = set_sort(set_expr)
    out = []
    if name not in declared_sorts:
        out.append(f"(declare-const {name} {sort})")
        declared_sorts[name] = sort
    membership = set_membership(name, set_expr)
    if membership is not None:
        out.append(f"(assert {membership})")
    return out


def transpile_assertion(stmt, declared_sorts):
    return [f"(assert {transpile_expr(stmt['expr'], declared_sorts=declared_sorts)})"]


def transpile_body_stmts(stmts, declared_sorts):
    out = []
    for s in stmts:
        if s["kind"] == "binding":
            out.extend(transpile_binding(s, declared_sorts))
        elif s["kind"] == "assertion":
            out.extend(transpile_assertion(s, declared_sorts))
        else:
            raise ValueError(f"unknown stmt kind: {s['kind']}")
    return out


def transpile_claim(decl):
    """A claim is a pure constraint model: bindings + assertions, no state
    pairs, no transition. Solves once."""
    out = [f"; --- claim {decl['name']} ---"]
    declared = {}

    # Parameters become plain declared consts.
    for p in decl["params"]:
        sort = set_sort(p["set"])
        out.append(f"(declare-const {p['name']} {sort})")
        declared[p["name"]] = sort
        m = set_membership(p["name"], p["set"])
        if m is not None:
            out.append(f"(assert {m})")

    out.extend(transpile_body_stmts(decl["body"], declared))
    return "\n".join(out)


def transpile_fsm(decl):
    """An FSM's parameters become STATE PAIRS: each `name ∈ S` produces
    `_name` and `name`. Body statements reference either."""
    out = [f"; --- fsm {decl['name']} ---"]
    declared = {}

    # State pairs for parameters.
    for p in decl["params"]:
        sort = set_sort(p["set"])
        prev_name = f"_{p['name']}"
        out.append(f"(declare-const {prev_name} {sort})")
        out.append(f"(declare-const {p['name']} {sort})")
        declared[prev_name] = sort
        declared[p["name"]] = sort
        m = set_membership(p["name"], p["set"])
        if m is not None:
            out.append(f"(assert {m})")
        m_prev = set_membership(prev_name, p["set"])
        if m_prev is not None:
            out.append(f"(assert {m_prev})")

    # effects channel — auto-declared for FSMs.
    out.append("(declare-const effects (Seq Effect))")
    declared["effects"] = "(Seq Effect)"

    out.extend(transpile_body_stmts(decl["body"], declared))
    return "\n".join(out)


def transpile_type(decl):
    """`type T = | A(fields) | B(fields)` → (declare-datatypes ...)."""
    name = decl["name"]
    parts = []
    for v in decl["variants"]:
        if v["fields"]:
            fs = " ".join(
                f"({f['name']} {set_sort(f['set'])})" for f in v["fields"])
            parts.append(f"({v['name']} {fs})")
        else:
            parts.append(f"({v['name']})")
    return (f"; --- type {name} ---\n"
            f"(declare-datatypes (({name} 0)) (({' '.join(parts)})))")


# ── Top-level driver ────────────────────────────────────────────────

def transpile(program):
    """Walk the program AST; return one SMT-LIB body string."""
    out = [PRELUDE]
    for decl in program["decls"]:
        if decl["kind"] == "claim":
            out.append(transpile_claim(decl))
        elif decl["kind"] == "fsm":
            out.append(transpile_fsm(decl))
        elif decl["kind"] == "type":
            out.append(transpile_type(decl))
        else:
            raise ValueError(f"unknown decl kind: {decl['kind']}")
    return "\n\n".join(out)
