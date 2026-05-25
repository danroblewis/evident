//! Byte-identical equivalence of the self-hosted Seq-concat desugar
//! (`portable::desugar::EvidentDesugar`) against the canonical Rust pass
//! (`portable::desugar::RustDesugar`, which wraps
//! `runtime::desugar::desugar_seq_concat` verbatim — the pass `load.rs`
//! actually runs).
//!
//! Two halves:
//!   1. `synthetic_battery` — hand-built raw schemas exercising every
//!      flatten shape (literal-operand concat, identifier-lookup concat,
//!      multi-operand chains, nested-in-ternary / -match / -ClaimCall,
//!      subclaim recursion) AND the no-op shapes (string concat, unbound
//!      identifier). This is the substantive proof: it's where the
//!      transform actually FIRES, so it distinguishes the two impls.
//!   2. `corpus_*` — every schema in the example/stdlib corpus, parsed RAW
//!      (pre-desugar) via `evident_runtime::parse_program`, desugared by
//!      both impls, compared. Real code barely uses Seq-concat (most `++`
//!      is string concat → no-op), so this mostly proves the Evident impl
//!      never CORRUPTS real schemas.
//!
//! Comparison is on the SHARED marshaler's `Value` tree
//! (`schema_decl_to_value`) — `SchemaDecl`/`Expr` don't derive `PartialEq`,
//! and both sides round-trip through the same encoder so any difference is
//! a genuine rewrite difference, not marshaler noise.

use std::path::Path;

use evident_runtime::ast::{BinOp, BodyItem, Expr, Keyword, Mapping, MatchArm, MatchPattern, SchemaDecl};
use evident_runtime::parse_program;
use evident_runtime::portable::desugar::{DesugarImpl, EvidentDesugar, RustDesugar};
use evident_runtime::translate::ast_encoder::schema_decl_to_value;

const STDLIB: &str = "../stdlib";

fn evident() -> EvidentDesugar {
    EvidentDesugar::new(Path::new(STDLIB)).expect("load stdlib/passes/desugar.ev")
}

/// Desugar a clone with each impl; assert the results are byte-identical
/// (via the shared marshaler). Returns the (shared) desugared form so
/// callers can additionally assert it changed / matches an expected shape.
fn assert_equiv(ev: &EvidentDesugar, raw: &SchemaDecl, what: &str) -> SchemaDecl {
    let mut a = raw.clone();
    let mut b = raw.clone();
    RustDesugar.desugar_seq_concat(&mut a);
    ev.desugar_seq_concat(&mut b);
    assert_eq!(
        schema_decl_to_value(&a),
        schema_decl_to_value(&b),
        "desugar diverged on `{what}`:\n  rust    = {:#?}\n  evident = {:#?}",
        a, b
    );
    a
}

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

// ── 1. Synthetic battery — where the transform actually fires ──

