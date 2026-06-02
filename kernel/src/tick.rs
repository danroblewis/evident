//! Tick loop: solve, walk effects, dispatch, repeat.
//!
//! Z3 lifecycle (per docs/plans/architecture-invariants.md §"Z3 model
//! lifecycle", fix proposal docs/plans/kernel-fix-incremental-solving.md):
//!   - The program body is parsed ONCE; its asserted ASTs are cached.
//!   - A single `.simplify()` pass runs over the cached body BEFORE the loop
//!     (invariant #4 permits exactly one pre-loop simplify; per-tick simplify
//!     stays forbidden). The simplified ASTs are what every tick re-uses.
//!   - Each tick layers only the tick-local equality pins (state-carry
//!     `_<name>`, `last_results`, `is_first_tick`), then solves.
//!   - The pins are built by parsing a tiny string of `<declarations
//!     extracted from the body> + <equality asserts>`. Re-declaring the
//!     symbols makes them intern to the same variables as the cached body ASTs
//!     (Z3 hash-conses sorts + func_decls within a context).
//!
//! Two pin mechanisms, selectable at runtime via `EVIDENT_PIN_MECH`:
//!   - A (default; unset or `=A`): "cached-ASTs + pre-loop simplify". Each
//!     tick gets a FRESH solver, re-asserts the cached simplified body ASTs
//!     (no re-parse — Z3 keeps them interned), asserts the pin ASTs, checks.
//!     Per-tick pin cost is O(K) in the number of pins, independent of body
//!     size; the body asserts are by-reference re-uses, not rebuilds.
//!   - B (`=B`): "check-with-assumptions" — the legacy FsmRunner's
//!     `s.check(*pins)` shape (legacy-python/docs/runtime-architecture.md
//!     §"Architecture A is a library pattern on Architecture B"). ONE
//!     persistent solver holds the simplified body for the program's life;
//!     each tick passes the pin ASTs as assumptions to
//!     `Z3_solver_check_assumptions`.
//!
//! Note: an earlier attempt asserted the body onto ONE persistent solver and
//! used `push`/`pop` per tick. That is the literal shape of the fix proposal,
//! but it regressed multi-tick datatype-state fixtures ~36x (the incremental
//! solver forgoes the one-shot preprocessing a fresh solve applies to the
//! growing carried-state pins) — a kernel test timed out at 30s. Caching the
//! parsed ASTs removes the audit's per-tick re-parse cost (invariant #1)
//! without that regression. B shares that incremental-mode characteristic and
//! is offered for benchmarking, not as the default. See
//! docs/plans/kernel-fix-incremental-solving.md.

use std::ffi::{CStr, CString};
use std::ptr;
use std::time::Instant;

use crate::manifest::Manifest;
use z3_sys::*;

/// Per-tick pin mechanism, chosen once at startup from `EVIDENT_PIN_MECH`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mech {
    /// A (default): fresh solver per tick, re-assert cached simplified body
    /// ASTs + assert the pin ASTs.
    A,
    /// B: one persistent solver holding the body; pin ASTs passed as
    /// assumptions to `Z3_solver_check_assumptions` each tick.
    B,
}

#[derive(Debug, Clone)]
pub enum Sv {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    /// A Datatype-typed value: (variant constructor name, recursively-decoded payload values).
    /// Lets the kernel carry algebraic data (e.g. a TokenList) across ticks.
    Datatype(String, Vec<Sv>),
    /// A bounded Seq value (e.g. `Seq(Rect)`), one element per slot. Produced
    /// by the functionizer's record-Seq recomposition so a scalar step can
    /// index into it (`rs[0].w`); never a primitive state-carry, so it has no
    /// SMT-LIB pin form.
    Seq(Vec<Sv>),
}

impl Sv {
    /// Emit as SMT-LIB literal expression suitable for an `(assert (= ...))`.
    fn smtlib(&self) -> String {
        match self {
            Sv::Int(n) if *n >= 0  => n.to_string(),
            Sv::Int(n)             => format!("(- {})", -n),
            Sv::Bool(b)            => b.to_string(),
            Sv::Str(s)             => z3_string_literal(s),
            Sv::Real(r) if *r >= 0.0 => format!("{:?}", r),
            Sv::Real(r)            => format!("(- {:?})", -r),
            Sv::Datatype(variant, fields) => {
                if fields.is_empty() {
                    variant.clone()
                } else {
                    let parts: Vec<String> = fields.iter().map(|f| f.smtlib()).collect();
                    format!("({} {})", variant, parts.join(" "))
                }
            }
            // Seq values are functionizer-internal (record-Seq intermediates)
            // and never carried as a primitive state field, so this is unused.
            Sv::Seq(_) => unreachable!("Sv::Seq has no SMT-LIB pin form"),
        }
    }
}

/// Emit `s` as an SMT-LIB string literal (with surrounding quotes), escaping
/// every non-ASCII codepoint as `\u{hex}`.
///
/// This is the root-cause fix for the multi-byte UTF-8 state-carry growth bug
/// (docs/plans/blocked-compiler-driver.md). The kernel re-asserts carried
/// String values each tick by writing them into a tiny SMT-LIB pin string that
/// Z3's parser then reads. Z3's SMT-LIB parser consumes a string literal's
/// bytes one at a time: a raw UTF-8 byte sequence like `∈` (E2 88 88) becomes
/// THREE Z3 characters, not one. So `"a∈b"` written raw parses to a 5-char Z3
/// string; reading it back (`Z3_get_string` + `unescape_z3`) yields three
/// Latin-1 codepoints whose UTF-8 re-encoding is longer still — the string
/// grows 5 → 8 → 14 → … every tick and `#input` never stabilises.
///
/// Escaping non-ASCII to `\u{hex}` makes Z3 store the real codepoint as ONE
/// character, and `unescape_z3` (the read side) already reverses it — so the
/// carry round-trips losslessly. Mirrors bootstrap
/// `translate::extract::escape_non_ascii` (the reference encode-side fix for
/// the sibling emit bug #16). `"` is doubled per SMT-LIB string escaping;
/// ASCII (incl. control chars / backslash) passes through unchanged, matching
/// what `unescape_z3` expects.
fn z3_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push_str("\"\"");
        } else if c.is_ascii() {
            out.push(c);
        } else {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        }
    }
    out.push('"');
    out
}

pub fn run(src: &str, manifest: &Manifest) -> Result<u8, String> {
    unsafe { run_inner(src, manifest) }
}

