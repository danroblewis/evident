//! Tick-loop timing summary. Gated by `EVIDENT_LOOP_TIMING`.

/// Per-FSM rows: `(claim_name, solve_total, ticks_solved)`.
/// Empty slice → omit the per-FSM breakdown.
pub(super) fn print_timing_summary_full(
    loop_t0: std::time::Instant,
    steps: usize,
    total_solve: std::time::Duration,
    total_dispatch: std::time::Duration,
    per_fsm: &[(&str, std::time::Duration, usize)],
) {
    let wall = loop_t0.elapsed();
    let other = wall.saturating_sub(total_solve).saturating_sub(total_dispatch);
    eprintln!("[timing] ── summary ──────────────────────────────");
    eprintln!("[timing] steps:    {steps}");
    eprintln!("[timing] wall:     {:>7.2}ms ({:>5.1}ms/step)",
        wall.as_secs_f64() * 1000.0,
        if steps > 0 { wall.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] solve:    {:>7.2}ms ({:>5.1}ms/step)",
        total_solve.as_secs_f64() * 1000.0,
        if steps > 0 { total_solve.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    for (name, solve, ticks) in per_fsm {
        let per_tick = if *ticks > 0 {
            solve.as_secs_f64() * 1000.0 / *ticks as f64
        } else { 0.0 };
        eprintln!("[timing]   {:<10} {:>7.2}ms ({:>5.1}ms/tick × {} ticks)",
            name, solve.as_secs_f64() * 1000.0, per_tick, ticks);
    }
    eprintln!("[timing] dispatch: {:>7.2}ms ({:>5.1}ms/step)",
        total_dispatch.as_secs_f64() * 1000.0,
        if steps > 0 { total_dispatch.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] other:    {:>7.2}ms (encoding, decoding, idle)",
        other.as_secs_f64() * 1000.0);
}