#[test]
fn synthetic_battery() {
    let ev = evident();

    // A. ⟨a⟩ ++ ⟨b⟩  →  ⟨a, b⟩
    let r = assert_equiv(&ev,
        &schema("A", vec![bind("effects", concat(seq(vec![ident("a")]), seq(vec![ident("b")])))]),
        "literal-concat");
    assert!(matches!(&r.body[0], BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, rhs))
        if matches!(rhs.as_ref(), Expr::SeqLit(items) if items.len() == 2)),
        "literal-concat should flatten to a 2-elem SeqLit: {:#?}", r.body[0]);

    // B. identifier lookup: xs = ⟨a⟩ ; effects = xs ++ ⟨b⟩  →  ⟨a, b⟩
    assert_equiv(&ev, &schema("B", vec![
        bind("xs", seq(vec![ident("a")])),
        bind("effects", concat(ident("xs"), seq(vec![ident("b")]))),
    ]), "identifier-lookup");

    // C. string concat: "." ++ trail  →  UNCHANGED (operands don't resolve)
    assert_equiv(&ev, &schema("C", vec![bind("msg", concat(str_("."), ident("trail")))]),
        "string-concat-noop");

    // D. unbound identifier: ys ++ ⟨b⟩  →  UNCHANGED (ys not gathered)
    assert_equiv(&ev, &schema("D", vec![bind("effects", concat(ident("ys"), seq(vec![ident("b")])))]),
        "unbound-identifier-noop");

    // E. concat nested in a ternary then-arm.
    assert_equiv(&ev, &schema("E", vec![bind("effects", Expr::Ternary(
        Box::new(ident("c")),
        Box::new(concat(seq(vec![ident("a")]), seq(vec![ident("b")]))),
        Box::new(seq(vec![ident("d")])),
    ))]), "ternary-arm");

    // F. concat in a ClaimCall mapping value.
    assert_equiv(&ev, &schema("F", vec![BodyItem::ClaimCall {
        name: "foo".into(),
        mappings: vec![Mapping { slot: "x".into(), value: concat(seq(vec![int(1)]), seq(vec![int(2)])) }],
    }]), "claimcall-mapping");

    // G. concat in a match arm body.
    assert_equiv(&ev, &schema("G", vec![bind("effects", Expr::Match(
        Box::new(ident("s")),
        vec![
            MatchArm {
                pattern: MatchPattern::Ctor { name: "A".into(), binds: vec![] },
                body: Box::new(concat(seq(vec![int(1)]), seq(vec![int(2)]))),
            },
            MatchArm {
                pattern: MatchPattern::Wildcard,
                body: Box::new(seq(vec![int(3)])),
            },
        ],
    ))]), "match-arm");

    // H. three-operand chain: ⟨a⟩ ++ ⟨b⟩ ++ ⟨c⟩  →  ⟨a, b, c⟩
    let r = assert_equiv(&ev, &schema("H", vec![bind("effects",
        concat(concat(seq(vec![ident("a")]), seq(vec![ident("b")])), seq(vec![ident("c")])))]),
        "three-operand-chain");
    assert!(matches!(&r.body[0], BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, rhs))
        if matches!(rhs.as_ref(), Expr::SeqLit(items) if items.len() == 3)),
        "chain should flatten to a 3-elem SeqLit: {:#?}", r.body[0]);

    // I. subclaim recursion: a concat inside a nested subclaim flattens too.
    assert_equiv(&ev, &schema("I", vec![
        BodyItem::SubclaimDecl(schema("Sub", vec![
            bind("effects", concat(seq(vec![int(1)]), seq(vec![int(2)]))),
        ])),
    ]), "subclaim-recursion");

    // J. CLAUDE.md-style named chunks: xs = ⟨a,b⟩ ; ys = ⟨c⟩ ; eff = xs ++ ys ++ ⟨d⟩
    let r = assert_equiv(&ev, &schema("J", vec![
        bind("xs", seq(vec![ident("a"), ident("b")])),
        bind("ys", seq(vec![ident("c")])),
        bind("effects", concat(concat(ident("xs"), ident("ys")), seq(vec![ident("d")]))),
    ]), "named-chunks");
    // effects (last item) must be a 4-elem SeqLit.
    let last = r.body.last().unwrap();
    assert!(matches!(last, BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, rhs))
        if matches!(rhs.as_ref(), Expr::SeqLit(items) if items.len() == 4)),
        "named-chunks should flatten to a 4-elem SeqLit: {last:#?}");

    // K. last-wins on duplicate bindings (mirrors the HashMap insert):
    //    xs = ⟨a⟩ ; xs = ⟨z⟩ ; effects = xs ++ ⟨b⟩  →  ⟨z, b⟩
    assert_equiv(&ev, &schema("K", vec![
        bind("xs", seq(vec![ident("a")])),
        bind("xs", seq(vec![ident("z")])),
        bind("effects", concat(ident("xs"), seq(vec![ident("b")]))),
    ]), "duplicate-binding-last-wins");
}

// ── 2. Corpus — both impls agree on every real schema ──

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
fn corpus_both_impls_agree() {
    let ev = evident();
    let mut files = Vec::new();
    ev_files(Path::new("../examples"), &mut files);
    ev_files(Path::new("../stdlib"), &mut files);
    files.sort();

    let mut schemas_checked = 0usize;
    for file in &files {
        let src = std::fs::read_to_string(file).unwrap();
        // Some corpus files are pass files that re-declare AST enums or use
        // constructs that only parse in a specific context; a parse failure
        // here just means "not a schema source we can desugar in isolation".
        // Desugar is structural and per-schema, so any file that parses is
        // fair game.
        let Ok(prog) = parse_program(&src) else { continue };
        for s in &prog.schemas {
            assert_equiv(&ev, s, &format!("{}::{}", file.display(), s.name));
            schemas_checked += 1;
        }
    }
    assert!(schemas_checked >= 50,
        "expected to check ≥50 corpus schemas; checked {schemas_checked}");
}
