"""Evident parser — lex + parse → AST.

Rewritten 2026-06-01 against `docs/evident-language-spec.md` (the
authoritative spec extracted from the Rust runtime on `main`). This
supersedes the original bootstrap parser, which got six of seven
composition mechanisms wrong, used `;` for comments, and used `=>` /
`:` for match arms instead of `⇒`.

Surface language (spec §1-§14, Appendix A) plus one documented
extension: `fti Name(type_params)` for Foreign Type Interfaces,
documented at the end of evident-language-spec.md.

Lexer
-----
* Line comments: `--` to end of line.
* Indentation-sensitive blocks (Python-style NEWLINE / INDENT / DEDENT).
* Newlines are silently consumed inside `( [ { ⟨` groups (paren_depth).
* Keywords:
    claim fsm fti type schema subclaim external enum
    match matches import in true false mapsto
* Operators (Unicode + ASCII alt where applicable):
    ∈ in   ∉   ∋   ∧   ∨   ¬   ⇒ =>   ≤ <=   ≥ >=   ≠ !=
    ∀   ∃   ↦ mapsto   ⟨   ⟩   #
    = < > + - * /   ++   ,   |   ?   :   ..   .

AST
---
Plain Python dicts with a "kind" tag. The kinds the transpiler walks:
    program     {decls}
    enum        {name, variants}
    schema      {kw, external, name, type_params, params, body}
                  -- kw ∈ {"claim","fsm","type","schema","subclaim"}
    fti         {name, type_params, body}              -- our extension
    import      {path}
    variant     {name, fields}                 -- enum variant

  Body items:
    membership  {name(s), set, pins}           -- x ∈ T (pins?)
    passthrough {name}                         -- ..ClaimName
    claim_call  {name, mappings}               -- Name(slot ↦ val, …)
    tuple_in    {args, name}                   -- (a, b) ∈ ClaimName
    subclaim    {decl}                         -- subclaim Name body
    chained_mem {names, set, lows, highs}      -- 0 < x ∈ Int < 5
    constraint  {expr}                         -- any expression

  Expressions:
    int / real / bool / str
    ident       {name}
    qualified   {parts}        -- a.b.c (dotted ident chain)
    field       {recv, name}   -- recv.field built postfix
    index       {recv, idx}    -- seq[i]
    binop       {op, l, r}
    unop        {op, x}
    call        {name, args}   -- Name(args)
    seq         {items}        -- ⟨e1, e2, …⟩
    set_lit     {items}        -- {e1, e2, …}   (RHS of ∈)
    set_range   {lo, hi}       -- {lo..hi}     (RHS of ∈ or quantifier)
    quantifier  {q, vars, range, body}    -- ∀/∃ vars ∈ range : body
    ternary     {cond, t, f}              -- c ? a : b
    match       {scrutinee, arms}
    matches     {expr, pattern}           -- e matches Pat
    cardinality {x}                       -- #expr
    mapping     {slot, value}             -- slot ↦ value (inside ClaimCall)

  Patterns:
    pat_wildcard
    pat_bind      {name}        -- lowercase
    pat_ctor      {name, args}  -- Capitalized [(pattern…)]
    pat_int / pat_str / pat_bool   -- literal patterns (extension)
"""


# ──────────────────────────── lexer ────────────────────────────

KEYWORDS = {
    "claim", "fsm", "fti", "type", "schema", "subclaim",
    "external", "enum", "match", "matches", "import",
    "in", "mapsto", "true", "false",
}

# Multi-char symbols. Order matters: longer prefixes first.
MULTI_SYMBOLS = [
    "++", "..", "=>", "<=", ">=", "!=",
    "↦", "⇒", "≤", "≥", "≠",
]

# Single-char ASCII + Unicode tokens.
SINGLE_SYMBOLS = set(
    "∈∉∋∨∧¬∀∃⟨⟩↦⇒≤≥≠=+-*/()[]{}<>,:|?#."
)


class Token:
    __slots__ = ("kind", "value", "line", "col")
    def __init__(self, kind, value, line, col):
        self.kind, self.value, self.line, self.col = kind, value, line, col
    def __repr__(self):
        return f"Token({self.kind!r}, {self.value!r}, L{self.line}:{self.col})"