unsafe fn run_inner(src: &str, manifest: &Manifest) -> Result<u8, String> {
    let cfg = Z3_mk_config();
    let ctx = Z3_mk_context(cfg);
    Z3_del_config(cfg);

    // Build the model ONCE: parse the body a single time and CACHE its asserted
    // ASTs. The cached ASTs are re-asserted into a fresh solver each tick — no
    // per-tick re-parse of the body (the audit's dominant cost for large
    // compiler.smt2), while each tick keeps Z3's one-shot preprocessing.
    let body_vec = {
        let body_cstr = match CString::new(src) {
            Ok(c) => c,
            Err(e) => {
                Z3_del_context(ctx);
                return Err(format!("smtlib body has interior NUL: {e}"));
            }
        };
        let empty_sym: Vec<Z3_symbol> = Vec::new();
        let empty_sort: Vec<Z3_sort> = Vec::new();
        let empty_decl: Vec<Z3_func_decl> = Vec::new();
        let asts = Z3_parse_smtlib2_string(
            ctx, body_cstr.as_ptr(),
            0, empty_sym.as_ptr(), empty_sort.as_ptr(),
            0, empty_sym.as_ptr(), empty_decl.as_ptr(),
        );
        if asts.is_null() {
            let err_ptr = Z3_get_error_msg(ctx, Z3_get_error_code(ctx));
            let err = if err_ptr.is_null() { String::new() }
                      else { CStr::from_ptr(err_ptr).to_string_lossy().into_owned() };
            Z3_del_context(ctx);
            return Err(format!("smtlib parse failed: {err}"));
        }
        Z3_ast_vector_inc_ref(ctx, asts);
        asts
    };
    let body_n = Z3_ast_vector_size(ctx, body_vec);

    // Pre-loop `.simplify()` pass (architecture-invariants.md §"Z3 model
    // lifecycle" #4: ONE simplify before the loop is allowed and desired;
    // per-tick simplify stays forbidden). Simplify each cached body assertion
    // once; the simplified ASTs are what every tick re-uses. inc_ref keeps
    // them alive for the program's lifetime, so the source vector is dropped.
    let body: Vec<Z3_ast> = (0..body_n)
        .map(|i| {
            let s = Z3_simplify(ctx, Z3_ast_vector_get(ctx, body_vec, i));
            Z3_inc_ref(ctx, s);
            s
        })
        .collect();
    Z3_ast_vector_dec_ref(ctx, body_vec);

    // Pin mechanism, selectable at runtime. Default A. See the module doc.
    let mech = match std::env::var("EVIDENT_PIN_MECH").ok().as_deref() {
        Some("B") | Some("b") => Mech::B,
        _ => Mech::A,
    };

    // Declarations (datatypes, consts) extracted from the body. Each tick's
    // tiny pin string re-declares these so its symbols intern to the same
    // base-scope variables — including ones the body declares but never
    // references in an assert (e.g. `is_first_tick`, `last_results`), which a
    // post-parse AST walk could not recover.
    let decl_preamble = extract_declarations(src);

    // B only: one persistent solver, simplified body asserted once. Each tick
    // layers pins as check-assumptions instead of re-asserting the body.
    let persistent_solver = if mech == Mech::B {
        let s = Z3_mk_solver(ctx);
        Z3_solver_inc_ref(ctx, s);
        for &a in &body {
            Z3_solver_assert(ctx, s, a);
        }
        Some(s)
    } else {
        None
    };

    // Functionizer (task #18). After parse + pre-loop simplify, attempt to
    // extract a native/JIT program for {state fields, effects}. `functionize`
    // verifies its own output against a real Z3 solve on tick 0 and tick 1; on
    // any mismatch (or an unsupported shape) it returns None and the kernel
    // runs the existing Z3 path unchanged. Two env flags gate it:
    //   EVIDENT_FUNCTIONIZE=0      → skip entirely (prior kernel behaviour).
    //   EVIDENT_FUNCTIONIZE_JIT=0  → extract + interpret, but don't JIT.
    let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE").ok().as_deref() != Some("0");
    let jit_on = std::env::var("EVIDENT_FUNCTIONIZE_JIT").ok().as_deref() != Some("0");

    // Diagnostics (task #22). Three env-gated levels, off by default. See
    // docs/plans/functionizer-integration.md §"Diagnostic flags".
    //   EVIDENT_FUNCTIONIZE_STATS=1        → one-line summary at exit.
    //   EVIDENT_FUNCTIONIZE_STATS=verbose  → summary + per-step load report.
    //   EVIDENT_FUNCTIONIZE_TRACE=1        → per-tick timing lines.
    let stats_level = crate::functionize::StatsLevel::from_env();
    let trace = std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok();

    let (functionized, mut stats) = if functionize_on {
        crate::functionize::functionize(ctx, &body, manifest, &decl_preamble, jit_on, stats_level, trace)
    } else {
        let mut s = crate::functionize::FunctionizeStats::new(stats_level, trace);
        s.disabled = true;
        s.total_asserts = body.len();
        s.residual = body.len();
        s.refuse_reason = Some("EVIDENT_FUNCTIONIZE=0".to_string());
        (None, s)
    };
    if trace {
        match &functionized {
            Some(p) => eprintln!(
                "[fz] functionized: {} steps ({} jit, {} interp), {} predicates",
                p.steps.len(), p.jit_count, p.interp_count, p.predicates.len()),
            None => eprintln!("[fz] not functionized — running Z3 path"),
        }
    }
    stats.print_load_report();
    let timing_on = stats.timing_on();

    let mut prev_state: Vec<Option<Sv>> = vec![None; manifest.state_fields.len()];
    let mut prev_results: Vec<Res> = Vec::new();
    let mut is_first = true;

    // T_total spans only the tick loop (not the one-shot functionize/verify
    // setup above). `mark()` returns an Instant when timing is on, else None,
    // so the off path makes no syscall.
    stats.loop_start = if timing_on { Some(Instant::now()) } else { None };
    let mark = || if timing_on { Some(Instant::now()) } else { None };
    let since = |t: Option<Instant>| t.map(|t| t.elapsed()).unwrap_or_default();

    const TICK_LIMIT: usize = 100_000;
    for tick in 0..TICK_LIMIT {
        let mut tick_func = std::time::Duration::ZERO;
        let mut tick_z3 = std::time::Duration::ZERO;
        let mut tick_dispatch = std::time::Duration::ZERO;
        // Functionizer fast path: evaluate the extracted program (native or
        // JIT) for this tick and skip Z3 entirely. `run_program` returns None
        // for any shape/predicate it can't honour this tick → fall through to
        // the Z3 solve below. (`prev_results` is intentionally not threaded in:
        // a body that reads `last_results` won't have verified, so it never
        // reaches here.)
        if let Some(prog) = &functionized {
            let tf = mark();
            let inputs = crate::functionize::build_inputs(is_first, &prev_state, manifest);
            let run_opt = crate::functionize::run_program(ctx, prog, &inputs);
            let dt = since(tf);
            tick_func += dt;
            stats.t_func += dt;
            if let Some(run) = run_opt {
                let mut new_state: Vec<Sv> = Vec::with_capacity(manifest.state_fields.len());
                let mut covered = true;
                for (name, _) in &manifest.state_fields {
                    match run.scalars.get(name) {
                        Some(v) => new_state.push(v.clone()),
                        None => { covered = false; break; }
                    }
                }
                if covered {
                    let td = mark();
                    let mut exit_code: Option<u8> = None;
                    let mut new_results: Vec<Res> = Vec::new();
                    for eff in run.effects.iter().take(manifest.max_effects) {
                        match dispatch_effect_sv(eff)? {
                            EffectOutcome::Continue(r) => new_results.push(r),
                            EffectOutcome::Exit(code) => { exit_code = Some(code); break; }
                        }
                    }
                    let dd = since(td);
                    tick_dispatch += dd;
                    stats.t_dispatch += dd;
                    stats.ticks += 1;
                    if trace {
                        eprintln!("[functionizer] tick {tick}: {:.2}ms func / {:.2}ms z3 / {:.2}ms dispatch",
                            tick_func.as_secs_f64() * 1000.0, tick_z3.as_secs_f64() * 1000.0,
                            tick_dispatch.as_secs_f64() * 1000.0);
                    }
                    if let Some(code) = exit_code {
                        if let Some(s) = persistent_solver { Z3_solver_dec_ref(ctx, s); }
                        Z3_del_context(ctx);
                        return Ok(code);
                    }
                    let stuck = !is_first && prev_state.iter().zip(new_state.iter())
                        .all(|(p, n)| matches!(p, Some(pv) if compare_sv(pv, n)));
                    if stuck {
                        eprintln!("kernel: stuck (state unchanged with no Exit emitted)");
                        if let Some(s) = persistent_solver { Z3_solver_dec_ref(ctx, s); }
                        Z3_del_context(ctx);
                        return Ok(1);
                    }
                    prev_state = new_state.into_iter().map(Some).collect();
                    prev_results = new_results;
                    is_first = false;
                    continue;
                }
            }
            // run_program refused this tick → fall through to the Z3 path.
        }
        let tz = mark();
        // A: fresh solver re-asserting the cached simplified body (no parse),
        // so Z3 preprocesses the whole problem each tick. B: the persistent
        // solver, body already loaded once above.
        let solver = match persistent_solver {
            Some(s) => s,
            None => {
                let s = Z3_mk_solver(ctx);
                Z3_solver_inc_ref(ctx, s);
                for &a in &body {
                    Z3_solver_assert(ctx, s, a);
                }
                s
            }
        };

        // Build ONLY the per-tick equality pins. The declarations preamble makes
        // the pin symbols re-declare and intern to the cached body's variables.
        let mut pins = String::with_capacity(decl_preamble.len() + 256);
        pins.push_str(&decl_preamble);

        if !is_first {
            for (idx, (name, _)) in manifest.state_fields.iter().enumerate() {
                if let Some(v) = &prev_state[idx] {
                    emit_state_pin(&mut pins, name, v);
                }
            }
        }
        // last_results: assert length + per-index value. On tick 0 the array
        // is empty (length 0); subsequent ticks carry the prior tick's results.
        pins.push_str(&format!("(assert (= last_results__len {}))\n", prev_results.len()));
        for (i, r) in prev_results.iter().enumerate() {
            pins.push_str(&format!("(assert (= (select last_results {i}) {}))\n", r.smtlib()));
        }
        if is_first {
            pins.push_str("(assert is_first_tick)\n");
        } else {
            pins.push_str("(assert (not is_first_tick))\n");
        }

        // Parse the pin string into the list of equality ASTs (the body's
        // variables, re-interned via the declaration preamble).
        let (pin_vec, pin_asts) = match parse_pins(ctx, &pins) {
            Ok(x) => x,
            Err(err) => {
                Z3_solver_dec_ref(ctx, solver);
                Z3_del_context(ctx);
                return Err(format!("smtlib parse failed on tick {tick}: {err}"));
            }
        };

        // The A/B fork: assert-then-check vs. check-with-assumptions.
        let res = match mech {
            Mech::A => apply_pins_a(ctx, solver, &pin_asts),
            Mech::B => apply_pins_b(ctx, solver, &pin_asts),
        };
        Z3_ast_vector_dec_ref(ctx, pin_vec);

        if res == Z3_L_FALSE {
            eprintln!("kernel: UNSAT on tick {tick}");
            Z3_solver_dec_ref(ctx, solver);
            Z3_del_context(ctx);
            return Ok(2);
        }
        if res != Z3_L_TRUE {
            Z3_solver_dec_ref(ctx, solver);
            Z3_del_context(ctx);
            return Err(format!("solver returned UNKNOWN on tick {tick}"));
        }

        let model = Z3_solver_get_model(ctx, solver);
        Z3_model_inc_ref(ctx, model);

        // Read state values.
        let mut new_state: Vec<Sv> = Vec::with_capacity(manifest.state_fields.len());
        for (name, ty) in &manifest.state_fields {
            new_state.push(read_state_var(ctx, model, name, ty)?);
        }

        // Read effects length + walk.
        let effects_len = read_int_const(ctx, model, &format!("{}__len", manifest.effects_name))?;
        let effects_len = effects_len.min(manifest.max_effects as i64).max(0) as usize;

        let effects_decl = find_const_decl(ctx, model, &manifest.effects_name)
            .ok_or_else(|| format!("effects var `{}` not in model", manifest.effects_name))?;
        let effects_ast = Z3_mk_app(ctx, effects_decl, 0, ptr::null());
        let int_sort = Z3_mk_int_sort(ctx);
        let dz = since(tz);
        tick_z3 += dz;
        stats.t_z3 += dz;

        let td = mark();
        let mut exit_code: Option<u8> = None;
        let mut new_results: Vec<Res> = Vec::new();
        for i in 0..effects_len {
            let i_ast = Z3_mk_int64(ctx, i as i64, int_sort);
            let select_ast = Z3_mk_select(ctx, effects_ast, i_ast);
            let mut eval_out: Z3_ast = ptr::null_mut();
            let ok = Z3_model_eval(ctx, model, select_ast, true, &mut eval_out);
            if !ok {
                return Err(format!("model eval failed for effects[{i}]"));
            }
            match dispatch_effect(ctx, eval_out)? {
                EffectOutcome::Continue(r) => { new_results.push(r); }
                EffectOutcome::Exit(code)  => { exit_code = Some(code); break; }
            }
        }
        let dd = since(td);
        tick_dispatch += dd;
        stats.t_dispatch += dd;
        stats.ticks += 1;
        if trace {
            eprintln!("[functionizer] tick {tick}: {:.2}ms func / {:.2}ms z3 / {:.2}ms dispatch",
                tick_func.as_secs_f64() * 1000.0, tick_z3.as_secs_f64() * 1000.0,
                tick_dispatch.as_secs_f64() * 1000.0);
        }

        if let Some(code) = exit_code {
            Z3_model_dec_ref(ctx, model);
            Z3_solver_dec_ref(ctx, solver);
            Z3_del_context(ctx);
            return Ok(code);
        }

        // Stuck halt check.
        let stuck = !is_first && prev_state.iter().zip(new_state.iter())
            .all(|(p, n)| matches!(p, Some(pv) if compare_sv(pv, n)));
        Z3_model_dec_ref(ctx, model);
        if stuck {
            eprintln!("kernel: stuck (state unchanged with no Exit emitted)");
            Z3_solver_dec_ref(ctx, solver);
            Z3_del_context(ctx);
            return Ok(1);
        }
        // A's solver is fresh per tick — drop it. B's persistent solver is
        // kept for the next tick and freed once the loop exits.
        if mech == Mech::A {
            Z3_solver_dec_ref(ctx, solver);
        }

        prev_state   = new_state.into_iter().map(Some).collect();
        prev_results = new_results;
        is_first     = false;
    }

    if let Some(s) = persistent_solver {
        Z3_solver_dec_ref(ctx, s);
    }
    Z3_del_context(ctx);
    Err(format!("tick limit ({TICK_LIMIT}) reached"))
}

