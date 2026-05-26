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

static void test_forall_unroll() {
    // ∀ i ∈ {1..3} : n > i  forces n ≥ 4.
    auto r = run("claim T\n    n ∈ Int\n    ∀ i ∈ {1..3} : n > i\n", "T");
    CHECK(r.satisfied, "forall: unroll sat");
    for (auto &[k, v] : r.bindings) if (k == "n") CHECK(v.i > 3, "forall: n>3");
}

static void test_forall_singleton_forced() {
    auto r = run("claim T\n    n ∈ Int\n    ∀ i ∈ {3..3} : n = i\n", "T");
    CHECK(r.satisfied, "forall: singleton sat");
    for (auto &[k, v] : r.bindings) if (k == "n") CHECK(v.i == 3, "forall: n=3 forced");
}

static void test_forall_unsat() {
    // i > 2 is false at i=0,1,2 → asserting ∀ is UNSAT.
    auto r = run("claim T\n    ∀ i ∈ {0..4} : i > 2\n", "T");
    CHECK(!r.satisfied, "forall: false-in-range unsat");
}

static void test_exists_unroll() {
    auto r = run("claim T\n    n ∈ Int\n    ∃ i ∈ {0..5} : n = i * i\n", "T");
    CHECK(r.satisfied, "exists: square sat");
    for (auto &[k, v] : r.bindings) if (k == "n") {
        bool sq = false;
        for (int i = 0; i <= 5; i++) if (v.i == (int64_t)i * i) sq = true;
        CHECK(sq, "exists: n is a square in [0,5]");
    }
}

static void test_exists_empty_unsat() {
    // {5..1} folds to an empty range → ∃ is false → UNSAT.
    auto r = run("claim T\n    n ∈ Int\n    ∃ i ∈ {5..1} : n = i\n", "T");
    CHECK(!r.satisfied, "exists: empty range unsat");
}

static void test_forall_symbolic_bound_rejected() {
    // A bound that doesn't fold to a constant is out of subset.
    bool threw = false;
    try {
        run("claim T\n    n ∈ Int\n    m ∈ Int\n    ∀ i ∈ {0..m} : n > i\n", "T");
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "forall: symbolic bound rejected as out of subset");
}

static void test_record_field_access() {
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    v ∈ IVec2\n    v.x = 3\n    v.y = 4\n"
                 "    s ∈ Int\n    s = v.x + v.y\n", "T");
    CHECK(r.satisfied, "record: field access sat");
    for (auto &[k, v] : r.bindings) {
        if (k == "v.x") CHECK(v.i == 3, "record: v.x=3");
        if (k == "s")   CHECK(v.i == 7, "record: s=7");
    }
}

static void test_record_eq_lift() {
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    a ∈ IVec2\n    b ∈ IVec2\n    a = b\n"
                 "    a.x = 5\n    bx ∈ Int\n    bx = b.x\n", "T");
    CHECK(r.satisfied, "record: eq lift sat");
    for (auto &[k, v] : r.bindings) if (k == "bx") CHECK(v.i == 5, "record: a=b propagates a.x to b.x");
}

static void test_record_eq_conflict_unsat() {
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    a ∈ IVec2\n    b ∈ IVec2\n    a = b\n"
                 "    a.x = 1\n    b.x = 2\n", "T");
    CHECK(!r.satisfied, "record: eq with conflicting field unsat");
}

static void test_record_bounding_box() {
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    p ∈ IVec2\n    lo ∈ IVec2(0, 0)\n"
                 "    hi ∈ IVec2(10, 10)\n    lo ≤ p ≤ hi\n    p.y = 20\n", "T");
    CHECK(!r.satisfied, "record: out-of-box unsat (chain lift)");
}

static void test_record_literal() {
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    p ∈ IVec2\n    p = IVec2(11, 22)\n"
                 "    s ∈ Int\n    s = p.x + p.y\n", "T");
    CHECK(r.satisfied, "record: literal eq sat");
    for (auto &[k, v] : r.bindings) if (k == "s") CHECK(v.i == 33, "record: literal s=33");
}

static void test_record_nested() {
    auto r = run("type IVec2(x, y ∈ Int)\ntype Player(pos ∈ IVec2)\nclaim T\n    pl ∈ Player\n"
                 "    pl.pos.x = 11\n    pl.pos.y = 22\n    s ∈ Int\n    s = pl.pos.x + pl.pos.y\n", "T");
    CHECK(r.satisfied, "record: nested sat");
    for (auto &[k, v] : r.bindings) if (k == "s") CHECK(v.i == 33, "record: nested s=33");
}

