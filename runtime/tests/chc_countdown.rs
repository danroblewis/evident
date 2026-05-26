//! CHC / Z3 Fixedpoint (Spacer) binding + worked countdown.
//!
//! Proves both halves of the parent-constrains-child slice (see
//! `runtime/src/chc.rs` and `docs/research/fsm-behavioral-constraints.md`):
//!   1. the raw `z3-sys` Fixedpoint FFI round-trips (mk → register → add_rule
//!      → query → verdict), distinguishing reachable (counterexample) from
//!      unreachable (proved);
//!   2. Spacer discharges the countdown safety property *both* ways — it proves
//!      "∀ seed ≥ 0, the `count-1` countdown settles at 0" (the unbounded
//!      guarantee, no N) and returns a counterexample for the broken `count-2`
//!      variant (odd seeds settle at -1).

use evident_runtime::chc::{countdown_settles_at_zero, int_sort, ChcResult, Fixedpoint, Relation};

use z3::ast::{Ast, Bool, Int};
use z3::{Config, Context};

/// Minimal binding smoke test: a trivially-reachable goal. Proves the raw
/// Fixedpoint FFI round-trips independent of any tricky invariant inference.
#[test]
fn fixedpoint_binding_smoke() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let fp = Fixedpoint::new(&ctx);
    fp.set_engine_spacer();

    let int = int_sort(&ctx);
    let r = Relation::new(&ctx, "R", &[int]);
    let goal = Relation::new(&ctx, "Goal", &[]);
    fp.register_relation(&r);
    fp.register_relation(&goal);

    let five = Int::from_i64(&ctx, 5);
    // fact: R(5)
    fp.add_rule(&r.apply(&[&five]), "fact");
    // ∀c. R(c) → Goal
    {
        let c = Int::new_const(&ctx, "c");
        let rule = z3::ast::forall_const(
            &ctx,
            &[&c as &dyn Ast],
            &[],
            &r.apply(&[&c]).implies(&goal.apply(&[])),
        );
        fp.add_rule(&rule, "reach");
    }
    // Goal is reachable (via R(5)).
    assert_eq!(fp.query(&goal.apply(&[])), ChcResult::Counterexample);
}

/// Negative-control for the smoke test: an *unreachable* goal must come back
/// `Proved` (Z3_L_FALSE), proving the binding distinguishes both verdicts.
#[test]
fn fixedpoint_unreachable_goal_proved() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let fp = Fixedpoint::new(&ctx);
    fp.set_engine_spacer();

    let int = int_sort(&ctx);
    let r = Relation::new(&ctx, "R", &[int]);
    let goal = Relation::new(&ctx, "Goal", &[]);
    fp.register_relation(&r);
    fp.register_relation(&goal);

    // R holds only of 5; Goal fires only on 7 — never reachable.
    let five = Int::from_i64(&ctx, 5);
    let seven = Int::from_i64(&ctx, 7);
    fp.add_rule(&r.apply(&[&five]), "fact");
    {
        let c = Int::new_const(&ctx, "c");
        let body = Bool::and(&ctx, &[&r.apply(&[&c]), &c._eq(&seven)]);
        let rule =
            z3::ast::forall_const(&ctx, &[&c as &dyn Ast], &[], &body.implies(&goal.apply(&[])));
        fp.add_rule(&rule, "reach");
    }
    assert_eq!(fp.query(&goal.apply(&[])), ChcResult::Proved);
}

/// The property holds: ∀ seed ≥ 0, the `count - 1` countdown settles at 0.
/// Spacer finds `Inv(c) ≡ c ≥ 0`; the bad state is unsatisfiable ⇒ UNSAT of the
/// bad-state query ⇒ Proved. This is the unbounded guarantee — no N.
#[test]
fn countdown_minus_one_proved() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    assert_eq!(
        countdown_settles_at_zero(&ctx, 1),
        ChcResult::Proved,
        "∀ seed ≥ 0, count-1 countdown must provably settle at 0"
    );
}

/// The broken `count - 2` variant: odd seeds step past 0 to -1, a reachable bad
/// state. Spacer must return a counterexample, NOT a false "proved".
#[test]
fn countdown_minus_two_counterexample() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    assert_eq!(
        countdown_settles_at_zero(&ctx, 2),
        ChcResult::Counterexample,
        "count-2 countdown must NOT prove settle-at-0 (odd seeds settle at -1)"
    );
}
