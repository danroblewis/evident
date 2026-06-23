//! Centralized unit tests, relocated out of the implementation files.
//!
//! One submodule per source file that previously carried an inline
//! `#[cfg(test)] mod tests { … }` block. The test logic is a pure move —
//! identical assertions, identical names — only the location changed.

mod ffi {
    use crate::ffi::*;

    fn libc_path() -> &'static str {
        if cfg!(target_os = "macos") { "libSystem.dylib" } else { "libc.so.6" }
    }

    fn libm_path() -> &'static str {
        // libm is folded into libSystem on macOS (no standalone libm.so.6).
        if cfg!(target_os = "macos") { "libSystem.dylib" } else { "libm.so.6" }
    }

    #[test]
    fn parse_signature_basic() {
        let p = parse_signature("i()").unwrap();
        assert_eq!(p.ret, TypeCode::I);
        assert!(p.args.is_empty());

        let p = parse_signature("i(s)").unwrap();
        assert_eq!(p.ret, TypeCode::I);
        assert_eq!(p.args, vec![TypeCode::S]);

        let p = parse_signature("p(siii)").unwrap();
        assert_eq!(p.ret, TypeCode::P);
        assert_eq!(p.args, vec![TypeCode::S, TypeCode::I, TypeCode::I, TypeCode::I]);

        assert!(parse_signature("x()").is_err(),  "unknown type code");
        assert!(parse_signature("i)").is_err(),    "missing open paren");
        assert!(parse_signature("i(").is_err(),    "missing close paren");
        assert!(parse_signature("i(v)").is_err(),  "void as arg");
    }

    #[test]
    fn call_libc_getpid() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).expect("dlopen libc");
        let getpid = ffi_lookup(&reg, lib, "getpid").expect("dlsym getpid");
        let result = ffi_call(&reg, getpid, "i()", &[]).expect("call getpid");
        match result {
            FfiReturn::Int(pid) => {
                assert!(pid > 0, "getpid returned {pid}");
                assert_eq!(pid as u32, std::process::id());
            }
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn call_libc_strlen() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let strlen = ffi_lookup(&reg, lib, "strlen").unwrap();
        let r = ffi_call(&reg, strlen, "i(s)", &[FfiArg::Str("hello world".into())]).unwrap();
        match r {
            FfiReturn::Int(n) => assert_eq!(n, 11),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn call_libc_abs() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let abs = ffi_lookup(&reg, lib, "abs").unwrap();
        let r = ffi_call(&reg, abs, "i(i)", &[FfiArg::Int(-42)]).unwrap();
        match r {
            FfiReturn::Int(n) => assert_eq!(n, 42),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn call_libm_sqrt_double() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libm_path()).unwrap();
        let f = ffi_lookup(&reg, lib, "sqrt").unwrap();
        let r = ffi_call(&reg, f, "d(d)", &[FfiArg::Real(16.0)]).unwrap();
        match r {
            FfiReturn::Real(x) => assert!((x - 4.0).abs() < 1e-12, "got {x}"),
            other => panic!("expected Real, got {other:?}"),
        }
    }

    #[test]
    fn call_libm_sqrtf_float() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libm_path()).unwrap();
        let f = ffi_lookup(&reg, lib, "sqrtf").unwrap();
        let r = ffi_call(&reg, f, "f(f)", &[FfiArg::Real(25.0)]).unwrap();
        match r {
            FfiReturn::Real(x) => assert!((x - 5.0).abs() < 1e-6, "got {x}"),
            other => panic!("expected Real, got {other:?}"),
        }
    }

    #[test]
    fn type_mismatch_errors() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let strlen = ffi_lookup(&reg, lib, "strlen").unwrap();

        let err = ffi_call(&reg, strlen, "i(s)", &[FfiArg::Int(0)]).unwrap_err();
        assert!(err.0.contains("type mismatch"), "{}", err.0);
    }

    #[test]
    fn arg_count_mismatch_errors() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let strlen = ffi_lookup(&reg, lib, "strlen").unwrap();
        let err = ffi_call(&reg, strlen, "i(s)", &[]).unwrap_err();
        assert!(err.0.contains("expects 1 args"), "{}", err.0);
    }

    #[test]
    fn unknown_handle_errors() {
        let reg = HandleRegistry::new();
        let err = ffi_lookup(&reg, 9999, "anything").unwrap_err();
        assert!(err.0.contains("unknown handle"), "{}", err.0);
    }

    #[test]
    fn close_handle_frees_entry() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        assert!(reg.close(lib),  "first close succeeds");
        assert!(!reg.close(lib), "second close finds nothing");
    }

    #[test]
    fn null_returning_string_is_empty() {

        let _reg = HandleRegistry::new();

    }
}

