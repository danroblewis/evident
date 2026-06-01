"""Evident parser — lex + parse → AST.

Grammar (BNF) lives in docs/runtime-architecture.md. Highlights:
  - claim NAME(params) body
  - fsm   NAME(params) body
  - type  NAME = | Variant(fields) | ...
  - bindings:  name ∈ set_expr
  - assertions: any expr
  - match expr: pat => result; ...
  - Unicode operators: ∈ ∨ ∧ ¬ ≠ ≤ ≥
  - Significant indentation (Python-style NEWLINE/INDENT/DEDENT)
  - Line comments start with `;`

The AST node types are simple dicts with a 'kind' tag, intended to be
walked by transpile.py without further ceremony.
"""


# ──────────────────────────── lexer ────────────────────────────

KEYWORDS = {"claim", "fsm", "type", "match", "mod", "true", "false"}

# Multi-char symbols must come before single-char prefixes.
# `++` is the sequence-concatenation operator (M7); it appears before
# any single-char `+` so the lexer prefers the two-character form.
MULTI_SYMBOLS = ["++", "..", "=>", "≤", "≥", "≠"]
SINGLE_SYMBOLS = set("∈∨∧¬=+-*/()[]{},:|")


class Token:
    __slots__ = ("kind", "value", "line", "col")
    def __init__(self, kind, value, line, col):
        self.kind = kind
        self.value = value
        self.line = line
        self.col = col
    def __repr__(self):
        return f"Token({self.kind!r}, {self.value!r}, L{self.line}:{self.col})"


def lex(source):
    """Source text → list of Tokens. Handles indentation by emitting
    INDENT / DEDENT tokens at column changes after NEWLINEs."""
    tokens = []
    indent_stack = [0]
    i = 0
    line = 1
    col = 1
    line_start = True  # are we at the start of a logical line?
    paren_depth = 0    # inside ( [ { we suppress NEWLINE/INDENT/DEDENT

    def emit(kind, value):
        tokens.append(Token(kind, value, line, col))

    while i < len(source):
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

        # Comments — `;` to end of line.
        if c == ";":
            while i < len(source) and source[i] != "\n":
                i += 1
            continue

        # Leading whitespace on a new line → maybe INDENT / DEDENT.
        if line_start and paren_depth == 0:
            indent = 0
            while i < len(source) and source[i] == " ":
                indent += 1
                i += 1
                col += 1
            if i >= len(source) or source[i] == "\n" or source[i] == ";":
                # Blank or comment-only line; don't track indent.
                continue
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
            i += 1; col += 1
            chars = []
            while i < len(source) and source[i] != '"':
                if source[i] == "\\" and i + 1 < len(source):
                    esc = source[i+1]
                    chars.append({"n": "\n", "t": "\t", "r": "\r",
                                  '"': '"', "\\": "\\"}.get(esc, esc))
                    i += 2; col += 2
                else:
                    chars.append(source[i])
                    i += 1; col += 1
            if i >= len(source):
                raise SyntaxError(f"L{line}:{start} unterminated string")
            i += 1; col += 1
            tokens.append(Token("STRING", "".join(chars), line, start))
            continue

        # Integers.
        if c.isdigit():
            start = col
            j = i
            while j < len(source) and source[j].isdigit():
                j += 1
            tokens.append(Token("INTEGER", int(source[i:j]), line, start))
            col += j - i
            i = j
            continue

        # Identifiers + keywords.
        if c.isalpha() or c == "_":
            start = col
            j = i
            while j < len(source) and (source[j].isalnum() or source[j] == "_"):
                j += 1
            word = source[i:j]
            col += j - i
            i = j
            # Qualified-name lookahead. A `.` between two IDENT characters
            # (NOT `..`, which is the range operator) extends the token
            # into a qualified name `a.b.c`. The leading-underscore form
            # `_s.contents` is naturally handled: `_s` is read as an IDENT
            # above, and the `.contents` extension joins it. Emit a single
            # QUALIFIED token whose value is the parts list.
            parts = [word]
            while (i + 1 < len(source) and source[i] == "."
                   and source[i+1] != "."
                   and (source[i+1].isalpha() or source[i+1] == "_")):
                i += 1; col += 1  # consume `.`
                k = i
                while k < len(source) and (source[k].isalnum() or source[k] == "_"):
                    k += 1
                parts.append(source[i:k])
                col += k - i
                i = k
            if len(parts) == 1:
                kind = "KEYWORD" if word in KEYWORDS else "IDENT"
                tokens.append(Token(kind, word, line, start))
            else:
                tokens.append(Token("QUALIFIED", parts, line, start))
            continue

        # Multi-char symbols.
        matched = False
        for sym in MULTI_SYMBOLS:
            if source[i:i+len(sym)] == sym:
                tokens.append(Token("SYMBOL", sym, line, col))
                i += len(sym)
                col += len(sym)
                matched = True
                break
        if matched:
            continue

        # Single-char symbols (including Unicode ones).
        if c in SINGLE_SYMBOLS:
            if c in "([{": paren_depth += 1
            if c in ")]}": paren_depth -= 1
            tokens.append(Token("SYMBOL", c, line, col))
            i += 1
            col += 1
            continue

        raise SyntaxError(f"L{line}:{col} unexpected character {c!r}")

    # Flush remaining DEDENTs and an end marker.
    while len(indent_stack) > 1:
        indent_stack.pop()
        emit("DEDENT", 0)
    emit("EOF", None)
    return tokens


