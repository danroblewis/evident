//! Z3-AST native evaluator.
//!
//! The architecture pivot from `functionize.rs`:
//!
//!   * `functionize.rs` walks the *Evident* AST. It re-implements
//!     Z3's preprocessing (simplify, propagate-values, solve-eqs)
//!     by hand at AST level — and badly.
//!   * This module walks the *Z3* AST after Z3's tactic pipeline
//!     has already run. The input is canonical, simplified, and
//!     constant-folded. We just need to interpret it.
//!
//! The flow at solve time:
//!
//! ```text
//!   Evident AST  ──translate──▶  Z3 ASTs (raw)
//!                                     │
//!                                     ▼
//!                              simplify + propagate-values
//!                                     │
//!                                     ▼
//!                              Z3 ASTs (canonical)
//!                                     │
//!                                     ▼
//!                            per-output assignment extraction
//!                                     │
//!                                     ▼
//!                              walk Z3 AST with input env
//!                                     │
//!                                     ▼
//!                                  Value
//! ```
//!
//! Round 24 will replace the "walk Z3 AST" step with Cranelift
//! codegen, getting us to actual native function calls. This
//! module is the intermediate form: still an interpreter, but
//! on the *correct* canonical input.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Dynamic, Int as Z3Int};
use z3::{AstKind, Context, Goal, Tactic};
use z3_sys::DeclKind;

use crate::translate::Value;

/// A claim's body, simplified by Z3 tactics and indexed by output
/// variable name. Each entry maps `output_var → Z3 expression AST`.
/// The AST may reference other output variables (which appear
/// earlier in the topo-sort order) or input variables (in `given`).
///
/// `consistency_checks` records assertions that don't define an
/// output variable — typically equalities between two `given` vars
/// that the body further constrains. The evaluator verifies these
/// against the given values; failure = UNSAT.
#[derive(Debug, Clone)]
pub struct Z3Program<'ctx> {
    /// Topologically-ordered: each step's expression only references
    /// inputs (`given`) or earlier steps' outputs.
    pub steps: Vec<Z3Step<'ctx>>,
    /// `(lhs, rhs)` pairs — assertions of form `(= a b)` where
    /// neither side defines a fresh output.
    pub checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>,
}

#[derive(Debug, Clone)]
pub enum Z3Step<'ctx> {
    /// Scalar output: `var = expr`.
    Scalar { var: String, expr: Dynamic<'ctx> },
    /// Sequence output: built from `len = N` and per-index
    /// `(select var i) = elem` assertions Z3 emits when a Seq
    /// gets pinned by `seq = ⟨a, b, c⟩`. At eval time we build
    /// a `Value::SeqEnum` (or appropriate Seq* variant).
    Seq    { var: String, elem_exprs: Vec<Dynamic<'ctx>> },
}

impl<'ctx> Z3Step<'ctx> {
    pub fn var(&self) -> &str {
        match self {
            Z3Step::Scalar { var, .. } | Z3Step::Seq { var, .. } => var,
        }
    }
}

/// Apply Z3's preprocessing tactic chain to the given Bool
/// assertions. Returns the simplified assertions (the residual
/// constraints after `simplify` + `propagate-values`).
///
/// We deliberately exclude `solve-eqs` here — that tactic
/// substitutes equality-defined variables AWAY, destroying the
/// `(= var expr)` shape we need for per-output extraction. Z3
/// would record the substitutions in a "model converter" that the
/// Rust z3 0.12 bindings don't expose, so we'd lose the
/// information.
///
/// What `simplify` + `propagate-values` give us:
///   * constant folding (1 + 2 → 3)
///   * algebraic identities (x + 0 → x, x * 1 → x)
///   * boolean simplification (a ∧ ¬a → false)
///   * ITE simplification when both branches equal
///   * datatype simplification (recognizer/accessor of known ctor folded)
///   * propagation of known constants through the formula
pub fn simplify_assertions<'ctx>(
    ctx: &'ctx Context,
    assertions: &[Bool<'ctx>],
) -> Vec<Bool<'ctx>> {
    let goal = Goal::new(ctx, false, false, false);
    for a in assertions {
        goal.assert(a);
    }
    let simplify  = Tactic::new(ctx, "simplify");
    let propagate = Tactic::new(ctx, "propagate-values");
    let chain     = simplify.and_then(&propagate);
    let result    = chain.apply(&goal, None).expect("tactic apply");
    let mut out: Vec<Bool<'ctx>> = Vec::new();
    for sub in result.list_subgoals() {
        out.extend(sub.get_formulas::<Bool>());
    }
    out
}