/// Parse the per-tick pin string into the list of equality ASTs. Returns the
/// owning `Z3_ast_vector` (inc_ref'd; the caller dec_refs it after the check
/// + model read) alongside a flat `Vec<Z3_ast>` of the assertion bodies. The
/// declaration preamble in `pins` re-interns the symbols to the body's
/// base-scope variables.
unsafe fn parse_pins(ctx: Z3_context, pins: &str) -> Result<(Z3_ast_vector, Vec<Z3_ast>), String> {
    let cstr = CString::new(pins).map_err(|e| format!("pin string has interior NUL: {e}"))?;
    let empty_sym: Vec<Z3_symbol> = Vec::new();
    let empty_sort: Vec<Z3_sort> = Vec::new();
    let empty_decl: Vec<Z3_func_decl> = Vec::new();
    let v = Z3_parse_smtlib2_string(
        ctx, cstr.as_ptr(),
        0, empty_sym.as_ptr(), empty_sort.as_ptr(),
        0, empty_sym.as_ptr(), empty_decl.as_ptr(),
    );
    if v.is_null() {
        let err_ptr = Z3_get_error_msg(ctx, Z3_get_error_code(ctx));
        let err = if err_ptr.is_null() { String::new() }
                  else { CStr::from_ptr(err_ptr).to_string_lossy().into_owned() };
        return Err(err);
    }
    Z3_ast_vector_inc_ref(ctx, v);
    let n = Z3_ast_vector_size(ctx, v);
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        out.push(Z3_ast_vector_get(ctx, v, i));
    }
    Ok((v, out))
}