# ──────────────────────────── parser ────────────────────────────

class Parser:
    """Hand-written recursive-descent parser. The grammar is small enough
    that ceremony beyond this would be overkill."""

    def __init__(self, tokens):
        self.tokens = tokens
        self.pos = 0

    def peek(self, n=0):
        return self.tokens[self.pos + n]

    def eat(self, kind=None, value=None):
        t = self.tokens[self.pos]
        if kind and t.kind != kind:
            raise SyntaxError(
                f"L{t.line}:{t.col} expected {kind}, got {t.kind} ({t.value!r})")
        if value is not None and t.value != value:
            raise SyntaxError(
                f"L{t.line}:{t.col} expected {value!r}, got {t.value!r}")
        self.pos += 1
        return t

    def accept(self, kind=None, value=None):
        t = self.tokens[self.pos]
        if kind and t.kind != kind: return None
        if value is not None and t.value != value: return None
        self.pos += 1
        return t

    # --- program ---

    def parse_program(self):
        decls = []
        while self.peek().kind != "EOF":
            # Skip leading newlines between top-level declarations.
            if self.accept("NEWLINE"): continue
            decls.append(self.parse_toplevel())
        return {"kind": "program", "decls": decls}

    def parse_toplevel(self):
        t = self.peek()
        if t.kind == "KEYWORD" and t.value == "claim": return self.parse_claim()
        if t.kind == "KEYWORD" and t.value == "fsm":   return self.parse_fsm()
        if t.kind == "KEYWORD" and t.value == "type":  return self.parse_type()
        raise SyntaxError(f"L{t.line}:{t.col} expected top-level decl, got {t.value!r}")

    def parse_claim(self):
        self.eat("KEYWORD", "claim")
        name = self.eat("IDENT").value
        self.eat("SYMBOL", "(")
        params = self.parse_params()
        self.eat("SYMBOL", ")")
        body = self.parse_body()
        return {"kind": "claim", "name": name, "params": params, "body": body}

    def parse_fsm(self):
        self.eat("KEYWORD", "fsm")
        name = self.eat("IDENT").value
        self.eat("SYMBOL", "(")
        params = self.parse_params()
        self.eat("SYMBOL", ")")
        body = self.parse_body()
        return {"kind": "fsm", "name": name, "params": params, "body": body}

    def parse_type(self):
        self.eat("KEYWORD", "type")
        name = self.eat("IDENT").value
        self.eat("SYMBOL", "=")
        self.accept("NEWLINE")
        self.accept("INDENT")
        variants = []
        # First variant may have leading `|` or not.
        self.accept("SYMBOL", "|")
        variants.append(self.parse_variant())
        while self.accept("SYMBOL", "|"):
            variants.append(self.parse_variant())
        self.accept("NEWLINE")
        self.accept("DEDENT")
        return {"kind": "type", "name": name, "variants": variants}

    def parse_variant(self):
        name = self.eat("IDENT").value
        fields = []
        if self.accept("SYMBOL", "("):
            if self.peek().kind != "SYMBOL" or self.peek().value != ")":
                fields.append(self.parse_param())
                while self.accept("SYMBOL", ","):
                    fields.append(self.parse_param())
            self.eat("SYMBOL", ")")
        return {"kind": "variant", "name": name, "fields": fields}

    def parse_params(self):
        params = []
        if self.peek().kind == "SYMBOL" and self.peek().value == ")":
            return params
        params.append(self.parse_param())
        while self.accept("SYMBOL", ","):
            params.append(self.parse_param())
        return params

    def parse_param(self):
        name = self.eat("IDENT").value
        self.eat("SYMBOL", "∈")
        set_expr = self.parse_set_expr()
        return {"kind": "param", "name": name, "set": set_expr}

    def parse_body(self):
        self.eat("NEWLINE")
        self.eat("INDENT")
        stmts = []
        while self.peek().kind != "DEDENT" and self.peek().kind != "EOF":
            if self.accept("NEWLINE"): continue
            stmts.append(self.parse_stmt())
        self.accept("DEDENT")
        return stmts

    def parse_stmt(self):
        # Binding: IDENT ∈ set_expr  (must be followed by NEWLINE or DEDENT)
        if self.peek().kind == "IDENT" and \
           self.peek(1).kind == "SYMBOL" and self.peek(1).value == "∈":
            name = self.eat("IDENT").value
            self.eat("SYMBOL", "∈")
            set_expr = self.parse_set_expr()
            self.accept("NEWLINE")
            return {"kind": "binding", "name": name, "set": set_expr}
        # Assertion: any expression.
        e = self.parse_expr()
        self.accept("NEWLINE")
        return {"kind": "assertion", "expr": e}

    # --- expressions (precedence climbing) ---

    def parse_expr(self):    return self.parse_disjunction()

    def parse_disjunction(self):
        left = self.parse_conjunction()
        while self.accept("SYMBOL", "∨"):
            right = self.parse_conjunction()
            left = {"kind": "binop", "op": "∨", "l": left, "r": right}
        return left

    def parse_conjunction(self):
        left = self.parse_comparison()
        while self.accept("SYMBOL", "∧"):
            right = self.parse_comparison()
            left = {"kind": "binop", "op": "∧", "l": left, "r": right}
        return left

    def parse_comparison(self):
        left = self.parse_addition()
        for op in ("=", "≠", "<", "≤", ">", "≥"):
            if self.accept("SYMBOL", op):
                right = self.parse_addition()
                return {"kind": "binop", "op": op, "l": left, "r": right}
        return left

    def parse_addition(self):
        # `++` (seq concat) at the same precedence as `+`/`-`. Order in
        # the loop matters: try `++` before `+` so `a ++ b` doesn't read
        # as `a + (+ b)`.
        left = self.parse_multiplication()
        while True:
            for op in ("++", "+", "-"):
                if self.accept("SYMBOL", op):
                    right = self.parse_multiplication()
                    left = {"kind": "binop", "op": op, "l": left, "r": right}
                    break
            else:
                break
        return left

    def parse_multiplication(self):
        left = self.parse_unary()
        while True:
            tok = self.peek()
            if tok.kind == "SYMBOL" and tok.value in ("*", "/"):
                op = tok.value; self.pos += 1
            elif tok.kind == "KEYWORD" and tok.value == "mod":
                op = "mod"; self.pos += 1
            else:
                break
            right = self.parse_unary()
            left = {"kind": "binop", "op": op, "l": left, "r": right}
        return left

    def parse_unary(self):
        if self.accept("SYMBOL", "-"):
            return {"kind": "unop", "op": "-", "x": self.parse_unary()}
        if self.accept("SYMBOL", "¬"):
            return {"kind": "unop", "op": "¬", "x": self.parse_unary()}
        return self.parse_primary()

    def parse_primary(self):
        t = self.peek()
        if t.kind == "INTEGER":
            self.pos += 1
            return {"kind": "int", "value": t.value}
        if t.kind == "STRING":
            self.pos += 1
            return {"kind": "str", "value": t.value}
        if t.kind == "KEYWORD" and t.value in ("true", "false"):
            self.pos += 1
            return {"kind": "bool", "value": t.value == "true"}
        if t.kind == "KEYWORD" and t.value == "match":
            return self.parse_match()
        if t.kind == "SYMBOL" and t.value == "(":
            self.pos += 1
            e = self.parse_expr()
            self.eat("SYMBOL", ")")
            return e
        if t.kind == "SYMBOL" and t.value == "[":
            return self.parse_seq_literal()
        if t.kind == "IDENT":
            return self.parse_ident_or_call()
        if t.kind == "QUALIFIED":
            # A dotted name like `s.contents` or `_s.contents`. It is a
            # value expression denoting a namespaced variable; the
            # transpiler flattens it to `s__contents` / `_s__contents`.
            # Qualified names are not callable in v1 (no `s.foo(args)`).
            self.pos += 1
            return {"kind": "qualified", "parts": list(t.value)}
        raise SyntaxError(f"L{t.line}:{t.col} unexpected {t.kind} {t.value!r}")

    def parse_ident_or_call(self):
        name = self.eat("IDENT").value
        if self.accept("SYMBOL", "("):
            args = []
            if not (self.peek().kind == "SYMBOL" and self.peek().value == ")"):
                args.append(self.parse_expr())
                while self.accept("SYMBOL", ","):
                    args.append(self.parse_expr())
            self.eat("SYMBOL", ")")
            return {"kind": "call", "name": name, "args": args}
        return {"kind": "ident", "name": name}

    def parse_seq_literal(self):
        self.eat("SYMBOL", "[")
        items = []
        if not (self.peek().kind == "SYMBOL" and self.peek().value == "]"):
            items.append(self.parse_expr())
            while self.accept("SYMBOL", ","):
                items.append(self.parse_expr())
        self.eat("SYMBOL", "]")
        return {"kind": "seq", "items": items}

    def parse_match(self):
        self.eat("KEYWORD", "match")
        scrutinee = self.parse_expr()
        self.eat("SYMBOL", ":")
        self.accept("NEWLINE")
        self.accept("INDENT")
        arms = []
        while self.peek().kind != "DEDENT" and self.peek().kind != "EOF":
            if self.accept("NEWLINE"): continue
            pat = self.parse_pattern()
            self.eat("SYMBOL", "=>")
            body = self.parse_expr()
            arms.append({"kind": "arm", "pattern": pat, "body": body})
            self.accept("NEWLINE")
        self.accept("DEDENT")
        return {"kind": "match", "scrutinee": scrutinee, "arms": arms}

    def parse_pattern(self):
        t = self.peek()
        if t.kind == "INTEGER":
            self.pos += 1
            return {"kind": "pat_int", "value": t.value}
        if t.kind == "STRING":
            self.pos += 1
            return {"kind": "pat_str", "value": t.value}
        if t.kind == "KEYWORD" and t.value in ("true", "false"):
            self.pos += 1
            return {"kind": "pat_bool", "value": t.value == "true"}
        if t.kind == "IDENT" and t.value == "_":
            self.pos += 1
            return {"kind": "pat_wildcard"}
        if t.kind == "IDENT":
            name = self.eat("IDENT").value
            if self.accept("SYMBOL", "("):
                args = [self.parse_pattern()]
                while self.accept("SYMBOL", ","):
                    args.append(self.parse_pattern())
                self.eat("SYMBOL", ")")
                return {"kind": "pat_ctor", "name": name, "args": args}
            return {"kind": "pat_bind", "name": name}
        raise SyntaxError(f"L{t.line}:{t.col} expected pattern, got {t.value!r}")

    # --- set expressions ---

    def parse_set_expr(self):
        # `{lo..hi}` or `{a, b, c}` enumeration
        if self.accept("SYMBOL", "{"):
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
        # `IDENT` or `IDENT(set_expr)` for generics
        name = self.eat("IDENT").value
        if self.accept("SYMBOL", "("):
            inner = self.parse_set_expr()
            self.eat("SYMBOL", ")")
            return {"kind": "set_named", "name": name, "param": inner}
        return {"kind": "set_named", "name": name, "param": None}


def parse(source):
    """Parse an Evident source string into an AST."""
    return Parser(lex(source)).parse_program()
