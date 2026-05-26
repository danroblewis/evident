#include "parser.h"

namespace evc {

namespace {
const char *kind_name(Token::Kind k) {
    using K = Token::Kind;
    switch (k) {
        case K::Ident: return "Ident"; case K::Int: return "Int"; case K::Real: return "Real";
        case K::Str: return "Str"; case K::True: return "true"; case K::False: return "false";
        case K::Schema: return "schema"; case K::Claim: return "claim"; case K::Type: return "type";
        case K::Subclaim: return "subclaim"; case K::Fsm: return "fsm"; case K::External: return "external";
        case K::Enum: return "enum"; case K::Match: return "match"; case K::Matches: return "matches";
        case K::Import: return "import"; case K::In: return "∈"; case K::NotIn: return "∉";
        case K::ContainsRev: return "∋"; case K::Eq: return "="; case K::Neq: return "≠";
        case K::Lt: return "<"; case K::Le: return "≤"; case K::Gt: return ">"; case K::Ge: return "≥";
        case K::Plus: return "+"; case K::PlusPlus: return "++"; case K::Minus: return "-";
        case K::Star: return "*"; case K::Slash: return "/"; case K::And: return "∧"; case K::Or: return "∨";
        case K::Not: return "¬"; case K::Implies: return "⇒"; case K::LParen: return "("; case K::RParen: return ")";
        case K::LBrace: return "{"; case K::RBrace: return "}"; case K::LBracket: return "["; case K::RBracket: return "]";
        case K::LSeq: return "⟨"; case K::RSeq: return "⟩"; case K::Hash: return "#"; case K::Comma: return ",";
        case K::Pipe: return "|"; case K::Question: return "?"; case K::DotDot: return ".."; case K::Dot: return ".";
        case K::Colon: return ":"; case K::ForAll: return "∀"; case K::Exists: return "∃"; case K::MapsTo: return "↦";
        case K::Newline: return "<newline>"; case K::Indent: return "<indent>"; case K::Eof: return "<eof>";
    }
    return "?";
}

std::optional<BinOp> peek_compare_op(Token::Kind k) {
    using K = Token::Kind;
    switch (k) {
        case K::Eq:  return BinOp::Eq;
        case K::Neq: return BinOp::Neq;
        case K::Lt:  return BinOp::Lt;
        case K::Le:  return BinOp::Le;
        case K::Gt:  return BinOp::Gt;
        case K::Ge:  return BinOp::Ge;
        default: return std::nullopt;
    }
}
}  // namespace

void Parser::eat(K k) {
    if (kind() == k) { bump(); return; }
    throw ParseError(std::string("expected ") + kind_name(k) + ", got " + kind_name(kind()));
}

void Parser::skip_blank_newlines() {
    while (is(K::Newline)) bump();
}

// ---------------------------------------------------------------------------
// program / enum
// ---------------------------------------------------------------------------
Program Parser::parse_program() {
    Program prog;
    if (is(K::Indent)) bump();
    for (;;) {
        skip_blank_newlines();
        while (is(K::Indent)) bump();
        switch (kind()) {
            case K::Eof: return prog;
            case K::Schema: case K::Claim: case K::Type: case K::Fsm: case K::External:
                prog.schemas.push_back(parse_schema_decl());
                break;
            case K::Import: {
                bump();
                Token t = bump();
                if (t.kind != K::Str)
                    throw ParseError("expected string literal after 'import', got " + std::string(kind_name(t.kind)));
                prog.imports.push_back(t.str);
                break;
            }
            case K::Enum:
                prog.enums.push_back(parse_enum_decl());
                break;
            default:
                throw ParseError(std::string("expected schema/claim/type/import/enum, got ") + kind_name(kind()));
        }
    }
}

EnumDecl Parser::parse_enum_decl() {
    bump();  // enum
    Token nm = bump();
    if (nm.kind != K::Ident) throw ParseError("expected enum name");
    EnumDecl decl;
    decl.name = nm.str;
    if (bump().kind != K::Eq) throw ParseError("expected '=' after enum name");

    std::optional<size_t> block_indent;
    if (is(K::Newline)) {
        size_t saved = pos_;
        bump();
        while (is(K::Newline)) bump();
        if (is(K::Indent) && peek().indent > 0) {
            block_indent = peek().indent;
            bump();
            if (is(K::Pipe)) bump();
        } else {
            pos_ = saved;
        }
    }
    for (;;) {
        Token vn = bump();
        if (vn.kind != K::Ident) throw ParseError("expected variant name in enum");
        EnumVariant var;
        var.name = vn.str;
        if (is(K::LParen)) {
            bump();  // (
            if (is(K::RParen))
                throw ParseError("variant `" + var.name + "` has empty payload — drop the parens for nullary");
            size_t idx = 0;
            for (;;) {
                std::string ft = parse_enum_field_type(var.name);
                var.fields.push_back(EnumField{"f" + std::to_string(idx), ft});
                idx++;
                if (is(K::Comma)) { bump(); continue; }
                break;
            }
            if (bump().kind != K::RParen) throw ParseError("expected ')' after variant payload");
        }
        decl.variants.push_back(std::move(var));
        if (is(K::Pipe)) { bump(); continue; }
        if (block_indent) {
            if (is(K::Newline)) {
                size_t cont_save = pos_;
                bump();
                while (is(K::Newline)) bump();
                if (is(K::Indent) && peek().indent == *block_indent) {
                    bool looks_like_variant = (kind(1) == K::Ident || kind(1) == K::Pipe);
                    if (looks_like_variant) {
                        bump();  // indent
                        if (is(K::Pipe)) bump();
                        continue;
                    }
                }
                pos_ = cont_save;
            }
        }
        break;
    }
    if (decl.variants.empty()) throw ParseError("enum must have at least one variant");
    return decl;
}

std::string Parser::parse_enum_field_type(const std::string &v_name) {
    Token h = bump();
    if (h.kind != K::Ident) throw ParseError("expected field type in variant `" + v_name + "`");
    std::string head = h.str;
    if (is(K::LParen)) {
        bump();  // (
        std::string inner = parse_enum_field_type(v_name);
        if (bump().kind != K::RParen)
            throw ParseError("expected ')' after compound type in variant `" + v_name + "`");
        return head + "(" + inner + ")";
    }
    return head;
}

// ---------------------------------------------------------------------------
// schema
// ---------------------------------------------------------------------------
SchemaDecl Parser::parse_schema_decl() {
    bool external = false;
    if (is(K::External)) { bump(); external = true; }
    Keyword kw;
    switch (kind()) {
        case K::Schema:
            if (external) throw ParseError("`external schema` is not allowed — use `external type`");
            kw = Keyword::Schema; break;
        case K::Claim: kw = Keyword::Claim; break;
        case K::Type:  kw = Keyword::Type; break;
        case K::Fsm:   kw = Keyword::Fsm; break;
        default: throw ParseError(std::string("expected keyword, got ") + kind_name(kind()));
    }
    bump();
    Token nm = bump();
    if (nm.kind != K::Ident) throw ParseError("expected schema name");

    std::vector<std::string> type_params;
    if (is(K::Lt)) {
        bump();  // <
        for (;;) {
            Token p = bump();
            if (p.kind != K::Ident) throw ParseError("expected type parameter name");
            type_params.push_back(p.str);
            if (is(K::Comma)) { bump(); continue; }
            if (is(K::Gt)) { bump(); break; }
            throw ParseError("expected `,` or `>` in type parameters");
        }
    }

    std::vector<BodyItem> body;
    if (is(K::LParen)) body = parse_first_line_params();
    size_t param_count = body.size();
    auto rest = parse_indented_body();
    for (auto &b : rest) body.push_back(std::move(b));

    SchemaDecl d;
    d.keyword = kw; d.name = nm.str; d.type_params = std::move(type_params);
    d.body = std::move(body); d.param_count = param_count; d.external = external;
    return d;
}

std::vector<BodyItem> Parser::parse_first_line_params() {
    eat(K::LParen);
    std::vector<BodyItem> items;
    if (is(K::RParen)) { bump(); return items; }
    for (;;) {
        std::vector<std::string> names;
        for (;;) {
            Token t = bump();
            if (t.kind != K::Ident) throw ParseError("expected param name");
            names.push_back(t.str);
            if (is(K::Comma)) { bump(); continue; }
            if (is(K::In)) { bump(); break; }
            throw ParseError("expected ',' or '∈' after param name");
        }
        Token h = bump();
        if (h.kind != K::Ident) throw ParseError("expected type name in first-line params");
        std::string head = h.str;
        std::string type_name;
        bool compound_head = (head == "Seq" || head == "Set" || head == "Bag" || head == "Map");
        if (compound_head && is(K::LParen)) {
            bump();  // (
            Token ih = bump();
            if (ih.kind != K::Ident) throw ParseError("expected inner type for " + head);
            std::string inner = ih.str;
            if (auto args = try_parse_generic_args_suffix()) inner += *args;
            eat(K::RParen);
            type_name = head + "(" + inner + ")";
        } else if (is(K::Lt)) {
            auto args = try_parse_generic_args_suffix();
            type_name = head + (args ? *args : "");
        } else {
            type_name = head;
        }
        for (auto &nm : names) {
            BodyItem m(BodyItem::Kind::Membership);
            m.name = nm; m.type_name = type_name;
            items.push_back(std::move(m));
        }
        if (is(K::Comma)) { bump(); continue; }
        if (is(K::RParen)) { bump(); break; }
        throw ParseError("expected ',' or ')' after param group");
    }
    return items;
}

std::vector<BodyItem> Parser::parse_indented_body() {
    skip_blank_newlines();
    if (!(is(K::Indent) && peek().indent > 0)) return {};
    size_t body_indent = peek().indent;
    std::vector<BodyItem> body;
    for (;;) {
        if (is(K::Indent) && peek().indent == body_indent) bump();
        else break;
        auto items = parse_body_item();
        for (auto &b : items) body.push_back(std::move(b));
        if (is(K::Newline)) bump();
        else if (is(K::Eof)) break;
    }
    return body;
}

BodyItem Parser::parse_subclaim() {
    bump();  // subclaim
    Token nm = bump();
    if (nm.kind != K::Ident) throw ParseError("expected name after subclaim");
    std::vector<BodyItem> body;
    if (is(K::LParen)) body = parse_first_line_params();
    size_t param_count = body.size();
    auto rest = parse_indented_body();
    for (auto &b : rest) body.push_back(std::move(b));

    auto sub = std::make_shared<SchemaDecl>();
    sub->keyword = Keyword::Subclaim; sub->name = nm.str;
    sub->body = std::move(body); sub->param_count = param_count;
    BodyItem item(BodyItem::Kind::Subclaim);
    item.subclaim = sub;
    return item;
}

// ---------------------------------------------------------------------------
// body items
// ---------------------------------------------------------------------------
std::optional<std::vector<BodyItem>> Parser::try_parse_chained_membership() {
    size_t saved = pos_;
    ExprPtr first;
    try { first = parse_addsub(); }
    catch (const ParseError &) { pos_ = saved; return std::nullopt; }

    std::vector<ExprPtr> operands{first};
    std::vector<BinOp> ops;
    struct Memb { size_t idx; std::string type_name; std::vector<std::string> names; };
    std::optional<Memb> membership;

    for (;;) {
        std::vector<std::string> extra_names;
        if (is(K::Comma)) {
            bool last_is_bare = operands.back()->kind == Expr::Kind::Ident &&
                                operands.back()->str.find('.') == std::string::npos;
            if (last_is_bare) {
                size_t mn_save = pos_;
                std::vector<std::string> names;
                while (is(K::Comma)) {
                    size_t inner_save = pos_;
                    bump();  // ,
                    if (is(K::Ident)) {
                        Token nx = peek();
                        K after = kind(1);
                        if (after == K::Comma || after == K::In) {
                            bump();
                            names.push_back(nx.str);
                            continue;
                        }
                    }
                    pos_ = inner_save;
                    break;
                }
                if (is(K::In) && !names.empty()) extra_names = names;
                else pos_ = mn_save;
            }
        }

        if (is(K::In)) {
            if (membership) { pos_ = saved; return std::nullopt; }
            bump();
            if (!is(K::Ident)) { pos_ = saved; return std::nullopt; }
            std::string head = peek().str;
            auto after_chain_class = [&](K k) {
                return k == K::Newline || k == K::Eof || k == K::Indent ||
                       peek_compare_op(k).has_value();
            };
            bool is_compound = (kind(1) == K::LParen) && (kind(2) == K::Ident) && (kind(3) == K::RParen);
            std::string type_name;
            if (is_compound) {
                bump();  // head
                bump();  // (
                std::string inner = bump().str;
                bump();  // )
                if (!after_chain_class(kind())) { pos_ = saved; return std::nullopt; }
                type_name = head + "(" + inner + ")";
            } else {
                if (!after_chain_class(kind(1))) { pos_ = saved; return std::nullopt; }
                bump();  // head
                type_name = head;
            }
            size_t var_idx = operands.size() - 1;
            if (!(operands[var_idx]->kind == Expr::Kind::Ident &&
                  operands[var_idx]->str.find('.') == std::string::npos)) {
                pos_ = saved; return std::nullopt;
            }
            std::vector<std::string> all_names{operands[var_idx]->str};
            for (auto &e : extra_names) all_names.push_back(e);
            membership = Memb{var_idx, type_name, all_names};
            continue;
        }
        if (auto op = peek_compare_op(kind())) {
            bump();
            ExprPtr rhs;
            try { rhs = parse_addsub(); }
            catch (const ParseError &) { pos_ = saved; return std::nullopt; }
            operands.push_back(rhs);
            ops.push_back(*op);
            continue;
        }
        break;
    }

    if (!membership) { pos_ = saved; return std::nullopt; }
    if (!(is(K::Newline) || is(K::Eof) || is(K::Indent))) { pos_ = saved; return std::nullopt; }

    std::vector<BodyItem> items;
    for (auto &nm : membership->names) {
        BodyItem m(BodyItem::Kind::Membership);
        m.name = nm; m.type_name = membership->type_name;
        items.push_back(std::move(m));
    }
    for (auto &nm : membership->names) {
        ExprPtr var = mkIdent(nm);
        for (size_t k = 0; k < ops.size(); k++) {
            ExprPtr lhs = (k == membership->idx) ? var : operands[k];
            ExprPtr rhs = (k + 1 == membership->idx) ? var : operands[k + 1];
            BodyItem c(BodyItem::Kind::Constraint);
            c.expr = mkBinary(ops[k], lhs, rhs);
            items.push_back(std::move(c));
        }
    }
    return items;
}

std::vector<BodyItem> Parser::parse_body_item() {
    // halts_within(F, N)
    if (is(K::Ident) && peek().str == "halts_within" && kind(1) == K::LParen) {
        size_t saved = pos_;
        bump();  // halts_within
        bump();  // (
        if (kind(0) == K::Ident && kind(1) == K::Comma && kind(2) == K::Int && kind(3) == K::RParen) {
            std::string fsm = peek(0).str;
            int64_t nval = peek(2).ival;
            pos_ += 4;
            if (nval < 0) throw ParseError("halts_within: N must be non-negative");
            BodyItem b(BodyItem::Kind::HaltsWithin);
            b.fsm_name = fsm; b.n = nval;
            return {std::move(b)};
        }
        pos_ = saved;
    }

    if (is(K::DotDot)) {
        bump();
        Token t = bump();
        if (t.kind != K::Ident) throw ParseError("expected claim name after '..'");
        BodyItem b(BodyItem::Kind::Passthrough);
        b.name = t.str;
        return {std::move(b)};
    }

    if (is(K::Subclaim)) return {parse_subclaim()};

    // ClaimCall: IDENT(slot ↦ value, …)   (also IDENT<...>(slot ↦ …))
    if (is(K::Ident)) {
        std::optional<size_t> lparen_offset;
        if (kind(1) == K::LParen) {
            lparen_offset = 1;
        } else if (kind(1) == K::Lt) {
            int depth = 0;
            size_t idx = pos_ + 1;
            for (;;) {
                if (idx >= toks_.size()) break;
                K k = toks_[idx].kind;
                if (k == K::Lt) depth++;
                else if (k == K::Gt) { depth--; if (depth == 0) { lparen_offset = idx - pos_ + 1; break; } }
                else if (k == K::Eof) break;
                idx++;
            }
        }
        if (lparen_offset && kind(*lparen_offset) == K::LParen) {
            bool is_claim_call = (kind(*lparen_offset + 1) == K::Ident) &&
                                 (kind(*lparen_offset + 2) == K::MapsTo);
            if (is_claim_call) {
                std::string name = bump().str;
                if (is(K::Lt)) { if (auto a = try_parse_generic_args_suffix()) name += *a; }
                eat(K::LParen);
                std::vector<Mapping> mappings;
                if (!is(K::RParen)) {
                    for (;;) {
                        Token slot = bump();
                        if (slot.kind != K::Ident) throw ParseError("expected mapping slot name");
                        eat(K::MapsTo);
                        ExprPtr value = parse_expr();
                        mappings.push_back(Mapping{slot.str, value});
                        if (is(K::Comma)) { bump(); continue; }
                        break;
                    }
                }
                eat(K::RParen);
                BodyItem b(BodyItem::Kind::ClaimCall);
                b.name = name; b.mappings = std::move(mappings);
                return {std::move(b)};
            }
        }
    }

    if (auto items = try_parse_chained_membership()) return std::move(*items);

    // membership: name[, name…] ∈ Type [pins]
    if (is(K::Ident)) {
        size_t saved = pos_;
        std::vector<std::string> lhs_names{bump().str};
        while (is(K::Comma)) {
            size_t inner_save = pos_;
            bump();  // ,
            if (is(K::Ident)) {
                Token nx = peek();
                K after = kind(1);
                if (after == K::Comma || after == K::In) { bump(); lhs_names.push_back(nx.str); continue; }
            }
            pos_ = inner_save;
            break;
        }
        if (is(K::In)) {
            bump();
            if (is(K::Ident)) {
                std::string head = peek().str;
                if (auto tp = try_parse_type_and_pins(head)) {
                    std::vector<BodyItem> items;
                    for (auto &nm : lhs_names) {
                        BodyItem m(BodyItem::Kind::Membership);
                        m.name = nm; m.type_name = tp->first; m.pins = tp->second;
                        items.push_back(std::move(m));
                    }
                    return items;
                }
            }
            pos_ = saved;
        } else {
            pos_ = saved;
        }
    }

    ExprPtr e = parse_expr();
    BodyItem c(BodyItem::Kind::Constraint);
    c.expr = e;
    return {std::move(c)};
}

// ---------------------------------------------------------------------------
// types / pins
// ---------------------------------------------------------------------------
std::optional<std::string> Parser::try_parse_generic_args_suffix() {
    if (!is(K::Lt)) return std::nullopt;
    bump();  // <
    std::string out = "<";
    bool first = true;
    for (;;) {
        if (!first) out += ", ";
        first = false;
        Token nm = bump();
        if (nm.kind != K::Ident) throw ParseError("expected type argument name");
        out += nm.str;
        if (auto inner = try_parse_generic_args_suffix()) out += *inner;
        if (is(K::Comma)) { bump(); }
        else if (is(K::Gt)) { bump(); break; }
        else throw ParseError("expected `,` or `>` in type arguments");
    }
    out += ">";
    return out;
}

std::optional<std::pair<std::string, Pins>> Parser::try_parse_type_and_pins(const std::string &head) {
    if (kind(1) == K::Lt) {
        bump();  // head
        auto args = try_parse_generic_args_suffix();
        std::string composite = head + (args ? *args : "");
        return std::make_pair(composite, Pins{});
    }
    K after_head = kind(1);
    bool plain_terminated = (after_head == K::Newline || after_head == K::Eof || after_head == K::Indent);
    bool has_paren = (after_head == K::LParen);
    if (plain_terminated) {
        bump();
        return std::make_pair(head, Pins{});
    }
    if (has_paren) {
        K inside_first = kind(2);
        K inside_second = kind(3);
        bool is_named_pin = (inside_first == K::Ident) && (inside_second == K::MapsTo);
        bool looks_like_compound = (inside_first == K::Ident) &&
                                   (inside_second == K::RParen || inside_second == K::Lt);
        bool is_known_compound_head = (head == "Seq" || head == "Set" || head == "Bag" || head == "Map");

        if (is_named_pin) {
            bump();  // type ident
            bump();  // (
            Pins pins; pins.kind = Pins::Kind::Named;
            for (;;) {
                Token slot = bump();
                if (slot.kind != K::Ident) throw ParseError("expected pin slot name");
                eat(K::MapsTo);
                ExprPtr value = parse_expr();
                pins.named.push_back(Mapping{slot.str, value});
                if (is(K::Comma)) { bump(); continue; }
                break;
            }
            eat(K::RParen);
            return std::make_pair(head, std::move(pins));
        } else if (is_known_compound_head && looks_like_compound) {
            bump();  // outer ident
            bump();  // (
            std::string inner_head = bump().str;
            std::string inner = inner_head;
            if (auto args = try_parse_generic_args_suffix()) inner = inner_head + *args;
            bump();  // )
            K after = kind();
            bool line_end = (after == K::Newline || after == K::Eof || after == K::Indent);
            if (line_end) return std::make_pair(head + "(" + inner + ")", Pins{});
            return std::nullopt;
        } else {
            bump();  // type ident
            bump();  // (
            Pins pins; pins.kind = Pins::Kind::Positional;
            if (!is(K::RParen)) {
                for (;;) {
                    pins.positional.push_back(parse_expr());
                    if (is(K::Comma)) { bump(); continue; }
                    break;
                }
            }
            eat(K::RParen);
            return std::make_pair(head, std::move(pins));
        }
    }
    return std::nullopt;
}

// ---------------------------------------------------------------------------
// expressions
// ---------------------------------------------------------------------------
ExprPtr Parser::parse_expr() {
    if (is(K::ForAll) || is(K::Exists)) return parse_quantifier();
    return parse_implies();
}

ExprPtr Parser::parse_quantifier() {
    bool is_forall = is(K::ForAll);
    bump();
    std::vector<std::string> vars;
    if (is(K::LParen)) {
        bump();  // (
        for (;;) {
            Token t = bump();
            if (t.kind != K::Ident) throw ParseError("expected bound variable name in tuple binding");
            vars.push_back(t.str);
            if (is(K::Comma)) { bump(); continue; }
            break;
        }
        eat(K::RParen);
        if (vars.size() < 2) throw ParseError("tuple binding `(…)` must contain ≥ 2 names");
    } else {
        Token t = bump();
        if (t.kind != K::Ident) throw ParseError("expected bound variable name");
        vars.push_back(t.str);
    }
    eat(K::In);
    ExprPtr range = parse_postfix();
    eat(K::Colon);

    auto make = [&](ExprPtr body) {
        auto e = mk(is_forall ? Expr::Kind::Forall : Expr::Kind::Exists);
        e->names = vars;
        e->children.push_back(range);
        e->children.push_back(body);
        return e;
    };

    // Block form: `∀ var ∈ range :\n    body…`
    if (is(K::Newline)) {
        size_t saved = pos_;
        bump();
        while (is(K::Newline)) bump();
        if (is(K::Indent)) {
            size_t block_indent = peek().indent;
            std::vector<ExprPtr> conjuncts;
            for (;;) {
                if (is(K::Indent) && peek().indent == block_indent) bump();
                else break;
                conjuncts.push_back(parse_implies());
                if (is(K::Newline)) bump();
                else if (is(K::Eof)) break;
            }
            if (conjuncts.empty()) {
                pos_ = saved;
            } else {
                ExprPtr body = conjuncts[0];
                for (size_t k = 1; k < conjuncts.size(); k++)
                    body = mkBinary(BinOp::And, body, conjuncts[k]);
                return make(body);
            }
        } else {
            pos_ = saved;
        }
    }
    ExprPtr body = parse_expr();
    return make(body);
}

ExprPtr Parser::parse_implies() {
    if (is(K::ForAll) || is(K::Exists)) return parse_quantifier();
    ExprPtr lhs = parse_ternary();
    if (is(K::Implies)) {
        bump();
        // Block form: `A ⇒\n    body…`
        if (is(K::Newline)) {
            size_t saved = pos_;
            bump();
            while (is(K::Newline)) bump();
            if (is(K::Indent)) {
                size_t block_indent = peek().indent;
                std::vector<ExprPtr> conjuncts;
                for (;;) {
                    if (is(K::Indent) && peek().indent == block_indent) bump();
                    else break;
                    conjuncts.push_back(parse_implies());
                    if (is(K::Newline)) bump();
                    else if (is(K::Eof)) break;
                }
                if (conjuncts.empty()) {
                    pos_ = saved;
                } else {
                    ExprPtr acc = conjuncts[0];
                    for (size_t k = 1; k < conjuncts.size(); k++)
                        acc = mkBinary(BinOp::And, acc, conjuncts[k]);
                    return mkBinary(BinOp::Implies, lhs, acc);
                }
            } else {
                pos_ = saved;
            }
        }
        ExprPtr rhs = parse_implies();
        return mkBinary(BinOp::Implies, lhs, rhs);
    }
    return lhs;
}

ExprPtr Parser::parse_ternary() {
    ExprPtr cond = parse_or();
    if (!is(K::Question)) return cond;
    bump();  // ?
    ExprPtr then_b = parse_ternary();
    if (bump().kind != K::Colon) throw ParseError("expected `:` after ternary then-branch");
    ExprPtr else_b = parse_ternary();
    auto e = mk(Expr::Kind::Ternary);
    e->children = {cond, then_b, else_b};
    return e;
}

ExprPtr Parser::parse_or() {
    ExprPtr lhs = parse_and();
    while (is(K::Or)) { bump(); lhs = mkBinary(BinOp::Or, lhs, parse_and()); }
    return lhs;
}

ExprPtr Parser::parse_and() {
    ExprPtr lhs = parse_compare();
    while (is(K::And)) { bump(); lhs = mkBinary(BinOp::And, lhs, parse_compare()); }
    return lhs;
}

ExprPtr Parser::parse_compare() {
    ExprPtr lhs = parse_addsub();
    if (is(K::Matches)) {
        bump();
        MatchPattern p = parse_match_pattern();
        auto e = mk(Expr::Kind::Matches);
        e->children.push_back(lhs);
        e->pattern = std::move(p);
        return e;
    }
    if (is(K::In)) {
        bump();
        ExprPtr rhs = parse_addsub();
        auto e = mk(Expr::Kind::In);
        e->children = {lhs, rhs};
        return e;
    }
    if (is(K::NotIn)) {
        bump();
        ExprPtr rhs = parse_addsub();
        auto in = mk(Expr::Kind::In);
        in->children = {lhs, rhs};
        return mkNot(in);
    }
    if (is(K::ContainsRev)) {
        bump();
        ExprPtr rhs = parse_addsub();
        auto e = mk(Expr::Kind::In);
        e->children = {rhs, lhs};  // a ∋ b  ==>  b ∈ a
        return e;
    }
    auto op = peek_compare_op(kind());
    if (op) {
        bump();
        ExprPtr rhs = parse_addsub();
        if (peek_compare_op(kind())) {
            // chained comparisons: AND-combine pairwise, sharing inner operands
            std::vector<ExprPtr> operands{lhs, rhs};
            std::vector<BinOp> ops{*op};
            while (auto next = peek_compare_op(kind())) {
                bump();
                operands.push_back(parse_addsub());
                ops.push_back(*next);
            }
            ExprPtr acc;
            for (size_t k = 0; k < ops.size(); k++) {
                ExprPtr pair = mkBinary(ops[k], operands[k], operands[k + 1]);
                acc = acc ? mkBinary(BinOp::And, acc, pair) : pair;
            }
            return acc;
        }
        return mkBinary(*op, lhs, rhs);
    }
    return lhs;
}

ExprPtr Parser::parse_addsub() {
    ExprPtr lhs = parse_muldiv();
    for (;;) {
        BinOp op;
        if (is(K::Plus)) op = BinOp::Add;
        else if (is(K::PlusPlus)) op = BinOp::Concat;
        else if (is(K::Minus)) op = BinOp::Sub;
        else break;
        bump();
        lhs = mkBinary(op, lhs, parse_muldiv());
    }
    return lhs;
}

ExprPtr Parser::parse_muldiv() {
    ExprPtr lhs = parse_unary();
    for (;;) {
        BinOp op;
        if (is(K::Star)) op = BinOp::Mul;
        else if (is(K::Slash)) op = BinOp::Div;
        else break;
        bump();
        lhs = mkBinary(op, lhs, parse_unary());
    }
    return lhs;
}

ExprPtr Parser::parse_unary() {
    if (is(K::Not)) { bump(); return mkNot(parse_unary()); }
    if (is(K::Minus)) { bump(); return mkBinary(BinOp::Sub, mkInt(0), parse_unary()); }
    if (is(K::Hash)) { bump(); auto e = mk(Expr::Kind::Card); e->children.push_back(parse_unary()); return e; }
    return parse_postfix();
}

ExprPtr Parser::parse_postfix() {
    ExprPtr e = parse_atom();
    for (;;) {
        if (is(K::LBracket)) {
            bump();
            ExprPtr idx = parse_expr();
            eat(K::RBracket);
            auto ix = mk(Expr::Kind::Index);
            ix->children = {e, idx};
            e = ix;
        } else if (is(K::Dot)) {
            bump();
            Token f = bump();
            if (f.kind != K::Ident) throw ParseError("expected field name after '.'");
            auto fe = mk(Expr::Kind::Field);
            fe->children.push_back(e);
            fe->str = f.str;
            e = fe;
        } else break;
    }
    return e;
}

ExprPtr Parser::parse_atom() {
    switch (kind()) {
        case K::Int: { auto e = mkInt(peek().ival); bump(); return e; }
        case K::Real: { auto e = mkReal(peek().rval); bump(); return e; }
        case K::Str: { auto e = mkStr(peek().str); bump(); return e; }
        case K::True: bump(); return mkBool(true);
        case K::False: bump(); return mkBool(false);
        case K::Match: return parse_match();
        case K::Ident: {
            std::string s = peek().str;
            bump();
            // dotted ident chain
            std::string name = s;
            while (is(K::Dot)) {
                bump();
                Token f = bump();
                if (f.kind != K::Ident) throw ParseError("expected field name after '.'");
                name += "."; name += f.str;
            }
            // optional <T> suffix only when followed by '('. Speculative: on a
            // bare comparison like `n < 3` the suffix parse fails — rewind and
            // treat `<` as the comparison operator (mirrors Rust's match-on-Err).
            if (is(K::Lt)) {
                size_t saved = pos_;
                try {
                    auto parsed = try_parse_generic_args_suffix();
                    if (parsed && is(K::LParen)) name += *parsed;
                    else pos_ = saved;
                } catch (const ParseError &) {
                    pos_ = saved;
                }
            }
            if (is(K::LParen)) {
                bump();  // (
                std::vector<ExprPtr> args;
                if (!is(K::RParen)) {
                    for (;;) {
                        args.push_back(parse_expr());
                        if (is(K::Comma)) { bump(); continue; }
                        break;
                    }
                }
                eat(K::RParen);
                auto e = mk(Expr::Kind::Call);
                e->str = name; e->children = std::move(args);
                return e;
            }
            return mkIdent(name);
        }
        case K::LParen: {
            bump();
            ExprPtr first = parse_expr();
            if (is(K::Comma)) {
                std::vector<ExprPtr> items{first};
                while (is(K::Comma)) { bump(); items.push_back(parse_expr()); }
                eat(K::RParen);
                auto e = mk(Expr::Kind::Tuple);
                e->children = std::move(items);
                return e;
            }
            eat(K::RParen);
            return first;
        }
        case K::LBrace: {
            bump();
            if (is(K::RBrace)) { bump(); return mk(Expr::Kind::SetLit); }
            ExprPtr first = parse_expr();
            if (is(K::DotDot)) {
                bump();
                ExprPtr hi = parse_expr();
                eat(K::RBrace);
                auto e = mk(Expr::Kind::Range);
                e->children = {first, hi};
                return e;
            }
            std::vector<ExprPtr> items{first};
            while (is(K::Comma)) { bump(); items.push_back(parse_expr()); }
            eat(K::RBrace);
            auto e = mk(Expr::Kind::SetLit);
            e->children = std::move(items);
            return e;
        }
        case K::LSeq: {
            bump();
            if (is(K::RSeq)) { bump(); return mk(Expr::Kind::SeqLit); }
            ExprPtr first = parse_expr();
            std::vector<ExprPtr> items{first};
            while (is(K::Comma)) { bump(); items.push_back(parse_expr()); }
            eat(K::RSeq);
            auto e = mk(Expr::Kind::SeqLit);
            e->children = std::move(items);
            return e;
        }
        default:
            throw ParseError(std::string("expected expression, got ") + kind_name(kind()));
    }
}

// ---------------------------------------------------------------------------
// patterns / match
// ---------------------------------------------------------------------------
ExprPtr Parser::parse_match() {
    bump();  // match
    ExprPtr scrut = parse_or();
    if (!is(K::Newline)) throw ParseError("expected newline + indented arms after `match scrutinee`");
    bump();
    while (is(K::Newline)) bump();
    if (!(is(K::Indent) && peek().indent > 0)) throw ParseError("expected indented arms after `match`");
    size_t arm_indent = peek().indent;
    std::vector<MatchArm> arms;
    for (;;) {
        if (is(K::Indent) && peek().indent == arm_indent) bump();
        else break;
        MatchPattern p = parse_match_pattern();
        if (bump().kind != K::Implies) throw ParseError("expected `⇒` after pattern");
        ExprPtr body = parse_or();
        arms.push_back(MatchArm{std::move(p), body});
        while (is(K::Newline)) bump();
    }
    if (arms.empty()) throw ParseError("match must have at least one arm");
    auto e = mk(Expr::Kind::Match);
    e->children.push_back(scrut);
    e->arms = std::move(arms);
    return e;
}

MatchPattern Parser::parse_match_pattern() {
    if (!is(K::Ident)) throw ParseError("expected pattern (Ctor, binding, or `_`)");
    std::string s = peek().str;
    bump();
    MatchPattern p;
    if (s == "_") { p.kind = MatchPattern::Kind::Wildcard; return p; }
    if (is(K::LParen)) {
        bump();  // (
        std::vector<MatchPattern> binds;
        if (!is(K::RParen)) {
            for (;;) {
                binds.push_back(parse_match_pattern());
                if (is(K::Comma)) { bump(); continue; }
                break;
            }
        }
        eat(K::RParen);
        p.kind = MatchPattern::Kind::Ctor; p.name = s; p.binds = std::move(binds);
        return p;
    }
    bool is_ctor = !s.empty() && (s[0] >= 'A' && s[0] <= 'Z');
    if (is_ctor) { p.kind = MatchPattern::Kind::Ctor; p.name = s; }
    else { p.kind = MatchPattern::Kind::Bind; p.name = s; }
    return p;
}

Program parse(const std::string &src) {
    return Parser(tokenize(src)).parse_program();
}

}  // namespace evc
