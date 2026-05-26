// Evident AST — a C++ mirror of runtime/src/core/ast.rs, restricted to the
// subset this seed runtime parses + emits. The shape tracks the Rust AST so a
// reader of both sees the same node names; nodes outside the implemented subset
// (Match/Matches/RunFsm) are present in the enum but only partially populated.
#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace evc {

// Four keywords collapse to one decl node, exactly as in Rust. `Fsm` is the
// sole FSM signal (kept for fidelity; the seed does not run a scheduler yet).
enum class Keyword { Schema, Claim, Type, Subclaim, Fsm };

enum class BinOp {
    Eq, Neq, Lt, Le, Gt, Ge,   // comparisons -> Bool
    And, Or, Implies,          // logic
    Add, Sub, Mul, Div,        // arithmetic
    Concat,                    // ++ string concat
};

struct Expr;
using ExprPtr = std::shared_ptr<Expr>;

// Match pattern (parsed for M4 enums; Ctor/Bind/Wildcard mirror Rust).
struct MatchPattern {
    enum class Kind { Ctor, Bind, Wildcard } kind = Kind::Wildcard;
    std::string name;                  // Ctor name or Bind name
    std::vector<MatchPattern> binds;   // sub-patterns for Ctor payload
};

struct MatchArm {
    MatchPattern pattern;
    ExprPtr body;
};

// One expression node. `kind` selects which payload fields are meaningful;
// `children` is the generic operand/element/arg list (layout documented per kind
// in parser.cpp and smtlib.cpp). One struct keeps the AST easy to grow.
struct Expr {
    enum class Kind {
        Ident, Int, Real, Bool, Str,
        SetLit, SeqLit, Range, In, Tuple,
        Forall, Exists, Call, Card, Index, Field,
        Binary, Not, Ternary, Match, Matches,
    } kind;

    std::string str;            // Ident/Field name, Str value, Call name
    int64_t ival = 0;           // Int
    double rval = 0.0;          // Real
    bool bval = false;          // Bool
    BinOp op = BinOp::Eq;       // Binary

    std::vector<ExprPtr> children;     // operands / elements / args / [range lo,hi] / [in lhs,rhs]
    std::vector<std::string> names;    // quantifier bound vars
    std::vector<MatchArm> arms;        // Match arms
    MatchPattern pattern;              // Matches recognizer pattern

    explicit Expr(Kind k) : kind(k) {}
};

// ---- convenience constructors (used by parser) ----
inline ExprPtr mk(Expr::Kind k) { return std::make_shared<Expr>(k); }
inline ExprPtr mkIdent(std::string n) { auto e = mk(Expr::Kind::Ident); e->str = std::move(n); return e; }
inline ExprPtr mkInt(int64_t v) { auto e = mk(Expr::Kind::Int); e->ival = v; return e; }
inline ExprPtr mkReal(double v) { auto e = mk(Expr::Kind::Real); e->rval = v; return e; }
inline ExprPtr mkBool(bool v) { auto e = mk(Expr::Kind::Bool); e->bval = v; return e; }
inline ExprPtr mkStr(std::string s) { auto e = mk(Expr::Kind::Str); e->str = std::move(s); return e; }
inline ExprPtr mkBinary(BinOp op, ExprPtr a, ExprPtr b) {
    auto e = mk(Expr::Kind::Binary); e->op = op;
    e->children.push_back(std::move(a)); e->children.push_back(std::move(b));
    return e;
}
inline ExprPtr mkNot(ExprPtr a) { auto e = mk(Expr::Kind::Not); e->children.push_back(std::move(a)); return e; }

// ---- declarations ----
struct Mapping { std::string slot; ExprPtr value; };

struct Pins {
    enum class Kind { None, Named, Positional } kind = Kind::None;
    std::vector<Mapping> named;
    std::vector<ExprPtr> positional;
};

struct SchemaDecl;

struct BodyItem {
    enum class Kind { Membership, Passthrough, Subclaim, ClaimCall, Constraint, HaltsWithin } kind;

    // Membership
    std::string name;
    std::string type_name;
    Pins pins;

    // Constraint
    ExprPtr expr;

    // ClaimCall (name reused) / mappings
    std::vector<Mapping> mappings;

    // Subclaim
    std::shared_ptr<SchemaDecl> subclaim;

    // HaltsWithin
    std::string fsm_name;
    int64_t n = 0;

    explicit BodyItem(Kind k) : kind(k) {}
};

struct SchemaDecl {
    Keyword keyword = Keyword::Claim;
    std::string name;
    std::vector<std::string> type_params;
    std::vector<BodyItem> body;
    size_t param_count = 0;
    bool external = false;
};

struct EnumField { std::string name; std::string type_name; };
struct EnumVariant { std::string name; std::vector<EnumField> fields; };
struct EnumDecl { std::string name; std::vector<EnumVariant> variants; };

struct Program {
    std::vector<SchemaDecl> schemas;
    std::vector<std::string> imports;
    std::vector<EnumDecl> enums;
};

}  // namespace evc
