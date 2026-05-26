#include "smtlib.h"

#include <optional>
#include <sstream>

namespace evc {

std::string Sort::smt() const {
    switch (tag) {
        case Tag::Int:  return "Int";
        case Tag::Bool: return "Bool";
        case Tag::Real: return "Real";
        case Tag::Str:  return "String";
        case Tag::Enum: return enum_name;
    }
    return "Int";
}

namespace {

// Map an Evident scalar type name to an SMT sort. Nat/Pos are Int with an added
// bound emitted by the caller. Enum sorts handled separately by the emitter.
std::optional<Sort> scalar_sort(const std::string &type_name) {
    if (type_name == "Int" || type_name == "Nat" || type_name == "Pos") return Sort{Sort::Tag::Int, {}};
    if (type_name == "Bool") return Sort{Sort::Tag::Bool, {}};
    if (type_name == "Real") return Sort{Sort::Tag::Real, {}};
    if (type_name == "String") return Sort{Sort::Tag::Str, {}};
    return std::nullopt;
}

// SMT-LIB int literal — negatives wrap in (- n). Magnitude computed via unsigned
// to dodge INT64_MIN overflow on negate.
std::string int_lit_safe(int64_t i) {
    if (i >= 0) return std::to_string(i);
    unsigned long long mag = (unsigned long long)(-(i + 1)) + 1ULL;
    return "(- " + std::to_string(mag) + ")";
}

std::string real_lit(double r) {
    std::ostringstream os;
    double mag = r < 0 ? -r : r;
    os << mag;
    std::string s = os.str();
    if (s.find('.') == std::string::npos && s.find('e') == std::string::npos &&
        s.find('E') == std::string::npos)
        s += ".0";
    if (r < 0) return "(- " + s + ")";
    return s;
}

std::string str_lit(const std::string &s) {
    std::string out = "\"";
    for (char c : s) {
        if (c == '"') out += '"';  // SMT-LIB doubles internal quotes
        out += c;
    }
    out += "\"";
    return out;
}

// ---------------------------------------------------------------------------
// Emitter: SchemaDecl -> SMT-LIB. Stateful so enum datatypes + quantifier
// unrolling (M4) can extend it without re-threading parameters.
// ---------------------------------------------------------------------------
class Emitter {
public:
    explicit Emitter(const Program &prog) : prog_(prog) { index_enums(); }

    EmitResult emit(const SchemaDecl &schema);

private:
    const Program &prog_;
    std::unordered_map<std::string, Sort> env_;
    std::vector<std::pair<std::string, Sort>> declared_;
    std::string out_;

    // enum registries (built in M4; scalar subset leaves them empty)
    std::unordered_map<std::string, const EnumDecl *> enum_by_name_;
    // variant name -> (enum name, field count)
    std::unordered_map<std::string, std::pair<std::string, size_t>> variant_;

    void index_enums();
    void emit_datatypes();

    std::optional<Sort> sort_of(const Expr &e);
    std::string expr(const Expr &e);
    std::string binary(BinOp op, const Expr &a, const Expr &b);
    std::string in_expr(const Expr &lhs, const Expr &rhs);

