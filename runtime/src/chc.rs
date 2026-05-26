//! Constrained Horn Clauses → Z3 Fixedpoint/Spacer, reached via raw `z3-sys`.
//!
//! This is the first slice of "a parent constraint-model constrains a child
//! FSM's behavior over its *whole run*" — the principled, *unbounded* tool for
//! that question (vs. the BMC unroller in `fsm_unroll/compose.rs`, which only
//! covers run lengths ≤ N). See `docs/research/fsm-behavioral-constraints.md`
//! § 2 for the Horn encoding and § 3 for the binding evidence.
//!
//! A transition system `M = (S, I, Tr)` against a safety property `P` lowers to
//! three Horn-clause roles over one invariant relation `Inv : S → Bool`:
//!
//! ```text
//! (1) initiation   :  I(s)                  →  Inv(s)
//! (2) consecution  :  Inv(s) ∧ Tr(s, s')    →  Inv(s')
//! (3) safety/query :  Inv(s) ∧ ¬P(s)        →  false      (the bad state)
//! ```
//!
//! Spacer searches for an interpretation of `Inv`. If the bad state is
//! *unreachable* the property holds **for all seeds in `I`, at any run length**
//! — the unbounded guarantee. If reachable, Spacer returns a concrete
//! counterexample seed + trace.
//!
//! There is no safe `z3`-crate wrapper for the Fixedpoint API (`z3` 0.12.1 has
//! zero `fixedpoint` references); the full C API *is* bound in `z3-sys` 0.8.1
//! (`Z3_mk_fixedpoint` `lib.rs:6215`, `…_register_relation:6355`, `…_add_rule:6231`,
//! `…_query:6271`, `…_get_answer:6297`). We reach `Z3_context` from `z3::Context`
//! with the same single-field-newtype transmute `translate/exprs/string_ops.rs`
//! already ships, guarded by a compile-time layout assertion.
//!
//! ADDITIVE: this module is on no existing runtime path. The worked countdown
//! (§ tests) proves both the binding and that Spacer discharges the property.
//! Wiring it into the nested-FSM selector (via `compose.rs::build_f1`'s already-
//! extracted `Tr`) is a later slice; see the research § 6.2 / § 7.3 step 5.

use std::ffi::{CStr, CString};

use z3::ast::{Ast, Bool, Int};
use z3::Context;

use z3_sys::{Z3_func_decl, Z3_sort, Z3_symbol};

// `z3::Context` is a single-field newtype around `Z3_context`; we reach the raw
// pointer for the unwrapped Fixedpoint builders. The const_assert guards the
// layout (a build break, not a silent unsoundness, if `z3::Context` grows a
// field). Same pattern as `translate/exprs/string_ops.rs`.
const _: () = {
    assert!(
        std::mem::size_of::<Context>() == std::mem::size_of::<z3_sys::Z3_context>(),
        "z3::Context is no longer a single-pointer newtype; raw_ctx is unsound"
    );
};

#[inline]
fn raw_ctx(ctx: &Context) -> z3_sys::Z3_context {
    // SAFETY: `Context` is a single-field newtype around `Z3_context`; the
    // `size_of` assertion above verifies the layout hasn't changed.
    unsafe { *(ctx as *const Context as *const z3_sys::Z3_context) }
}

/// Mint a string symbol.
fn mk_symbol(ctx: &Context, name: &str) -> Z3_symbol {
    let c = CString::new(name).expect("relation/rule name has no interior NUL");
    unsafe { z3_sys::Z3_mk_string_symbol(raw_ctx(ctx), c.as_ptr()) }
}

/// `Int` sort, via the raw builder (Z3 interns sorts, so this is the same sort
/// object the safe `z3::ast::Int` constants carry).
pub fn int_sort(ctx: &Context) -> Z3_sort {
    unsafe { z3_sys::Z3_mk_int_sort(raw_ctx(ctx)) }
}

/// An uninterpreted relation `name : domain → Bool` — a Spacer invariant
/// predicate. Owns a ref on the func_decl: **the decl MUST be ref-protected**,
/// or Z3 garbage-collects/reuses its memory across later allocations, silently
/// corrupting every application built from it (a sort-mismatch in `and` and a
/// segfault — the bug that motivated this type). The safe `z3::FuncDecl` does
/// the same `inc_ref`; we own it here because we keep the raw decl for the C API.
pub struct Relation<'ctx> {
    ctx: &'ctx Context,
    decl: Z3_func_decl,
}