/// Mechanism A: assert the pin ASTs onto the (fresh) solver, then a one-shot
/// `check`. Z3 preprocesses body + pins together each tick.
unsafe fn apply_pins_a(ctx: Z3_context, solver: Z3_solver, pin_asts: &[Z3_ast]) -> Z3_lbool {
    for &a in pin_asts {
        Z3_solver_assert(ctx, solver, a);
    }
    Z3_solver_check(ctx, solver)
}

/// Mechanism B: pass the pin ASTs as assumptions to the persistent solver
/// (the legacy `s.check(*pins)` shape). The body stays asserted across ticks;
/// assumptions are temporary to this check.
unsafe fn apply_pins_b(ctx: Z3_context, solver: Z3_solver, pin_asts: &[Z3_ast]) -> Z3_lbool {
    Z3_solver_check_assumptions(ctx, solver, pin_asts.len() as u32, pin_asts.as_ptr())
}

/// Extract every top-level declaration s-expression (`declare-fun`,
/// `declare-const`, `declare-datatypes`, `declare-sort`, `define-*`) from a
/// kernel `.smt2` body, preserving order. The result is prepended to each
/// tick's pin string so that the pins' symbols re-declare and intern to the
/// body's base-scope variables (Z3 hash-conses sorts + func_decls per context).
///
/// Asserts are skipped — the body is parsed once and its ASTs cached. The
/// scanner is paren-balanced and respects `"`-quoted strings (`""` escapes a
/// quote) and `;` line comments so parens inside them are not miscounted.
fn extract_declarations(src: &str) -> String {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut out = String::new();
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut form_start = 0usize;
    let mut in_string = false;
    while i < n {
        let c = bytes[i];
        if in_string {
            if c == b'"' {
                if i + 1 < n && bytes[i + 1] == b'"' { i += 2; continue; } // escaped ""
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            b';' => { while i < n && bytes[i] != b'\n' { i += 1; } }
            b'"' => { in_string = true; i += 1; }
            b'(' => {
                if depth == 0 { form_start = i; }
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    let form = &src[form_start..i];
                    if is_declaration_form(form) {
                        out.push_str(form);
                        out.push('\n');
                    }
                }
            }
            _ => { i += 1; }
        }
    }
    out
}

/// True when a top-level s-expression's head keyword is a declaration (not an
/// assertion or a solver command).
fn is_declaration_form(form: &str) -> bool {
    const KW: &[&str] = &[
        "declare-fun", "declare-const", "declare-datatypes", "declare-datatype",
        "declare-sort", "define-fun-rec", "define-funs-rec", "define-fun",
        "define-sort", "define-const",
    ];
    let head = form.trim_start_matches('(').trim_start();
    KW.iter().any(|k| {
        head.starts_with(k)
            && head[k.len()..].chars().next().map_or(true, |c| c.is_whitespace() || c == '(')
    })
}

// ── Effect dispatch ─────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Res {
    No,
    Int(i64),
    Str(String),
    Real(f64),
    Eof,
    Error(String),
}

