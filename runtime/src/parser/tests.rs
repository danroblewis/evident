use super::*;

#[test]
fn parse_simple_nat() {
    let p = parse("schema SimpleNat\n    n ∈ Nat\n    n > 5\n").unwrap();
    assert_eq!(p.schemas.len(), 1);
    let s = &p.schemas[0];
    assert_eq!(s.name, "SimpleNat");
    assert!(matches!(s.keyword, Keyword::Schema));
    assert_eq!(s.body.len(), 2);
    assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
        if name == "n" && type_name == "Nat"));
    assert!(matches!(&s.body[1], BodyItem::Constraint(_)));
}

#[test]
fn parse_cardinality_and_index() {
    let p = parse("schema S\n    s ∈ Seq(Int)\n    #s = 3\n    s[0] > 0\n").unwrap();
    let s = &p.schemas[0];
    assert_eq!(s.body.len(), 3);
    assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
        if name == "s" && type_name == "Seq(Int)"));
    // #s = 3
    match &s.body[1] {
        BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, _)) => {
            assert!(matches!(lhs.as_ref(), Expr::Cardinality(_)));
        }
        other => panic!("expected #s = 3 constraint, got {:?}", other),
    }
    // s[0] > 0
    match &s.body[2] {
        BodyItem::Constraint(Expr::Binary(BinOp::Gt, lhs, _)) => {
            assert!(matches!(lhs.as_ref(), Expr::Index(_, _)));
        }
        other => panic!("expected s[0] > 0 constraint, got {:?}", other),
    }
}

#[test]
fn parse_arithmetic_constraint() {
    // n > 5 + 3 * 2  →  n > (5 + (3 * 2))
    let p = parse("schema X\n    n ∈ Nat\n    n > 5 + 3 * 2\n").unwrap();
    let s = &p.schemas[0];
    let constraint = match &s.body[1] {
        BodyItem::Constraint(e) => e,
        _ => panic!(),
    };
    // Top should be a > comparison; right side should be 5 + (3*2)
    match constraint {
        Expr::Binary(BinOp::Gt, _, rhs) => match rhs.as_ref() {
            Expr::Binary(BinOp::Add, _, r2) => match r2.as_ref() {
                Expr::Binary(BinOp::Mul, _, _) => {}
                other => panic!("expected Mul on rhs, got {:?}", other),
            }
            other => panic!("expected Add at top, got {:?}", other),
        }
        other => panic!("expected Gt, got {:?}", other),
    }
}

#[test]
fn parse_chained_membership_two_sided() {
    // `0 < pos_x ∈ Int < 5` → Membership + Constraint(0 < pos_x) + Constraint(pos_x < 5).
    let p = parse("claim t\n    0 < pos_x ∈ Int < 5\n").unwrap();
    let s = &p.schemas[0];
    assert_eq!(s.body.len(), 3, "expected 3 body items, got {}", s.body.len());
    assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
        if name == "pos_x" && type_name == "Int"));
    assert!(matches!(&s.body[1], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
    assert!(matches!(&s.body[2], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
}

#[test]
fn parse_chained_membership_pin_form() {
    // `pos_x ∈ Int = 5` desugars to Membership + Constraint(=).
    let p = parse("claim t\n    pos_x ∈ Int = 5\n").unwrap();
    let s = &p.schemas[0];
    assert_eq!(s.body.len(), 2);
    assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
        if name == "pos_x" && type_name == "Int"));
    assert!(matches!(&s.body[1], BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, _))));
}

#[test]
fn parse_chained_membership_compound_type() {
    // No comparison after type, so chained-membership doesn't trigger; regular path.
    let p = parse("claim t\n    s ∈ Seq(Int)\n    #s = 3\n").unwrap();
    let s = &p.schemas[0];
    assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
        if name == "s" && type_name == "Seq(Int)"));
}

#[test]
fn parse_chained_membership_does_not_eat_set_membership() {
    // `x ∈ pts ∧ x > 0` — `∧` after Ident isn't a comparison op;
    // chain detector bails and parses it as `(x ∈ pts) ∧ (x > 0)`.
    let p = parse("claim t\n    pts ∈ Set(Int)\n    x ∈ Int\n    x ∈ pts ∧ x > 0\n").unwrap();
    let s = &p.schemas[0];
    match s.body.last().unwrap() {
        BodyItem::Constraint(Expr::Binary(BinOp::And, _, _)) => {}
        other => panic!("expected `(x ∈ pts) ∧ (x > 0)` constraint, got {:?}", other),
    }
}

