// Seed unit/integration tests — no framework, just asserts + a pass counter.
// Covers lexer, parser, SMT-LIB emit, and end-to-end Z3 solve on the subset.
// Source is UTF-8; Evident's Unicode operators appear literally below.

#include <cstdio>
#include <string>

#include "lexer.h"
#include "parser.h"
#include "smtlib.h"
#include "solve.h"

using namespace evc;

static int g_failures = 0;
static int g_checks = 0;

#define CHECK(cond, msg) do { \
    g_checks++; \
    if (!(cond)) { g_failures++; std::printf("FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); } \
} while (0)

static SolveResult run(const std::string &src, const std::string &claim) {
    Program prog = parse(src);
    const SchemaDecl *s = nullptr;
    for (const auto &d : prog.schemas) if (d.name == claim) s = &d;
    if (!s) throw std::runtime_error("claim not found: " + claim);
    return solve(emit_schema(*s, prog));
}

static void test_lexer() {
    auto toks = tokenize("claim T\n    n ∈ Nat\n    n > 5\n");
    CHECK(toks[0].kind == Token::Kind::Indent, "lex: first is indent");
    CHECK(toks[1].kind == Token::Kind::Claim, "lex: claim keyword");
    CHECK(toks[2].kind == Token::Kind::Ident && toks[2].str == "T", "lex: schema name");

    auto u = tokenize("a ∈ Set ∧ b ≤ 5 ⇒ ¬c");
    std::vector<Token::Kind> kinds;
    for (auto &t : u) if (t.kind != Token::Kind::Indent) kinds.push_back(t.kind);
    CHECK(kinds[1] == Token::Kind::In, "lex: in");
    CHECK(kinds[3] == Token::Kind::And, "lex: and");
    CHECK(kinds[5] == Token::Kind::Le, "lex: le");
    CHECK(kinds[7] == Token::Kind::Implies, "lex: implies");
    CHECK(kinds[8] == Token::Kind::Not, "lex: not");
}

static void test_parser() {
    Program p = parse("claim T\n    n ∈ Nat\n    n > 5\n");
    CHECK(p.schemas.size() == 1, "parse: one schema");
    CHECK(p.schemas[0].name == "T", "parse: name T");
    CHECK(p.schemas[0].body.size() == 2, "parse: 2 body items");
    CHECK(p.schemas[0].body[0].kind == BodyItem::Kind::Membership, "parse: membership");
    CHECK(p.schemas[0].body[1].kind == BodyItem::Kind::Constraint, "parse: constraint");
}

static void test_emit() {
    Program p = parse("claim T\n    n ∈ Nat\n    n > 5\n");
    std::string text = schema_to_smtlib(p.schemas[0], p);
    CHECK(text.find("(declare-const n Int)") != std::string::npos, "emit: declare n");
    CHECK(text.find("(assert (>= n 0))") != std::string::npos, "emit: Nat bound");
    CHECK(text.find("(assert (> n 5))") != std::string::npos, "emit: n>5");
}

static void test_solve_sat() {
    auto r = run("claim T\n    n ∈ Nat\n    n > 5\n", "T");
    CHECK(r.satisfied, "solve: n>5 sat");
    bool found = false;
    for (auto &[k, v] : r.bindings) if (k == "n") { found = true; CHECK(v.i > 5, "solve: n>5 model"); }
    CHECK(found, "solve: n bound present");
}

static void test_solve_unsat() {
    auto r = run("claim T\n    n ∈ Int\n    n > 10\n    n < 3\n", "T");
    CHECK(!r.satisfied, "solve: n>10 and n<3 unsat");
}

static void test_solve_bool_implies() {
    auto r = run("claim T\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n", "T");
    CHECK(r.satisfied, "solve: bool implies sat");
    for (auto &[k, v] : r.bindings) if (k == "q") CHECK(v.b == true, "solve: q forced true");
}

static void test_solve_set_membership() {
    auto r = run("claim T\n    m ∈ Int\n    m ∈ {2, 4, 6}\n    m > 3\n", "T");
    CHECK(r.satisfied, "solve: set membership sat");
    for (auto &[k, v] : r.bindings) if (k == "m") CHECK(v.i == 4 || v.i == 6, "solve: m in {4,6}");
}

static void test_solve_real() {
    auto r = run("claim T\n    x ∈ Real\n    x + x = 3.0\n", "T");
    CHECK(r.satisfied, "solve: real sat");
    for (auto &[k, v] : r.bindings) if (k == "x") CHECK(v.r > 1.49 && v.r < 1.51, "solve: x=1.5");
}

static void test_chained_membership() {
    // 0 < x ∈ Int < 5  declares x and bounds it
    auto r = run("claim T\n    0 < x ∈ Int < 5\n", "T");
    CHECK(r.satisfied, "solve: chained membership sat");
    for (auto &[k, v] : r.bindings) if (k == "x") CHECK(v.i > 0 && v.i < 5, "solve: 0<x<5");
}

static void test_enum_nullary() {
    auto r = run("enum Color = Red | Green | Blue\nclaim T\n    c ∈ Color\n    c = Green\n", "T");
    CHECK(r.satisfied, "enum: nullary sat");
    for (auto &[k, v] : r.bindings) if (k == "c")
        CHECK(v.tag == Value::Tag::Enum && v.s == "Green", "enum: c=Green");
}

static void test_enum_payload_ctor() {
    auto r = run("enum Result = Ok(Int) | Err(String)\nclaim T\n    r ∈ Result\n    r = Ok(7)\n", "T");
    CHECK(r.satisfied, "enum: payload ctor sat");
    for (auto &[k, v] : r.bindings) if (k == "r")
        CHECK(v.tag == Value::Tag::Enum && v.s == "Ok(7)", "enum: r=Ok(7)");
}

static void test_enum_match_extract() {
    auto r = run("enum Result = Ok(Int) | Err(String)\nclaim T\n    r ∈ Result\n    n ∈ Int\n"
                 "    r = Ok(42)\n    n = match r\n        Ok(v) ⇒ v\n        Err(s) ⇒ 0\n", "T");
    CHECK(r.satisfied, "enum: match sat");
    for (auto &[k, v] : r.bindings) if (k == "n") CHECK(v.i == 42, "enum: match extracts 42");
}

static void test_enum_matches_recognizer() {
    auto r = run("enum Result = Ok(Int) | Err(String)\nclaim T\n    r ∈ Result\n    b ∈ Bool\n"
                 "    r = Ok(1)\n    b = (r matches Ok(_))\n", "T");
    CHECK(r.satisfied, "enum: matches sat");
    for (auto &[k, v] : r.bindings) if (k == "b") CHECK(v.b == true, "enum: matches true");
}

static void test_enum_unsat() {
    auto r = run("enum Color = Red | Green | Blue\nclaim T\n    c ∈ Color\n    c = Red\n    c = Blue\n", "T");
    CHECK(!r.satisfied, "enum: two distinct nullary unsat");
}

static void test_out_of_subset_reported() {
    bool threw = false;
    try {
        Program p = parse("claim T\n    xs ∈ Seq(Int)\n");
        schema_to_smtlib(p.schemas[0], p);
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "emit: Seq(Int) rejected as out of subset");
}

int main() {
    try {
        test_lexer();
        test_parser();
        test_emit();
        test_solve_sat();
        test_solve_unsat();
        test_solve_bool_implies();
        test_solve_set_membership();
        test_solve_real();
        test_chained_membership();
        test_enum_nullary();
        test_enum_payload_ctor();
        test_enum_match_extract();
        test_enum_matches_recognizer();
        test_enum_unsat();
        test_out_of_subset_reported();
    } catch (const std::exception &e) {
        std::printf("EXCEPTION: %s\n", e.what());
        return 1;
    }
    std::printf("%d checks, %d failures\n", g_checks, g_failures);
    return g_failures == 0 ? 0 : 1;
}
