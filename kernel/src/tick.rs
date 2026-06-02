//! Tick loop: solve, walk effects, dispatch, repeat.
//!
//! MVP scope (v0.1):
//!   - Built-in effects: Println, Print, Exit
//!   - Time/ReadLine/ReadFile/WriteFile/LibCall: stubs (continue)
//!   - Solver state: fresh `Z3_solver` per tick, fresh parse of full SMT each tick
//!     (concatenated body + carry asserts + is_first_tick assert)

use std::ffi::{CStr, CString};
use std::ptr;

use crate::manifest::Manifest;
use z3_sys::*;

#[derive(Debug, Clone)]
enum Sv {
    Int(i64),
    Bool(bool),
    Str(String),
}

impl Sv {
    /// Emit as SMT-LIB literal expression.
    fn smtlib(&self) -> String {
        match self {
            Sv::Int(n) if *n >= 0  => n.to_string(),
            Sv::Int(n)             => format!("(- {})", -n),
            Sv::Bool(b)            => b.to_string(),
            Sv::Str(s)             => format!("\"{}\"", s.replace('"', "\"\"")),
        }
    }
}

pub fn run(src: &str, manifest: &Manifest) -> Result<u8, String> {
    unsafe { run_inner(src, manifest) }
}

unsafe fn run_inner(src: &str, manifest: &Manifest) -> Result<u8, String> {
    let cfg = Z3_mk_config();
    let ctx = Z3_mk_context(cfg);
    Z3_del_config(cfg);

    let mut prev_state: Vec<Option<Sv>> = vec![None; manifest.state_fields.len()];
    let mut prev_results: Vec<Res> = Vec::new();
    let mut is_first = true;

    const TICK_LIMIT: usize = 100_000;
    for tick in 0..TICK_LIMIT {
        // Build per-tick SMT: body + carry + is_first_tick wiring, all in one parse.
        let mut full = String::with_capacity(src.len() + 256);
        full.push_str(src);

        if !is_first {
            for (idx, (name, _)) in manifest.state_fields.iter().enumerate() {
                if let Some(v) = &prev_state[idx] {
                    full.push_str(&format!("(assert (= _{name} {}))\n", v.smtlib()));
                }
            }
        }
        // last_results: assert length + per-index value. On tick 0 the array
        // is empty (length 0); subsequent ticks carry the prior tick's results.
        full.push_str(&format!("(assert (= last_results__len {}))\n", prev_results.len()));
        for (i, r) in prev_results.iter().enumerate() {
            full.push_str(&format!("(assert (= (select last_results {i}) {}))\n", r.smtlib()));
        }
        if is_first {
            full.push_str("(assert is_first_tick)\n");
        } else {
            full.push_str("(assert (not is_first_tick))\n");
        }

        let solver = Z3_mk_solver(ctx);
        Z3_solver_inc_ref(ctx, solver);

        let cstr = CString::new(full.as_str()).unwrap();
        let empty_sym: Vec<Z3_symbol> = Vec::new();
        let empty_sort: Vec<Z3_sort> = Vec::new();
        let empty_decl: Vec<Z3_func_decl> = Vec::new();
        let asts = Z3_parse_smtlib2_string(
            ctx, cstr.as_ptr(),
            0, empty_sym.as_ptr(), empty_sort.as_ptr(),
            0, empty_sym.as_ptr(), empty_decl.as_ptr(),
        );
        if asts.is_null() {
            Z3_solver_dec_ref(ctx, solver);
            Z3_del_context(ctx);
            let err_ptr = Z3_get_error_msg(ctx, Z3_get_error_code(ctx));
            let err = if err_ptr.is_null() { String::new() }
                      else { CStr::from_ptr(err_ptr).to_string_lossy().into_owned() };
            return Err(format!("smtlib parse failed on tick {tick}: {err}"));
        }

        let n = Z3_ast_vector_size(ctx, asts);
        for i in 0..n {
            let a = Z3_ast_vector_get(ctx, asts, i);
            Z3_solver_assert(ctx, solver, a);
        }

        let res = Z3_solver_check(ctx, solver);
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
            Z3_solver_dec_ref(ctx, solver);
            Z3_del_context(ctx);
            return Ok(code);
        }

        // Stuck halt check.
        let stuck = !is_first && prev_state.iter().zip(new_state.iter())
            .all(|(p, n)| matches!(p, Some(pv) if compare_sv(pv, n)));
        Z3_model_dec_ref(ctx, model);
        Z3_solver_dec_ref(ctx, solver);
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
        "Println" => {
            let arg0 = Z3_get_app_arg(ctx, app, 0);
            let s = decode_string_literal(ctx, arg0)?;
            println!("{s}");
            Ok(EffectOutcome::Continue(Res::No))
        }
        "Print" => {
            let arg0 = Z3_get_app_arg(ctx, app, 0);
            let s = decode_string_literal(ctx, arg0)?;
            print!("{s}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            Ok(EffectOutcome::Continue(Res::No))
        }
        "Exit" => {
            let arg0 = Z3_get_app_arg(ctx, app, 0);
            let code = decode_int_literal(ctx, arg0)?;
            let code = code.clamp(0, 255) as u8;
            Ok(EffectOutcome::Exit(code))
        }
        "Time" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(EffectOutcome::Continue(Res::Int(ms)))
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
        other    => Err(format!("unsupported state-field type `{other}` for `{name}`")),
    }
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
    Ok(CStr::from_ptr(p).to_string_lossy().into_owned())
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