impl Res {
    /// Emit as SMT-LIB constructor expression matching emit.rs's Result decl.
    fn smtlib(&self) -> String {
        match self {
            Res::No        => "NoResult".to_string(),
            Res::Eof       => "EofResult".to_string(),
            Res::Int(n)    => format!("(IntResult {})",
                if *n >= 0 { n.to_string() } else { format!("(- {})", -n) }),
            Res::Str(s)    => format!("(StringResult {})", z3_string_literal(s)),
            Res::Real(r)   => format!("(RealResult {})",
                if *r >= 0.0 { r.to_string() } else { format!("(- {})", -r) }),
            Res::Error(s)  => format!("(ErrorResult {})", z3_string_literal(s)),
        }
    }
}

enum EffectOutcome {
    Continue(Res),
    Exit(u8),
}

unsafe fn dispatch_effect(ctx: Z3_context, eff: Z3_ast) -> Result<EffectOutcome, String> {
    let app = Z3_to_app(ctx, eff);
    if app.is_null() {
        return Err(format!("effect is not an application: {}", ast_to_string(ctx, eff)));
    }
    let decl = Z3_get_app_decl(ctx, app);
    let sym = Z3_get_decl_name(ctx, decl);
    let name = decode_sym(ctx, sym);

    match name.as_str() {
        // Println, Print, Time were here in iter 1; demoted to LibCall
        // sugar in iter 2.5+. See stdlib/kernel.ev → BuildPrintln /
        // BuildPrint / BuildTime.
        "Exit" => {
            let arg0 = Z3_get_app_arg(ctx, app, 0);
            let code = decode_int_literal(ctx, arg0)?;
            let code = code.clamp(0, 255) as u8;
            Ok(EffectOutcome::Exit(code))
        }
        // Println and Time were here in iter 1; demoted to LibCall sugar
        // in iter 2.5. See stdlib/kernel.ev → BuildPrintln / BuildTime.
        "ReadLine" => {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => Ok(EffectOutcome::Continue(Res::Eof)),
                Ok(_) => {
                    if line.ends_with('\n') { line.pop(); }
                    if line.ends_with('\r') { line.pop(); }
                    Ok(EffectOutcome::Continue(Res::Str(line)))
                }
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("read_line: {e}")))),
            }
        }
        "ReadFile" => {
            let arg0 = Z3_get_app_arg(ctx, app, 0);
            let path = decode_string_literal(ctx, arg0)?;
            match std::fs::read_to_string(&path) {
                Ok(s)  => Ok(EffectOutcome::Continue(Res::Str(s))),
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("read_file({path}): {e}")))),
            }
        }
        "WriteFile" => {
            let arg0 = Z3_get_app_arg(ctx, app, 0);
            let arg1 = Z3_get_app_arg(ctx, app, 1);
            let path = decode_string_literal(ctx, arg0)?;
            let contents = decode_string_literal(ctx, arg1)?;
            match std::fs::write(&path, contents) {
                Ok(())  => Ok(EffectOutcome::Continue(Res::No)),
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("write_file({path}): {e}")))),
            }
        }
        "LibCall" => {
            let lib_ast  = Z3_get_app_arg(ctx, app, 0);
            let fn_ast   = Z3_get_app_arg(ctx, app, 1);
            let args_ast = Z3_get_app_arg(ctx, app, 2);
            let lib = decode_string_literal(ctx, lib_ast)?;
            let func = decode_string_literal(ctx, fn_ast)?;
            let args = decode_libargs(ctx, args_ast)?;
            match crate::libcall::call(&lib, &func, &args) {
                Ok(ret) => Ok(EffectOutcome::Continue(Res::Int(ret))),
                Err(e)  => Ok(EffectOutcome::Continue(Res::Error(format!("LibCall({lib}, {func}): {e}")))),
            }
        }
        other => {
            eprintln!("kernel: unknown effect variant `{other}`; skipping");
            Ok(EffectOutcome::Continue(Res::Error(format!("unknown effect variant `{other}`"))))
        }
    }
}

// ── Model reads + helpers ───────────────────────────────────────

/// Upper bound on the length of a carried `Seq(T)` state field. The work-stacks
/// these carry are bounded by AST node count; this is a runaway backstop.
const SEQ_CARRY_CAP: i64 = 100_000;

unsafe fn read_state_var(ctx: Z3_context, model: Z3_model, name: &str, ty: &str) -> Result<Sv, String> {
    // `Seq(ElemType)` state fields (the cons→Seq carry path): read the array +
    // companion `__len` and decode each element by the inner type. The element
    // type string is whatever bootstrap's discover_state_fields rendered inside
    // the parentheses (Int/Bool/String/Real or a user datatype name).
    if let Some(inner) = ty.strip_prefix("Seq(").and_then(|s| s.strip_suffix(')')) {
        return read_seq_var(ctx, model, name, inner.trim());
    }
    let decl = find_const_decl(ctx, model, name)
        .ok_or_else(|| format!("state var `{name}` not in model"))?;
    let ast = Z3_mk_app(ctx, decl, 0, ptr::null());
    let mut out: Z3_ast = ptr::null_mut();
    if !Z3_model_eval(ctx, model, ast, true, &mut out) {
        return Err(format!("model eval failed for `{name}`"));
    }
    match ty {
        "Int"    => Ok(Sv::Int(decode_int_literal(ctx, out)?)),
        "Bool"   => Ok(Sv::Bool(decode_bool_literal(ctx, out)?)),
        "String" => Ok(Sv::Str(decode_string_literal(ctx, out)?)),
        "Real"   => Ok(Sv::Real(decode_real_literal(ctx, out)?)),
        // Anything else: treat as a Datatype. Walk the value recursively.
        _        => decode_datatype_value(ctx, out),
    }
}