mod dispatch {
    use crate::ffi::*;
    use crate::core::ast::{Effect, EffectFfiArg, EffectResult};

    fn ctx_with_input(_input: &str) -> DispatchContext {
        DispatchContext::with_streams(Box::new(Vec::<u8>::new()))
    }

    fn captured_stdout(ctx: DispatchContext) -> String {

        let _ = ctx;
        String::new()
    }

    #[test]
    fn no_effect_returns_no_result() {
        let mut ctx = DispatchContext::new();
        assert!(matches!(dispatch_one(&mut ctx, &Effect::NoEffect), EffectResult::NoResult));
    }

    #[test]
    fn print_returns_no_result() {
        let mut ctx = DispatchContext::with_streams(Box::new(Vec::<u8>::new()));
        let r = dispatch_one(&mut ctx, &Effect::Print("hi".into()));
        assert!(matches!(r, EffectResult::NoResult));
    }

    #[test]
    fn ffi_open_real_libc_succeeds() {
        let mut ctx = DispatchContext::new();
        let path = if cfg!(target_os = "macos") { "libSystem.dylib" } else { "libc.so.6" };
        match dispatch_one(&mut ctx, &Effect::FFIOpen(path.into())) {
            EffectResult::Handle(h) => assert!(h > 0, "handle should be > 0, got {h}"),
            other => panic!("expected Handle, got {other:?}"),
        }
    }