static void test_record_arith_broadcast() {
    // c = a + b lifts per-axis; c = a * 5 / 2 broadcasts the scalar.
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    a ∈ IVec2(3, 4)\n    b ∈ IVec2(10, 20)\n"
                 "    c ∈ IVec2\n    c = a + b\n    sx ∈ Int\n    sx = c.x\n    sy ∈ Int\n    sy = c.y\n", "T");
    CHECK(r.satisfied, "record: arith broadcast sat");
    for (auto &[k, v] : r.bindings) {
        if (k == "sx") CHECK(v.i == 13, "record: c.x = 13");
        if (k == "sy") CHECK(v.i == 24, "record: c.y = 24");
    }
}

static void test_record_scalar_broadcast_intdiv() {
    auto r = run("type IVec2(x, y ∈ Int)\nclaim T\n    a ∈ IVec2(6, 8)\n    c ∈ IVec2\n"
                 "    c = a * 5 / 2\n    sx ∈ Int\n    sx = c.x\n    sy ∈ Int\n    sy = c.y\n", "T");
    CHECK(r.satisfied, "record: scalar broadcast sat");
    for (auto &[k, v] : r.bindings) {
        if (k == "sx") CHECK(v.i == 15, "record: 6*5/2 = 15");  // (6*5) div 2
        if (k == "sy") CHECK(v.i == 20, "record: 8*5/2 = 20");
    }
}

static void test_record_ternary_rejected() {
    // Record-valued ternary is out of subset (the Rust oracle drops it).
    bool threw = false;
    try {
        run("type IVec2(x, y ∈ Int)\nclaim T\n    a ∈ IVec2(1, 2)\n    b ∈ IVec2(9, 9)\n"
            "    f ∈ Bool\n    f = true\n    c ∈ IVec2\n    c = (f ? a : b)\n", "T");
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "record: record-valued ternary rejected as out of subset");
}

static void test_record_local_invariant() {
    // A refined record's local invariant (lo ≤ hi) is instantiated per instance,
    // rebound to the instance's leaves. Valid range → SAT.
    auto ok = run("type Rng(lo, hi ∈ Int)\n    lo ≤ hi\nclaim T\n    d ∈ Rng\n    d.lo = 3\n    d.hi = 5\n", "T");
    CHECK(ok.satisfied, "record: refined type valid range sat");
    // Violating the invariant → UNSAT (the rebinding actually constrains d).
    auto bad = run("type Rng(lo, hi ∈ Int)\n    lo ≤ hi\nclaim T\n    d ∈ Rng\n    d.lo = 5\n    d.hi = 3\n", "T");
    CHECK(!bad.satisfied, "record: refined type violated invariant unsat");
}

static void test_seq_elems() {
    auto r = run("claim T\n    xs ∈ Seq(Int)\n    #xs = 3\n    xs[0] = 10\n    xs[1] = 20\n    xs[2] = 30\n"
                 "    s ∈ Int\n    s = xs[0] + xs[1] + xs[2]\n", "T");
    CHECK(r.satisfied, "seq: elems sat");
    for (auto &[k, v] : r.bindings) {
        if (k == "s")  CHECK(v.i == 60, "seq: s = 60");
        if (k == "xs") CHECK(v.tag == Value::Tag::Seq && v.s == "[10, 20, 30]", "seq: xs = [10, 20, 30]");
    }
}

static void test_seq_len() {
    auto r = run("claim T\n    xs ∈ Seq(Int)\n    #xs = 5\n    n ∈ Int\n    n = #xs\n", "T");
    CHECK(r.satisfied, "seq: len sat");
    for (auto &[k, v] : r.bindings) if (k == "n") CHECK(v.i == 5, "seq: #xs = 5");
}

static void test_seq_len_conflict_unsat() {
    auto r = run("claim T\n    xs ∈ Seq(Int)\n    #xs = 3\n    #xs = 4\n", "T");
    CHECK(!r.satisfied, "seq: conflicting lengths unsat");
}

static void test_seq_elem_conflict_unsat() {
    auto r = run("claim T\n    xs ∈ Seq(Int)\n    #xs = 2\n    xs[0] = 1\n    xs[0] = 2\n", "T");
    CHECK(!r.satisfied, "seq: one index two values unsat");
}

static void test_bool_seq() {
    auto r = run("claim T\n    bs ∈ Seq(Bool)\n    #bs = 2\n    bs[0] = true\n    bs[1] = false\n", "T");
    CHECK(r.satisfied, "seq: bool seq sat");
    for (auto &[k, v] : r.bindings) if (k == "bs")
        CHECK(v.tag == Value::Tag::Seq && v.s == "[true, false]", "seq: bs = [true, false]");
}

static void test_seq_concat_rejected() {
    // Runtime concat of opaque Seq vars is out of subset (the oracle drops it).
    bool threw = false;
    try {
        run("claim T\n    a ∈ Seq(Int)\n    b ∈ Seq(Int)\n    c ∈ Seq(Int)\n    c = a ++ b\n", "T");
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "seq: ++ runtime concat rejected as out of subset");
}