/// Read a `Seq(T)` state field from the model: a Z3 `(Array Int T)` named `name`
/// plus a companion `name__len` Int (bootstrap's Seq representation). Each of the
/// first `len` elements is decoded by `elem_ty`. Tolerant of an unconstrained Seq
/// that Z3 dropped from the model (the empty-effects quirk): a missing `__len` or
/// missing array decl reads back as the empty Seq, which carries correctly.
unsafe fn read_seq_var(ctx: Z3_context, model: Z3_model, name: &str, elem_ty: &str) -> Result<Sv, String> {
    let len_name = format!("{name}__len");
    let len = match find_const_decl(ctx, model, &len_name) {
        Some(_) => read_int_const(ctx, model, &len_name).unwrap_or(0),
        None => 0,
    };
    let len = len.clamp(0, SEQ_CARRY_CAP) as usize;
    let Some(arr_decl) = find_const_decl(ctx, model, name) else {
        return Ok(Sv::Seq(Vec::new()));
    };
    let arr_ast = Z3_mk_app(ctx, arr_decl, 0, ptr::null());
    let int_sort = Z3_mk_int_sort(ctx);
    let mut elems = Vec::with_capacity(len);
    for i in 0..len {
        let i_ast = Z3_mk_int64(ctx, i as i64, int_sort);
        let sel = Z3_mk_select(ctx, arr_ast, i_ast);
        let mut out: Z3_ast = ptr::null_mut();
        if !Z3_model_eval(ctx, model, sel, true, &mut out) {
            return Err(format!("model eval failed for `{name}[{i}]`"));
        }
        let v = match elem_ty {
            "Int"    => Sv::Int(decode_int_literal(ctx, out)?),
            "Bool"   => Sv::Bool(decode_bool_literal(ctx, out)?),
            "String" => Sv::Str(decode_string_literal(ctx, out)?),
            "Real"   => Sv::Real(decode_real_literal(ctx, out)?),
            _        => decode_datatype_value(ctx, out)?,
        };
        elems.push(v);
    }
    Ok(Sv::Seq(elems))
}

/// Append the per-tick equality pin(s) for one carried state field. A `Seq(T)`
/// value pins its companion `__len` and one `(select _name i)` per element; every
/// other value pins `(= _name <literal>)`. Used by both the main tick loop and
/// the functionizer's setup-time solve so the two stay in lockstep.
fn emit_state_pin(pins: &mut String, name: &str, v: &Sv) {
    match v {
        Sv::Seq(elems) => {
            pins.push_str(&format!("(assert (= _{name}__len {}))\n", elems.len()));
            for (i, e) in elems.iter().enumerate() {
                pins.push_str(&format!("(assert (= (select _{name} {i}) {}))\n", e.smtlib()));
            }
        }
        _ => pins.push_str(&format!("(assert (= _{name} {}))\n", v.smtlib())),
    }
}

/// Recursively decode a Datatype value (e.g. `(TLCons (LParen) TLNil)`) into
/// the Sv tree. The kernel doesn't know the schema; it walks the AST and
/// reads constructor names + payload sorts on the fly via Z3's reflection.
unsafe fn decode_datatype_value(ctx: Z3_context, ast: Z3_ast) -> Result<Sv, String> {
    let app = Z3_to_app(ctx, ast);
    if app.is_null() {
        // Sometimes a primitive literal slips in (Int payload of a variant);
        // fall back to int/string decode.
        if let Ok(n) = decode_int_literal(ctx, ast) { return Ok(Sv::Int(n)); }
        if let Ok(s) = decode_string_literal(ctx, ast) { return Ok(Sv::Str(s)); }
        if let Ok(b) = decode_bool_literal(ctx, ast) { return Ok(Sv::Bool(b)); }
        if let Ok(r) = decode_real_literal(ctx, ast) { return Ok(Sv::Real(r)); }
        return Err(format!("can't decode value: {}", ast_to_string(ctx, ast)));
    }
    let decl = Z3_get_app_decl(ctx, app);
    let variant = decode_sym(ctx, Z3_get_decl_name(ctx, decl));
    let n_args = Z3_get_app_num_args(ctx, app);
    let mut fields = Vec::with_capacity(n_args as usize);
    for i in 0..n_args {
        let arg = Z3_get_app_arg(ctx, app, i);
        // Each payload field gets recursively decoded. Primitives hit the
        // numeric/string fast path above.
        let sort = Z3_get_sort(ctx, arg);
        let sort_name = decode_sym(ctx, Z3_get_sort_name(ctx, sort));
        let child = match sort_name.as_str() {
            "Int"    => Sv::Int(decode_int_literal(ctx, arg)?),
            "Bool"   => Sv::Bool(decode_bool_literal(ctx, arg)?),
            "String" => Sv::Str(decode_string_literal(ctx, arg)?),
            "Real"   => Sv::Real(decode_real_literal(ctx, arg)?),
            _        => decode_datatype_value(ctx, arg)?,
        };
        fields.push(child);
    }
    Ok(Sv::Datatype(variant, fields))
}

unsafe fn read_int_const(ctx: Z3_context, model: Z3_model, name: &str) -> Result<i64, String> {
    let decl = find_const_decl(ctx, model, name)
        .ok_or_else(|| format!("var `{name}` not in model"))?;
    let ast = Z3_mk_app(ctx, decl, 0, ptr::null());
    let mut out: Z3_ast = ptr::null_mut();
    if !Z3_model_eval(ctx, model, ast, true, &mut out) {
        return Err(format!("model eval failed for `{name}`"));
    }
    decode_int_literal(ctx, out)
}

unsafe fn find_const_decl(ctx: Z3_context, model: Z3_model, name: &str) -> Option<Z3_func_decl> {
    let n = Z3_model_get_num_consts(ctx, model);
    for i in 0..n {
        let d = Z3_model_get_const_decl(ctx, model, i);
        let dn = Z3_get_decl_name(ctx, d);
        if decode_sym(ctx, dn) == name {
            return Some(d);
        }
    }
    None
}

unsafe fn decode_sym(ctx: Z3_context, sym: Z3_symbol) -> String {
    let kind = Z3_get_symbol_kind(ctx, sym);
    if kind == SymbolKind::String {
        let p = Z3_get_symbol_string(ctx, sym);
        CStr::from_ptr(p).to_string_lossy().into_owned()
    } else {
        Z3_get_symbol_int(ctx, sym).to_string()
    }
}

unsafe fn decode_int_literal(ctx: Z3_context, ast: Z3_ast) -> Result<i64, String> {
    let mut out: i64 = 0;
    if Z3_get_numeral_int64(ctx, ast, &mut out) {
        return Ok(out);
    }
    // Try as application of unary `-`.
    let app = Z3_to_app(ctx, ast);
    if !app.is_null() && Z3_get_app_num_args(ctx, app) == 1 {
        let arg = Z3_get_app_arg(ctx, app, 0);
        if Z3_get_numeral_int64(ctx, arg, &mut out) {
            return Ok(-out);
        }
    }
    Err(format!("not an int literal: {}", ast_to_string(ctx, ast)))
}

unsafe fn decode_bool_literal(ctx: Z3_context, ast: Z3_ast) -> Result<bool, String> {
    let bv = Z3_get_bool_value(ctx, ast);
    if bv == Z3_L_TRUE  { Ok(true) }
    else if bv == Z3_L_FALSE { Ok(false) }
    else { Err(format!("not a bool literal: {}", ast_to_string(ctx, ast))) }
}

