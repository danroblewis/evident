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
    struct VariantInfo {
        std::string enum_name;
        std::vector<std::string> field_types;  // payload field type names ("Int", "LL", …)
    };

    const Program &prog_;
    std::unordered_map<std::string, Sort> env_;
    std::vector<std::pair<std::string, Sort>> declared_;
    std::string out_;

    // enum registries
    std::unordered_map<std::string, const EnumDecl *> enum_by_name_;
    std::unordered_map<std::string, VariantInfo> variant_;  // variant name -> info

    // active substitutions for match-arm binds (name -> already-emitted term)
    std::unordered_map<std::string, std::string> subst_;

    void index_enums();
    std::vector<std::string> reachable_enums(const SchemaDecl &schema);
    void emit_datatypes(const SchemaDecl &schema);
    std::string field_sort_name(const std::string &type_name);

    std::optional<Sort> sort_of(const Expr &e);
    std::string expr(const Expr &e);
    std::string binary(BinOp op, const Expr &a, const Expr &b);
    std::string in_expr(const Expr &lhs, const Expr &rhs);
    std::string emit_match(const Expr &e);
    std::string emit_matches(const Expr &e);
    std::string emit_ctor_call(const Expr &e);
    std::string recognizer(const std::string &ctor, const std::string &term);
    std::string accessor(const std::string &ctor, size_t field_idx, const std::string &term);

    [[noreturn]] void fail(const std::string &m) { throw SmtError(m); }
};

void Emitter::index_enums() {
    for (const auto &e : prog_.enums) {
        enum_by_name_[e.name] = &e;
        for (const auto &v : e.variants) {
            VariantInfo info;
            info.enum_name = e.name;
            for (const auto &f : v.fields) info.field_types.push_back(f.type_name);
            variant_[v.name] = std::move(info);
        }
    }
}

// SMT sort name for an enum payload field type: scalar -> its SMT name; enum
// name -> itself (for recursion/refs); anything else is out of subset.
std::string Emitter::field_sort_name(const std::string &tn) {
    if (auto s = scalar_sort(tn)) return s->smt();
    if (enum_by_name_.count(tn)) return tn;
    fail("enum payload type `" + tn + "` unsupported (out of subset)");
}

// Which enums must be declared for `schema`: those named by a membership, plus
// any enum reachable through payload fields (recursive / mutually-recursive).
std::vector<std::string> Emitter::reachable_enums(const SchemaDecl &schema) {
    std::vector<std::string> order;       // declaration order, deduped
    std::unordered_map<std::string, bool> seen;
    std::vector<std::string> work;

    auto add = [&](const std::string &name) {
        if (enum_by_name_.count(name) && !seen.count(name)) {
            seen[name] = true;
            order.push_back(name);
            work.push_back(name);
        }
    };
    for (const auto &item : schema.body)
        if (item.kind == BodyItem::Kind::Membership) add(item.type_name);

    while (!work.empty()) {
        std::string e = work.back(); work.pop_back();
        for (const auto &v : enum_by_name_[e]->variants)
            for (const auto &f : v.fields)
                if (enum_by_name_.count(f.type_name)) add(f.type_name);
    }
    // Emit in program declaration order for determinism.
    std::vector<std::string> ordered;
    for (const auto &e : prog_.enums)
        if (seen.count(e.name)) ordered.push_back(e.name);
    return ordered;
}

void Emitter::emit_datatypes(const SchemaDecl &schema) {
    auto names = reachable_enums(schema);
    if (names.empty()) return;

    std::string sorts, bodies;
    for (const auto &name : names) {
        sorts += "(" + name + " 0)";
        const EnumDecl *e = enum_by_name_[name];
        std::string ctors;
        for (const auto &v : e->variants) {
            if (v.fields.empty()) {
                ctors += "(" + v.name + ")";
            } else {
                std::string fields;
                for (size_t i = 0; i < v.fields.size(); i++)
                    fields += " (" + v.name + "_f" + std::to_string(i) + " " +
                              field_sort_name(v.fields[i].type_name) + ")";
                ctors += "(" + v.name + fields + ")";
            }
            ctors += " ";
        }
        bodies += "(" + ctors + ")";
    }
    out_ += "(declare-datatypes (" + sorts + ") (" + bodies + "))\n";
}

EmitResult Emitter::emit(const SchemaDecl &schema) {
    emit_datatypes(schema);

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
            if (v != variant_.end()) return Sort{Sort::Tag::Enum, v->second.enum_name};
            return std::nullopt;
        }
        case K::Not: return Sort{Sort::Tag::Bool, {}};
        case K::In: case K::Matches: return Sort{Sort::Tag::Bool, {}};
        case K::Match: {
            for (const auto &arm : e.arms)
                if (auto s = sort_of(*arm.body)) return s;
            return std::nullopt;
        }
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
            if (v != variant_.end()) return Sort{Sort::Tag::Enum, v->second.enum_name};
            return std::nullopt;
        }
        default: return std::nullopt;
    }
}