static void test_seq_forall() {
    // ∀ x ∈ xs : x > 0 unrolls over the pinned length; pinning xs[0]=-1 → UNSAT.
    auto ok = run("claim T\n    xs ∈ Seq(Int)\n    #xs = 3\n    ∀ x ∈ xs : x > 0\n    xs[0] = 5\n", "T");
    CHECK(ok.satisfied, "seq forall: all-positive sat");
    auto bad = run("claim T\n    xs ∈ Seq(Int)\n    #xs = 2\n    ∀ x ∈ xs : x > 0\n    xs[0] = -1\n", "T");
    CHECK(!bad.satisfied, "seq forall: pinned negative unsat");
}

static void test_seq_forall_unpinned_rejected() {
    // ∀ x ∈ xs without a pinned length can't unroll — out of subset.
    bool threw = false;
    try {
        run("claim T\n    xs ∈ Seq(Int)\n    ∀ x ∈ xs : x > 0\n", "T");
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "seq forall: unpinned length rejected as out of subset");
}

static void test_seq_real_rejected() {
    // Seq(Real) isn't in the oracle's element set — out of subset.
    bool threw = false;
    try {
        run("claim T\n    rs ∈ Seq(Real)\n    #rs = 1\n    rs[0] = 1.5\n", "T");
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "seq: Seq(Real) rejected as out of subset");
}

// M5 — the seed runs an Evident program that is itself a transform. The pass
// logic lives in the .ev source (match + ctor construction); the seed reflects
// the pinned input AST, solves, and reifies `output`. Asserting the output AST
// proves the self-hosting round-trip end-to-end.
static const char *PASS_PRELUDE = "enum Expr = Lit(Int) | Add(Expr, Expr) | Mul(Expr, Expr)\n";

static std::string run_pass_output(const std::string &claim_src, const std::string &claim) {
    auto r = run(std::string(PASS_PRELUDE) + claim_src, claim);
    for (auto &[k, v] : r.bindings) if (k == "output") return v.s;
    return "<no output>";
}

static void test_pass_identity() {
    std::string out = run_pass_output(
        "claim P\n    input ∈ Expr\n    output ∈ Expr\n"
        "    input = Add(Lit(2), Mul(Lit(3), Lit(4)))\n    output = input\n", "P");
    CHECK(out == "Add(Lit(2), Mul(Lit(3), Lit(4)))", "M5: identity reifies input round-trip");
}

static void test_pass_swap() {
    std::string out = run_pass_output(
        "claim P\n    input ∈ Expr\n    output ∈ Expr\n"
        "    input = Add(Lit(1), Mul(Lit(7), Lit(8)))\n"
        "    output = match input\n        Add(a, b) ⇒ Add(b, a)\n        _ ⇒ input\n", "P");
    CHECK(out == "Add(Mul(Lit(7), Lit(8)), Lit(1))", "M5: swap rewrites top Add operands");
}

static void test_pass_mul_to_add() {
    std::string out = run_pass_output(
        "claim P\n    input ∈ Expr\n    output ∈ Expr\n"
        "    input = Mul(Lit(5), Add(Lit(6), Lit(7)))\n"
        "    output = match input\n        Mul(a, b) ⇒ Add(a, b)\n        _ ⇒ input\n", "P");
    CHECK(out == "Add(Lit(5), Add(Lit(6), Lit(7)))", "M5: Mul lowered to Add");
}

static void test_out_of_subset_reported() {
    // Set(Int) has no lowering in the seed yet — must be reported, not mis-emitted.
    bool threw = false;
    try {
        Program p = parse("claim T\n    xs ∈ Set(Int)\n");
        schema_to_smtlib(p.schemas[0], p);
    } catch (const SmtError &) { threw = true; }
    CHECK(threw, "emit: Set(Int) rejected as out of subset");
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
        test_forall_unroll();
        test_forall_singleton_forced();
        test_forall_unsat();
        test_exists_unroll();
        test_exists_empty_unsat();
        test_forall_symbolic_bound_rejected();
        test_record_field_access();
        test_record_eq_lift();
        test_record_eq_conflict_unsat();
        test_record_bounding_box();
        test_record_literal();
        test_record_nested();
        test_record_arith_broadcast();
        test_record_scalar_broadcast_intdiv();
        test_record_ternary_rejected();
        test_record_local_invariant();
        test_seq_elems();
        test_seq_len();
        test_seq_len_conflict_unsat();
        test_seq_elem_conflict_unsat();
        test_bool_seq();
        test_seq_forall();
        test_seq_forall_unpinned_rejected();
        test_seq_concat_rejected();
        test_seq_real_rejected();
        test_pass_identity();
        test_pass_swap();
        test_pass_mul_to_add();
        test_out_of_subset_reported();
    } catch (const std::exception &e) {
        std::printf("EXCEPTION: %s\n", e.what());
        return 1;
    }
    std::printf("%d checks, %d failures\n", g_checks, g_failures);
    return g_failures == 0 ? 0 : 1;
}