impl<'ctx> Relation<'ctx> {
    /// Mint a relation and protect its decl from collection.
    pub fn new(ctx: &'ctx Context, name: &str, domain: &[Z3_sort]) -> Self {
        let raw = raw_ctx(ctx);
        let sym = mk_symbol(ctx, name);
        let bool_sort = unsafe { z3_sys::Z3_mk_bool_sort(raw) };
        let decl = unsafe {
            z3_sys::Z3_mk_func_decl(
                raw,
                sym,
                domain.len() as ::std::os::raw::c_uint,
                domain.as_ptr(),
                bool_sort,
            )
        };
        // Protect from GC across subsequent allocations (the load-bearing fix).
        unsafe { z3_sys::Z3_inc_ref(raw, z3_sys::Z3_func_decl_to_ast(raw, decl)) };
        Relation { ctx, decl }
    }

    /// The raw `Z3_func_decl`, for `Z3_fixedpoint_register_relation`.
    pub fn raw(&self) -> Z3_func_decl {
        self.decl
    }

    /// Apply this relation to `args`, yielding the Bool application `name(args)`.
    pub fn apply(&self, args: &[&Int<'ctx>]) -> Bool<'ctx> {
        let raw_args: Vec<z3_sys::Z3_ast> = args.iter().map(|a| a.get_z3_ast()).collect();
        unsafe {
            Bool::wrap(
                self.ctx,
                z3_sys::Z3_mk_app(
                    raw_ctx(self.ctx),
                    self.decl,
                    raw_args.len() as ::std::os::raw::c_uint,
                    raw_args.as_ptr(),
                ),
            )
        }
    }
}

impl Drop for Relation<'_> {
    fn drop(&mut self) {
        let raw = raw_ctx(self.ctx);
        unsafe { z3_sys::Z3_dec_ref(raw, z3_sys::Z3_func_decl_to_ast(raw, self.decl)) };
    }
}

/// What Spacer returned for a safety query. Never let `Unknown` masquerade as
/// `Proved` — CHC is undecidable, so `Z3_L_UNDEF` is a real, distinct outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChcResult {
    /// `Z3_L_FALSE` — the bad state is unreachable; the property holds for all
    /// seeds in the precondition, at any run length (unbounded).
    Proved,
    /// `Z3_L_TRUE` — the bad state is reachable; Spacer found a counterexample
    /// (a seed + trace reaching a `¬P` state).
    Counterexample,
    /// `Z3_L_UNDEF` — Spacer gave up (timeout / divergence / incompleteness).
    /// Carries `Z3_fixedpoint_get_reason_unknown` for diagnostics.
    Unknown(String),
}

/// A thin, refcount-managing wrapper over a `Z3_fixedpoint` object. We own the
/// `unsafe` surface: `inc_ref` on construction, `dec_ref` on `Drop`.
pub struct Fixedpoint<'ctx> {
    ctx: &'ctx Context,
    fp: z3_sys::Z3_fixedpoint,
}

impl<'ctx> Fixedpoint<'ctx> {
    /// Create the engine and bump its refcount (the object is not auto-managed).
    pub fn new(ctx: &'ctx Context) -> Self {
        let raw = raw_ctx(ctx);
        let fp = unsafe { z3_sys::Z3_mk_fixedpoint(raw) };
        unsafe { z3_sys::Z3_fixedpoint_inc_ref(raw, fp) };
        Fixedpoint { ctx, fp }
    }

    /// Select the Spacer (IC3/PDR-modulo-theories) engine. It is Z3 4.12's
    /// default for the fixedpoint object, but we set it explicitly for clarity.
    pub fn set_engine_spacer(&self) {
        let raw = raw_ctx(self.ctx);
        unsafe {
            let params = z3_sys::Z3_mk_params(raw);
            z3_sys::Z3_params_inc_ref(raw, params);
            let key = mk_symbol(self.ctx, "engine");
            let val = mk_symbol(self.ctx, "spacer");
            z3_sys::Z3_params_set_symbol(raw, params, key, val);
            z3_sys::Z3_fixedpoint_set_params(raw, self.fp, params);
            z3_sys::Z3_params_dec_ref(raw, params);
        }
    }

    /// Declare an invariant predicate (an uninterpreted relation Spacer must
    /// characterize).
    pub fn register_relation(&self, rel: &Relation<'ctx>) {
        unsafe { z3_sys::Z3_fixedpoint_register_relation(raw_ctx(self.ctx), self.fp, rel.raw()) };
    }

    /// Add a Horn clause. `rule` is a `∀…. body → head` implication built with
    /// the safe `z3` AST API; we hand its raw `Z3_ast` to the C API.
    pub fn add_rule(&self, rule: &Bool<'ctx>, name: &str) {
        let sym = mk_symbol(self.ctx, name);
        unsafe {
            z3_sys::Z3_fixedpoint_add_rule(raw_ctx(self.ctx), self.fp, rule.get_z3_ast(), sym)
        };
    }