    [[noreturn]] void fail(const std::string &m) { throw SmtError(m); }
};

void Emitter::index_enums() {
    for (const auto &e : prog_.enums) {
        enum_by_name_[e.name] = &e;
        for (const auto &v : e.variants)
            variant_[v.name] = {e.name, v.fields.size()};
    }
}

void Emitter::emit_datatypes() {
    // M4: emit declare-datatypes for enums. Scalar subset: nothing.
    // (Implemented when enums are wired; left as a no-op preamble for now.)
}

EmitResult Emitter::emit(const SchemaDecl &schema) {
    emit_datatypes();

    // Pass 1: declarations.
    for (const auto &item : schema.body) {
        if (item.kind != BodyItem::Kind::Membership) continue;
        const std::string &name = item.name;
        const std::string &tn = item.type_name;

        std::optional<Sort> sort = scalar_sort(tn);
        if (!sort) {
            // enum type?
            auto it = enum_by_name_.find(tn);
            if (it != enum_by_name_.end()) sort = Sort{Sort::Tag::Enum, tn};
        }
        if (!sort) fail("unsupported type `" + tn + "` for `" + name + "`");

        if (env_.count(name)) continue;  // first decl wins
        env_[name] = *sort;
        declared_.push_back({name, *sort});
        out_ += "(declare-const " + name + " " + sort->smt() + ")\n";
        if (tn == "Nat") out_ += "(assert (>= " + name + " 0))\n";
        else if (tn == "Pos") out_ += "(assert (> " + name + " 0))\n";

        if (item.pins.kind != Pins::Kind::None)
            fail("pins on scalar `" + name + "` not supported");
    }

    // Pass 2: constraints.
    for (const auto &item : schema.body) {
        switch (item.kind) {
            case BodyItem::Kind::Membership: break;
            case BodyItem::Kind::Constraint:
                out_ += "(assert " + expr(*item.expr) + ")\n";
                break;
            case BodyItem::Kind::Passthrough: fail("passthrough `.." + item.name + "` not supported");
            case BodyItem::Kind::Subclaim: fail("subclaim not supported");
            case BodyItem::Kind::ClaimCall: fail("claim call `" + item.name + "` not supported");
            case BodyItem::Kind::HaltsWithin: fail("halts_within not supported");
        }
    }

    return EmitResult{out_, declared_};
}

std::optional<Sort> Emitter::sort_of(const Expr &e) {
    using K = Expr::Kind;
    switch (e.kind) {
        case K::Int:  return Sort{Sort::Tag::Int, {}};
        case K::Real: return Sort{Sort::Tag::Real, {}};
        case K::Bool: return Sort{Sort::Tag::Bool, {}};
        case K::Str:  return Sort{Sort::Tag::Str, {}};
        case K::Ident: {
            auto it = env_.find(e.str);
            if (it != env_.end()) return it->second;
            auto v = variant_.find(e.str);
            if (v != variant_.end()) return Sort{Sort::Tag::Enum, v->second.first};
            return std::nullopt;
        }
        case K::Not: return Sort{Sort::Tag::Bool, {}};
        case K::In: case K::Matches: return Sort{Sort::Tag::Bool, {}};
        case K::Binary:
            switch (e.op) {
                case BinOp::Eq: case BinOp::Neq: case BinOp::Lt: case BinOp::Le:
                case BinOp::Gt: case BinOp::Ge: case BinOp::And: case BinOp::Or:
                case BinOp::Implies:
                    return Sort{Sort::Tag::Bool, {}};
                case BinOp::Concat: return Sort{Sort::Tag::Str, {}};
                case BinOp::Add: case BinOp::Sub: case BinOp::Mul: case BinOp::Div: {
                    auto a = sort_of(*e.children[0]);
                    auto b = sort_of(*e.children[1]);
                    if ((a && a->tag == Sort::Tag::Real) || (b && b->tag == Sort::Tag::Real))
                        return Sort{Sort::Tag::Real, {}};
                    if ((a && a->tag == Sort::Tag::Int) || (b && b->tag == Sort::Tag::Int))
                        return Sort{Sort::Tag::Int, {}};
                    return std::nullopt;
                }
            }
            return std::nullopt;
        case K::Ternary: {
            auto t = sort_of(*e.children[1]);
            return t ? t : sort_of(*e.children[2]);
        }
        case K::Call: {
            auto v = variant_.find(e.str);
            if (v != variant_.end()) return Sort{Sort::Tag::Enum, v->second.first};
            return std::nullopt;
        }
        default: return std::nullopt;
    }
}

std::string Emitter::expr(const Expr &e) {
    using K = Expr::Kind;
    switch (e.kind) {
        case K::Ident: {
            if (env_.count(e.str)) return e.str;
            auto v = variant_.find(e.str);
            if (v != variant_.end() && v->second.second == 0) return e.str;  // nullary ctor
            fail("undeclared identifier `" + e.str + "` (out of scalar subset)");
        }
        case K::Int:  return int_lit_safe(e.ival);
        case K::Real: return real_lit(e.rval);
        case K::Bool: return e.bval ? "true" : "false";
        case K::Str:  return str_lit(e.str);
        case K::Not:  return "(not " + expr(*e.children[0]) + ")";
        case K::Binary: return binary(e.op, *e.children[0], *e.children[1]);
        case K::Ternary:
            return "(ite " + expr(*e.children[0]) + " " + expr(*e.children[1]) +
                   " " + expr(*e.children[2]) + ")";
        case K::In: return in_expr(*e.children[0], *e.children[1]);

        // Out of scalar subset (some wired in M4).
        case K::SetLit:  fail("set literal (not as ∈ RHS) unsupported");
        case K::SeqLit:  fail("sequence literal unsupported");
        case K::Range:   fail("bare range unsupported (only as ∈ RHS)");
        case K::Tuple:   fail("tuple unsupported");
        case K::Forall:  fail("∀ quantifier unsupported (quantifier-free subset)");
        case K::Exists:  fail("∃ quantifier unsupported (quantifier-free subset)");
        case K::Call:    fail("call `" + e.str + "` unsupported");
        case K::Card:    fail("cardinality `#` unsupported");
        case K::Index:   fail("indexing `[]` unsupported");
        case K::Field:   fail("field access unsupported (records out of subset)");
        case K::Match:   fail("match unsupported");
        case K::Matches: fail("matches recognizer unsupported");
    }
    fail("unreachable expr kind");
}

std::string Emitter::binary(BinOp op, const Expr &a, const Expr &b) {
    if (op == BinOp::Neq)
        return "(not (= " + expr(a) + " " + expr(b) + "))";
    std::string sym;
    switch (op) {
        case BinOp::Eq: sym = "="; break;
        case BinOp::Lt: sym = "<"; break;
        case BinOp::Le: sym = "<="; break;
        case BinOp::Gt: sym = ">"; break;
        case BinOp::Ge: sym = ">="; break;
        case BinOp::And: sym = "and"; break;
        case BinOp::Or: sym = "or"; break;
        case BinOp::Implies: sym = "=>"; break;
        case BinOp::Add: sym = "+"; break;
        case BinOp::Sub: sym = "-"; break;
        case BinOp::Mul: sym = "*"; break;
        case BinOp::Concat: sym = "str.++"; break;
        case BinOp::Div: {
            // Int division -> div; Real -> /. Infer from operand sorts.
            auto sa = sort_of(a);
            auto sb = sort_of(b);
            bool real = (sa && sa->tag == Sort::Tag::Real) || (sb && sb->tag == Sort::Tag::Real);
            sym = real ? "/" : "div";
            break;
        }
        case BinOp::Neq: break;  // handled above
    }
    return "(" + sym + " " + expr(a) + " " + expr(b) + ")";
}

std::string Emitter::in_expr(const Expr &lhs, const Expr &rhs) {
    std::string l = expr(lhs);
    if (rhs.kind == Expr::Kind::Range) {
        std::string lo = expr(*rhs.children[0]);
        std::string hi = expr(*rhs.children[1]);
        return "(and (>= " + l + " " + lo + ") (<= " + l + " " + hi + "))";
    }
    if (rhs.kind == Expr::Kind::SetLit) {
        if (rhs.children.empty()) return "false";
        std::vector<std::string> parts;
        for (const auto &el : rhs.children)
            parts.push_back("(= " + l + " " + expr(*el) + ")");
        if (parts.size() == 1) return parts[0];
        std::string s = "(or";
        for (auto &p : parts) s += " " + p;
        s += ")";
        return s;
    }
    fail("∈ RHS must be a set literal or range in the scalar subset");
}

}  // namespace

EmitResult emit_schema(const SchemaDecl &schema, const Program &prog) {
    Emitter em(prog);
    return em.emit(schema);
}

std::string schema_to_smtlib(const SchemaDecl &schema, const Program &prog) {
    return emit_schema(schema, prog).text;
}

}  // namespace evc
