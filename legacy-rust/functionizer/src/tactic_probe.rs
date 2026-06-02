//! Sanity-check Z3's tactic.solver() vs plain Solver::new on
//! a trivial satisfiable problem.

use z3::ast::{Ast, Int};
use z3::{Config, Context, SatResult, Solver, Tactic};

fn main() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);

    let x = Int::new_const(&ctx, "x");
    let y = Int::new_const(&ctx, "y");
    let five = Int::from_i64(&ctx, 5);
    let one  = Int::from_i64(&ctx, 1);

    // Plain solver
    let s_plain = Solver::new(&ctx);
    s_plain.assert(&x._eq(&five));
    s_plain.assert(&y._eq(&(x.clone() + one.clone())));
    println!("plain solver:        {:?}", s_plain.check());
    if let Some(m) = s_plain.get_model() {
        println!("  x = {:?}, y = {:?}", m.eval(&x, true), m.eval(&y, true));
    }

    // simplify-tactic solver
    let s_simp = Tactic::new(&ctx, "simplify").solver();
    s_simp.assert(&x._eq(&five));
    s_simp.assert(&y._eq(&(x.clone() + one.clone())));
    println!("simplify tactic:     {:?}", s_simp.check());
    if let Some(m) = s_simp.get_model() {
        println!("  x = {:?}, y = {:?}", m.eval(&x, true), m.eval(&y, true));
    }

    // solve-eqs tactic solver
    let s_eq = Tactic::new(&ctx, "solve-eqs").solver();
    s_eq.assert(&x._eq(&five));
    s_eq.assert(&y._eq(&(x.clone() + one.clone())));
    println!("solve-eqs tactic:    {:?}", s_eq.check());
    if let Some(m) = s_eq.get_model() {
        println!("  x = {:?}, y = {:?}", m.eval(&x, true), m.eval(&y, true));
    } else {
        println!("  (no model)");
    }

    let _ = SatResult::Sat;  // suppress unused warning
}