def lex(source):
    """Source text → list of Tokens."""
    tokens = []
    indent_stack = [0]
    i = 0
    line = 1
    col = 1
    line_start = True
    paren_depth = 0
    src_len = len(source)

    def emit(kind, value):
        tokens.append(Token(kind, value, line, col))

    while i < src_len:
        c = source[i]

        # End-of-line.
        if c == "\n":
            if paren_depth == 0 and not line_start:
                emit("NEWLINE", "\n")
                line_start = True
            i += 1
            line += 1
            col = 1
            continue

        # `--` line comments (spec §1: line comments only).
        if c == "-" and i + 1 < src_len and source[i + 1] == "-":
            while i < src_len and source[i] != "\n":
                i += 1
            continue

        # Leading whitespace on a logical line → maybe INDENT / DEDENT.
        if line_start and paren_depth == 0:
            indent = 0
            while i < src_len and (source[i] == " " or source[i] == "\t"):
                indent += 4 if source[i] == "\t" else 1
                i += 1
                col += 1
            if i >= src_len or source[i] == "\n":
                continue  # blank line
            if source[i] == "-" and i + 1 < src_len and source[i + 1] == "-":
                continue  # comment-only line
            if indent > indent_stack[-1]:
                indent_stack.append(indent)
                emit("INDENT", indent)
            else:
                while indent < indent_stack[-1]:
                    indent_stack.pop()
                    emit("DEDENT", indent)
                if indent != indent_stack[-1]:
                    raise SyntaxError(
                        f"L{line}:{col} unindent does not match outer level")
            line_start = False
            continue

        # Inline whitespace.
        if c == " " or c == "\t":
            i += 1
            col += 1
            continue

        # Strings.
        if c == '"':
            start = col
            i += 1
            col += 1
            chars = []
            while i < src_len and source[i] != '"':
                if source[i] == "\\" and i + 1 < src_len:
                    esc = source[i + 1]
                    chars.append({"n": "\n", "t": "\t",
                                  '"': '"', "\\": "\\"}.get(esc, esc))
                    i += 2
                    col += 2
                else:
                    chars.append(source[i])
                    i += 1
                    col += 1
            if i >= src_len:
                raise SyntaxError(f"L{line}:{start} unterminated string")
            i += 1
            col += 1
            tokens.append(Token("STRING", "".join(chars), line, start))
            continue

        # Numbers. Real if dot followed by digit; else Int.
        if c.isdigit():
            start = col
            j = i
            while j < src_len and source[j].isdigit():
                j += 1
            if (j + 1 < src_len and source[j] == "."
                    and source[j + 1].isdigit()):
                # Real literal.
                j += 1
                while j < src_len and source[j].isdigit():
                    j += 1
                tokens.append(Token("REAL", float(source[i:j]), line, start))
            else:
                tokens.append(Token("INTEGER", int(source[i:j]), line, start))
            col += j - i
            i = j
            continue

        # Identifiers + keywords. ASCII identifiers per the reference.
        if c.isascii() and (c.isalpha() or c == "_"):
            start = col
            j = i
            while j < src_len and source[j].isascii() and (
                    source[j].isalnum() or source[j] == "_"):
                j += 1
            word = source[i:j]
            col += j - i
            i = j
            kind = "KEYWORD" if word in KEYWORDS else "IDENT"
            tokens.append(Token(kind, word, line, start))
            continue

        # Multi-char symbols.
        matched = False
        for sym in MULTI_SYMBOLS:
            slen = len(sym)
            if source[i:i + slen] == sym:
                tokens.append(Token("SYMBOL", sym, line, col))
                i += slen
                col += slen
                matched = True
                break
        if matched:
            continue

        # Single-char symbols.
        if c in SINGLE_SYMBOLS:
            if c in "([{⟨":
                paren_depth += 1
            if c in ")]}⟩":
                paren_depth -= 1
            tokens.append(Token("SYMBOL", c, line, col))
            i += 1
            col += 1
            continue

        raise SyntaxError(
            f"L{line}:{col} unexpected character {c!r} (U+{ord(c):04X})")

    while len(indent_stack) > 1:
        indent_stack.pop()
        tokens.append(Token("DEDENT", 0, line, col))
    tokens.append(Token("EOF", None, line, col))
    return tokens


# ──────────────────────────── parser ────────────────────────────

# Tokens that act as the "in" / "∈" operator interchangeably.
def _is_in(tok):
    return ((tok.kind == "SYMBOL" and tok.value == "∈")
            or (tok.kind == "KEYWORD" and tok.value == "in"))


# Tokens that act as `↦` / `mapsto`.
def _is_mapsto(tok):
    return ((tok.kind == "SYMBOL" and tok.value == "↦")
            or (tok.kind == "KEYWORD" and tok.value == "mapsto"))


# Tokens that act as `⇒` / `=>`.
def _is_implies(tok):
    return tok.kind == "SYMBOL" and tok.value in ("⇒", "=>")


# Comparison operators.
_CMP_OPS = {"=", "≠", "!=", "<", "≤", "<=", ">", "≥", ">="}
def _is_cmp(tok):
    return tok.kind == "SYMBOL" and tok.value in _CMP_OPS

# Canonicalize ASCII alternatives to their Unicode spelling for AST stability.
def _canon_op(op):
    return {"!=": "≠", "<=": "≤", ">=": "≥", "=>": "⇒"}.get(op, op)