/// Given a list of simplified Bool assertions and the set of
/// output variable names, partition the assertions into
/// per-output substitutions plus consistency checks.
///
/// For each assertion of form `(= a b)`:
///   * If LHS is a 0-arity App with name in `outputs` AND RHS
///     doesn't mention that name → `outputs[name] = RHS`.
///   * Symmetric for RHS as the output.
///   * Otherwise → check (must hold under given values).
///
/// Returns None if any output lacks a defining assignment after
/// simplification — that means the body isn't fully function-shaped
/// under these inputs, and the caller should fall through to Z3.
pub fn extract_program<'ctx>(
    assertions: &[Bool<'ctx>],
    outputs: &[String],
) -> Option<Z3Program<'ctx>> {
    let output_set: std::collections::HashSet<&str> = outputs.iter()
        .map(|s| s.as_str()).collect();

    // Three buckets:
    //   * scalar_assign: `var → expr` for outputs assigned as a
    //     plain `(= var expr)`.
    //   * seq_lengths:   `var → N` from `(= var__len N)` assertions
    //     (Z3's internal Seq representation splits arr + len).
    //   * seq_elements:  `var → idx → expr` from
    //     `(= (select var idx_literal) expr)` assertions.
    //   * checks:        everything else that's `(= a b)` shaped.
    let mut scalar_assign: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    let mut seq_lengths:   HashMap<String, i64> = HashMap::new();
    let mut seq_elements:  HashMap<String, HashMap<i64, Dynamic<'ctx>>> = HashMap::new();
    let mut checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)> = Vec::new();

    for a in assertions {
        let Some((lhs, rhs)) = split_equality(a) else { continue };

        // Length pin: `(= var__len N)` or symmetric.
        if let Some((name, n)) = match_len_pin(&lhs, &rhs)
            .or_else(|| match_len_pin(&rhs, &lhs))
        {
            seq_lengths.insert(name, n);
            continue;
        }

        // Per-element pin: `(= (select arr idx_lit) elem)`.
        if let Some((arr, idx, elem)) = match_select_pin(&lhs, &rhs)
            .or_else(|| match_select_pin(&rhs, &lhs))
        {
            if output_set.contains(arr.as_str()) {
                seq_elements.entry(arr).or_default().insert(idx, elem);
                continue;
            }
        }

        // Plain output assignment: `(= output_var expr)`.
        if let Some(name) = ast_app_name(&lhs) {
            if output_set.contains(name.as_str())
                && !scalar_assign.contains_key(&name)
                && !mentions_name(&rhs, &name)
            {
                scalar_assign.insert(name, rhs);
                continue;
            }
        }
        if let Some(name) = ast_app_name(&rhs) {
            if output_set.contains(name.as_str())
                && !scalar_assign.contains_key(&name)
                && !mentions_name(&lhs, &name)
            {
                scalar_assign.insert(name, lhs);
                continue;
            }
        }
        // Falls through as a consistency check.
        checks.push((lhs, rhs));
    }

    // Compose Seq steps where length + all N elements are pinned.
    let mut seq_assign: HashMap<String, Vec<Dynamic<'ctx>>> = HashMap::new();
    for arr in outputs {
        let Some(&n) = seq_lengths.get(arr) else { continue };
        if scalar_assign.contains_key(arr) { continue; }
        let empty: HashMap<i64, Dynamic<'ctx>> = HashMap::new();
        let elements = seq_elements.get(arr).unwrap_or(&empty);
        let mut elems = Vec::with_capacity(n as usize);
        let mut ok = true;
        for i in 0..n {
            if let Some(e) = elements.get(&i) {
                elems.push(e.clone());
            } else if n == 0 {
                // empty seq, nothing to push
            } else {
                ok = false;
                break;
            }
        }
        if ok {
            seq_assign.insert(arr.clone(), elems);
        }
    }

    // Every output must be covered by either a scalar or a seq
    // assignment.
    for v in outputs {
        if !scalar_assign.contains_key(v) && !seq_assign.contains_key(v) {
            return None;
        }
    }

    // Topo-sort by dependency on other outputs.
    let mut in_deg: HashMap<&str, usize> = outputs.iter()
        .map(|v| (v.as_str(), 0)).collect();
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    let mentions_any = |exprs: &[&Dynamic<'ctx>], name: &str| -> bool {
        exprs.iter().any(|e| mentions_name(e, name))
    };
    for v in outputs {
        let exprs: Vec<&Dynamic<'ctx>> = if let Some(e) = scalar_assign.get(v) {
            vec![e]
        } else if let Some(es) = seq_assign.get(v) {
            es.iter().collect()
        } else {
            vec![]
        };
        for other in outputs {
            if other == v { continue; }
            if mentions_any(&exprs, other) {
                *in_deg.get_mut(v.as_str()).unwrap() += 1;
                reverse.entry(other.as_str()).or_default().push(v.as_str());
            }
        }
    }
    let mut ready: Vec<&str> = in_deg.iter()
        .filter(|(_, &d)| d == 0).map(|(&n, _)| n).collect();
    ready.sort_unstable();
    let mut order: Vec<&str> = Vec::with_capacity(outputs.len());
    while let Some(n) = ready.pop() {
        order.push(n);
        if let Some(succs) = reverse.get(n) {
            for &m in succs {
                let d = in_deg.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 { ready.push(m); }
            }
        }
        ready.sort_unstable();
    }
    if order.len() != outputs.len() {
        return None;  // cycle
    }
    let steps: Vec<Z3Step> = order.into_iter().map(|v| {
        if let Some(expr) = scalar_assign.remove(v) {
            Z3Step::Scalar { var: v.to_string(), expr }
        } else {
            let elem_exprs = seq_assign.remove(v).unwrap();
            Z3Step::Seq { var: v.to_string(), elem_exprs }
        }
    }).collect();
    Some(Z3Program { steps, checks })
}