unsafe fn decode_string_literal(ctx: Z3_context, ast: Z3_ast) -> Result<String, String> {
    let p = Z3_get_string(ctx, ast);
    if p.is_null() {
        return Err(format!("not a string literal: {}", ast_to_string(ctx, ast)));
    }
    let raw = CStr::from_ptr(p).to_string_lossy().into_owned();
    Ok(unescape_z3(&raw))
}

/// Z3 escapes non-ASCII bytes as `\u{NN}` in its string output (mirroring
/// the runtime's encode-side z3_string fn). Reverse it here.
fn unescape_z3(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' { out.push(c); continue; }
        // Expect `\u{HEX}`
        if chars.peek() == Some(&'u') {
            chars.next();
            if chars.peek() == Some(&'{') {
                chars.next();
                let mut hex = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '}' { chars.next(); break; }
                    hex.push(ch);
                    chars.next();
                }
                if let Ok(n) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(n) {
                        out.push(ch);
                        continue;
                    }
                }
            }
        }
        out.push(c);
    }
    out
}

/// Walk a `__SeqOf_LibArg` Datatype value (Cons-cell shape produced by the
/// runtime's `Seq(UserType)` encoding) and decode each `LibArg` variant into
/// our internal representation.
///
/// The runtime emits the seq as a recursive enum:
///   `__SeqOf_LibArg = __Empty_LibArg | __Cell_LibArg(LibArg, __SeqOf_LibArg)`
/// We walk by pattern-matching on the constructor name.
unsafe fn decode_libargs(ctx: Z3_context, mut ast: Z3_ast) -> Result<Vec<crate::libcall::LibArg>, String> {
    let mut out = Vec::new();
    loop {
        let app = Z3_to_app(ctx, ast);
        if app.is_null() {
            return Err(format!("LibArg seq is not an application: {}", ast_to_string(ctx, ast)));
        }
        let decl = Z3_get_app_decl(ctx, app);
        let name = decode_sym(ctx, Z3_get_decl_name(ctx, decl));
        if name == "__Empty_LibArg" {
            return Ok(out);
        }
        if name == "__Cell_LibArg" {
            // (Cell head tail)
            let head = Z3_get_app_arg(ctx, app, 0);
            let tail = Z3_get_app_arg(ctx, app, 1);
            out.push(decode_libarg(ctx, head)?);
            ast = tail;
            continue;
        }
        return Err(format!("unexpected LibArg seq constructor `{name}`"));
    }
}

unsafe fn decode_libarg(ctx: Z3_context, ast: Z3_ast) -> Result<crate::libcall::LibArg, String> {
    let app = Z3_to_app(ctx, ast);
    if app.is_null() {
        return Err(format!("LibArg is not an application: {}", ast_to_string(ctx, ast)));
    }
    let decl = Z3_get_app_decl(ctx, app);
    let name = decode_sym(ctx, Z3_get_decl_name(ctx, decl));
    let arg0 = Z3_get_app_arg(ctx, app, 0);
    use crate::libcall::LibArg;
    match name.as_str() {
        "ArgInt"  => Ok(LibArg::Int(decode_int_literal(ctx, arg0)?)),
        "ArgStr"  => Ok(LibArg::Str(decode_string_literal(ctx, arg0)?)),
        "ArgReal" => Ok(LibArg::Real(decode_real_literal(ctx, arg0)?)),
        other     => Err(format!("unknown LibArg variant `{other}`")),
    }
}

unsafe fn decode_real_literal(ctx: Z3_context, ast: Z3_ast) -> Result<f64, String> {
    // Z3 reals are stored as rationals; pull numerator/denominator and divide.
    // For pinned literals this round-trips cleanly.
    let s = ast_to_string(ctx, ast);
    // Simple parser for cases like "3.14" or "(/ 314 100)" — try direct parse first.
    if let Ok(v) = s.parse::<f64>() { return Ok(v); }
    // Fall back to numerator/denominator extraction via Z3.
    let num = Z3_get_numerator(ctx, ast);
    let den = Z3_get_denominator(ctx, ast);
    let mut n: i64 = 0;
    let mut d: i64 = 0;
    if Z3_get_numeral_int64(ctx, num, &mut n) && Z3_get_numeral_int64(ctx, den, &mut d) && d != 0 {
        return Ok(n as f64 / d as f64);
    }
    Err(format!("not a real literal: {s}"))
}

