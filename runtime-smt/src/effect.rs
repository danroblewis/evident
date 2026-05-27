//! N2 effect dispatcher — interprets a decoded `EffectValue` as IO and produces
//! the per-effect `Result` value the next tick reads via `last_results`.
//!
//! ## Effect → Result map
//!
//! Each dispatched effect produces a `Result`-enum [`Value`] (the `enum Result`
//! from `stdlib/runtime.ev`, mirrored in `runtime-contract/FORMAT.md` §3). The
//! engine threads the per-tick ordered `Vec<Value>` of these into the FOLLOWING
//! tick's `given["last_results"]` so the FSM can `match last_results[0]`:
//!
//! | Effect          | Result                                        |
//! |-----------------|-----------------------------------------------|
//! | `IntToStr(n)`   | `StringResult(<decimal of n>)`                |
//! | `ParseInt(s)`   | `IntResult(parsed)` / `ErrorResult(<msg>)`    |
//! | `MonotonicTime` | `IntResult(<stub>)` — deterministic, see below|
//! | `Time`          | `IntResult(<stub>)` — deterministic, see below|
//! | `Println`/`Print`/`Exit`/`NoEffect`/other | `NoResult`          |
//!
//! `MonotonicTime`/`Time` are a **deterministic stub** here: this engine has no
//! wall clock (and a real clock would make runs non-reproducible, breaking the
//! byte-identical cross-check), so both return a fixed `IntResult(0)`. A future
//! milestone wiring a frame clock as a given source would replace the stub.
//!
//! ## IO behavior
//!
//! Two constructors are dispatched to IO:
//!   * `Println(String)` — write the string + `\n` to `out`.
//!   * `Exit(Int)`       — request graceful process exit with the given code.
//!
//! Unknown constructors produce no IO (and `NoResult`) so fixtures can carry
//! uninterpreted effects (e.g. `Tick`, `DrawRect`, …) without error.
//!
//! Exit is *graceful*: `dispatch_all_with_results` dispatches every effect in
//! the tick, including those that follow an `Exit`, then returns the first exit
//! code seen alongside the full ordered result list.

use std::io::Write;

use crate::spec::EffectValue;
use crate::z3c::Value;

/// Result of dispatching a single effect: the control-flow outcome plus the
/// `Result`-enum value the effect produced (for `last_results` threading).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Continue,
    Exit(i32),
}

/// Build a nullary `Result` value (`NoResult`).
fn no_result() -> Value {
    Value::nullary("NoResult")
}

/// Build an applied `Result` value, e.g. `StringResult("7")`.
fn result(ctor: &str, arg: Value) -> Value {
    Value::Enum { ctor: ctor.to_string(), args: vec![arg] }
}

/// Map a dispatched effect to the `Result`-enum value it produces, per the table
/// in this module's docs. Pure — does not touch `out`.
fn effect_result(eff: &EffectValue) -> Value {
    match eff.ctor.as_str() {
        "IntToStr" => match eff.args.first() {
            Some(Value::Int(n)) => result("StringResult", Value::Str(n.to_string())),
            // No / wrong arg: lenient empty string (mirrors the dispatcher's
            // lenient stance for missing payloads).
            _ => result("StringResult", Value::Str(String::new())),
        },
        "ParseInt" => match eff.args.first() {
            Some(Value::Str(s)) => match s.parse::<i64>() {
                Ok(n) => result("IntResult", Value::Int(n)),
                Err(e) => result("ErrorResult", Value::Str(format!("ParseInt: {e}: {s:?}"))),
            },
            _ => result("ErrorResult", Value::Str("ParseInt: missing string argument".into())),
        },
        // No wall clock in this engine → deterministic stub (see module docs).
        "MonotonicTime" | "Time" => result("IntResult", Value::Int(0)),
        // Println/Print/Exit/NoEffect and any uninterpreted effect → NoResult.
        _ => no_result(),
    }
}

/// Dispatch one decoded effect to IO and report its `Result` value.
///
/// * `Println` with a first `Value::Str(s)` arg → writes `s\n` to `out`; returns
///   `(Continue, NoResult)`. No `Str` first arg → writes an empty line (lenient).
/// * `Exit` with a first `Value::Int(c)` arg → returns `(Exit(c as i32),
///   NoResult)` and writes nothing. No `Int` first arg → code 0.
/// * Any other constructor → `(Continue, effect_result(eff))`, no IO.
pub fn dispatch_one(
    eff: &EffectValue,
    out: &mut dyn Write,
) -> std::io::Result<(DispatchOutcome, Value)> {
    let outcome = match eff.ctor.as_str() {
        "Println" => {
            let text = match eff.args.first() {
                Some(Value::Str(s)) => s.as_str(),
                _ => "",
            };
            writeln!(out, "{text}")?;
            DispatchOutcome::Continue
        }
        "Exit" => {
            let code = match eff.args.first() {
                Some(Value::Int(c)) => *c as i32,
                _ => 0,
            };
            DispatchOutcome::Exit(code)
        }
        _ => DispatchOutcome::Continue,
    };
    Ok((outcome, effect_result(eff)))
}