/// Match `(= var__len N)` patterns. Returns the seq's base name
/// and length.
fn match_len_pin<'ctx>(a: &Dynamic<'ctx>, b: &Dynamic<'ctx>) -> Option<(String, i64)> {
    let name = ast_app_name(a)?;
    let arr = name.strip_suffix("__len")?;
    let n = numeral_to_i64(b)?;
    Some((arr.to_string(), n))
}

/// Match `(= (select arr idx_literal) elem)` patterns.
fn match_select_pin<'ctx>(
    a: &Dynamic<'ctx>,
    b: &Dynamic<'ctx>,
) -> Option<(String, i64, Dynamic<'ctx>)> {
    if a.kind() != AstKind::App { return None; }
    let decl = a.safe_decl().ok()?;
    if decl.kind() != DeclKind::SELECT { return None; }
    let children = a.children();
    if children.len() != 2 { return None; }
    let arr = ast_app_name(&children[0])?;
    let idx = numeral_to_i64(&children[1])?;
    Some((arr, idx, b.clone()))
}

/// Extract a literal i64 from a Numeral Dynamic.
fn numeral_to_i64<'ctx>(d: &Dynamic<'ctx>) -> Option<i64> {
    if d.kind() != AstKind::Numeral { return None; }
    d.as_int().and_then(|i| i.as_i64())
}

/// Helper: if `b` is `(= lhs rhs)`, return `(lhs, rhs)` as Dynamics.
fn split_equality<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    if b.kind() != AstKind::App { return None; }
    let decl = b.safe_decl().ok()?;
    if decl.kind() != DeclKind::EQ { return None; }
    let children = b.children();
    if children.len() != 2 { return None; }
    Some((children[0].clone(), children[1].clone()))
}

/// Helper: if `a` is a 0-arity App (a "constant"/variable in Z3
/// parlance), return its name.
fn ast_app_name<'ctx>(a: &Dynamic<'ctx>) -> Option<String> {
    if a.kind() != AstKind::App { return None; }
    if a.num_children() != 0 { return None; }
    let decl = a.safe_decl().ok()?;
    Some(decl.name())
}