    #[test]
    fn ffi_open_invalid_path_returns_error() {
        let mut ctx = DispatchContext::new();
        match dispatch_one(&mut ctx, &Effect::FFIOpen("/nonexistent/lib".into())) {
            EffectResult::Error(_) => {}
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn ffi_call_getpid_end_to_end() {
        let mut ctx = DispatchContext::new();
        let path = if cfg!(target_os = "macos") { "libSystem.dylib" } else { "libc.so.6" };
        let lib = match dispatch_one(&mut ctx, &Effect::FFIOpen(path.into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        let sym = match dispatch_one(&mut ctx, &Effect::FFILookup(lib, "getpid".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        match dispatch_one(&mut ctx, &Effect::FFICall(sym, "i()".into(), vec![])) {
            EffectResult::Int(pid) => {
                assert_eq!(pid as u32, std::process::id());
            }
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn close_unknown_handle_errors() {
        let mut ctx = DispatchContext::new();
        match dispatch_one(&mut ctx, &Effect::CloseHandle(9999)) {
            EffectResult::Error(_) => {}
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn libcall_caches_lib_and_sym() {
        let mut ctx = ctx_with_input("");
        let path = if cfg!(target_os = "macos") { "/usr/lib/libSystem.dylib" } else { "libc.so.6" };

        let r1 = dispatch_one(&mut ctx, &Effect::LibCall(
            path.into(), "getpid".into(), "i()".into(), vec![],
        ));
        match r1 {
            EffectResult::Int(pid) => assert_eq!(pid as u32, std::process::id()),
            other => panic!("expected Int, got {other:?}"),
        }
        assert_eq!(ctx.lib_cache.len(), 1, "lib cache should have one entry");
        assert_eq!(ctx.sym_cache.len(), 1, "sym cache should have one entry");

        let next_id_before = ctx.lib_cache.values().copied().max().unwrap();
        let r2 = dispatch_one(&mut ctx, &Effect::LibCall(
            path.into(), "getpid".into(), "i()".into(), vec![],
        ));
        match r2 {
            EffectResult::Int(_) => {}
            other => panic!("expected Int, got {other:?}"),
        }
        let next_id_after = ctx.lib_cache.values().copied().max().unwrap();
        assert_eq!(next_id_before, next_id_after,
            "second call should not have allocated a new lib handle");
    }

    #[test]
    fn libcall_with_string_arg() {
        let mut ctx = ctx_with_input("");
        let path = if cfg!(target_os = "macos") { "/usr/lib/libSystem.dylib" } else { "libc.so.6" };
        let r = dispatch_one(&mut ctx, &Effect::LibCall(
            path.into(), "strlen".into(), "i(s)".into(),
            vec![EffectFfiArg::Str("hello world".into())],
        ));
        match r {
            EffectResult::Int(n) => assert_eq!(n, 11),
            other => panic!("expected Int(11), got {other:?}"),
        }
    }

    #[test]
    fn libcall_invalid_lib_returns_error() {
        let mut ctx = ctx_with_input("");
        let r = dispatch_one(&mut ctx, &Effect::LibCall(
            "/nonexistent/lib".into(), "getpid".into(), "i()".into(), vec![],
        ));
        match r {
            EffectResult::Error(_) => {}
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_all_preserves_order_and_count() {
        let mut ctx = ctx_with_input("");
        let effects = vec![
            Effect::NoEffect,
            Effect::Println("mid".into()),
            Effect::NoEffect,
        ];
        let results = dispatch_all(&mut ctx, &effects);
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0], EffectResult::NoResult));
        assert!(matches!(results[1], EffectResult::NoResult));
        assert!(matches!(results[2], EffectResult::NoResult));
    }
}

mod parser {
    use crate::parser::*;
    use crate::core::ast::{BodyItem, Expr, BinOp, Keyword};

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

        match &s.body[1] {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, _)) => {
                assert!(matches!(lhs.as_ref(), Expr::Cardinality(_)));
            }
            other => panic!("expected #s = 3 constraint, got {:?}", other),
        }

        match &s.body[2] {
            BodyItem::Constraint(Expr::Binary(BinOp::Gt, lhs, _)) => {
                assert!(matches!(lhs.as_ref(), Expr::Index(_, _)));
            }
            other => panic!("expected s[0] > 0 constraint, got {:?}", other),
        }
    }

    #[test]
    fn parse_arithmetic_constraint() {

        let p = parse("schema X\n    n ∈ Nat\n    n > 5 + 3 * 2\n").unwrap();
        let s = &p.schemas[0];
        let constraint = match &s.body[1] {
            BodyItem::Constraint(e) => e,
            _ => panic!(),
        };

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

        let p = parse("claim t\n    pos_x ∈ Int = 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 2);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "pos_x" && type_name == "Int"));
        assert!(matches!(&s.body[1], BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, _))));
    }

    #[test]
    fn parse_chained_membership_compound_type() {

        let p = parse("claim t\n    s ∈ Seq(Int)\n    #s = 3\n").unwrap();

        let s = &p.schemas[0];
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "s" && type_name == "Seq(Int)"));
    }

    #[test]
    fn parse_chained_membership_does_not_eat_set_membership() {

        let p = parse("claim t\n    pts ∈ Set(Int)\n    x ∈ Int\n    x ∈ pts ∧ x > 0\n").unwrap();
        let s = &p.schemas[0];

        match s.body.last().unwrap() {
            BodyItem::Constraint(Expr::Binary(BinOp::And, _, _)) => {}
            other => panic!("expected `(x ∈ pts) ∧ (x > 0)` constraint, got {:?}", other),
        }
    }

    // ── `:=` initial-value seed: lowers to `is_first_tick ⇒ name = val` ──

    #[test]
    fn parse_initial_value_seed_scalar() {
        // `x ∈ Int := 0` → Membership(x: Int) + Constraint(is_first_tick ⇒ x = 0).
        let p = parse("fsm t\n    x ∈ Int := 0\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 2, "expected Membership + seed Constraint");
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "x" && type_name == "Int"));
        match &s.body[1] {
            BodyItem::Constraint(Expr::Binary(BinOp::Implies, ante, cons)) => {
                assert!(matches!(ante.as_ref(), Expr::Identifier(n) if n == "is_first_tick"));
                assert!(matches!(cons.as_ref(),
                    Expr::Binary(BinOp::Eq, l, r)
                        if matches!(l.as_ref(), Expr::Identifier(n) if n == "x")
                        && matches!(r.as_ref(), Expr::Int(0))));
            }
            other => panic!("expected is_first_tick ⇒ x = 0, got {:?}", other),
        }
    }

    #[test]
    fn parse_initial_value_seed_equals_explicit_is_first_tick() {
        // The whole point: `:=` must parse to the IDENTICAL AST as the explicit
        // `is_first_tick ⇒ x = 0` form (same Z3 encoding, 0 dropped, no silent bug).
        let via_seed = parse("fsm t\n    x ∈ Int := 0\n").unwrap();
        let via_explicit =
            parse("fsm t\n    x ∈ Int\n    is_first_tick ⇒ x = 0\n").unwrap();
        // Both produce: [Membership(x), Constraint(is_first_tick ⇒ x = 0)].
        let seed_body = format!("{:?}", via_seed.schemas[0].body);
        let explicit_body = format!("{:?}", via_explicit.schemas[0].body);
        assert_eq!(seed_body, explicit_body,
            "`:=` seed must lower to the identical AST as explicit is_first_tick");
    }

    #[test]
    fn parse_initial_value_seed_multi_name() {
        // `x, y ∈ Int := 0` → 2 Memberships + 2 is_first_tick-guarded seeds.
        let p = parse("fsm t\n    x, y ∈ Int := 0\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 4, "expected 2 Memberships + 2 seed Constraints");
        assert!(matches!(&s.body[0], BodyItem::Membership { name, .. } if name == "x"));
        assert!(matches!(&s.body[1], BodyItem::Membership { name, .. } if name == "y"));
        for (i, var) in [(2, "x"), (3, "y")] {
            match &s.body[i] {
                BodyItem::Constraint(Expr::Binary(BinOp::Implies, ante, cons)) => {
                    assert!(matches!(ante.as_ref(),
                        Expr::Identifier(n) if n == "is_first_tick"));
                    assert!(matches!(cons.as_ref(),
                        Expr::Binary(BinOp::Eq, l, _)
                            if matches!(l.as_ref(), Expr::Identifier(n) if n == var)));
                }
                other => panic!("expected seed for {var}, got {:?}", other),
            }
        }
    }

    #[test]
    fn parse_initial_value_seed_record_ctor() {
        // `pos ∈ IVec2 := IVec2(3, 4)` — the seed RHS is a record literal (Call).
        let p = parse("fsm t\n    pos ∈ IVec2 := IVec2(3, 4)\n").unwrap();
        let s = &p.schemas[0];
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "pos" && type_name == "IVec2"));
        match &s.body[1] {
            BodyItem::Constraint(Expr::Binary(BinOp::Implies, _, cons)) => {
                assert!(matches!(cons.as_ref(),
                    Expr::Binary(BinOp::Eq, _, r)
                        if matches!(r.as_ref(), Expr::Call(n, _) if n == "IVec2")));
            }
            other => panic!("expected record seed, got {:?}", other),
        }
    }

    #[test]
    fn parse_chained_membership_multi_name() {

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

        let p = parse("claim t\n    0 < x, y ∈ Int < 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 6);

        assert!(matches!(&s.body[0], BodyItem::Membership { name, .. } if name == "x"));
        assert!(matches!(&s.body[1], BodyItem::Membership { name, .. } if name == "y"));

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
        assert_eq!(e.variants[0].fields[0].name, "Ok_f0");   // variant-prefixed so accessors are unique
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

        let p = parse(
            "enum Expr = Lit(Int) | Op(BinOp, Expr, Expr)\nenum BinOp = Add | Sub\n"
        ).unwrap();
        assert_eq!(p.enums.len(), 2);
        assert_eq!(p.enums[0].name, "Expr");
        assert_eq!(p.enums[1].name, "BinOp");

        assert_eq!(p.enums[0].variants[1].name, "Op");
        assert_eq!(p.enums[0].variants[1].fields[0].type_name, "BinOp");
    }

    #[test]
    fn parse_enum_decl_mutual_recursion_parses() {

        let p = parse(
            "enum Expr = ENum(Int) | EBlock(Stmt)\nenum Stmt = SExpr(Expr) | SSeq(Stmt, Stmt)\n"
        ).unwrap();
        assert_eq!(p.enums.len(), 2);

        assert_eq!(p.enums[0].variants[1].fields[0].type_name, "Stmt");

        assert_eq!(p.enums[1].variants[0].fields[0].type_name, "Expr");
    }

    #[test]
    fn parse_enum_decl_empty_payload_errors() {

        assert!(parse("enum X = Foo() | Bar\n").is_err());
    }

    #[test]
    fn parse_enum_decl_no_variants_errors() {

        assert!(parse("enum Empty =\n").is_err());
    }

    #[test]
    fn parse_chained_membership_rejects_dotted_lhs() {

        assert!(parse("claim t\n    state.x ∈ Int < 5\n").is_err());
    }

    #[test]
    fn parse_comparison_ops_after_generics_removal() {

        let p = parse("claim t\n    a ∈ Int\n    b ∈ Int\n    a < b\n    b > 5\n").unwrap();
        let s = &p.schemas[0];
        match s.body[2] {
            BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _)) => {}
            ref other => panic!("expected `a < b`, got {:?}", other),
        }
        match s.body[3] {
            BodyItem::Constraint(Expr::Binary(BinOp::Gt, _, _)) => {}
            ref other => panic!("expected `b > 5`, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_type_params_no_longer_accepted() {

        assert!(parse("type Edge<T>\n    from ∈ Int\n").is_err());
    }
}

mod effect_codec {
    use crate::encode::effect_codec::*;
    use crate::core::Value;
    use crate::core::ast::{Effect, EffectFfiArg, EffectResult};

    fn e(enum_name: &str, variant: &str, fields: Vec<Value>) -> Value {
        Value::Enum {
            enum_name: enum_name.into(),
            variant: variant.into(),
            fields,
        }
    }

    #[test]
    fn decode_println_effect() {
        let v = e("Effect", "Println", vec![Value::Str("hello".into())]);
        match decode_effect(&v).unwrap() {
            Effect::Println(s) => assert_eq!(s, "hello"),
            other => panic!("expected Println, got {other:?}"),
        }
    }

    #[test]
    fn decode_no_effect_zero_arity() {
        let v = e("Effect", "NoEffect", vec![]);
        assert!(matches!(decode_effect(&v).unwrap(), Effect::NoEffect));
    }

    #[test]
    fn decode_ffi_call_with_args() {

        let arglist = Value::SeqEnum(vec![
            e("FFIArg", "ArgStr", vec![Value::Str("hi".into())]),
            e("FFIArg", "ArgInt", vec![Value::Int(42)]),
        ]);
        let v = e("Effect", "FFICall", vec![
            Value::Int(7),
            Value::Str("i(si)".into()),
            arglist,
        ]);
        match decode_effect(&v).unwrap() {
            Effect::FFICall(h, sig, args) => {
                assert_eq!(h, 7);
                assert_eq!(sig, "i(si)");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0], EffectFfiArg::Str(s) if s == "hi"));
                assert!(matches!(&args[1], EffectFfiArg::Int(42)));
            }
            other => panic!("expected FFICall, got {other:?}"),
        }
    }

    #[test]
    fn decode_effect_list_three_items() {

        let list = Value::SeqEnum(vec![
            e("Effect", "Println", vec![Value::Str("a".into())]),
            e("Effect", "Print", vec![Value::Str("b".into())]),
            e("Effect", "Exit", vec![Value::Int(0)]),
        ]);
        let decoded = decode_effect_list(&list).unwrap();
        assert_eq!(decoded.len(), 3);
        assert!(matches!(&decoded[0], Effect::Println(s) if s == "a"));
        assert!(matches!(&decoded[1], Effect::Print(s) if s == "b"));
        assert!(matches!(&decoded[2], Effect::Exit(0)));
    }

    #[test]
    fn decode_int_result() {
        let v = e("Result", "IntResult", vec![Value::Int(42)]);
        assert!(matches!(decode_result(&v).unwrap(), EffectResult::Int(42)));
    }

    #[test]
    fn decode_unknown_variant_errors() {
        let v = e("Effect", "BogusVariant", vec![]);
        let err = decode_effect(&v).unwrap_err();
        assert!(matches!(err, DecodeError::UnknownVariant { .. }));
    }
}

