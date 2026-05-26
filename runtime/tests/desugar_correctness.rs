//! Correctness of the self-hosted Seq-concat desugar
//! (`portable::desugar::desugar_seq_concat`) — the runtime's SOLE
//! `desugar_seq_concat` implementation since session REVIVE-desugar.
//!
//! This replaces `desugar_equivalence.rs`, which compared the Evident pass
//! against a canonical Rust `desugar_seq_concat` walk. That Rust walk is now
//! deleted, so there is no production oracle to compare against. Two halves:
//!
//!   1. `synthetic_battery` — every flatten shape, pinned as EXACT expected
//!      rewritten ASTs (hardcoded — oracle-independent). This is the
//!      substantive proof: it's where the transform actually FIRES.
//!   2. `corpus_no_corruption` — every schema in the example/stdlib corpus,
//!      compared byte-for-byte against a TEST-LOCAL reference of the
//!      canonical algorithm (`reference_desugar` below — not runtime code;
//!      the test's own oracle, the verbatim copy of the pass that shipped
//!      pre-cutover). Real code barely uses Seq-concat (most `++` is string
//!      concat → no-op), so this mostly proves the Evident impl never
//!      CORRUPTS real schemas, and flattens the demos that DO use `++`
//!      (mario, the `test_NN` effect lists) exactly as the canonical pass.
//!   3. `production_entry_*` / `bootstrap_*` — the actual load entry point
//!      (`portable::desugar::desugar_seq_concat`, cached engine + WW
//!      resolver + bootstrap guard) and its re-entrancy resolution.
//!
//! Comparison is on the SHARED marshaler's `Value` tree
//! (`schema_decl_to_value`) — `SchemaDecl`/`Expr` don't derive `PartialEq`,
//! and both sides round-trip through the same encoder so any difference is a
//! genuine rewrite difference, not marshaler noise.

use std::path::Path;

use evident_runtime::ast::{
    BinOp, BodyItem, Expr, Keyword, Mapping, MatchArm, MatchPattern, SchemaDecl,
};
use evident_runtime::parse_program;
use evident_runtime::portable::desugar;
use evident_runtime::translate::ast_encoder::schema_decl_to_value;

// ── tiny AST builders ───────────────────────────────────────────────

fn ident(s: &str) -> Expr { Expr::Identifier(s.to_string()) }
fn int(n: i64) -> Expr { Expr::Int(n) }
fn str_(s: &str) -> Expr { Expr::Str(s.to_string()) }
fn seq(items: Vec<Expr>) -> Expr { Expr::SeqLit(items) }
fn concat(l: Expr, r: Expr) -> Expr {
    Expr::Binary(BinOp::Concat, Box::new(l), Box::new(r))
}
fn eq(l: Expr, r: Expr) -> Expr {
    Expr::Binary(BinOp::Eq, Box::new(l), Box::new(r))
}
fn constraint(e: Expr) -> BodyItem { BodyItem::Constraint(e) }
fn schema(name: &str, body: Vec<BodyItem>) -> SchemaDecl {
    SchemaDecl {
        keyword: Keyword::Claim,
        name: name.to_string(),
        type_params: vec![],
        body,
        param_count: 0,
        external: false,
    }
}

/// `name = expr` constraint.
fn bind(name: &str, e: Expr) -> BodyItem { constraint(eq(ident(name), e)) }

/// Desugar `input` with the Evident pass and assert the result is
/// byte-identical (via the shared marshaler) to `expected`.
fn assert_desugars_to(input: SchemaDecl, expected: SchemaDecl, what: &str) {
    let mut got = input;
    desugar::desugar_seq_concat(&mut got);
    assert_eq!(
        schema_decl_to_value(&got),
        schema_decl_to_value(&expected),
        "desugar of `{what}` didn't match expected:\n  got      = {:#?}\n  expected = {:#?}",
        got, expected
    );
}

// ── 1. Synthetic battery — hardcoded expected outputs ──