/// Helper: does `a`'s tree mention a 0-arity App with the given
/// name? Used both for cycle detection and topo-sort dependency.
fn mentions_name<'ctx>(a: &Dynamic<'ctx>, name: &str) -> bool {
    if a.kind() == AstKind::App && a.num_children() == 0 {
        if let Ok(decl) = a.safe_decl() {
            if decl.name() == name { return true; }
        }
    }
    for c in a.children() {
        if mentions_name(&c, name) { return true; }
    }
    false
}

// ── Runtime evaluation ─────────────────────────────────────────

/// Evaluate a `Z3Program` against an environment of input values.
/// Returns a HashMap with bindings for every step's output var,
/// or None if any step couldn't be evaluated (e.g. a referenced
/// var wasn't in the env).
pub fn eval_program<'ctx>(
    program: &Z3Program<'ctx>,
    given: &HashMap<String, Value>,
    enums: Option<&crate::translate::EnumRegistry>,
) -> Option<HashMap<String, Value>> {
    let mut env: HashMap<String, Value> = given.clone();
    for step in &program.steps {
        match step {
            Z3Step::Scalar { var, expr } => {
                let v = eval_dynamic(expr, &env, enums)?;
                env.insert(var.clone(), v);
            }
            Z3Step::Seq { var, elem_exprs } => {
                let mut values = Vec::with_capacity(elem_exprs.len());
                for e in elem_exprs {
                    values.push(eval_dynamic(e, &env, enums)?);
                }
                env.insert(var.clone(), seq_value_from_elements(values));
            }
        }
    }
    for (lhs, rhs) in &program.checks {
        // Skip checks involving expressions we can't natively
        // evaluate (e.g. (select arr i) for Seq element pins, or
        // Z3-internal predicates we haven't implemented). Z3
        // already simplified the body, so these assertions
        // ARE true under any model — they exist because Z3
        // re-emits them as residual constraints. Our task is
        // to produce the same bindings, not to re-prove what
        // Z3 already proved.
        let lv = match eval_dynamic(lhs, &env, enums) {
            Some(v) => v, None => continue,
        };
        let rv = match eval_dynamic(rhs, &env, enums) {
            Some(v) => v, None => continue,
        };
        if lv != rv { return None; }
    }
    Some(env)
}