/// Back-compat shim: dispatch one effect, discarding its `Result` value.
pub fn dispatch(eff: &EffectValue, out: &mut dyn Write) -> std::io::Result<DispatchOutcome> {
    Ok(dispatch_one(eff, out)?.0)
}

/// Dispatch a whole tick's effects, in order, returning both the exit code (if
/// any) and the ordered per-effect `Result` values.
///
/// ALL effects are dispatched even if one is an `Exit` (Exit is graceful
/// end-of-tick — co-emitted `Println`s still run). The `Some(code)` is the FIRST
/// `Exit`'s code; `results[i]` is the `Result` produced by `effects[i]` and is
/// what the NEXT tick reads as `last_results[i]`.
pub fn dispatch_all_with_results(
    effects: &[EffectValue],
    out: &mut dyn Write,
) -> std::io::Result<(Option<i32>, Vec<Value>)> {
    let mut exit_code: Option<i32> = None;
    let mut results: Vec<Value> = Vec::with_capacity(effects.len());
    for eff in effects {
        let (outcome, res) = dispatch_one(eff, out)?;
        results.push(res);
        if let DispatchOutcome::Exit(code) = outcome {
            if exit_code.is_none() {
                exit_code = Some(code);
            }
        }
    }
    Ok((exit_code, results))
}