    /// Pose the safety query (clause 3's bad-state application). Returns
    /// [`ChcResult::Counterexample`] if the bad state is reachable (`Z3_L_TRUE`),
    /// [`ChcResult::Proved`] if not (`Z3_L_FALSE`), [`ChcResult::Unknown`] otherwise.
    pub fn query(&self, query: &Bool<'ctx>) -> ChcResult {
        let raw = raw_ctx(self.ctx);
        let r = unsafe { z3_sys::Z3_fixedpoint_query(raw, self.fp, query.get_z3_ast()) };
        match r {
            z3_sys::Z3_L_FALSE => ChcResult::Proved,
            z3_sys::Z3_L_TRUE => ChcResult::Counterexample,
            _ => ChcResult::Unknown(self.reason_unknown()),
        }
    }

    fn reason_unknown(&self) -> String {
        let raw = raw_ctx(self.ctx);
        let p = unsafe { z3_sys::Z3_fixedpoint_get_reason_unknown(raw, self.fp) };
        if p.is_null() {
            return "<no reason>".to_string();
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Drop for Fixedpoint<'_> {
    fn drop(&mut self) {
        unsafe { z3_sys::Z3_fixedpoint_dec_ref(raw_ctx(self.ctx), self.fp) };
    }
}

/// The worked countdown — the parent-constrains-child slice, end-to-end.
///
/// Child FSM (the implementation):
/// ```evident
/// fsm countdown(count ∈ Int, halt ∈ Bool)
///     count = _count - decrement        -- step: count' = count - decrement
///     halt  = (_count ≤ 0)              -- halt when the input count ≤ 0
/// ```
///
/// Parent property (the spec, enforced over the *whole* run):
/// > "For every seed ≥ 0, the settled (halted) result is exactly 0."
///
/// Encoded over `Inv : Int → Bool` (the reachable-count relation) and a nullary
/// error relation `Bad`:
/// ```text
/// (1) initiation :  c ≥ 0                        →  Inv(c)
/// (2) step       :  Inv(c) ∧ c > 0               →  Inv(c - decrement)
/// (3) bad        :  Inv(c) ∧ c ≤ 0 ∧ c ≠ 0       →  Bad
/// query: Bad
/// ```
///
/// `decrement = 1` ⇒ the inductive invariant `Inv(c) ≡ c ≥ 0` proves safety
/// (`c ≥ 0 ∧ c ≤ 0 ⇒ c = 0`, so the bad state is unsatisfiable) ⇒ `Proved`.
/// `decrement = 2` ⇒ odd seeds step past 0 to `-1`, a reachable bad state ⇒
/// `Counterexample`. This is the exact parity-sensitivity the research § 2.4
/// calls the kind of whole-run bug nesting must catch.
pub fn countdown_settles_at_zero(ctx: &Context, decrement: i64) -> ChcResult {
    let fp = Fixedpoint::new(ctx);
    fp.set_engine_spacer();

    let int = int_sort(ctx);
    let inv = Relation::new(ctx, "Inv", &[int]);
    let bad = Relation::new(ctx, "Bad", &[]);
    fp.register_relation(&inv);
    fp.register_relation(&bad);

    let zero = Int::from_i64(ctx, 0);
    let dec = Int::from_i64(ctx, decrement);

    // (1) initiation:  ∀c. c ≥ 0 → Inv(c)
    {
        let c = Int::new_const(ctx, "c");
        let body = c.ge(&zero);
        let head = inv.apply(&[&c]);
        let rule = z3::ast::forall_const(ctx, &[&c as &dyn Ast], &[], &body.implies(&head));
        fp.add_rule(&rule, "init");
    }

    // (2) step:  ∀c. (Inv(c) ∧ c > 0) → Inv(c - decrement)
    {
        let c = Int::new_const(ctx, "c");
        let body = Bool::and(ctx, &[&inv.apply(&[&c]), &c.gt(&zero)]);
        let stepped = Int::sub(ctx, &[&c, &dec]);
        let head = inv.apply(&[&stepped]);
        let rule = z3::ast::forall_const(ctx, &[&c as &dyn Ast], &[], &body.implies(&head));
        fp.add_rule(&rule, "step");
    }

    // (3) bad:  ∀c. (Inv(c) ∧ c ≤ 0 ∧ c ≠ 0) → Bad
    {
        let c = Int::new_const(ctx, "c");
        let halted = c.le(&zero);
        let nonzero = c._eq(&zero).not();
        let body = Bool::and(ctx, &[&inv.apply(&[&c]), &halted, &nonzero]);
        let head = bad.apply(&[]);
        let rule = z3::ast::forall_const(ctx, &[&c as &dyn Ast], &[], &body.implies(&head));
        fp.add_rule(&rule, "bad");
    }

    // query: is Bad reachable?
    fp.query(&bad.apply(&[]))
}
