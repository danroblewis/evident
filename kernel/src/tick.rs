//! Tick loop: solve, walk effects, dispatch, repeat.
//!
//! Z3 lifecycle (per docs/plans/architecture-invariants.md §"Z3 model
//! lifecycle", exploration docs/plans/kernel-fix-incremental-solving.md):
//!   - The program body is parsed ONCE, then `.simplify()`'d ONCE before the
//!     tick loop (invariant #4 allows a single pre-loop simplify). The
//!     simplified ASTs are cached and reused every tick — no per-tick re-parse
//!     of the body (the audit's dominant cost for large compiler.smt2).
//!   - Each tick the tick-local equality pins (state-carry `_<name>`,
//!     `last_results`, `is_first_tick`) are SUBSTITUTED into the cached body
//!     ASTs via `Z3_substitute`, then a fresh solver solves the substituted
//!     body. Pins are built by parsing a tiny `<declarations extracted from the
//!     body> + <equality asserts>` string; re-declaring the symbols makes them
//!     intern to the same variables as the cached body (Z3 hash-conses sorts +
//!     func_decls within a context), so `(= _x 5)`'s lhs/rhs are the right
//!     subterms to substitute.
//!
//! Note: the pin-application mechanism was chosen by an explicit exploration
//! (six variants, all with the pre-loop simplify). A PERSISTENT solver with
//! either `check_assumptions(pins)` (tiny-runtime's `s.check(*pins)`) or
//! `push`/`pop` reproduced a ~450x / suite-timeout regression on growing
//! datatype-state fixtures (the incremental solver forgoes the one-shot
//! preprocessing a fresh solve applies to the carried-state pins). Substitution
//! into a fresh solver applies the pins directly to the model and is as fast as
//! the cached-ASTs baseline. Full table in
//! docs/plans/kernel-fix-incremental-solving.md.

use std::ffi::{CStr, CString};
use std::ptr;

use crate::manifest::Manifest;
use z3_sys::*;

#[derive(Debug, Clone)]
enum Sv {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    /// A Datatype-typed value: (variant constructor name, recursively-decoded payload values).
    /// Lets the kernel carry algebraic data (e.g. a TokenList) across ticks.
    Datatype(String, Vec<Sv>),
}

impl Sv {
    /// Emit as SMT-LIB literal expression suitable for an `(assert (= ...))`.
    fn smtlib(&self) -> String {
        match self {
            Sv::Int(n) if *n >= 0  => n.to_string(),
            Sv::Int(n)             => format!("(- {})", -n),
            Sv::Bool(b)            => b.to_string(),
            Sv::Str(s)             => format!("\"{}\"", s.replace('"', "\"\"")),
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
        }
    }
}

pub fn run(src: &str, manifest: &Manifest) -> Result<u8, String> {
    unsafe { run_inner(src, manifest) }
}

/// Parse a tiny SMT-LIB string (declarations preamble + equality pins) into an
/// inc_ref'd ast_vector. Caller dec_refs. Returns the parse error message on
/// failure.
unsafe fn parse_pins(ctx: Z3_context, pins: &str) -> Result<Z3_ast_vector, String> {
    let cstr = CString::new(pins).map_err(|e| format!("pin string has interior NUL: {e}"))?;
    let empty_sym: Vec<Z3_symbol> = Vec::new();
    let empty_sort: Vec<Z3_sort> = Vec::new();
    let empty_decl: Vec<Z3_func_decl> = Vec::new();
    let asts = Z3_parse_smtlib2_string(
        ctx, cstr.as_ptr(),
        0, empty_sym.as_ptr(), empty_sort.as_ptr(),
        0, empty_sym.as_ptr(), empty_decl.as_ptr(),
    );
    if asts.is_null() {
        let err_ptr = Z3_get_error_msg(ctx, Z3_get_error_code(ctx));
        let err = if err_ptr.is_null() { String::new() }
                  else { CStr::from_ptr(err_ptr).to_string_lossy().into_owned() };
        return Err(err);
    }
    Z3_ast_vector_inc_ref(ctx, asts);
    Ok(asts)
}