std::string Emitter::expr(const Expr &e) {
    using K = Expr::Kind;
    switch (e.kind) {
        case K::Ident: {
            auto sub = subst_.find(e.str);
            if (sub != subst_.end()) return sub->second;  // match-arm bind
            if (env_.count(e.str)) return e.str;
            auto v = variant_.find(e.str);
            if (v != variant_.end() && v->second.field_types.empty()) return e.str;  // nullary ctor
            fail("undeclared identifier `" + e.str + "` (out of subset)");
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
        case K::Call:    return emit_ctor_call(e);
        case K::Match:   return emit_match(e);
        case K::Matches: return emit_matches(e);

        // Out of subset (reported, never mis-emitted).
        case K::SetLit:  fail("set literal (not as ∈ RHS) unsupported");
        case K::SeqLit:  fail("sequence literal unsupported");
        case K::Range:   fail("bare range unsupported (only as ∈ RHS)");
        case K::Tuple:   fail("tuple unsupported");
        case K::Forall:  fail("∀ quantifier unsupported (quantifier-free subset)");
        case K::Exists:  fail("∃ quantifier unsupported (quantifier-free subset)");
        case K::Card:    fail("cardinality `#` unsupported");
        case K::Index:   fail("indexing `[]` unsupported");
        case K::Field:   fail("field access unsupported (records out of subset)");
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

std::string Emitter::recognizer(const std::string &ctor, const std::string &term) {
    return "((_ is " + ctor + ") " + term + ")";
}

std::string Emitter::accessor(const std::string &ctor, size_t field_idx, const std::string &term) {
    return "(" + ctor + "_f" + std::to_string(field_idx) + " " + term + ")";
}

// `Ctor(arg, …)` -> `(Ctor arg …)`; nullary handled in expr() Ident path.
std::string Emitter::emit_ctor_call(const Expr &e) {
    auto v = variant_.find(e.str);
    if (v == variant_.end()) fail("call `" + e.str + "` unsupported (not an enum constructor)");
    const VariantInfo &info = v->second;
    if (e.children.size() != info.field_types.size())
        fail("constructor `" + e.str + "` arity mismatch (expected " +
             std::to_string(info.field_types.size()) + ", got " +
             std::to_string(e.children.size()) + ")");
    if (info.field_types.empty()) return e.str;
    std::string s = "(" + e.str;
    for (const auto &arg : e.children) s += " " + expr(*arg);
    s += ")";
    return s;
}

// `e matches Ctor(_, …)` -> recognizer; payload binds ignored.
std::string Emitter::emit_matches(const Expr &e) {
    const Expr &scrut = *e.children[0];
    auto ss = sort_of(scrut);
    if (!ss || ss->tag != Sort::Tag::Enum)
        fail("`matches` scrutinee must be enum-typed");
    if (e.pattern.kind != MatchPattern::Kind::Ctor)
        fail("`matches` requires a constructor pattern");
    if (!variant_.count(e.pattern.name))
        fail("`matches` unknown constructor `" + e.pattern.name + "`");
    return recognizer(e.pattern.name, expr(scrut));
}

// match scrut \n  Ctor(binds) => body  ...  -> nested ite over recognizers, with
// payload binds substituted (one level: Bind / Wildcard sub-patterns).
std::string Emitter::emit_match(const Expr &e) {
    const Expr &scrut = *e.children[0];
    auto ss = sort_of(scrut);
    if (!ss || ss->tag != Sort::Tag::Enum)
        fail("match scrutinee must be enum-typed");
    std::string S = expr(scrut);

    // Emit one arm's body with its binds in scope (subst + temp env), restoring after.
    auto emit_arm = [&](const MatchArm &arm) -> std::string {
        std::vector<std::string> added_subst;
        std::vector<std::string> added_env;
        const MatchPattern &p = arm.pattern;
        if (p.kind == MatchPattern::Kind::Ctor) {
            auto v = variant_.find(p.name);
            if (v == variant_.end()) fail("match: unknown constructor `" + p.name + "`");
            const VariantInfo &info = v->second;
            for (size_t i = 0; i < p.binds.size() && i < info.field_types.size(); i++) {
                const MatchPattern &b = p.binds[i];
                if (b.kind == MatchPattern::Kind::Bind) {
                    subst_[b.name] = accessor(p.name, i, S);
                    added_subst.push_back(b.name);
                    if (auto fs = scalar_sort(info.field_types[i])) { env_[b.name] = *fs; added_env.push_back(b.name); }
                    else if (enum_by_name_.count(info.field_types[i])) { env_[b.name] = Sort{Sort::Tag::Enum, info.field_types[i]}; added_env.push_back(b.name); }
                } else if (b.kind == MatchPattern::Kind::Ctor) {
                    fail("nested constructor patterns in match unsupported (one level only)");
                }
            }
        } else if (p.kind == MatchPattern::Kind::Bind) {
            subst_[p.name] = S;
            added_subst.push_back(p.name);
            env_[p.name] = *ss;
            added_env.push_back(p.name);
        }
        std::string body = expr(*arm.body);
        for (auto &n : added_subst) subst_.erase(n);
        for (auto &n : added_env) env_.erase(n);
        return body;
    };

    if (e.arms.empty()) fail("match has no arms");
    // Last arm is the base else value (assumed to match by exhaustiveness).
    std::string acc = emit_arm(e.arms.back());
    for (size_t k = e.arms.size(); k-- > 1;) {
        const MatchArm &arm = e.arms[k - 1];
        std::string body = emit_arm(arm);
        std::string cond;
        if (arm.pattern.kind == MatchPattern::Kind::Ctor)
            cond = recognizer(arm.pattern.name, S);
        else
            cond = "true";  // catch-all
        acc = "(ite " + cond + " " + body + " " + acc + ")";
    }
    return acc;
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
