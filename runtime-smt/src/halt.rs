//! Halt detector — pure decision function; no IO, no Z3.

/// Why a run stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HaltReason {
    /// An `Exit(code)` effect was dispatched this tick (graceful).
    Exit(i32),
    /// The FSM raised its declared halt signal this tick.
    HaltFlag,
    /// Fixed point: state unchanged AND no effects emitted this tick.
    NoProgress,
    /// The driver's safety cap was hit.  `decide` never returns this variant;
    /// it exists so the driver can construct it directly when its tick counter
    /// expires.
    MaxTicks,
}

/// Decide whether to halt after a tick.
///
/// # Parameters
/// - `exit_code`: `Some(code)` if an `Exit(code)` effect was dispatched this tick.
/// - `halt_flag`: the FSM's declared halt signal this tick.
/// - `progressed`: `true` if the state changed OR at least one effect was emitted
///   this tick.
///
/// # Precedence
/// `Exit` > `HaltFlag` > `NoProgress`.
///
/// # Returns
/// `Some(reason)` to stop the run, `None` to continue.
/// This function never returns `Some(HaltReason::MaxTicks)`.
pub fn decide(exit_code: Option<i32>, halt_flag: bool, progressed: bool) -> Option<HaltReason> {
    if let Some(code) = exit_code {
        return Some(HaltReason::Exit(code));
    }
    if halt_flag {
        return Some(HaltReason::HaltFlag);
    }
    if !progressed {
        return Some(HaltReason::NoProgress);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{decide, HaltReason};

    #[test]
    fn exit_zero_no_halt_flag_progressed() {
        assert_eq!(decide(Some(0), false, true), Some(HaltReason::Exit(0)));
    }

    #[test]
    fn exit_beats_halt_flag() {
        // Exit(42) wins even when halt_flag is true
        assert_eq!(decide(Some(42), true, true), Some(HaltReason::Exit(42)));
    }

    #[test]
    fn halt_flag_no_exit_progressed() {
        assert_eq!(decide(None, true, true), Some(HaltReason::HaltFlag));
    }

    #[test]
    fn halt_flag_beats_no_progress() {
        // HaltFlag wins even when progressed is false
        assert_eq!(decide(None, true, false), Some(HaltReason::HaltFlag));
    }

    #[test]
    fn no_progress_no_exit_no_halt_flag() {
        assert_eq!(decide(None, false, false), Some(HaltReason::NoProgress));
    }

    #[test]
    fn none_when_all_clear() {
        // No exit, no halt flag, and state did progress → keep going
        assert_eq!(decide(None, false, true), None);
    }

    #[test]
    fn decide_never_returns_max_ticks() {
        // Exhaustive 2x2x2 truth table: decide must never yield MaxTicks
        let exit_codes: [Option<i32>; 2] = [None, Some(1)];
        let halt_flags = [false, true];
        let progressed_values = [false, true];

        for &exit_code in &exit_codes {
            for &halt_flag in &halt_flags {
                for &progressed in &progressed_values {
                    let result = decide(exit_code, halt_flag, progressed);
                    assert_ne!(
                        result,
                        Some(HaltReason::MaxTicks),
                        "decide({:?}, {}, {}) unexpectedly returned MaxTicks",
                        exit_code,
                        halt_flag,
                        progressed
                    );
                }
            }
        }
    }
}
