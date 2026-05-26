#include "smtlib.h"

#include <algorithm>
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
        case Tag::Seq:  return "(Seq " + (elem ? elem->smt() : std::string("Int")) + ")";
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

// `Seq(Elem)` -> a Seq sort. The element types match the Rust oracle's `SeqElem`
// set (Int, Bool, String) so the seed doesn't claim a capability the spec lacks;
// Real/Nat/Pos/enum/record/nested-Seq elements return nullopt (-> out of subset,
// matching the oracle, which drops them). NB: Seq(String) emits correct SMT-LIB
// but Z3's seq theory returns `unknown` on the plain solver (a documented
// divergence — the oracle's array representation decides it; see c-runtime.md).
std::optional<Sort> seq_sort(const std::string &type_name) {
    if (type_name.size() < 5 || type_name.compare(0, 4, "Seq(") != 0 || type_name.back() != ')')
        return std::nullopt;
    std::string inner = type_name.substr(4, type_name.size() - 5);
    if (inner == "Int")    return Sort::seq(Sort(Sort::Tag::Int));
    if (inner == "Bool")   return Sort::seq(Sort(Sort::Tag::Bool));
    if (inner == "String") return Sort::seq(Sort(Sort::Tag::Str));
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

bool is_arith(BinOp op) {
    return op == BinOp::Add || op == BinOp::Sub || op == BinOp::Mul || op == BinOp::Div;
}

// Per-field arithmetic operator symbol: `/` only when the leaf is Real, else
// integer `div` — mirrors the scalar binary() Div sort-inference.
std::string arith_sym(BinOp op, const Sort &leaf) {
    switch (op) {
        case BinOp::Add: return "+";
        case BinOp::Sub: return "-";
        case BinOp::Mul: return "*";
        case BinOp::Div: return leaf.tag == Sort::Tag::Real ? "/" : "div";
        default: return "?";
    }
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
    explicit Emitter(const Program &prog) : prog_(prog) { index_enums(); index_types(); }

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

    // record registries (M4c): record types are expanded to per-field Z3 leaves
    // (`v.x`, `v.pos.y`, …) — the same dotted-leaf shape the Rust runtime uses, so
    // model values cross-check exactly. `type/schema` decls whose body is all field
    // memberships of scalar/record types qualify; constraints/subclaims do not.
    std::unordered_map<std::string, const SchemaDecl *> type_decls_;  // type/schema name -> decl
    std::unordered_map<std::string, std::string> record_vars_;        // var path -> record type

    // active substitutions for match-arm binds (name -> already-emitted term)
    std::unordered_map<std::string, std::string> subst_;

    void index_enums();
    void index_types();
    std::vector<std::string> reachable_enums(const SchemaDecl &schema);
    void emit_datatypes(const SchemaDecl &schema);
    std::string field_sort_name(const std::string &type_name);

    // records
    bool is_record_type(const std::string &name);
    std::vector<const BodyItem *> record_fields(const std::string &type_name);
    void declare_record(const std::string &var, const std::string &type_name);
    void instantiate_invariants(const std::string &var, const std::string &type_name);
    void apply_record_pins(const std::string &var, const std::string &type_name, const Pins &pins);
    void emit_pin_eq(const std::string &leaf, const std::string &field_type, const Expr &value);
    std::optional<std::string> record_type_of_expr(const Expr &e);
    std::vector<std::pair<std::string, Sort>> record_leaves(const Expr &e, const std::string &type_name);
    std::string record_compare(BinOp op, const Expr &a, const Expr &b);

    std::optional<Sort> sort_of(const Expr &e);
    std::string expr(const Expr &e);
    std::string binary(BinOp op, const Expr &a, const Expr &b);
    std::string in_expr(const Expr &lhs, const Expr &rhs);
    std::optional<int64_t> eval_const_int(const Expr &e);
    std::string emit_quantifier(const Expr &e);
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

void Emitter::index_types() {
    for (const auto &s : prog_.enums) (void)s;  // enums indexed separately
    for (const auto &s : prog_.schemas)
        if (s.keyword == Keyword::Type || s.keyword == Keyword::Schema)
            type_decls_[s.name] = &s;
}

// A record type is a `type`/`schema` decl whose body is ALL field memberships,
// each field scalar or itself a record (recursively, cycle-guarded). A decl with
// constraints / subclaims / passthroughs is NOT a record — classifying it as one
// would silently drop those, so it stays out of subset (membership fails loudly).
bool Emitter::is_record_type(const std::string &name) {
    static thread_local std::vector<std::string> visiting;
    auto it = type_decls_.find(name);
    if (it == type_decls_.end()) return false;
    const SchemaDecl *d = it->second;
    if (!d->type_params.empty()) return false;
    if (std::find(visiting.begin(), visiting.end(), name) != visiting.end()) return false;
    bool any_field = false;
    visiting.push_back(name);
    bool ok = true;
    for (const auto &b : d->body) {
        if (b.kind == BodyItem::Kind::Constraint) continue;  // local invariant (refined record)
        if (b.kind != BodyItem::Kind::Membership) { ok = false; break; }  // subclaim/passthrough/call -> not a record
        any_field = true;
        const std::string &ft = b.type_name;
        if (scalar_sort(ft)) continue;
        if (!is_record_type(ft)) { ok = false; break; }  // nested record only (enum fields: later)
    }
    visiting.pop_back();
    return ok && any_field;
}

std::vector<const BodyItem *> Emitter::record_fields(const std::string &type_name) {
    std::vector<const BodyItem *> fields;
    const SchemaDecl *d = type_decls_[type_name];
    for (const auto &b : d->body)
        if (b.kind == BodyItem::Kind::Membership) fields.push_back(&b);
    return fields;
}

// `v ∈ Rec` -> per-field leaf consts (`v.f`), recursing into nested-record fields
// (`v.pos.x`). Registers record_vars_ for v and every nested-record path so field
// access and the comparison lift can find the structure.
void Emitter::declare_record(const std::string &var, const std::string &type_name) {
    record_vars_[var] = type_name;
    for (const BodyItem *f : record_fields(type_name)) {
        std::string leaf = var + "." + f->name;
        const std::string &ft = f->type_name;
        if (auto s = scalar_sort(ft)) {
            if (env_.count(leaf)) continue;
            env_[leaf] = *s;
            declared_.push_back({leaf, *s});
            out_ += "(declare-const " + leaf + " " + s->smt() + ")\n";
            if (ft == "Nat") out_ += "(assert (>= " + leaf + " 0))\n";
            else if (ft == "Pos") out_ += "(assert (> " + leaf + " 0))\n";
        } else {
            declare_record(leaf, ft);  // nested record (declares its leaves + its invariants)
        }
    }
    instantiate_invariants(var, type_name);  // this type's local invariants, rebound to `var`
}

// Emit a refined record's local-invariant constraints (`lo ≤ hi`) for instance
// `var`, rebinding each scalar field's bare name to its leaf (`lo` -> `var.lo`)
// via subst_ + env_ — the same scoped-bind trick emit_match uses. Scalar-field
// invariants only; an invariant over record-typed fields would not lift here and
// fails loudly.
void Emitter::instantiate_invariants(const std::string &var, const std::string &type_name) {
    const SchemaDecl *d = type_decls_[type_name];
    bool has_invariant = false;
    for (const auto &b : d->body)
        if (b.kind == BodyItem::Kind::Constraint) { has_invariant = true; break; }
    if (!has_invariant) return;

    // Bind every scalar field name -> its leaf, saving any shadowed outer binding.
    std::vector<std::pair<std::string, std::optional<std::string>>> saved_subst;
    std::vector<std::pair<std::string, std::optional<Sort>>> saved_env;
    for (const BodyItem *f : record_fields(type_name)) {
        if (auto s = scalar_sort(f->type_name)) {
            saved_subst.push_back({f->name, subst_.count(f->name) ? std::optional<std::string>(subst_[f->name]) : std::nullopt});
            saved_env.push_back({f->name, env_.count(f->name) ? std::optional<Sort>(env_[f->name]) : std::nullopt});
            subst_[f->name] = var + "." + f->name;
            env_[f->name] = *s;
        }
    }
    for (const auto &b : d->body)
        if (b.kind == BodyItem::Kind::Constraint)
            out_ += "(assert " + expr(*b.expr) + ")\n";
    for (auto &p : saved_subst) { if (p.second) subst_[p.first] = *p.second; else subst_.erase(p.first); }
    for (auto &p : saved_env)   { if (p.second) env_[p.first] = *p.second;   else env_.erase(p.first); }
}

void Emitter::emit_pin_eq(const std::string &leaf, const std::string &ft, const Expr &value) {
    if (scalar_sort(ft)) {
        out_ += "(assert (= " + leaf + " " + expr(value) + "))\n";
    } else {
        // nested-record field pinned to a record value: lift componentwise.
        Expr lhs(Expr::Kind::Ident);
        lhs.str = leaf;
        out_ += "(assert " + record_compare(BinOp::Eq, lhs, value) + ")\n";
    }
}

void Emitter::apply_record_pins(const std::string &var, const std::string &type_name, const Pins &pins) {
    if (pins.kind == Pins::Kind::None) return;
    auto fields = record_fields(type_name);
    if (pins.kind == Pins::Kind::Named) {
        for (const auto &m : pins.named) {
            const BodyItem *f = nullptr;
            for (const BodyItem *ff : fields)
                if (ff->name == m.slot) { f = ff; break; }
            if (!f) fail("unknown pin slot `" + m.slot + "` on record `" + type_name + "`");
            emit_pin_eq(var + "." + f->name, f->type_name, *m.value);
        }
    } else {  // positional: pin leading fields, args <= field count
        if (pins.positional.size() > fields.size())
            fail("too many positional pins for record `" + type_name + "`");
        for (size_t i = 0; i < pins.positional.size(); i++)
            emit_pin_eq(var + "." + fields[i]->name, fields[i]->type_name, *pins.positional[i]);
    }
}

std::optional<std::string> Emitter::record_type_of_expr(const Expr &e) {
    if (e.kind == Expr::Kind::Ident) {
        auto it = record_vars_.find(e.str);
        if (it != record_vars_.end()) return it->second;
    }
    if (e.kind == Expr::Kind::Call && is_record_type(e.str)) return e.str;
    // Arithmetic broadcast (M4c): a record op anything (or anything op a record)
    // is record-valued; ternary is record-valued if either branch is.
    if (e.kind == Expr::Kind::Binary && is_arith(e.op)) {
        if (auto l = record_type_of_expr(*e.children[0])) return l;
        if (auto r = record_type_of_expr(*e.children[1])) return r;
    }
    // NB: record-valued ternary deliberately not recognized — see record_leaves.
    return std::nullopt;
}

// Flatten a record-valued expression into its ordered scalar leaf terms (the
// emitted SMT term + sort per leaf). Identifier -> dotted leaf names; record
// literal `Rec(a, b)` -> the args' emitted terms; nested records flatten.
std::vector<std::pair<std::string, Sort>> Emitter::record_leaves(const Expr &e, const std::string &type_name) {
    std::vector<std::pair<std::string, Sort>> out;
    auto fields = record_fields(type_name);

    if (e.kind == Expr::Kind::Ident) {
        for (const BodyItem *f : fields) {
            std::string leaf = e.str + "." + f->name;
            if (auto s = scalar_sort(f->type_name)) out.push_back({leaf, *s});
            else {
                Expr sub(Expr::Kind::Ident);
                sub.str = leaf;
                for (auto &n : record_leaves(sub, f->type_name)) out.push_back(n);
            }
        }
    } else if (e.kind == Expr::Kind::Call) {
        if (e.str != type_name)
            fail("record literal `" + e.str + "` does not match expected `" + type_name + "`");
        if (e.children.size() != fields.size())
            fail("record literal `" + e.str + "` arity mismatch (expected " +
                 std::to_string(fields.size()) + ", got " + std::to_string(e.children.size()) + ")");
        for (size_t i = 0; i < fields.size(); i++) {
            if (auto s = scalar_sort(fields[i]->type_name)) out.push_back({expr(*e.children[i]), *s});
            else for (auto &n : record_leaves(*e.children[i], fields[i]->type_name)) out.push_back(n);
        }
    } else if (e.kind == Expr::Kind::Binary && is_arith(e.op)) {
        // Arithmetic broadcast: record op record (zip), or record op scalar / scalar
        // op record (broadcast the scalar across every leaf). `c = a + b`,
        // `nxt.pos = cur.pos + cur.vel * dt / 1000`.
        const Expr &lhs = *e.children[0];
        const Expr &rhs = *e.children[1];
        auto lt = record_type_of_expr(lhs);
        auto rt = record_type_of_expr(rhs);
        if (lt && rt) {
            auto la = record_leaves(lhs, *lt);
            auto lb = record_leaves(rhs, *rt);
            if (la.size() != lb.size()) fail("record arithmetic shape mismatch (" + *lt + " vs " + *rt + ")");
            for (size_t i = 0; i < la.size(); i++)
                out.push_back({"(" + arith_sym(e.op, la[i].second) + " " + la[i].first + " " + lb[i].first + ")", la[i].second});
        } else if (lt) {
            std::string scalar = expr(rhs);
            for (auto &leaf : record_leaves(lhs, *lt))
                out.push_back({"(" + arith_sym(e.op, leaf.second) + " " + leaf.first + " " + scalar + ")", leaf.second});
        } else if (rt) {
            std::string scalar = expr(lhs);
            for (auto &leaf : record_leaves(rhs, *rt))
                out.push_back({"(" + arith_sym(e.op, leaf.second) + " " + scalar + " " + leaf.first + ")", leaf.second});
        } else {
            fail("record arithmetic with no record operand");
        }
    } else {
        // Record-valued ternary is intentionally out of subset: the Rust oracle
        // does NOT broadcast records through `ite` (it drops `c = (flag ? a : b)`
        // as "couldn't translate to Bool"), so the seed reports the boundary
        // rather than silently exceeding the oracle and diverging on the model.
        fail("expected a record value (identifier, literal, or record arithmetic)");
    }
    return out;
}

// Componentwise comparison/equality lift. `=`,`<`,`≤`,`>`,`≥` -> `and` of the
// per-field op (every axis); `≠` -> `or` of per-field `not =` (some axis differs).
std::string Emitter::record_compare(BinOp op, const Expr &a, const Expr &b) {
    auto ta = record_type_of_expr(a);
    auto tb = record_type_of_expr(b);
    if (!ta || !tb) fail("record compared with a non-record value");
    auto la = record_leaves(a, *ta);
    auto lb = record_leaves(b, *tb);
    if (la.size() != lb.size())
        fail("record comparison shape mismatch (" + *ta + " vs " + *tb + ")");
    std::string sym;
    switch (op) {
        case BinOp::Eq:  sym = "="; break;
        case BinOp::Neq: sym = "="; break;  // wrapped in (not ...) + or-combined below
        case BinOp::Lt:  sym = "<"; break;
        case BinOp::Le:  sym = "<="; break;
        case BinOp::Gt:  sym = ">"; break;
        case BinOp::Ge:  sym = ">="; break;
        default: fail("non-comparison op on records");
    }
    std::vector<std::string> parts;
    for (size_t i = 0; i < la.size(); i++) {
        std::string pair = "(" + sym + " " + la[i].first + " " + lb[i].first + ")";
        if (op == BinOp::Neq) pair = "(not " + pair + ")";
        parts.push_back(pair);
    }
    if (parts.empty()) return op == BinOp::Neq ? "false" : "true";
    if (parts.size() == 1) return parts[0];
    std::string combiner = (op == BinOp::Neq) ? "or" : "and";
    std::string s = "(" + combiner;
    for (auto &p : parts) s += " " + p;
    s += ")";
    return s;
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

        // Record type: expand to per-field leaves, then apply pins. Checked before
        // the scalar/enum path so a record-typed field never falls through.
        if (is_record_type(tn)) {
            if (!record_vars_.count(name)) declare_record(name, tn);
            apply_record_pins(name, tn, item.pins);
            continue;
        }

        // Seq (M4d): Z3 sequence theory, emitted as SMT-LIB text.
        if (auto ss = seq_sort(tn)) {
            if (item.pins.kind != Pins::Kind::None) fail("pins on Seq `" + name + "` not supported");
            if (env_.count(name)) continue;
            env_[name] = *ss;
            declared_.push_back({name, *ss});
            out_ += "(declare-const " + name + " " + ss->smt() + ")\n";
            continue;
        }
        // A Seq with an out-of-subset element type reaches here and fails loudly.
        if (tn.compare(0, 4, "Seq(") == 0)
            fail("Seq element type unsupported for `" + name +
                 "` (only Int/Bool/String — the Rust oracle's set; it drops the rest)");

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
        case K::Forall: case K::Exists: return Sort{Sort::Tag::Bool, {}};
        case K::Card: return Sort{Sort::Tag::Int, {}};
        case K::Index: {
            auto bs = sort_of(*e.children[0]);
            if (bs && bs->tag == Sort::Tag::Seq && bs->elem) return *bs->elem;
            return std::nullopt;
        }
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
        case K::Forall:  case K::Exists: return emit_quantifier(e);

        case K::Card: {
            auto bs = sort_of(*e.children[0]);
            if (!bs || bs->tag != Sort::Tag::Seq) fail("`#` cardinality only on Seq (out of subset)");
            return "(seq.len " + expr(*e.children[0]) + ")";
        }
        case K::Index: {
            auto bs = sort_of(*e.children[0]);
            if (!bs || bs->tag != Sort::Tag::Seq) fail("indexing `[]` only on Seq (out of subset)");
            return "(seq.nth " + expr(*e.children[0]) + " " + expr(*e.children[1]) + ")";
        }

        // Out of subset (reported, never mis-emitted).
        case K::SetLit:  fail("set literal (not as ∈ RHS) unsupported");
        case K::SeqLit:  fail("sequence literal unsupported");
        case K::Range:   fail("bare range unsupported (only as ∈ RHS)");
        case K::Tuple:   fail("tuple unsupported");
        case K::Field:   fail("field access unsupported (records out of subset)");
    }
    fail("unreachable expr kind");
}

std::string Emitter::binary(BinOp op, const Expr &a, const Expr &b) {
    // Record comparison/equality lift (M4c): if either side denotes a record,
    // compare componentwise. Catches `a = b`, `a ≤ b`, `lo ≤ p ≤ hi` (the chain
    // desugars to per-pair Binary), record literals, and pins.
    bool is_cmp = (op == BinOp::Eq || op == BinOp::Neq || op == BinOp::Lt ||
                   op == BinOp::Le || op == BinOp::Gt || op == BinOp::Ge);
    if (is_cmp && (record_type_of_expr(a) || record_type_of_expr(b)))
        return record_compare(op, a, b);

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
        case BinOp::Concat: {
            auto sa = sort_of(a);
            auto sb = sort_of(b);
            if ((sa && sa->tag == Sort::Tag::Seq) || (sb && sb->tag == Sort::Tag::Seq))
                fail("Seq `++` (runtime concat of opaque Seq vars) is out of subset — "
                     "the Rust oracle drops it (only `⟨…⟩` literal flattening is supported)");
            sym = "str.++";
            break;
        }
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

// Fold an expression to a compile-time integer constant, mirroring the Rust
// path's `literal_range` (translate_int + Z3 simplify) restricted to literal
// arithmetic. Returns nullopt for anything not constant-foldable — the caller
// then fails honestly (a symbolic quantifier bound is out of subset).
std::optional<int64_t> Emitter::eval_const_int(const Expr &e) {
    using K = Expr::Kind;
    switch (e.kind) {
        case K::Int: return e.ival;
        case K::Binary: {
            auto a = eval_const_int(*e.children[0]);
            auto b = eval_const_int(*e.children[1]);
            if (!a || !b) return std::nullopt;
            switch (e.op) {
                case BinOp::Add: return *a + *b;
                case BinOp::Sub: return *a - *b;
                case BinOp::Mul: return *a * *b;
                case BinOp::Div: return *b != 0 ? std::optional<int64_t>(*a / *b) : std::nullopt;
                default: return std::nullopt;
            }
        }
        default: return std::nullopt;
    }
}

// `∀ v ∈ {lo..hi} : body` → conjunction over the constant range (∃ → disjunction),
// substituting the bound var with each integer literal — exactly the Rust
// translator's unroll (quant.rs `literal_range` branch). Requires constant
// integer bounds at emit time; symbolic bounds / Seq / coindexed are out of subset.
std::string Emitter::emit_quantifier(const Expr &e) {
    bool is_forall = (e.kind == Expr::Kind::Forall);
    const char *q = is_forall ? "∀" : "∃";
    if (e.names.size() != 1)
        fail(std::string(q) + " tuple binding (coindexed/edges) unsupported (single-var ranges only)");
    const std::string &var = e.names[0];
    const Expr &range = *e.children[0];
    const Expr &body = *e.children[1];

    if (range.kind != Expr::Kind::Range)
        fail(std::string(q) + " range must be a constant integer range {lo..hi} (out of subset)");
    auto lo = eval_const_int(*range.children[0]);
    auto hi = eval_const_int(*range.children[1]);
    if (!lo || !hi)
        fail(std::string(q) + " range bounds must fold to integer constants (out of subset)");

    // Bind the loop var: subst_ gives expr() the literal term, env_ gives sort_of
    // its Int sort. Save/restore so nested or sibling quantifiers reusing the name
    // don't leak (mirrors emit_match's scoped binds).
    bool had_env = env_.count(var);
    Sort saved_env = had_env ? env_[var] : Sort{};
    bool had_subst = subst_.count(var);
    std::string saved_subst = had_subst ? subst_[var] : std::string{};
    env_[var] = Sort{Sort::Tag::Int, {}};

    std::vector<std::string> clauses;
    for (int64_t i = *lo; i <= *hi; i++) {
        subst_[var] = int_lit_safe(i);
        clauses.push_back(expr(body));
    }

    if (had_subst) subst_[var] = saved_subst; else subst_.erase(var);
    if (had_env) env_[var] = saved_env; else env_.erase(var);

    // and over empty = true; or over empty = false (Z3's identities, matched).
    if (clauses.empty()) return is_forall ? "true" : "false";
    if (clauses.size() == 1) return clauses[0];
    std::string s = std::string("(") + (is_forall ? "and" : "or");
    for (auto &c : clauses) s += " " + c;
    s += ")";
    return s;
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