unsafe fn run_inner(src: &str, manifest: &Manifest) -> Result<u8, String> {
    let cfg = Z3_mk_config();
    let ctx = Z3_mk_context(cfg);
    Z3_del_config(cfg);

    // Build the model ONCE: parse the body a single time and CACHE its asserted
    // ASTs. The cached ASTs are reused every tick — no per-tick re-parse of the
    // body (the audit's dominant cost for large compiler.smt2).
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

    // Pre-loop `.simplify()` (architecture-invariants.md §"Z3 model lifecycle"
    // #4: a single simplify BEFORE the loop is allowed and desired; per-tick
    // simplify remains forbidden). Simplify each asserted formula once and cache
    // the results. Pins are substituted into THESE simplified ASTs each tick.
    let mut simp: Vec<Z3_ast> = Vec::with_capacity(body_n as usize);
    for i in 0..body_n {
        let a = Z3_ast_vector_get(ctx, body_vec, i);
        let s = Z3_simplify(ctx, a);
        Z3_inc_ref(ctx, s);
        simp.push(s);
    }

    // Declarations (datatypes, consts) extracted from the body. Each tick's tiny
    // pin string re-declares these so the parsed pins' symbols intern to the same
    // base-scope variables as the cached body ASTs (Z3 hash-conses sorts +
    // func_decls per context) — including ones the body declares but never
    // references in an assert (e.g. `is_first_tick`, `last_results`), which a
    // post-parse AST walk could not recover.
    let decl_preamble = extract_declarations(src);

    let mut prev_state: Vec<Option<Sv>> = vec![None; manifest.state_fields.len()];
    let mut prev_results: Vec<Res> = Vec::new();
    let mut is_first = true;

    const TICK_LIMIT: usize = 100_000;
    for tick in 0..TICK_LIMIT {
        // Build this tick's pins as uniform `(= lhs rhs)` equalities (so every
        // pin can drive a substitution). The declarations preamble makes the pin
        // symbols re-declare and intern to the cached body's variables.
        let mut pins = String::with_capacity(decl_preamble.len() + 256);
        pins.push_str(&decl_preamble);

        if !is_first {
            for (idx, (name, _)) in manifest.state_fields.iter().enumerate() {
                if let Some(v) = &prev_state[idx] {
                    pins.push_str(&format!("(assert (= _{name} {}))\n", v.smtlib()));
                }
            }
        }
        // last_results: assert length + per-index value. On tick 0 the array
        // is empty (length 0); subsequent ticks carry the prior tick's results.
        pins.push_str(&format!("(assert (= last_results__len {}))\n", prev_results.len()));
        for (i, r) in prev_results.iter().enumerate() {
            pins.push_str(&format!("(assert (= (select last_results {i}) {}))\n", r.smtlib()));
        }
        let first_v = if is_first { "true" } else { "false" };
        pins.push_str(&format!("(assert (= is_first_tick {first_v}))\n"));

        // ── Apply pins by SUBSTITUTING them into the cached body AST ───────
        // This is the winning mechanism from the pin-application exploration
        // (docs/plans/kernel-fix-incremental-solving.md): a persistent solver
        // with check-with-assumptions / push-pop reproduced a ~450x datatype-
        // state regression, while substitution is as fast as the cached-ASTs
        // baseline and applies the pins directly to the model.
        let solver = Z3_mk_solver(ctx);
        Z3_solver_inc_ref(ctx, solver);

        let pv = match parse_pins(ctx, &pins) {
            Ok(v) => v,
            Err(e) => {
                Z3_solver_dec_ref(ctx, solver);
                Z3_del_context(ctx);
                return Err(format!("smtlib parse failed on tick {tick}: {e}"));
            }
        };
        // Split equality pins into substitutions (nullary-const lhs, e.g. `_x`,
        // `is_first_tick`, `last_results__len`) and residual asserts (compound
        // lhs like `(select last_results i)` that is not a single replaceable
        // subterm). Substitute the former into the body; assert the latter.
        let pv_n = Z3_ast_vector_size(ctx, pv);
        let mut from: Vec<Z3_ast> = Vec::new();
        let mut to: Vec<Z3_ast> = Vec::new();
        let mut residual: Vec<Z3_ast> = Vec::new();
        for i in 0..pv_n {
            let eq = Z3_ast_vector_get(ctx, pv, i);
            let app = Z3_to_app(ctx, eq);
            let mut handled = false;
            if !app.is_null() && Z3_get_app_num_args(ctx, app) == 2 {
                let lhs = Z3_get_app_arg(ctx, app, 0);
                let rhs = Z3_get_app_arg(ctx, app, 1);
                let lapp = Z3_to_app(ctx, lhs);
                if !lapp.is_null() && Z3_get_app_num_args(ctx, lapp) == 0 {
                    from.push(lhs);
                    to.push(rhs);
                    handled = true;
                }
            }
            if !handled { residual.push(eq); }
        }
        for &a in &simp {
            let a2 = if from.is_empty() { a }
                     else { Z3_substitute(ctx, a, from.len() as u32, from.as_ptr(), to.as_ptr()) };
            Z3_solver_assert(ctx, solver, a2);
        }
        for &r in &residual { Z3_solver_assert(ctx, solver, r); }

        let res = Z3_solver_check(ctx, solver);

        // Done with the parsed pin vector (its asts are now retained by the
        // solver's assertions / substitution results).
        Z3_ast_vector_dec_ref(ctx, pv);

        macro_rules! drop_solver { () => {{ Z3_solver_dec_ref(ctx, solver); }} }

        if res == Z3_L_FALSE {
            eprintln!("kernel: UNSAT on tick {tick}");
            drop_solver!();
            Z3_del_context(ctx);
            return Ok(2);
        }
        if res != Z3_L_TRUE {
            drop_solver!();
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

        if let Some(code) = exit_code {
            Z3_model_dec_ref(ctx, model);
            drop_solver!();
            Z3_del_context(ctx);
            return Ok(code);
        }

        // Stuck halt check.
        let stuck = !is_first && prev_state.iter().zip(new_state.iter())
            .all(|(p, n)| matches!(p, Some(pv) if compare_sv(pv, n)));
        Z3_model_dec_ref(ctx, model);
        drop_solver!();
        if stuck {
            eprintln!("kernel: stuck (state unchanged with no Exit emitted)");
            Z3_del_context(ctx);
            return Ok(1);
        }

        prev_state   = new_state.into_iter().map(Some).collect();
        prev_results = new_results;
        is_first     = false;
    }

    Z3_del_context(ctx);
    Err(format!("tick limit ({TICK_LIMIT}) reached"))
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
            Res::Str(s)    => format!("(StringResult \"{}\")", s.replace('"', "\"\"")),
            Res::Real(r)   => format!("(RealResult {})",
                if *r >= 0.0 { r.to_string() } else { format!("(- {})", -r) }),
            Res::Error(s)  => format!("(ErrorResult \"{}\")", s.replace('"', "\"\"")),
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

unsafe fn read_state_var(ctx: Z3_context, model: Z3_model, name: &str, ty: &str) -> Result<Sv, String> {
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
        _ => false,
    }
}