#[test]
fn parse_chained_membership_multi_name() {
    // `x, y, z ∈ Int < 5` → 3 Memberships + 3 Constraints.
    let p = parse("claim t\n    x, y, z ∈ Int < 5\n").unwrap();
    let s = &p.schemas[0];
    assert_eq!(s.body.len(), 6, "expected 3 Memberships + 3 Constraints");
    for (i, name) in ["x", "y", "z"].iter().enumerate() {
        assert!(matches!(&s.body[i], BodyItem::Membership { name: n, type_name, .. }
            if n == *name && type_name == "Int"));
    }
    for i in 3..6 {
        assert!(matches!(&s.body[i], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
    }
}

#[test]
fn parse_chained_membership_multi_name_two_sided() {
    // `0 < x, y ∈ Int < 5` → 2 Memberships + 4 Constraints (lower + upper per name).
    let p = parse("claim t\n    0 < x, y ∈ Int < 5\n").unwrap();
    let s = &p.schemas[0];
    assert_eq!(s.body.len(), 6);
    // First two are Memberships
    assert!(matches!(&s.body[0], BodyItem::Membership { name, .. } if name == "x"));
    assert!(matches!(&s.body[1], BodyItem::Membership { name, .. } if name == "y"));
    // Next four are Constraints (per-name pair)
    for i in 2..6 {
        assert!(matches!(&s.body[i], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
    }
}

#[test]
fn parse_enum_decl_basic() {
    let p = parse("enum Day = Mon | Tue | Wed\n").unwrap();
    assert_eq!(p.enums.len(), 1);
    let e = &p.enums[0];
    assert_eq!(e.name, "Day");
    let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Mon", "Tue", "Wed"]);
    assert!(e.variants.iter().all(|v| v.fields.is_empty()));
}

#[test]
fn parse_enum_decl_alongside_claim() {
    let p = parse("enum Color = Red | Green | Blue\n\nclaim t\n    c ∈ Color\n").unwrap();
    assert_eq!(p.enums.len(), 1);
    assert_eq!(p.schemas.len(), 1);
}

#[test]
fn parse_enum_decl_single_variant_ok() {
    let p = parse("enum Singleton = Only\n").unwrap();
    assert_eq!(p.enums[0].variants.len(), 1);
    assert_eq!(p.enums[0].variants[0].name, "Only");
    assert!(p.enums[0].variants[0].fields.is_empty());
}

#[test]
fn parse_enum_decl_payload_variants() {
    let p = parse("enum Result = Ok(Int) | Err(String)\n").unwrap();
    let e = &p.enums[0];
    assert_eq!(e.variants.len(), 2);
    assert_eq!(e.variants[0].name, "Ok");
    assert_eq!(e.variants[0].fields.len(), 1);
    assert_eq!(e.variants[0].fields[0].name, "f0");
    assert_eq!(e.variants[0].fields[0].type_name, "Int");
    assert_eq!(e.variants[1].name, "Err");
    assert_eq!(e.variants[1].fields[0].type_name, "String");
}

#[test]
fn parse_enum_decl_recursive_self_reference() {
    let p = parse("enum LinkedList = Nil | Cons(Int, LinkedList)\n").unwrap();
    let e = &p.enums[0];
    assert_eq!(e.variants.len(), 2);
    assert_eq!(e.variants[1].name, "Cons");
    assert_eq!(e.variants[1].fields.len(), 2);
    assert_eq!(e.variants[1].fields[0].type_name, "Int");
    assert_eq!(e.variants[1].fields[1].type_name, "LinkedList");
}

#[test]
fn parse_enum_decl_mixed_arities() {
    let p = parse("enum Maybe = None | Some(Int)\n").unwrap();
    let e = &p.enums[0];
    assert!(e.variants[0].fields.is_empty());
    assert_eq!(e.variants[1].fields.len(), 1);
}

#[test]
fn parse_enum_decl_multiline_no_leading_pipe() {
    let p = parse(
        "enum Expr =\n    ENum(Int)\n    EVar(String)\n    EAdd(Expr, Expr)\n"
    ).unwrap();
    let e = &p.enums[0];
    assert_eq!(e.variants.len(), 3);
    assert_eq!(e.variants[0].name, "ENum");
    assert_eq!(e.variants[1].name, "EVar");
    assert_eq!(e.variants[2].name, "EAdd");
}

#[test]
fn parse_enum_decl_multiline_with_leading_pipe() {
    let p = parse(
        "enum Color =\n    | Red\n    | Green\n    | Blue\n"
    ).unwrap();
    let e = &p.enums[0];
    assert_eq!(e.variants.len(), 3);
    let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Red", "Green", "Blue"]);
}

#[test]
fn parse_enum_decl_forward_reference_parses() {
    // Parser doesn't validate types; confirms AST shape: 2 enum decls,
    // first references second by name in a payload field.
    let p = parse(
        "enum Expr = Lit(Int) | Op(BinOp, Expr, Expr)\nenum BinOp = Add | Sub\n"
    ).unwrap();
    assert_eq!(p.enums.len(), 2);
    assert_eq!(p.enums[0].name, "Expr");
    assert_eq!(p.enums[1].name, "BinOp");
    // Op variant's first field references BinOp.
    assert_eq!(p.enums[0].variants[1].name, "Op");
    assert_eq!(p.enums[0].variants[1].fields[0].type_name, "BinOp");
}

#[test]
fn parse_enum_decl_mutual_recursion_parses() {
    // Expr ↔ Stmt — each references the other in payloads.
    let p = parse(
        "enum Expr = ENum(Int) | EBlock(Stmt)\nenum Stmt = SExpr(Expr) | SSeq(Stmt, Stmt)\n"
    ).unwrap();
    assert_eq!(p.enums.len(), 2);
    // Expr.EBlock references Stmt.
    assert_eq!(p.enums[0].variants[1].fields[0].type_name, "Stmt");
    // Stmt.SExpr references Expr.
    assert_eq!(p.enums[1].variants[0].fields[0].type_name, "Expr");
}

#[test]
fn parse_enum_decl_empty_payload_errors() {
    // `Variant()` is rejected — drop the parens for nullary variants.
    assert!(parse("enum X = Foo() | Bar\n").is_err());
}

#[test]
fn parse_enum_decl_no_variants_errors() {
    // The grammar requires at least one variant after `=`.
    // Parser rejects "got X" where X is the unexpected token after `=`.
    assert!(parse("enum Empty =\n").is_err());
}

#[test]
fn parse_chained_membership_rejects_dotted_lhs() {
    // Dotted LHS not allowed in chained-membership; errors on trailing `< 5`.
    assert!(parse("claim t\n    state.x ∈ Int < 5\n").is_err());
}