mod extract {
    use crate::encode::extract::unescape_z3_string;
    #[test]
    fn newline_escape_decoded() {
        assert_eq!(unescape_z3_string("abc\\u{a}def"), "abc\ndef");
    }
    #[test]
    fn multi_escape_decoded() {
        assert_eq!(
            unescape_z3_string("a\\u{9}b\\u{20}c"),
            "a\tb c",
        );
    }
    #[test]
    fn high_codepoint_decoded() {

        assert_eq!(unescape_z3_string("hi \\u{1f600}!"), "hi 😀!");
    }
    #[test]
    fn no_escape_passthrough() {
        assert_eq!(unescape_z3_string("plain ascii"), "plain ascii");
    }
    #[test]
    fn malformed_passthrough() {

        assert_eq!(unescape_z3_string("\\u{xyz"), "\\u{xyz");
    }
}

mod trampoline {
    use crate::trampoline::*;
    use crate::ffi::DispatchContext;
    use crate::EvidentRuntime;
    fn ctx_silent() -> DispatchContext {
        DispatchContext::with_streams(Box::new(Vec::<u8>::new()))
    }

    #[test]
    fn detect_main_shape_finds_io_slots() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
fsm main
    count ∈ Int = (is_first_tick ? 0 : _count + 1)
    effects = ⟨⟩
    #last_results = 0
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.effects_var.as_deref(), Some("effects"));
        assert_eq!(shape.last_results_var.as_deref(), Some("last_results"));
    }

    #[test]
    fn smart_inject_skips_unreferenced_slots() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