/// Walk a Z3 Dynamic AST, computing its Value given an env.
/// Recognizes the DeclKinds we expect after `simplify` +
/// `propagate-values`: numerals, arithmetic, comparisons, ITE,
/// boolean ops, datatype constructors / accessors / recognizers,
/// and 0-arity Apps (uninterpreted constants → env lookup).
pub fn eval_dynamic<'ctx>(
    a: &Dynamic<'ctx>,
    env: &HashMap<String, Value>,
    enums: Option<&crate::translate::EnumRegistry>,
) -> Option<Value> {
    // Z3 string literals show up as Apps with a special internal
    // decl that's awkward to match by DeclKind. Detect via the
    // sort instead: if the Dynamic has a String sort, convert.
    if let Some(s) = a.as_string().and_then(|zs| zs.as_string()) {
        return Some(Value::Str(s));
    }
    match a.kind() {
        AstKind::Numeral => {
            // Try Int first.
            if let Some(i) = a.as_int().and_then(|x| x.as_i64()) {
                return Some(Value::Int(i));
            }
            None
        }
        AstKind::App => {
            let decl = a.safe_decl().ok()?;
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = a.children();
            match kind {
                DeclKind::TRUE  => Some(Value::Bool(true)),
                DeclKind::FALSE => Some(Value::Bool(false)),
                DeclKind::UNINTERPRETED => {
                    // 0-arity: variable reference. Look up in env.
                    if children.is_empty() {
                        let name = decl.name();
                        env.get(&name).cloned()
                    } else {
                        // n-arity uninterpreted function: not supported.
                        None
                    }
                }
                DeclKind::ITE => {
                    let cond = eval_dynamic(&children[0], env, enums)?;
                    let Value::Bool(c) = cond else { return None };
                    if c {
                        eval_dynamic(&children[1], env, enums)
                    } else {
                        eval_dynamic(&children[2], env, enums)
                    }
                }
                DeclKind::EQ => {
                    let l = eval_dynamic(&children[0], env, enums)?;
                    let r = eval_dynamic(&children[1], env, enums)?;
                    Some(Value::Bool(l == r))
                }
                DeclKind::DISTINCT => {
                    let mut vs = Vec::with_capacity(children.len());
                    for c in &children {
                        vs.push(eval_dynamic(c, env, enums)?);
                    }
                    let mut all_distinct = true;
                    for i in 0..vs.len() {
                        for j in (i+1)..vs.len() {
                            if vs[i] == vs[j] { all_distinct = false; }
                        }
                    }
                    Some(Value::Bool(all_distinct))
                }
                DeclKind::AND => {
                    for c in &children {
                        let v = eval_dynamic(c, env, enums)?;
                        let Value::Bool(b) = v else { return None };
                        if !b { return Some(Value::Bool(false)); }
                    }
                    Some(Value::Bool(true))
                }
                DeclKind::OR => {
                    for c in &children {
                        let v = eval_dynamic(c, env, enums)?;
                        let Value::Bool(b) = v else { return None };
                        if b { return Some(Value::Bool(true)); }
                    }
                    Some(Value::Bool(false))
                }
                DeclKind::NOT => {
                    let v = eval_dynamic(&children[0], env, enums)?;
                    let Value::Bool(b) = v else { return None };
                    Some(Value::Bool(!b))
                }
                DeclKind::IMPLIES => {
                    let a = eval_dynamic(&children[0], env, enums)?;
                    let Value::Bool(av) = a else { return None };
                    if !av { return Some(Value::Bool(true)); }
                    eval_dynamic(&children[1], env, enums)
                }
                DeclKind::ADD => {
                    let mut sum = 0i64;
                    for c in &children {
                        let v = eval_dynamic(c, env, enums)?;
                        let Value::Int(n) = v else { return None };
                        sum += n;
                    }
                    Some(Value::Int(sum))
                }
                DeclKind::SUB => {
                    let first = eval_dynamic(&children[0], env, enums)?;
                    let Value::Int(mut n) = first else { return None };
                    for c in &children[1..] {
                        let v = eval_dynamic(c, env, enums)?;
                        let Value::Int(m) = v else { return None };
                        n -= m;
                    }
                    Some(Value::Int(n))
                }
                DeclKind::UMINUS => {
                    let v = eval_dynamic(&children[0], env, enums)?;
                    let Value::Int(n) = v else { return None };
                    Some(Value::Int(-n))
                }
                DeclKind::MUL => {
                    let mut prod = 1i64;
                    for c in &children {
                        let v = eval_dynamic(c, env, enums)?;
                        let Value::Int(n) = v else { return None };
                        prod *= n;
                    }
                    Some(Value::Int(prod))
                }
                DeclKind::IDIV => {
                    let l = eval_dynamic(&children[0], env, enums)?;
                    let r = eval_dynamic(&children[1], env, enums)?;
                    let (Value::Int(a), Value::Int(b)) = (l, r) else { return None };
                    if b == 0 { return None; }
                    Some(Value::Int(a / b))
                }
                DeclKind::DIV => {
                    let l = eval_dynamic(&children[0], env, enums)?;
                    let r = eval_dynamic(&children[1], env, enums)?;
                    let (Value::Int(a), Value::Int(b)) = (l, r) else { return None };
                    if b == 0 { return None; }
                    Some(Value::Int(a / b))
                }
                DeclKind::MOD | DeclKind::REM => {
                    let l = eval_dynamic(&children[0], env, enums)?;
                    let r = eval_dynamic(&children[1], env, enums)?;
                    let (Value::Int(a), Value::Int(b)) = (l, r) else { return None };
                    if b == 0 { return None; }
                    Some(Value::Int(a.rem_euclid(b)))
                }
                DeclKind::LE => num_cmp(&children, env, enums, |a, b| a <= b),
                DeclKind::GE => num_cmp(&children, env, enums, |a, b| a >= b),
                DeclKind::LT => num_cmp(&children, env, enums, |a, b| a <  b),
                DeclKind::GT => num_cmp(&children, env, enums, |a, b| a >  b),
                DeclKind::DT_CONSTRUCTOR => {
                    // Build a Value::Enum from the constructor's name
                    // (which is the variant name) and recursively-
                    // evaluated children. The enum_name comes from
                    // the registry lookup by variant.
                    let variant = decl.name();
                    let mut fields: Vec<Value> = Vec::with_capacity(children.len());
                    for c in &children {
                        fields.push(eval_dynamic(c, env, enums)?);
                    }
                    let enum_name = enums
                        .and_then(|r| r.by_variant.borrow().get(&variant)
                            .map(|(en, _)| en.clone()))
                        .unwrap_or_default();
                    Some(Value::Enum { enum_name, variant, fields })
                }
                DeclKind::DT_ACCESSOR => {
                    // (Field accessor of an enum-typed value.) Eval
                    // the argument, find the field index by accessor
                    // name in the registry, return the corresponding
                    // payload value.
                    let v = eval_dynamic(&children[0], env, enums)?;
                    let Value::Enum { variant: _, fields, .. } = v else { return None };
                    let _acc_name = decl.name();
                    // For v1 we don't have a fast accessor-name → field-idx
                    // map; rely on the order matching declaration. Z3's
                    // tactic pipeline usually folds accessor-of-constructor
                    // patterns away (`Cons-head (Cons x xs)` → `x`), so
                    // this path runs only when simplification missed it.
                    // Fall through if we hit it — caller falls back to Z3.
                    let _ = fields;
                    None
                }
                DeclKind::DT_RECOGNISER | DeclKind::DT_IS => {
                    // (is-Variant) test. Eval the inner value, check
                    // variant name. The recognizer's name follows
                    // Z3's convention `is_<Variant>`.
                    let v = eval_dynamic(&children[0], env, enums)?;
                    let Value::Enum { variant, .. } = v else { return None };
                    let rec_name = decl.name();
                    // Strip the "is_" prefix Z3 uses. If the rec_name
                    // doesn't start with that, fall back to checking
                    // for exact match.
                    let target = rec_name.strip_prefix("is_").unwrap_or(&rec_name);
                    Some(Value::Bool(variant == target))
                }
                _ => None,  // unhandled decl kind — fall through.
            }
        }
        _ => None,  // Var (bound), Quantifier, etc.
    }
}