/// Back-compat: dispatch a whole tick's effects, discarding the result list.
///
/// ALL effects are dispatched even if one is an `Exit`. Returns `Some(code)` if
/// any `Exit` was dispatched (the FIRST `Exit`'s code wins), else `None`.
pub fn dispatch_all(effects: &[EffectValue], out: &mut dyn Write) -> std::io::Result<Option<i32>> {
    Ok(dispatch_all_with_results(effects, out)?.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn println_eff(s: &str) -> EffectValue {
        EffectValue { ctor: "Println".into(), args: vec![Value::Str(s.into())] }
    }

    fn exit_eff(code: i64) -> EffectValue {
        EffectValue { ctor: "Exit".into(), args: vec![Value::Int(code)] }
    }

    fn tick_eff() -> EffectValue {
        EffectValue { ctor: "Tick".into(), args: vec![] }
    }

    fn int_to_str_eff(n: i64) -> EffectValue {
        EffectValue { ctor: "IntToStr".into(), args: vec![Value::Int(n)] }
    }

    fn parse_int_eff(s: &str) -> EffectValue {
        EffectValue { ctor: "ParseInt".into(), args: vec![Value::Str(s.into())] }
    }

    // ------------------------------------------------------------------
    // dispatch — single-effect cases
    // ------------------------------------------------------------------

    #[test]
    fn println_writes_line_and_continues() {
        let eff = println_eff("hello");
        let mut buf: Vec<u8> = Vec::new();
        let outcome = dispatch(&eff, &mut buf).unwrap();
        assert_eq!(outcome, DispatchOutcome::Continue);
        assert_eq!(String::from_utf8(buf).unwrap(), "hello\n");
    }

    #[test]
    fn exit_returns_code_and_writes_nothing() {
        let eff = exit_eff(42);
        let mut buf: Vec<u8> = Vec::new();
        let outcome = dispatch(&eff, &mut buf).unwrap();
        assert_eq!(outcome, DispatchOutcome::Exit(42));
        assert!(buf.is_empty());
    }

    #[test]
    fn unknown_ctor_ignores_and_continues() {
        let eff = tick_eff();
        let mut buf: Vec<u8> = Vec::new();
        let outcome = dispatch(&eff, &mut buf).unwrap();
        assert_eq!(outcome, DispatchOutcome::Continue);
        assert!(buf.is_empty());
    }

    #[test]
    fn println_no_args_writes_empty_line() {
        let eff = EffectValue { ctor: "Println".into(), args: vec![] };
        let mut buf: Vec<u8> = Vec::new();
        let outcome = dispatch(&eff, &mut buf).unwrap();
        assert_eq!(outcome, DispatchOutcome::Continue);
        assert_eq!(String::from_utf8(buf).unwrap(), "\n");
    }

    #[test]
    fn exit_no_args_uses_code_zero() {
        let eff = EffectValue { ctor: "Exit".into(), args: vec![] };
        let mut buf: Vec<u8> = Vec::new();
        let outcome = dispatch(&eff, &mut buf).unwrap();
        assert_eq!(outcome, DispatchOutcome::Exit(0));
        assert!(buf.is_empty());
    }

    // ------------------------------------------------------------------
    // dispatch_all — multi-effect cases
    // ------------------------------------------------------------------

    #[test]
    fn dispatch_all_two_printlns() {
        let effs = vec![println_eff("a"), println_eff("b")];
        let mut buf: Vec<u8> = Vec::new();
        let result = dispatch_all(&effs, &mut buf).unwrap();
        assert_eq!(result, None);
        assert_eq!(String::from_utf8(buf).unwrap(), "a\nb\n");
    }

    #[test]
    fn dispatch_all_println_then_exit_is_graceful() {
        // Println should execute even though Exit follows.
        let effs = vec![println_eff("bye"), exit_eff(0)];
        let mut buf: Vec<u8> = Vec::new();
        let result = dispatch_all(&effs, &mut buf).unwrap();
        assert_eq!(result, Some(0));
        assert_eq!(String::from_utf8(buf).unwrap(), "bye\n");
    }

    #[test]
    fn dispatch_all_two_exits_first_code_wins() {
        // Both Exit effects are dispatched; first code (1) is returned.
        let effs = vec![exit_eff(1), exit_eff(2)];
        let mut buf: Vec<u8> = Vec::new();
        let result = dispatch_all(&effs, &mut buf).unwrap();
        assert_eq!(result, Some(1));
        // Both dispatched without error (no writes expected for Exit).
        assert!(buf.is_empty());
    }

    #[test]
    fn dispatch_all_empty_slice_returns_none() {
        let mut buf: Vec<u8> = Vec::new();
        let result = dispatch_all(&[], &mut buf).unwrap();
        assert_eq!(result, None);
        assert!(buf.is_empty());
    }

    // ------------------------------------------------------------------
    // Effect → Result production (last_results threading)
    // ------------------------------------------------------------------

    fn string_result(s: &str) -> Value {
        Value::Enum { ctor: "StringResult".into(), args: vec![Value::Str(s.into())] }
    }
    fn int_result(n: i64) -> Value {
        Value::Enum { ctor: "IntResult".into(), args: vec![Value::Int(n)] }
    }
    fn no_result_v() -> Value {
        Value::nullary("NoResult")
    }

    #[test]
    fn int_to_str_produces_string_result() {
        let mut buf: Vec<u8> = Vec::new();
        let (outcome, res) = dispatch_one(&int_to_str_eff(7), &mut buf).unwrap();
        assert_eq!(outcome, DispatchOutcome::Continue);
        assert_eq!(res, string_result("7"));
        assert!(buf.is_empty(), "IntToStr writes nothing");
    }

    #[test]
    fn int_to_str_negative_produces_string_result() {
        let mut buf: Vec<u8> = Vec::new();
        let (_outcome, res) = dispatch_one(&int_to_str_eff(-42), &mut buf).unwrap();
        assert_eq!(res, string_result("-42"));
    }

    #[test]
    fn parse_int_success_produces_int_result() {
        let mut buf: Vec<u8> = Vec::new();
        let (_outcome, res) = dispatch_one(&parse_int_eff("123"), &mut buf).unwrap();
        assert_eq!(res, int_result(123));
    }

    #[test]
    fn parse_int_failure_produces_error_result() {
        let mut buf: Vec<u8> = Vec::new();
        let (_outcome, res) = dispatch_one(&parse_int_eff("not-a-num"), &mut buf).unwrap();
        match res {
            Value::Enum { ctor, args } => {
                assert_eq!(ctor, "ErrorResult");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Value::Str(_)));
            }
            other => panic!("expected ErrorResult, got {other:?}"),
        }
    }

    #[test]
    fn time_and_monotonic_time_are_deterministic_int_results() {
        let mut buf: Vec<u8> = Vec::new();
        let time = EffectValue { ctor: "Time".into(), args: vec![] };
        let mono = EffectValue { ctor: "MonotonicTime".into(), args: vec![] };
        assert_eq!(dispatch_one(&time, &mut buf).unwrap().1, int_result(0));
        assert_eq!(dispatch_one(&mono, &mut buf).unwrap().1, int_result(0));
    }

    #[test]
    fn println_and_exit_produce_no_result() {
        let mut buf: Vec<u8> = Vec::new();
        assert_eq!(dispatch_one(&println_eff("x"), &mut buf).unwrap().1, no_result_v());
        assert_eq!(dispatch_one(&exit_eff(0), &mut buf).unwrap().1, no_result_v());
        assert_eq!(dispatch_one(&tick_eff(), &mut buf).unwrap().1, no_result_v());
    }

    #[test]
    fn dispatch_all_with_results_preserves_order_and_count() {
        // IntToStr(7) then Println("hi"): results are [StringResult("7"), NoResult].
        let effs = vec![int_to_str_eff(7), println_eff("hi")];
        let mut buf: Vec<u8> = Vec::new();
        let (exit, results) = dispatch_all_with_results(&effs, &mut buf).unwrap();
        assert_eq!(exit, None);
        assert_eq!(results, vec![string_result("7"), no_result_v()]);
        assert_eq!(String::from_utf8(buf).unwrap(), "hi\n");
    }
}