class Parser:
    """Recursive-descent over the spec grammar (Appendix A)."""

    def __init__(self, tokens):
        self.tokens = tokens
        self.pos = 0

    # --- low-level token helpers ---

    def peek(self, n=0):
        return self.tokens[self.pos + n]

    def at(self, kind, value=None):
        t = self.peek()
        if t.kind != kind:
            return False
        if value is not None and t.value != value:
            return False
        return True

    def at_sym(self, value):
        return self.at("SYMBOL", value)

    def at_kw(self, value):
        return self.at("KEYWORD", value)

    def eat(self, kind=None, value=None):
        t = self.peek()
        if kind and t.kind != kind:
            raise SyntaxError(
                f"L{t.line}:{t.col} expected {kind}, got {t.kind} ({t.value!r})")
        if value is not None and t.value != value:
            raise SyntaxError(
                f"L{t.line}:{t.col} expected {value!r}, got {t.value!r}")
        self.pos += 1
        return t

    def accept(self, kind=None, value=None):
        if self.at(kind, value):
            return self.eat(kind, value)
        return None

    def skip_newlines(self):
        while self.accept("NEWLINE"):
            pass

    # --- program ---

    def parse_program(self):
        decls = []
        self.skip_newlines()
        while not self.at("EOF"):
            self.skip_newlines()
            if self.at("EOF"):
                break
            decls.append(self.parse_toplevel())
            self.skip_newlines()
        return {"kind": "program", "decls": decls}

    def parse_toplevel(self):
        t = self.peek()
        if self.at_kw("import"):
            return self.parse_import()
        if self.at_kw("enum"):
            return self.parse_enum()
        if self.at_kw("fti"):
            return self.parse_fti()
        if self.at_kw("external"):
            return self.parse_schema(external=True)
        if t.kind == "KEYWORD" and t.value in ("claim", "fsm", "type", "schema"):
            return self.parse_schema(external=False)
        if self.at_kw("subclaim"):
            # Top-level subclaim is unusual but legal per the spec:
            # subclaim is "registered as a top-level schema at load time"
            # (§4). Parse it as a schema-shaped decl with kw = "subclaim".
            return self.parse_schema(external=False, force_subclaim=True)
        raise SyntaxError(
            f"L{t.line}:{t.col} expected top-level decl, got {t.kind} ({t.value!r})")

    # --- import ---

    def parse_import(self):
        self.eat("KEYWORD", "import")
        path = self.eat("STRING").value
        self.accept("NEWLINE")
        return {"kind": "import", "path": path}

    # --- enum ---

    def parse_enum(self):
        self.eat("KEYWORD", "enum")
        name = self.eat("IDENT").value
        self.eat("SYMBOL", "=")
        # Multi-line form: NEWLINE INDENT [|] V (NEWLINE [|] V)*
        # Single-line form: V | V | …
        if self.accept("NEWLINE"):
            self.eat("INDENT")
            self.accept("SYMBOL", "|")
            variants = [self.parse_enum_variant()]
            while True:
                self.skip_newlines()
                if self.at("DEDENT") or self.at("EOF"):
                    break
                self.accept("SYMBOL", "|")
                variants.append(self.parse_enum_variant())
            self.accept("DEDENT")
        else:
            variants = [self.parse_enum_variant()]
            while self.accept("SYMBOL", "|"):
                variants.append(self.parse_enum_variant())
            self.accept("NEWLINE")
        return {"kind": "enum", "name": name, "variants": variants}

    def parse_enum_variant(self):
        name = self.eat("IDENT").value
        fields = []
        if self.accept("SYMBOL", "("):
            if not self.at_sym(")"):
                fields.append(self.parse_type_ref())
                while self.accept("SYMBOL", ","):
                    fields.append(self.parse_type_ref())
            self.eat("SYMBOL", ")")
        return {"kind": "variant", "name": name, "fields": fields}

    # --- schema (type / claim / fsm / schema / subclaim) ---

    def parse_schema(self, external, force_subclaim=False):
        if external:
            self.eat("KEYWORD", "external")
            kw_tok = self.peek()
            if kw_tok.kind != "KEYWORD" or kw_tok.value not in (
                    "type", "claim", "fsm"):
                raise SyntaxError(
                    f"L{kw_tok.line}:{kw_tok.col} `external` must precede "
                    f"`type` / `claim` / `fsm`")
            kw = self.eat("KEYWORD").value
        elif force_subclaim:
            kw = self.eat("KEYWORD", "subclaim").value
        else:
            kw = self.eat("KEYWORD").value
            if kw not in ("type", "claim", "fsm", "schema"):
                raise SyntaxError(f"unexpected schema keyword: {kw!r}")
        name = self.eat("IDENT").value
        type_params = self.parse_type_params()
        params = []
        if self.accept("SYMBOL", "("):
            params = self.parse_param_groups()
            self.eat("SYMBOL", ")")
        body = self.parse_optional_body()
        return {
            "kind": "schema",
            "kw": kw,
            "external": external,
            "name": name,
            "type_params": type_params,
            "params": params,
            "body": body,
        }

    def parse_fti(self):
        """Our extension: `fti Name<T1, T2>(params)` body.

        Documented as an extension at the end of evident-language-spec.md.
        Semantically: an external-fsm-like declaration whose state-pair
        variables are materialized against external storage via libcall
        at tick boundaries. Tick 0 init runs libcalls; subsequent ticks
        rely on state-pair carry-forward; only the FTI writes external
        state.

        The first-line parens may either be (T1, T2) for the legacy
        bootstrap shape (type-only parameters) OR (param ∈ Type, …) for
        the spec-aligned shape. We accept both; the param_groups parser
        distinguishes by whether `∈` appears.
        """
        self.eat("KEYWORD", "fti")
        name = self.eat("IDENT").value
        type_params = self.parse_type_params()
        params = []
        legacy_type_params = []
        if self.accept("SYMBOL", "("):
            # Try param_groups first; if no `∈` shows up, the parens hold
            # bare type identifiers (legacy bootstrap shape, kept for the
            # transitional Stack / Queue FTI files in prelude/).
            saved = self.pos
            try:
                # Heuristic: look ahead until ',' or ')' for an `∈` token.
                k = saved
                depth = 0
                has_in = False
                while k < len(self.tokens):
                    t = self.tokens[k]
                    if t.kind == "SYMBOL" and t.value in "([{⟨":
                        depth += 1
                    elif t.kind == "SYMBOL" and t.value in ")]}⟩":
                        if depth == 0:
                            break
                        depth -= 1
                    elif depth == 0 and _is_in(t):
                        has_in = True
                        break
                    k += 1
                if has_in:
                    params = self.parse_param_groups()
                else:
                    if not self.at_sym(")"):
                        legacy_type_params.append(self.eat("IDENT").value)
                        while self.accept("SYMBOL", ","):
                            legacy_type_params.append(self.eat("IDENT").value)
            except SyntaxError:
                self.pos = saved
                raise
            self.eat("SYMBOL", ")")
        body = self.parse_optional_body()
        # Merge bracket-style and paren-style type params for the AST
        # so the transpiler sees a single list.
        all_type_params = list(type_params) + legacy_type_params
        return {
            "kind": "fti",
            "name": name,
            "type_params": all_type_params,
            "params": params,
            "body": body,
        }

    def parse_type_params(self):
        """`<T1, T2, …>` — generic type parameters."""
        if not self.accept("SYMBOL", "<"):
            return []
        params = [self.eat("IDENT").value]
        while self.accept("SYMBOL", ","):
            params.append(self.eat("IDENT").value)
        self.eat("SYMBOL", ">")
        return params

    def parse_param_groups(self):
        """First-line param list: `(x ∈ T, y ∈ U)` or `(x, y ∈ T)`.

        Per spec §3, multiple names sharing one type via comma are
        equivalent to repeating the type. We flatten into one params
        list of {name, set} dicts in declaration order.

        Within a single group, multiple `,`-separated IDENTs may share
        one type: `x, y ∈ T` and `world, world_next ∈ World` both
        produce two memberships. To distinguish "still in this group"
        from "starting next group" we look ahead from each comma: if
        the next bare-ident chain is terminated by `∈`, those idents
        are still in the same group.
        """
        params = []
        if self.at_sym(")"):
            return params
        while True:
            names = [self.eat("IDENT").value]
            while (self.at_sym(",") and self.peek(1).kind == "IDENT"
                   and self._comma_continues_param_group(self.pos)):
                self.eat("SYMBOL", ",")
                names.append(self.eat("IDENT").value)
            if not _is_in(self.peek()):
                t = self.peek()
                raise SyntaxError(
                    f"L{t.line}:{t.col} expected ∈ in param list, got {t.value!r}")
            self.eat()  # consume ∈/in
            set_expr = self.parse_type_ref()
            for nm in names:
                params.append({"kind": "param", "name": nm, "set": set_expr})
            if not self.accept("SYMBOL", ","):
                break
        return params

    def _comma_continues_param_group(self, comma_pos):
        """Look ahead from a `,` to decide whether it continues the
        current param group (next IDENT chain leads to `∈`) or starts
        a new one (next IDENT chain leads to `∈` too — but separated by
        more IDENTs). The discriminator: from `,`, walk IDENT (, IDENT)*
        at depth 0; if the first non-(`,`,IDENT) token is `∈`, the
        whole chain is one group.
        """
        k = comma_pos
        # Walk: comma, ident, (comma, ident)*, then check the next.
        while k < len(self.tokens):
            t = self.tokens[k]
            if t.kind == "SYMBOL" and t.value == ",":
                k += 1
                continue
            if t.kind == "IDENT":
                k += 1
                continue
            if _is_in(t):
                return True
            return False
        return False

    # --- type references (RHS of ∈ in a param / membership) ---

    def parse_type_ref(self):
        """type_ref := IDENT [<T,…>] | IDENT "(" type_ref ")" | {lo..hi} | {a,b,…}

        Returns a set_expr-shaped dict, consistent with the existing
        transpile.py expectations:
          {kind: "set_named", name, param, generics}
          {kind: "set_range", lo, hi}
          {kind: "set_enum",  items}
        """
        # Set / range literals as the RHS of ∈.
        if self.at_sym("{"):
            return self.parse_set_or_range()

        name = self.eat("IDENT").value
        # Optional `<T1, T2>` generic args.
        generics = []
        if self.at_sym("<"):
            # Only treat as generic args if we can match the closing >.
            # In a type-ref context this is unambiguous because comparisons
            # don't appear here.
            self.eat("SYMBOL", "<")
            generics.append(self.parse_type_ref())
            while self.accept("SYMBOL", ","):
                generics.append(self.parse_type_ref())
            self.eat("SYMBOL", ">")
        # Optional `(inner)` — could be:
        #   - container/generic-arg syntax: `Seq(Int)`, `Stack(Int)`
        #     (one IDENT-typed argument, no commas, no `↦`)
        #   - pins clause:  `IVec2(5, 7)`  (positional, value args)
        #                   `IVec2(x ↦ 5)` (named, `↦`)
        # The pins clause is left for the membership parser to consume.
        param = None
        if (self.at_sym("(") and not generics
                and self._lookahead_is_container_arg()):
            self.eat("SYMBOL", "(")
            param = self.parse_type_ref()
            self.eat("SYMBOL", ")")
        return {
            "kind": "set_named",
            "name": name,
            "param": param,
            "generics": generics,
        }

    def _lookahead_is_container_arg(self):
        """Inside `(...)`, is the content a single type_ref (bare IDENT
        possibly with nested `(…)` / `<…>`) with no `,` at depth 0 and
        no `↦`? Then it's container/generic-arg syntax."""
        k = self.pos
        if not (k < len(self.tokens)
                and self.tokens[k].kind == "SYMBOL"
                and self.tokens[k].value == "("):
            return False
        # Must start with an IDENT (type names are IDENTs).
        if self.tokens[k + 1].kind != "IDENT":
            return False
        # Scan inside the parens at depth 0, look for `,`, `↦`, `mapsto`.
        depth = 1
        j = k + 1
        while j < len(self.tokens) and depth > 0:
            t = self.tokens[j]
            if t.kind == "SYMBOL" and t.value in "([{⟨":
                depth += 1
            elif t.kind == "SYMBOL" and t.value in ")]}⟩":
                depth -= 1
                if depth == 0:
                    break
            elif depth == 1 and (t.kind == "SYMBOL" and t.value == ","
                                 or _is_mapsto(t)):
                return False
            j += 1
        return True

    def _lookahead_is_pin_clause(self):
        """Peek inside `(...)`: is it `IDENT ↦ …`? Then it's a pins
        clause, not container-head syntax."""
        n = len(self.tokens)
        k = self.pos
        if not (k < n
                and self.tokens[k].kind == "SYMBOL"
                and self.tokens[k].value == "("):
            return False
        if k + 2 >= n or self.tokens[k + 1].kind != "IDENT":
            return False
        return _is_mapsto(self.tokens[k + 2])

    def parse_set_or_range(self):
        self.eat("SYMBOL", "{")
        if self.accept("SYMBOL", "}"):
            return {"kind": "set_enum", "items": []}
        first = self.parse_expr()
        if self.accept("SYMBOL", ".."):
            hi = self.parse_expr()
            self.eat("SYMBOL", "}")
            return {"kind": "set_range", "lo": first, "hi": hi}
        items = [first]
        while self.accept("SYMBOL", ","):
            items.append(self.parse_expr())
        self.eat("SYMBOL", "}")
        return {"kind": "set_enum", "items": items}

    # --- body ---

    def parse_optional_body(self):
        """Body is optional — a schema may have no body at all (e.g.,
        a stub `type IVec2(x, y ∈ Int)` declaration)."""
        if not self.accept("NEWLINE"):
            return []
        if not self.accept("INDENT"):
            return []
        return self.parse_body_items()

    def parse_body_items(self):
        items = []
        while not self.at("DEDENT") and not self.at("EOF"):
            if self.accept("NEWLINE"):
                continue
            items.append(self.parse_body_item())
            self.accept("NEWLINE")
        self.accept("DEDENT")
        return items

    def parse_body_item(self):
        # subclaim Name…
        if self.at_kw("subclaim"):
            return self.parse_subclaim()

        # ..ClaimName (passthrough)
        if self.at_sym(".."):
            self.eat()
            name = self.eat("IDENT").value
            return {"kind": "passthrough", "name": name}

        # Try membership / chained_mem / claim_call / tuple_in / constraint.
        # The disambiguation is local: scan ahead for `∈` not inside
        # nested parens before any newline, and check the shape.

        # Tuple-in form: `(args) ∈ ClaimName`
        if self.at_sym("(") and self._lookahead_is_tuple_in():
            return self.parse_tuple_in()

        # Membership / chained-membership: `IDENT (, IDENT)* ∈ type [pins]`
        # (possibly with leading comparisons for chained form)
        m = self._try_parse_membership_or_chained()
        if m is not None:
            return m

        # Claim call with explicit `↦` mappings: `Name(slot ↦ val, …)`
        if self.peek().kind == "IDENT" and self._lookahead_is_claim_call():
            return self.parse_claim_call()

        # Fall through: constraint expression.
        expr = self.parse_expr()
        return {"kind": "constraint", "expr": expr}

    def parse_subclaim(self):
        self.eat("KEYWORD", "subclaim")
        name = self.eat("IDENT").value
        type_params = self.parse_type_params()
        params = []
        if self.accept("SYMBOL", "("):
            params = self.parse_param_groups()
            self.eat("SYMBOL", ")")
        body = self.parse_optional_body()
        return {
            "kind": "subclaim",
            "decl": {
                "kind": "schema",
                "kw": "subclaim",
                "external": False,
                "name": name,
                "type_params": type_params,
                "params": params,
                "body": body,
            },
        }

    def _lookahead_is_tuple_in(self):
        """Is the upcoming `(...)` followed by `∈ ClaimName`?"""
        # Walk past the matched parens.
        k = self.pos
        if self.tokens[k].kind != "SYMBOL" or self.tokens[k].value != "(":
            return False
        depth = 1
        k += 1
        while k < len(self.tokens) and depth > 0:
            t = self.tokens[k]
            if t.kind == "SYMBOL" and t.value in "([{⟨":
                depth += 1
            elif t.kind == "SYMBOL" and t.value in ")]}⟩":
                depth -= 1
            k += 1
        if k >= len(self.tokens):
            return False
        # Now self.tokens[k] is just past the closing `)`. Check for `∈ IDENT`.
        return _is_in(self.tokens[k]) and self.tokens[k + 1].kind == "IDENT"

    def parse_tuple_in(self):
        self.eat("SYMBOL", "(")
        args = [self.parse_expr()]
        while self.accept("SYMBOL", ","):
            args.append(self.parse_expr())
        self.eat("SYMBOL", ")")
        if not _is_in(self.peek()):
            raise SyntaxError("expected ∈ after tuple in tuple-in form")
        self.eat()
        name = self.eat("IDENT").value
        return {"kind": "tuple_in", "args": args, "name": name}

    def _try_parse_membership_or_chained(self):
        """Try to parse a `[expr cmp]* IDENT (, IDENT)* ∈ type [cmp expr]*` line.

        Returns None if the line doesn't fit. We restore self.pos on
        failure (lightweight, since these statements live at the start of
        a body item — no side effects yet).
        """
        saved = self.pos

        # First scan: is there a top-level `∈` before a NEWLINE that's
        # preceded by a bare-IDENT (no leading `(` or operator)?
        # We use a simple shape match: scan tokens at depth 0 until NEWLINE
        # or DEDENT or EOF; if we find `∈` and the token immediately before
        # it is IDENT and the previous-previous (if exists) is `,` or one
        # of {<, ≤, >, ≥, =, ≠}, it qualifies.

        k = saved
        depth = 0
        in_pos = -1
        while k < len(self.tokens):
            t = self.tokens[k]
            if t.kind in ("NEWLINE", "DEDENT", "EOF"):
                break
            if t.kind == "SYMBOL" and t.value in "([{⟨":
                depth += 1
            elif t.kind == "SYMBOL" and t.value in ")]}⟩":
                depth -= 1
            elif depth == 0 and _is_in(t):
                # Must not be a quantifier — i.e., the previous non-symbol
                # tokens must be a bare ident, not following ∀/∃.
                in_pos = k
                break
            k += 1

        if in_pos < 0:
            return None

        # Check that ∈ is preceded by an IDENT chain (no leading operator).
        # Walk backwards collecting IDENT (, IDENT)*; if anything else
        # appears that's not a leading expr-cmp shape, it's not a membership.
        # Leading expr-cmp shape: literal/expr CMP IDENT (, IDENT)* ∈
        names = []
        j = in_pos - 1
        # Collect IDENT tokens separated by `,`.
        if j < saved or self.tokens[j].kind != "IDENT":
            return None
        names.append(self.tokens[j].value)
        j -= 1
        while j >= saved and self.tokens[j].kind == "SYMBOL" \
                and self.tokens[j].value == "," \
                and j - 1 >= saved and self.tokens[j - 1].kind == "IDENT":
            names.insert(0, self.tokens[j - 1].value)
            j -= 2
        # After collecting names, either j < saved (no prefix expr) or
        # tokens[j] should be a comparison op (chained-membership lower).
        first_name_pos = j + 1
        prefix_lows = []
        if j >= saved:
            # Expect: [expr CMP]+ as prefix — for now only one prefix cmp.
            if self.tokens[j].kind == "SYMBOL" and self.tokens[j].value in _CMP_OPS:
                # Has a prefix `expr cmp IDENT` (chained-membership lower).
                pass  # we'll re-parse from saved on the chained branch
            else:
                # Not a clean membership form.
                return None

        # OK — re-parse from the beginning to produce the AST.
        self.pos = saved

        lows = []   # [(expr, op)]   for `expr OP IDENT ∈`
        highs = []  # [(op, expr)]   for `IDENT ∈ T OP expr`
        # Collect any leading "expr cmp" prefixes.
        # We know the name positions, so peel them off in front.
        if first_name_pos > saved:
            # Parse one or more `expr CMP` prefixes by re-using parse_addition.
            while self.pos < first_name_pos:
                e = self.parse_addition()
                if not _is_cmp(self.peek()):
                    raise SyntaxError(
                        f"L{self.peek().line}:{self.peek().col} expected comparison op in chained membership")
                op = _canon_op(self.eat().value)
                lows.append((e, op))
        # Now consume the IDENT list.
        names_consumed = [self.eat("IDENT").value]
        while self.accept("SYMBOL", ","):
            names_consumed.append(self.eat("IDENT").value)
        # Consume ∈.
        if not _is_in(self.peek()):
            self.pos = saved
            return None
        self.eat()
        set_expr = self.parse_type_ref()

        # Optional pins clause on a plain (single-name) membership: `(...)`.
        pins = None
        if not lows and len(names_consumed) == 1 and self.at_sym("(") \
                and not self._line_has_trailing_cmp():
            pins = self.parse_pins_clause()

        # Optional trailing `cmp expr (cmp expr)*` (chained membership upper).
        while _is_cmp(self.peek()):
            op = _canon_op(self.eat().value)
            e = self.parse_addition()
            highs.append((op, e))

        if lows or highs:
            return {
                "kind": "chained_mem",
                "names": names_consumed,
                "set": set_expr,
                "lows": lows,
                "highs": highs,
            }
        return {
            "kind": "membership",
            "names": names_consumed,
            "set": set_expr,
            "pins": pins,
        }

    def _line_has_trailing_cmp(self):
        """Peek ahead past a matched `(...)`; if a CMP op follows on the
        same line, the parens are NOT a pins clause but a higher-precedence
        grouping inside the trailing-bound expr. Conservative: we say the
        line has a trailing cmp if any CMP token appears later before
        NEWLINE/DEDENT/EOF at depth 0."""
        k = self.pos
        depth = 0
        while k < len(self.tokens):
            t = self.tokens[k]
            if t.kind in ("NEWLINE", "DEDENT", "EOF"):
                break
            if t.kind == "SYMBOL" and t.value in "([{⟨":
                depth += 1
            elif t.kind == "SYMBOL" and t.value in ")]}⟩":
                depth -= 1
            elif depth == 0 and _is_cmp(t):
                # Don't count the `=` inside `slot ↦ val` style — those
                # are inside a `(` group at depth ≥ 1, already filtered.
                return True
            k += 1
        return False

    def parse_pins_clause(self):
        """Pins after `x ∈ T`: either `(slot ↦ val, …)` (Named) or
        `(v1, v2, …)` (Positional). Disambig: second token is `↦`."""
        self.eat("SYMBOL", "(")
        if self.at_sym(")"):
            self.eat()
            return {"kind": "pins_positional", "args": []}
        # Lookahead: if `IDENT ↦`, this is Named.
        if (self.peek().kind == "IDENT"
                and _is_mapsto(self.peek(1))):
            mappings = [self.parse_mapping()]
            while self.accept("SYMBOL", ","):
                mappings.append(self.parse_mapping())
            self.eat("SYMBOL", ")")
            return {"kind": "pins_named", "mappings": mappings}
        # Positional.
        args = [self.parse_expr()]
        while self.accept("SYMBOL", ","):
            args.append(self.parse_expr())
        self.eat("SYMBOL", ")")
        return {"kind": "pins_positional", "args": args}

    def parse_mapping(self):
        slot = self.eat("IDENT").value
        if not _is_mapsto(self.peek()):
            t = self.peek()
            raise SyntaxError(
                f"L{t.line}:{t.col} expected ↦ in slot mapping, got {t.value!r}")
        self.eat()
        value = self.parse_expr()
        return {"kind": "mapping", "slot": slot, "value": value}

    def _lookahead_is_claim_call(self):
        """Detect `IDENT[<…>] ( IDENT ↦ …`."""
        n = len(self.tokens)
        k = self.pos
        if k >= n or self.tokens[k].kind != "IDENT":
            return False
        k += 1
        if k < n and self.tokens[k].kind == "SYMBOL" and self.tokens[k].value == "<":
            depth = 1
            k += 1
            while k < n and depth > 0:
                t = self.tokens[k]
                if t.kind == "SYMBOL" and t.value == "<":
                    depth += 1
                elif t.kind == "SYMBOL" and t.value == ">":
                    depth -= 1
                k += 1
        if k >= n or self.tokens[k].kind != "SYMBOL" or self.tokens[k].value != "(":
            return False
        k += 1
        if k >= n or self.tokens[k].kind != "IDENT":
            return False
        if k + 1 >= n:
            return False
        return _is_mapsto(self.tokens[k + 1])

    def parse_claim_call(self):
        name = self.eat("IDENT").value
        generics = self.parse_type_params()
        self.eat("SYMBOL", "(")
        mappings = [self.parse_mapping()]
        while self.accept("SYMBOL", ","):
            mappings.append(self.parse_mapping())
        self.eat("SYMBOL", ")")
        return {
            "kind": "claim_call",
            "name": name,
            "generics": generics,
            "mappings": mappings,
        }

    # ─────────────── expressions (Appendix A grammar) ───────────────

    def parse_expr(self):
        return self.parse_quantifier_or_implies()

    def parse_quantifier_or_implies(self):
        t = self.peek()
        if t.kind == "SYMBOL" and t.value in ("∀", "∃"):
            return self.parse_quantifier()
        return self.parse_implies()

    def parse_quantifier(self):
        q_tok = self.eat()
        q = q_tok.value
        # binder: IDENT  OR  ( IDENT (, IDENT)+ )
        vars_ = []
        if self.accept("SYMBOL", "("):
            vars_.append(self.eat("IDENT").value)
            while self.accept("SYMBOL", ","):
                vars_.append(self.eat("IDENT").value)
            self.eat("SYMBOL", ")")
        else:
            vars_.append(self.eat("IDENT").value)
        if not _is_in(self.peek()):
            t = self.peek()
            raise SyntaxError(
                f"L{t.line}:{t.col} expected ∈ after quantifier binder")
        self.eat()
        rng = self.parse_postfix()
        self.eat("SYMBOL", ":")
        body = self.parse_block_or_expr()
        return {"kind": "quantifier", "q": q, "vars": vars_,
                "range": rng, "body": body}

    def parse_block_or_expr(self):
        """Either `NEWLINE INDENT (expr NEWLINE)+ DEDENT` (AND-joined) or
        a single inline expression. Body expressions may themselves be
        quantifiers, so we recurse through parse_expr (not parse_implies)."""
        if self.accept("NEWLINE"):
            self.eat("INDENT")
            parts = []
            while not self.at("DEDENT") and not self.at("EOF"):
                if self.accept("NEWLINE"):
                    continue
                parts.append(self.parse_expr())
                self.accept("NEWLINE")
            self.accept("DEDENT")
            if len(parts) == 1:
                return parts[0]
            result = parts[0]
            for p in parts[1:]:
                result = {"kind": "binop", "op": "∧", "l": result, "r": p}
            return result
        return self.parse_expr()

    def parse_implies(self):
        left = self.parse_ternary()
        if _is_implies(self.peek()):
            self.eat()
            # Block-form implies.
            if self.at("NEWLINE"):
                right = self.parse_block_or_expr()
            else:
                right = self.parse_implies()
            return {"kind": "binop", "op": "⇒", "l": left, "r": right}
        return left

    def parse_ternary(self):
        cond = self.parse_or()
        if self.accept("SYMBOL", "?"):
            t = self.parse_ternary()
            self.eat("SYMBOL", ":")
            f = self.parse_ternary()
            return {"kind": "ternary", "cond": cond, "t": t, "f": f}
        return cond

    def parse_or(self):
        left = self.parse_and()
        while self.at("SYMBOL", "∨"):
            self.eat()
            right = self.parse_and()
            left = {"kind": "binop", "op": "∨", "l": left, "r": right}
        return left

    def parse_and(self):
        left = self.parse_compare()
        while self.at("SYMBOL", "∧"):
            self.eat()
            right = self.parse_compare()
            left = {"kind": "binop", "op": "∧", "l": left, "r": right}
        return left

    def parse_compare(self):
        left = self.parse_addition()
        # `e matches Pattern`
        if self.at_kw("matches"):
            self.eat()
            pat = self.parse_pattern()
            return {"kind": "matches", "expr": left, "pattern": pat}
        # Set-membership comparisons.
        if _is_in(self.peek()):
            self.eat()
            right = self.parse_addition_or_set_rhs()
            return {"kind": "binop", "op": "∈", "l": left, "r": right}
        if self.at("SYMBOL", "∉"):
            self.eat()
            right = self.parse_addition_or_set_rhs()
            inner = {"kind": "binop", "op": "∈", "l": left, "r": right}
            return {"kind": "unop", "op": "¬", "x": inner}
        if self.at("SYMBOL", "∋"):
            self.eat()
            right = self.parse_addition()
            return {"kind": "binop", "op": "∈", "l": right, "r": left}
        # Chained `=`/`<`/`≤`/`>`/`≥`/`≠`: combine left-to-right with ∧.
        if _is_cmp(self.peek()):
            op = _canon_op(self.eat().value)
            right = self.parse_addition()
            result = {"kind": "binop", "op": op, "l": left, "r": right}
            cur_rhs = right
            while _is_cmp(self.peek()):
                op2 = _canon_op(self.eat().value)
                nxt = self.parse_addition()
                step = {"kind": "binop", "op": op2, "l": cur_rhs, "r": nxt}
                result = {"kind": "binop", "op": "∧", "l": result, "r": step}
                cur_rhs = nxt
            return result
        return left

    def parse_addition_or_set_rhs(self):
        """RHS of `∈` — may be a set literal `{…}`, a range `{lo..hi}`,
        or any expression that denotes a value (type, identifier).

        We accept full expression syntax so things like `t.contents`
        still work, but `{…}` triggers a set/range literal."""
        if self.at_sym("{"):
            return self.parse_set_or_range()
        return self.parse_addition()

    def parse_addition(self):
        left = self.parse_multiplication()
        while True:
            t = self.peek()
            if t.kind == "SYMBOL" and t.value in ("+", "-", "++"):
                op = self.eat().value
                right = self.parse_multiplication()
                left = {"kind": "binop", "op": op, "l": left, "r": right}
            else:
                break
        return left

    def parse_multiplication(self):
        left = self.parse_unary()
        while True:
            t = self.peek()
            if t.kind == "SYMBOL" and t.value in ("*", "/"):
                op = self.eat().value
                right = self.parse_unary()
                left = {"kind": "binop", "op": op, "l": left, "r": right}
            else:
                break
        return left

    def parse_unary(self):
        if self.at_sym("¬"):
            self.eat()
            return {"kind": "unop", "op": "¬", "x": self.parse_unary()}
        if self.at_sym("-"):
            self.eat()
            return {"kind": "unop", "op": "-", "x": self.parse_unary()}
        if self.at_sym("#"):
            self.eat()
            return {"kind": "cardinality", "x": self.parse_unary()}
        return self.parse_postfix()

    def parse_postfix(self):
        e = self.parse_atom()
        while True:
            if self.accept("SYMBOL", "["):
                idx = self.parse_expr()
                self.eat("SYMBOL", "]")
                e = {"kind": "index", "recv": e, "idx": idx}
                continue
            if self.at_sym("."):
                # Only treat `.` as field access if not part of `..` range.
                if self.peek(1).kind == "SYMBOL" and self.peek(1).value == ".":
                    break
                self.eat()
                name = self.eat("IDENT").value
                # Collapse bare-ident dotted chains into `qualified` for
                # the transpiler's `s__contents` lowering.
                if e["kind"] == "ident":
                    e = {"kind": "qualified", "parts": [e["name"], name]}
                elif e["kind"] == "qualified":
                    e = {"kind": "qualified", "parts": e["parts"] + [name]}
                else:
                    e = {"kind": "field", "recv": e, "name": name}
                continue
            break
        return e

    def parse_atom(self):
        t = self.peek()
        if t.kind == "INTEGER":
            self.eat()
            return {"kind": "int", "value": t.value}
        if t.kind == "REAL":
            self.eat()
            return {"kind": "real", "value": t.value}
        if t.kind == "STRING":
            self.eat()
            return {"kind": "str", "value": t.value}
        if t.kind == "KEYWORD" and t.value in ("true", "false"):
            self.eat()
            return {"kind": "bool", "value": t.value == "true"}
        if t.kind == "KEYWORD" and t.value == "match":
            return self.parse_match()
        if self.at_sym("("):
            return self.parse_paren_or_tuple()
        if self.at_sym("⟨"):
            return self.parse_seq_literal_unicode()
        if self.at_sym("["):
            # Legacy bootstrap [...] form for seq literals — keep for
            # transitional .ev files (prelude/, the old examples).
            return self.parse_seq_literal_bracket()
        if self.at_sym("{"):
            return self.parse_set_or_range()
        if t.kind == "IDENT":
            return self.parse_ident_call()
        raise SyntaxError(f"L{t.line}:{t.col} unexpected {t.kind} {t.value!r}")

    def parse_paren_or_tuple(self):
        self.eat("SYMBOL", "(")
        first = self.parse_expr()
        if self.accept("SYMBOL", ","):
            items = [first, self.parse_expr()]
            while self.accept("SYMBOL", ","):
                items.append(self.parse_expr())
            self.eat("SYMBOL", ")")
            return {"kind": "tuple", "items": items}
        self.eat("SYMBOL", ")")
        return first

    def parse_seq_literal_unicode(self):
        self.eat("SYMBOL", "⟨")
        items = []
        if not self.at_sym("⟩"):
            items.append(self.parse_expr())
            while self.accept("SYMBOL", ","):
                items.append(self.parse_expr())
        self.eat("SYMBOL", "⟩")
        return {"kind": "seq", "items": items}

    def parse_seq_literal_bracket(self):
        self.eat("SYMBOL", "[")
        items = []
        if not self.at_sym("]"):
            items.append(self.parse_expr())
            while self.accept("SYMBOL", ","):
                items.append(self.parse_expr())
        self.eat("SYMBOL", "]")
        return {"kind": "seq", "items": items}

    def parse_ident_call(self):
        name = self.eat("IDENT").value
        # Optional generics `<T, …>` only treated as generics if shape is
        # `<T (, T)*>` followed by `(`. Otherwise `<` is comparison.
        if self.at_sym("<") and self._lookahead_is_generic_args():
            generics = self.parse_type_params()
        else:
            generics = []
        if self.accept("SYMBOL", "("):
            args = []
            if not self.at_sym(")"):
                args.append(self.parse_expr())
                while self.accept("SYMBOL", ","):
                    args.append(self.parse_expr())
            self.eat("SYMBOL", ")")
            return {"kind": "call", "name": name,
                    "generics": generics, "args": args}
        if generics:
            # `Name<T>` without call args — record-typed reference.
            return {"kind": "ident", "name": name, "generics": generics}
        return {"kind": "ident", "name": name}

    def _lookahead_is_generic_args(self):
        """Conservative check: `<IDENT (, IDENT|generic)*>` followed by `(`."""
        k = self.pos
        if self.tokens[k].kind != "SYMBOL" or self.tokens[k].value != "<":
            return False
        depth = 1
        k += 1
        while k < len(self.tokens) and depth > 0:
            t = self.tokens[k]
            if t.kind == "SYMBOL" and t.value == "<":
                depth += 1
            elif t.kind == "SYMBOL" and t.value == ">":
                depth -= 1
            elif t.kind in ("NEWLINE", "DEDENT", "EOF"):
                return False
            k += 1
        # Expect `(` right after the closing `>`.
        return (k < len(self.tokens)
                and self.tokens[k].kind == "SYMBOL"
                and self.tokens[k].value == "(")

    # --- match ---

    def parse_match(self):
        self.eat("KEYWORD", "match")
        scrutinee = self.parse_or()  # spec: match takes an or_expr
        # Per spec §6 — NO colon after scrutinee. Allow optional `:` to
        # gracefully accept the legacy bootstrap shape and emit no error.
        self.accept("SYMBOL", ":")
        self.eat("NEWLINE")
        self.eat("INDENT")
        arms = []
        while not self.at("DEDENT") and not self.at("EOF"):
            if self.accept("NEWLINE"):
                continue
            pat = self.parse_pattern()
            if not _is_implies(self.peek()):
                t = self.peek()
                raise SyntaxError(
                    f"L{t.line}:{t.col} expected ⇒ in match arm, got {t.value!r}")
            self.eat()
            body = self.parse_or()
            arms.append({"kind": "arm", "pattern": pat, "body": body})
            self.accept("NEWLINE")
        self.accept("DEDENT")
        return {"kind": "match", "scrutinee": scrutinee, "arms": arms}

    def parse_pattern(self):
        t = self.peek()
        # Literal patterns (extension over the spec for richer matches).
        if t.kind == "INTEGER":
            self.eat()
            return {"kind": "pat_int", "value": t.value}
        if t.kind == "STRING":
            self.eat()
            return {"kind": "pat_str", "value": t.value}
        if t.kind == "KEYWORD" and t.value in ("true", "false"):
            self.eat()
            return {"kind": "pat_bool", "value": t.value == "true"}
        # Wildcard.
        if t.kind == "IDENT" and t.value == "_":
            self.eat()
            return {"kind": "pat_wildcard"}
        if t.kind != "IDENT":
            raise SyntaxError(
                f"L{t.line}:{t.col} expected pattern, got {t.kind} {t.value!r}")
        name = self.eat("IDENT").value
        # Capitalization rule: lowercase → Bind; uppercase → Ctor.
        is_ctor = name[0].isupper()
        args = []
        if self.accept("SYMBOL", "("):
            args.append(self.parse_pattern())
            while self.accept("SYMBOL", ","):
                args.append(self.parse_pattern())
            self.eat("SYMBOL", ")")
            is_ctor = True
        if is_ctor:
            return {"kind": "pat_ctor", "name": name, "args": args}
        return {"kind": "pat_bind", "name": name}


def parse(source):
    """Parse an Evident source string into an AST."""
    return Parser(lex(source)).parse_program()