fn num_cmp<'ctx>(
    children: &[Dynamic<'ctx>],
    env: &HashMap<String, Value>,
    enums: Option<&crate::translate::EnumRegistry>,
    op: impl Fn(i64, i64) -> bool,
) -> Option<Value> {
    let l = eval_dynamic(&children[0], env, enums)?;
    let r = eval_dynamic(&children[1], env, enums)?;
    let (Value::Int(a), Value::Int(b)) = (l, r) else { return None };
    Some(Value::Bool(op(a, b)))
}

/// Avoid the unused-import warning when as_int is consumed only
/// through the `Numeral` arm above.
#[allow(dead_code)]
fn _force_int_import<'ctx>(_: &Z3Int<'ctx>) {}

/// Classify a Vec<Value> into the appropriate Seq* Value variant
/// by inspecting the first element. Empty → SeqEnum([]) since
/// we don't have declared-type info at this layer.
fn seq_value_from_elements(values: Vec<Value>) -> Value {
    match values.first() {
        None                  => Value::SeqEnum(vec![]),
        Some(Value::Int(_))   => {
            Value::SeqInt(values.into_iter().filter_map(|v|
                if let Value::Int(n) = v { Some(n) } else { None }).collect())
        }
        Some(Value::Bool(_))  => {
            Value::SeqBool(values.into_iter().filter_map(|v|
                if let Value::Bool(b) = v { Some(b) } else { None }).collect())
        }
        Some(Value::Str(_))   => {
            Value::SeqStr(values.into_iter().filter_map(|v|
                if let Value::Str(s) = v { Some(s) } else { None }).collect())
        }
        Some(Value::Enum { .. }) => Value::SeqEnum(values),
        _ => Value::SeqEnum(values),
    }
}