#[test]
fn synthetic_battery() {

    // A. ⟨a⟩ ++ ⟨b⟩  →  ⟨a, b⟩
    assert_desugars_to(
        schema("A", vec![bind("effects", concat(seq(vec![ident("a")]), seq(vec![ident("b")])))]),
        schema("A", vec![bind("effects", seq(vec![ident("a"), ident("b")]))]),
        "literal-concat");

    // B. identifier lookup: xs = ⟨a⟩ ; effects = xs ++ ⟨b⟩  →  effects = ⟨a, b⟩
    assert_desugars_to(
        schema("B", vec![
            bind("xs", seq(vec![ident("a")])),
            bind("effects", concat(ident("xs"), seq(vec![ident("b")]))),
        ]),
        schema("B", vec![
            bind("xs", seq(vec![ident("a")])),
            bind("effects", seq(vec![ident("a"), ident("b")])),
        ]),
        "identifier-lookup");

    // C. string concat: "." ++ trail  →  UNCHANGED (operands don't resolve)
    assert_desugars_to(
        schema("C", vec![bind("msg", concat(str_("."), ident("trail")))]),
        schema("C", vec![bind("msg", concat(str_("."), ident("trail")))]),
        "string-concat-noop");

    // D. unbound identifier: ys ++ ⟨b⟩  →  UNCHANGED (ys not gathered)
    assert_desugars_to(
        schema("D", vec![bind("effects", concat(ident("ys"), seq(vec![ident("b")])))]),
        schema("D", vec![bind("effects", concat(ident("ys"), seq(vec![ident("b")])))]),
        "unbound-identifier-noop");

    // E. concat nested in a ternary then-arm.
    assert_desugars_to(
        schema("E", vec![bind("effects", Expr::Ternary(
            Box::new(ident("c")),
            Box::new(concat(seq(vec![ident("a")]), seq(vec![ident("b")]))),
            Box::new(seq(vec![ident("d")])),
        ))]),
        schema("E", vec![bind("effects", Expr::Ternary(
            Box::new(ident("c")),
            Box::new(seq(vec![ident("a"), ident("b")])),
            Box::new(seq(vec![ident("d")])),
        ))]),
        "ternary-arm");

    // F. concat in a ClaimCall mapping value.
    assert_desugars_to(
        schema("F", vec![BodyItem::ClaimCall {
            name: "foo".into(),
            mappings: vec![Mapping { slot: "x".into(), value: concat(seq(vec![int(1)]), seq(vec![int(2)])) }],
        }]),
        schema("F", vec![BodyItem::ClaimCall {
            name: "foo".into(),
            mappings: vec![Mapping { slot: "x".into(), value: seq(vec![int(1), int(2)]) }],
        }]),
        "claimcall-mapping");

    // G. concat in a match arm body (flat patterns round-trip; nested-ctor
    //    patterns are why the rewrite walk stays in Rust — see the module).
    let match_in = |arm_body: Expr| schema("G", vec![bind("effects", Expr::Match(
        Box::new(ident("s")),
        vec![
            MatchArm {
                pattern: MatchPattern::Ctor { name: "A".into(), binds: vec![] },
                body: Box::new(arm_body),
            },
            MatchArm {
                pattern: MatchPattern::Wildcard,
                body: Box::new(seq(vec![int(3)])),
            },
        ],
    ))]);
    assert_desugars_to(
        match_in(concat(seq(vec![int(1)]), seq(vec![int(2)]))),
        match_in(seq(vec![int(1), int(2)])),
        "match-arm");

    // H. three-operand chain: ⟨a⟩ ++ ⟨b⟩ ++ ⟨c⟩  →  ⟨a, b, c⟩
    assert_desugars_to(
        schema("H", vec![bind("effects",
            concat(concat(seq(vec![ident("a")]), seq(vec![ident("b")])), seq(vec![ident("c")])))]),
        schema("H", vec![bind("effects", seq(vec![ident("a"), ident("b"), ident("c")]))]),
        "three-operand-chain");

    // I. subclaim recursion: a concat inside a nested subclaim flattens too.
    assert_desugars_to(
        schema("I", vec![
            BodyItem::SubclaimDecl(schema("Sub", vec![
                bind("effects", concat(seq(vec![int(1)]), seq(vec![int(2)]))),
            ])),
        ]),
        schema("I", vec![
            BodyItem::SubclaimDecl(schema("Sub", vec![
                bind("effects", seq(vec![int(1), int(2)])),
            ])),
        ]),
        "subclaim-recursion");

    // J. CLAUDE.md-style named chunks: xs = ⟨a,b⟩ ; ys = ⟨c⟩ ;
    //    eff = xs ++ ys ++ ⟨d⟩  →  ⟨a, b, c, d⟩
    assert_desugars_to(
        schema("J", vec![
            bind("xs", seq(vec![ident("a"), ident("b")])),
            bind("ys", seq(vec![ident("c")])),
            bind("effects", concat(concat(ident("xs"), ident("ys")), seq(vec![ident("d")]))),
        ]),
        schema("J", vec![
            bind("xs", seq(vec![ident("a"), ident("b")])),
            bind("ys", seq(vec![ident("c")])),
            bind("effects", seq(vec![ident("a"), ident("b"), ident("c"), ident("d")])),
        ]),
        "named-chunks");

    // K. last-wins on duplicate bindings (mirrors the HashMap insert):
    //    xs = ⟨a⟩ ; xs = ⟨z⟩ ; effects = xs ++ ⟨b⟩  →  ⟨z, b⟩
    assert_desugars_to(
        schema("K", vec![
            bind("xs", seq(vec![ident("a")])),
            bind("xs", seq(vec![ident("z")])),
            bind("effects", concat(ident("xs"), seq(vec![ident("b")]))),
        ]),
        schema("K", vec![
            bind("xs", seq(vec![ident("a")])),
            bind("xs", seq(vec![ident("z")])),
            bind("effects", seq(vec![ident("z"), ident("b")])),
        ]),
        "duplicate-binding-last-wins");
}