unsafe fn ast_to_string(ctx: Z3_context, ast: Z3_ast) -> String {
    let p = Z3_ast_to_string(ctx, ast);
    if p.is_null() { return "<null>".into(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

fn compare_sv(a: &Sv, b: &Sv) -> bool {
    match (a, b) {
        (Sv::Int(x),  Sv::Int(y))  => x == y,
        (Sv::Bool(x), Sv::Bool(y)) => x == y,
        (Sv::Str(x),  Sv::Str(y))  => x == y,
        (Sv::Real(x), Sv::Real(y)) => x == y,
        (Sv::Datatype(vx, fx), Sv::Datatype(vy, fy)) =>
            vx == vy && fx.len() == fy.len()
                && fx.iter().zip(fy.iter()).all(|(p, q)| compare_sv(p, q)),
        (Sv::Seq(xs), Sv::Seq(ys)) =>
            xs.len() == ys.len()
                && xs.iter().zip(ys.iter()).all(|(p, q)| compare_sv(p, q)),
        _ => false,
    }
}

// ── Functionizer support surface (crate::functionize) ───────────

pub(crate) unsafe fn decode_sym_pub(ctx: Z3_context, sym: Z3_symbol) -> String {
    decode_sym(ctx, sym)
}

pub(crate) fn unescape_z3_pub(s: &str) -> String {
    unescape_z3(s)
}

pub(crate) fn compare_sv_pub(a: &Sv, b: &Sv) -> bool {
    compare_sv(a, b)
}

/// Read the `effects` Seq from a solved model as a `Vec<Sv>` (one decoded
/// Datatype value per element). Used by the functionizer's setup-time
/// verification (tick.rs's main loop dispatches effects straight off the
/// model instead).
pub(crate) unsafe fn read_effects_sv(ctx: Z3_context, model: Z3_model, manifest: &Manifest) -> Result<Vec<Sv>, String> {
    let len = read_int_const(ctx, model, &format!("{}__len", manifest.effects_name)).unwrap_or(0);
    let len = len.min(manifest.max_effects as i64).max(0) as usize;
    let Some(effects_decl) = find_const_decl(ctx, model, &manifest.effects_name) else {
        return Ok(Vec::new());
    };
    let effects_ast = Z3_mk_app(ctx, effects_decl, 0, ptr::null());
    let int_sort = Z3_mk_int_sort(ctx);
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let i_ast = Z3_mk_int64(ctx, i as i64, int_sort);
        let select_ast = Z3_mk_select(ctx, effects_ast, i_ast);
        let mut eval_out: Z3_ast = ptr::null_mut();
        if !Z3_model_eval(ctx, model, select_ast, true, &mut eval_out) {
            return Err(format!("model eval failed for effects[{i}]"));
        }
        out.push(decode_datatype_value(ctx, eval_out)?);
    }
    Ok(out)
}

/// One-shot fresh-solver solve for a single tick (mechanism-A shape), reading
/// state + effects back as `Sv`. `last_results` is pinned empty — the
/// functionizer's verification doesn't model cross-tick result carry, so a
/// body that reads `last_results` will fail to verify and stay on Z3 (sound).
/// Returns `Ok(None)` on UNSAT/UNKNOWN.
pub(crate) unsafe fn solve_tick_sv(
    ctx: Z3_context,
    body: &[Z3_ast],
    decl_preamble: &str,
    manifest: &Manifest,
    is_first: bool,
    prev_state: &[Option<Sv>],
) -> Result<Option<(Vec<Sv>, Vec<Sv>)>, String> {
    let solver = Z3_mk_solver(ctx);
    Z3_solver_inc_ref(ctx, solver);
    for &a in body {
        Z3_solver_assert(ctx, solver, a);
    }
    let mut pins = String::new();
    pins.push_str(decl_preamble);
    if !is_first {
        for (idx, (name, _)) in manifest.state_fields.iter().enumerate() {
            if let Some(v) = &prev_state[idx] {
                emit_state_pin(&mut pins, name, v);
            }
        }
    }
    pins.push_str("(assert (= last_results__len 0))\n");
    pins.push_str(if is_first { "(assert is_first_tick)\n" } else { "(assert (not is_first_tick))\n" });

    let (pin_vec, pin_asts) = match parse_pins(ctx, &pins) {
        Ok(x) => x,
        Err(e) => {
            Z3_solver_dec_ref(ctx, solver);
            return Err(e);
        }
    };
    for &a in &pin_asts {
        Z3_solver_assert(ctx, solver, a);
    }
    let res = Z3_solver_check(ctx, solver);
    Z3_ast_vector_dec_ref(ctx, pin_vec);
    if res != Z3_L_TRUE {
        Z3_solver_dec_ref(ctx, solver);
        return Ok(None);
    }
    let model = Z3_solver_get_model(ctx, solver);
    Z3_model_inc_ref(ctx, model);
    let mut state = Vec::with_capacity(manifest.state_fields.len());
    for (name, ty) in &manifest.state_fields {
        state.push(read_state_var(ctx, model, name, ty)?);
    }
    let effects = read_effects_sv(ctx, model, manifest)?;
    Z3_model_dec_ref(ctx, model);
    Z3_solver_dec_ref(ctx, solver);
    Ok(Some((state, effects)))
}

/// Dispatch a single effect given as a decoded `Sv::Datatype` (the
/// functionizer fast path's effect representation). Mirrors `dispatch_effect`
/// (which works off a Z3 `Z3_ast`); kept in lockstep with it.
fn dispatch_effect_sv(eff: &Sv) -> Result<EffectOutcome, String> {
    let Sv::Datatype(name, fields) = eff else {
        return Err(format!("effect is not a datatype value: {eff:?}"));
    };
    match name.as_str() {
        "Exit" => {
            let code = match fields.first() {
                Some(Sv::Int(n)) => (*n).clamp(0, 255) as u8,
                _ => return Err("Exit payload not an int".to_string()),
            };
            Ok(EffectOutcome::Exit(code))
        }
        "ReadLine" => {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => Ok(EffectOutcome::Continue(Res::Eof)),
                Ok(_) => {
                    if line.ends_with('\n') { line.pop(); }
                    if line.ends_with('\r') { line.pop(); }
                    Ok(EffectOutcome::Continue(Res::Str(line)))
                }
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("read_line: {e}")))),
            }
        }
        "ReadFile" => {
            let path = sv_str(fields.first())?;
            match std::fs::read_to_string(&path) {
                Ok(s) => Ok(EffectOutcome::Continue(Res::Str(s))),
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("read_file({path}): {e}")))),
            }
        }
        "WriteFile" => {
            let path = sv_str(fields.first())?;
            let contents = sv_str(fields.get(1))?;
            match std::fs::write(&path, contents) {
                Ok(()) => Ok(EffectOutcome::Continue(Res::No)),
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("write_file({path}): {e}")))),
            }
        }
        "LibCall" => {
            let lib = sv_str(fields.first())?;
            let func = sv_str(fields.get(1))?;
            let args = decode_libargs_sv(fields.get(2))?;
            match crate::libcall::call(&lib, &func, &args) {
                Ok(ret) => Ok(EffectOutcome::Continue(Res::Int(ret))),
                Err(e) => Ok(EffectOutcome::Continue(Res::Error(format!("LibCall({lib}, {func}): {e}")))),
            }
        }
        other => {
            eprintln!("kernel: unknown effect variant `{other}`; skipping");
            Ok(EffectOutcome::Continue(Res::Error(format!("unknown effect variant `{other}`"))))
        }
    }
}

fn sv_str(v: Option<&Sv>) -> Result<String, String> {
    match v {
        Some(Sv::Str(s)) => Ok(s.clone()),
        other => Err(format!("expected String payload, got {other:?}")),
    }
}

fn decode_libargs_sv(v: Option<&Sv>) -> Result<Vec<crate::libcall::LibArg>, String> {
    use crate::libcall::LibArg;
    let mut out = Vec::new();
    let mut cur = v.cloned();
    loop {
        match cur {
            Some(Sv::Datatype(ref name, ref fs)) if name == "__Empty_LibArg" => return Ok(out),
            Some(Sv::Datatype(ref name, ref fs)) if name == "__Cell_LibArg" && fs.len() == 2 => {
                let arg = match &fs[0] {
                    Sv::Datatype(v, p) => match (v.as_str(), p.first()) {
                        ("ArgInt", Some(Sv::Int(n))) => LibArg::Int(*n),
                        ("ArgStr", Some(Sv::Str(s))) => LibArg::Str(s.clone()),
                        ("ArgReal", Some(Sv::Real(r))) => LibArg::Real(*r),
                        _ => return Err(format!("unknown LibArg variant {v}")),
                    },
                    other => return Err(format!("LibArg cell head not a datatype: {other:?}")),
                };
                out.push(arg);
                cur = Some(fs[1].clone());
            }
            other => return Err(format!("unexpected LibArg seq node: {other:?}")),
        }
    }
}
