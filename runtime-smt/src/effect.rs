//! N2 effect dispatcher — interprets a decoded `EffectValue` as IO.
//!
//! Two constructors are supported:
//!   * `Println(String)` — write the string + `\n` to `out`.
//!   * `Exit(Int)`       — request graceful process exit with the given code.
//!
//! Unknown constructors are silently ignored so fixtures can carry
//! uninterpreted effects (e.g. `Tick`, `DrawRect`, …) without error.
//!
//! Exit is *graceful*: `dispatch_all` dispatches every effect in the tick,
//! including those that follow an `Exit`, then returns the first exit code seen.

use std::io::Write;

use crate::spec::EffectValue;
use crate::z3c::Value;

/// Result of dispatching a single effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Continue,
    Exit(i32),
}

/// Dispatch one decoded effect to IO.
///
/// * `Println` with a first `Value::Str(s)` arg → writes `s\n` to `out`; returns
///   `Continue`. If `Println` has no `Str` first arg (or no args at all), writes
///   an empty line and returns `Continue` (lenient).
/// * `Exit` with a first `Value::Int(c)` arg → returns `Exit(c as i32)` and writes
///   nothing. If `Exit` has no `Int` first arg, treats the code as 0.
/// * Any other constructor → `Continue`, writes nothing.
pub fn dispatch(eff: &EffectValue, out: &mut dyn Write) -> std::io::Result<DispatchOutcome> {
    match eff.ctor.as_str() {
        "Println" => {
            let text = match eff.args.first() {
                Some(Value::Str(s)) => s.as_str(),
                _ => "",
            };
            writeln!(out, "{text}")?;
            Ok(DispatchOutcome::Continue)
        }
        "Exit" => {
            let code = match eff.args.first() {
                Some(Value::Int(c)) => *c as i32,
                _ => 0,
            };
            Ok(DispatchOutcome::Exit(code))
        }
        _ => Ok(DispatchOutcome::Continue),
    }
}

/// Dispatch a whole tick's effects, in order.
///
/// ALL effects are dispatched even if one is an `Exit` (Exit is graceful
/// end-of-tick — co-emitted `Println`s still run). Returns `Some(code)` if any
/// `Exit` was dispatched (the FIRST `Exit`'s code wins), else `None`.
pub fn dispatch_all(effects: &[EffectValue], out: &mut dyn Write) -> std::io::Result<Option<i32>> {
    let mut exit_code: Option<i32> = None;
    for eff in effects {
        match dispatch(eff, out)? {
            DispatchOutcome::Continue => {}
            DispatchOutcome::Exit(code) => {
                if exit_code.is_none() {
                    exit_code = Some(code);
                }
            }
        }
    }
    Ok(exit_code)
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
}