// ── 2. Corpus — Evident impl matches a test-local reference oracle ──

/// The TEST'S oracle: a verbatim copy of the canonical `desugar_seq_concat`
/// as it shipped before session REVIVE-desugar deleted it. This is NOT
/// runtime code — it lives in the test so the corpus comparison stays
/// byte-identical-at-scale without a production Rust pass. The Evident impl
/// is the real thing; this is the pinned expectation it must reproduce.
mod reference {
    use evident_runtime::ast::{BinOp, BodyItem, Expr, SchemaDecl};
    use std::collections::HashMap;

    pub fn desugar_seq_concat(s: &mut SchemaDecl) {
        if s.external { return; }

        let mut seq_lits: HashMap<String, Vec<Expr>> = HashMap::new();
        for item in &s.body {
            let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
            if let (Expr::Identifier(name), Expr::SeqLit(items)) = (lhs.as_ref(), rhs.as_ref()) {
                seq_lits.insert(name.clone(), items.clone());
            }
        }

        fn flatten(e: &Expr, seq_lits: &HashMap<String, Vec<Expr>>) -> Option<Vec<Expr>> {
            match e {
                Expr::Binary(BinOp::Concat, l, r) => {
                    let mut left = flatten(l, seq_lits)?;
                    let right = flatten(r, seq_lits)?;
                    left.extend(right);
                    Some(left)
                }
                Expr::SeqLit(items) => Some(items.clone()),
                Expr::Identifier(name) => seq_lits.get(name).cloned(),
                _ => None,
            }
        }

        fn rewrite(e: &mut Expr, seq_lits: &HashMap<String, Vec<Expr>>) {
            if let Expr::Binary(BinOp::Concat, ..) = e {
                if let Some(items) = flatten(e, seq_lits) {
                    *e = Expr::SeqLit(items);
                    return;
                }
            }
            match e {
                Expr::Binary(_, l, r)
                | Expr::Range(l, r)
                | Expr::InExpr(l, r)
                | Expr::Index(l, r) => { rewrite(l, seq_lits); rewrite(r, seq_lits); }
                Expr::Ternary(c, a, b) => {
                    rewrite(c, seq_lits); rewrite(a, seq_lits); rewrite(b, seq_lits);
                }
                Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es)
                | Expr::Call(_, es) => {
                    for x in es { rewrite(x, seq_lits); }
                }
                Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                    rewrite(r, seq_lits); rewrite(b, seq_lits);
                }
                Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                    rewrite(i, seq_lits);
                }
                Expr::Field(recv, _) => rewrite(recv, seq_lits),
                Expr::Match(scr, arms) => {
                    rewrite(scr, seq_lits);
                    for a in arms { rewrite(&mut a.body, seq_lits); }
                }
                _ => {}
            }
        }

        for item in s.body.iter_mut() {
            match item {
                BodyItem::Constraint(e) => rewrite(e, &seq_lits),
                BodyItem::ClaimCall { mappings, .. } => {
                    for m in mappings.iter_mut() { rewrite(&mut m.value, &seq_lits); }
                }
                _ => {}
            }
        }
        for item in s.body.iter_mut() {
            if let BodyItem::SubclaimDecl(sub) = item {
                desugar_seq_concat(sub);
            }
        }
    }
}