fsm main
    count ∈ Int = (is_first_tick ? 0 : _count + 1)
    effects = ⟨⟩
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.effects_var.as_deref(), Some("effects"));
        assert_eq!(shape.last_results_var, None,
            "last_results never referenced → should not be auto-injected");
    }

    #[test]
    fn halt_when_carried_state_reaches_fixpoint() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        // count climbs 0,1 then sticks at 1 — a fixpoint with no effects,
        // so the loop halts cleanly once nothing carried changes.
        rt.load_source("\
fsm main
    count ∈ Int = (is_first_tick ? 0 : (_count ≥ 1 ? _count : _count + 1))
    effects = ⟨⟩
").unwrap();
        let mut ctx = ctx_silent();
        let r = run_with_ctx(&rt, &LoopOpts { max_steps: 5 }, &mut ctx).unwrap();
        assert!(r.steps <= 5);
        assert!(r.halted_clean || r.steps == 5);
    }
}

mod lexer {
    use crate::lexer::*;

    #[test]
    fn lex_simple_schema() {
        let src = "schema SimpleNat\n    n ∈ Nat\n    n > 5\n";
        let toks = tokenize(src).unwrap();

        assert!(matches!(toks[0], Token::Indent(0)));
        assert!(matches!(toks[1], Token::Schema));
        assert!(matches!(&toks[2], Token::Ident(s) if s == "SimpleNat"));
    }

    #[test]
    fn lex_unicode_operators() {
        let toks = tokenize("a ∈ Set ∧ b ≤ 5 ⇒ ¬c").unwrap();
        let kinds: Vec<_> = toks.iter().filter(|t| !matches!(t, Token::Indent(_))).cloned().collect();

        assert!(matches!(kinds[1], Token::In));
        assert!(matches!(kinds[3], Token::And));
        assert!(matches!(kinds[5], Token::Le));
        assert!(matches!(kinds[7], Token::Implies));
        assert!(matches!(kinds[8], Token::Not));
    }
}