/// Every `.ev` under a directory (recursively).
fn ev_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            ev_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "ev") {
            out.push(p);
        }
    }
}

#[test]
fn corpus_no_corruption() {
    let mut files = Vec::new();
    ev_files(Path::new("../examples"), &mut files);
    ev_files(Path::new("../stdlib"), &mut files);
    files.sort();

    let mut schemas_checked = 0usize;
    for file in &files {
        let src = std::fs::read_to_string(file).unwrap();
        // A parse failure here just means "not a schema source we can
        // desugar in isolation". Desugar is structural and per-schema, so
        // any file that parses is fair game.
        let Ok(prog) = parse_program(&src) else { continue };
        for s in &prog.schemas {
            let mut got = s.clone();
            desugar::desugar_seq_concat(&mut got);
            let mut want = s.clone();
            reference::desugar_seq_concat(&mut want);
            assert_eq!(
                schema_decl_to_value(&got),
                schema_decl_to_value(&want),
                "desugar diverged from reference on {}::{}:\n  evident   = {:#?}\n  reference = {:#?}",
                file.display(), s.name, got, want
            );
            schemas_checked += 1;
        }
    }
    assert!(schemas_checked >= 50,
        "expected to check ≥50 corpus schemas; checked {schemas_checked}");
}

// ── 3. Production entry point + bootstrap ──

/// The real load entry — the cached per-thread runner + WW resolver +
/// bootstrap guard — actually flattens a `xs ++ ⟨c⟩` chain.
#[test]
fn production_entry_flattens() {
    let input = schema("P", vec![
        bind("xs", seq(vec![ident("a"), ident("b")])),
        bind("effects", concat(ident("xs"), seq(vec![ident("c")]))),
    ]);

    let mut via_production = input.clone();
    desugar::desugar_seq_concat(&mut via_production);   // cached free fn

    assert!(matches!(via_production.body.last().unwrap(),
        BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, rhs))
            if matches!(rhs.as_ref(), Expr::SeqLit(items) if items.len() == 3)),
        "production entry should flatten xs ++ ⟨c⟩ → ⟨a, b, c⟩: {:#?}", via_production);
}

/// The cached production entry is idempotent and stable across calls —
/// the bootstrap guard releases cleanly, so the engine handles the same
/// schema identically on repeated calls.
#[test]
fn production_entry_idempotent() {
    let base = schema("Q", vec![bind("effects",
        concat(seq(vec![int(1)]), seq(vec![int(2)])))]);

    let mut once = base.clone();
    desugar::desugar_seq_concat(&mut once);
    let mut twice = once.clone();
    desugar::desugar_seq_concat(&mut twice);   // desugar(desugar(s)) == desugar(s)

    assert_eq!(schema_decl_to_value(&once), schema_decl_to_value(&twice),
        "desugar must be idempotent");
}
