//! Performance regression tests.
//!
//! These are `#[ignore]` by default — `cargo test` skips them so the
//! standard suite stays fast and deterministic. To run:
//!
//!     cargo test --release --test perf -- --ignored --nocapture
//!
//! `--release` matters: a debug build's per-frame solve is ~10× slower
//! than release, which is exactly the trap that motivated these tests.
//! `--nocapture` lets you see the per-config timings.
//!
//! Why one big test, not three: each measurement reconfigures Z3 via
//! the `EVIDENT_Z3_ARITH_SOLVER` env var (which the runtime's
//! per-solver tuning hook reads). Env vars + parallel test threads
//! don't mix; the cleanest fix is to run all the configs in one
//! serial test, in a fixed order, with internal assertions on the
//! relationships between them.
//!
//! What it covers:
//!   - The per-frame solve on anchor_collect stays under a generous
//!     ceiling on reference hardware — primary regression guard.
//!   - `smt.arith.solver=2` is meaningfully faster than the Z3 4.8.12
//!     default on this workload. If this stops being true (most likely
//!     because the system Z3 was upgraded to 4.13+ which auto-picks a
//!     faster path), the explicit tuning in `apply_solver_tuning` in
//!     `runtime-rust/src/translate/eval.rs` can be removed — the
//!     failure message says exactly that.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use evident_runtime::runtime::EvidentRuntime;
use evident_runtime::translate::Value;

/// Synthesize the `given` map the SDL executor would pass per frame:
/// input.* (10 fields), window.* (6), state.player.* (4), and a 4-element
/// `state.dots ∈ Seq(DotState)` Seq-of-composite. Frame index varies a
/// few values so each call looks like a new frame to the solver.
fn make_anchor_collect_given(frame: i64) -> HashMap<String, Value> {
    let mut g = HashMap::new();
    let v = |n: i64| Value::Int(n);
    let b = |x: bool| Value::Bool(x);

    g.insert("input.right_held".into(), b(frame % 2 == 0));
    g.insert("input.left_held".into(),  b(false));
    g.insert("input.up_held".into(),    b(false));
    g.insert("input.down_held".into(),  b(frame % 3 == 0));
    g.insert("input.mouse_x".into(),    v(400 + (frame % 50)));
    g.insert("input.mouse_y".into(),    v(300));
    g.insert("input.click".into(),      b(false));
    g.insert("input.quit".into(),       b(false));
    g.insert("input.time".into(),       v(1700000000 + frame));
    g.insert("input.dt".into(),         v(16));

    g.insert("window.screen_x".into(), v(100));
    g.insert("window.screen_y".into(), v(100));
    g.insert("window.width".into(),    v(800));
    g.insert("window.height".into(),   v(600));
    g.insert("window.dx".into(),       v(0));
    g.insert("window.dy".into(),       v(0));

    g.insert("state.player.x".into(),         v(400));
    g.insert("state.player.y".into(),         v(300));
    g.insert("state.player.anchor_x".into(),  v(400));
    g.insert("state.player.anchor_y".into(),  v(300));

    let mut dots: Vec<HashMap<String, Value>> = Vec::with_capacity(4);
    for i in 0..4 {
        let mut d = HashMap::new();
        d.insert("pos_x".into(),     v(100 + i * 100 + frame % 10));
        d.insert("pos_y".into(),     v(100 + frame % 50));
        d.insert("vx".into(),        v(2));
        d.insert("vy".into(),        v(-3));
        d.insert("collected".into(), b(false));
        dots.push(d);
    }
    g.insert("state.dots".into(), Value::SeqComposite(dots));
    g
}

/// Resolve `programs/sdl_demo/anchor_collect.ev` whether the test is
/// run from the workspace root or from `runtime-rust/`.
fn anchor_collect_path() -> &'static Path {
    if Path::new("../programs/sdl_demo/anchor_collect.ev").exists() {
        Path::new("../programs/sdl_demo/anchor_collect.ev")
    } else {
        Path::new("programs/sdl_demo/anchor_collect.ev")
    }
}

/// Time `iters` per-frame solves with `smt.arith.solver = arith` and
/// return ms/iter. Disables the auto-tuner and pins the config via
/// `EVIDENT_Z3_ARITH_SOLVER` so the measurement reflects ONE config,
/// not an average across the auto-tuner's pricing windows.
///
/// Tests using this helper must run single-threaded
/// (`--test-threads=1`) — env vars are process-global.
fn bench_anchor_collect(arith_solver: u32, iters: usize) -> f64 {
    std::env::set_var("EVIDENT_Z3_AUTOTUNE", "0");
    std::env::set_var("EVIDENT_Z3_ARITH_SOLVER", arith_solver.to_string());

    let mut rt = EvidentRuntime::new();
    rt.load_source(evident_runtime::plugins::sdl::STDLIB_SDL_EV)
        .expect("sdl stdlib load");
    rt.load_file(anchor_collect_path()).expect("load anchor_collect");

    // Warmup primes the cache so we measure steady-state per-frame cost.
    let g0 = make_anchor_collect_given(0);
    let _ = rt.query_cached("main", &g0).expect("warmup query");

    let start = Instant::now();
    for i in 0..iters {
        let g = make_anchor_collect_given(i as i64);
        let r = rt.query_cached("main", &g).expect("query");
        assert!(r.satisfied, "anchor_collect main should always be SAT");
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;
    // Don't leak env config into the next test in this process.
    std::env::remove_var("EVIDENT_Z3_AUTOTUNE");
    std::env::remove_var("EVIDENT_Z3_ARITH_SOLVER");
    ms
}

/// Per-frame given for the drift_workload schema. Seed varies linearly
/// so the solver sees a fresh arithmetic problem every frame.
fn make_drift_given(frame: i64) -> HashMap<String, Value> {
    let mut g = HashMap::new();
    // Seed walks across the [-10000, 10000] range covered by the
    // schema bounds. Step is coprime with the bounds so we don't
    // revisit values quickly.
    let seed = ((frame * 137).rem_euclid(20001)) - 10000;
    g.insert("seed".into(), Value::Int(seed));
    g
}

/// Resolve the drift workload .ev file relative to either the workspace
/// root or `runtime-rust/`.
fn drift_workload_path() -> &'static Path {
    if Path::new("tests/data/drift_workload.ev").exists() {
        Path::new("tests/data/drift_workload.ev")
    } else {
        Path::new("runtime-rust/tests/data/drift_workload.ev")
    }
}

/// Run a long-session bench on the synthetic drift_workload schema.
/// Returns (first_window_mean, last_window_mean) ms/iter. Designed
/// to expose Z3 push/pop accumulation: per-frame givens force the
/// arithmetic theory to learn distinct facts, and over thousands of
/// frames the cached solver may drag.
fn long_session_drift(window: usize, epochs: usize) -> (f64, f64) {
    let mut rt = EvidentRuntime::new();
    rt.load_file(drift_workload_path()).expect("load drift_workload");

    // Warmup. Burn enough frames that any one-time caching is done.
    for i in 0..50 {
        let g = make_drift_given(i as i64);
        let _ = rt.query_cached("main", &g).expect("warmup");
    }

    let measure = |rt: &EvidentRuntime, base: i64, n: usize| -> f64 {
        let start = Instant::now();
        for i in 0..n {
            let g = make_drift_given(base + i as i64);
            let _ = rt.query_cached("main", &g).expect("query");
        }
        start.elapsed().as_secs_f64() * 1000.0 / n as f64
    };

    let first = measure(&rt, 50, window);

    // The "long session" — drive many more frames with no measurement.
    for e in 1..epochs {
        let base = (50 + window) as i64 + (e * window) as i64;
        for i in 0..window {
            let g = make_drift_given(base + i as i64);
            let _ = rt.query_cached("main", &g).expect("epoch");
        }
    }

    let last_base = (50 + window + (epochs - 1) * window) as i64;
    let last = measure(&rt, last_base, window);
    (first, last)
}

/// Push/pop drift regression guard. Runs a long session against the
/// synthetic `drift_workload.ev` schema (8 int vars, 5 arithmetic
/// constraints involving a per-frame `seed`, designed to force fresh
/// theory work every frame) and asserts the per-frame cost in the
/// LAST window is no more than 1.5× the FIRST window's cost.
///
/// Empirical baseline (Z3 4.8.12 on Ryzen 9 3900X, 2026-05): the
/// last window is *faster*, not slower — typically ~0.17× the first
/// window. Z3's incremental learning accumulates clauses that
/// *help* later queries rather than slow them down, and pinned-int
/// elimination at build_cache time prunes most of the per-frame work
/// before Z3 even sees it.
///
/// So this test is a forward-looking guard, not a current-failing
/// regression: it catches a future change (Z3 upgrade, runtime
/// refactor, new schema patterns) that introduces real drift. If it
/// ever fires, the suggested fix is EWMA-driven cache invalidation
/// — track per-frame solve time, and `cache.borrow_mut().remove
/// ("main")` once recent EWMA exceeds the established baseline by
/// some multiple. That makes the next call build a fresh solver,
/// which discards the now-harmful accumulated state.
#[test]
#[ignore]
fn long_session_no_drift() {
    // window=50, epochs=60 → 3000 frames total, large enough to
    // expose accumulation while still finishing in a few seconds.
    let (first, last) = long_session_drift(50, 60);
    let ratio = last / first;
    println!("first window:  {first:.2} ms/iter");
    println!("last window:   {last:.2} ms/iter");
    println!("drift ratio:   {ratio:.2}×");
    assert!(ratio < 1.5,
        "long-session push/pop drift detected: last window {last:.2} ms/iter \
         is {ratio:.2}× the first window's {first:.2} ms/iter (threshold 1.5×). \
         The cached solver likely needs EWMA-driven invalidation to recover \
         from accumulated learned-clause / theory state.");
}

/// Runtime should recover from a bad solver choice.
///
/// We have one concrete case where Z3 arith.solver=6 is ~5× slower
/// than =2 on anchor_collect (the original incident that motivated
/// this whole investigation). Forcing solver=6 via the env var, the
/// runtime currently has no recourse: every `query_cached` call
/// re-applies arith.solver=6 from the env, so steady-state stays
/// slow forever.
///
/// This test asserts that even when the *initial* config is bad, the
/// runtime should detect the persistent slowdown and switch to a
/// faster config (whether by trying alternates against shadow frames
/// or by simpler EWMA-driven cache-rebuild + reconfig). After warmup
/// + 200 frames, steady-state solve time should be < 10 ms regardless
/// of how the runtime was initially configured.
///
/// Currently FAILS — no auto-switching exists. The fix is the path
/// we discussed: track per-frame solve EWMA, on persistent slowness
/// rebuild the cached solver under a different arith.solver and keep
/// whichever config gives the better steady-state. Env var should
/// become a *hint* (initial config), not a hard pin.
#[test]
#[ignore]
fn runtime_recovers_from_bad_solver_choice() {
    // Force the known-slow config. Once the runtime has auto-switching,
    // this should be treated as the initial config, not the permanent one.
    std::env::set_var("EVIDENT_Z3_ARITH_SOLVER", "6");

    let mut rt = EvidentRuntime::new();
    rt.load_source(evident_runtime::plugins::sdl::STDLIB_SDL_EV)
        .expect("sdl stdlib load");
    rt.load_file(anchor_collect_path()).expect("load anchor_collect");

    // Burn a generous number of frames so any auto-switcher has time
    // to detect slowness, swap configs, and stabilize.
    for i in 0..200 {
        let g = make_anchor_collect_given(i);
        let _ = rt.query_cached("main", &g).expect("warmup");
    }

    // Measure the next 50 frames. If the runtime can switch, this
    // window should be fast (< 10 ms/iter, the same threshold we
    // require under the runtime's own default tuning).
    let start = Instant::now();
    for i in 200..250 {
        let g = make_anchor_collect_given(i);
        let _ = rt.query_cached("main", &g).expect("steady-state query");
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / 50.0;

    // Restore so subsequent tests in the same process aren't poisoned.
    std::env::remove_var("EVIDENT_Z3_ARITH_SOLVER");

    println!("steady-state ms/iter after starting with solver=6: {ms:.2}");
    assert!(ms < 10.0,
        "runtime did not recover from a bad initial solver choice. \
         After 200 warmup frames + 50 measured frames, steady-state is \
         {ms:.2} ms/iter — same as the unrecovered solver=6 baseline. \
         Implement auto-switching: track per-frame EWMA, rebuild cache \
         under a faster arith.solver when slowness persists. Env var \
         EVIDENT_Z3_ARITH_SOLVER should become a *hint* (initial config), \
         not a hard pin enforced on every call.");
}

/// All-in-one perf regression check. Runs three configs in sequence,
/// prints a summary table, and asserts:
///   1. The runtime's default tuning (arith.solver=2) keeps per-frame
///      solve under 15 ms.
///   2. arith.solver=2 is meaningfully (>2×) faster than arith.solver=6
///      on this Z3 version, justifying the explicit tuning in the
///      runtime. If this stops being true after a Z3 upgrade, the
///      tuning can probably be removed.
///
/// Run with:
///     cargo test --release --test perf -- --ignored --nocapture --test-threads=1
#[test]
#[ignore]
fn frame_loop_perf_regression() {
    const ITERS: usize = 50;

    let tuned    = bench_anchor_collect(2, ITERS);
    let baseline = bench_anchor_collect(6, ITERS);
    let speedup  = baseline / tuned;

    println!();
    println!("anchor_collect per-frame solve, {ITERS} iters per config:");
    println!("  arith.solver=2 (runtime default)  {tuned:6.2} ms/iter");
    println!("  arith.solver=6 (Z3 4.8.12 auto)   {baseline:6.2} ms/iter");
    println!("  speedup                           {speedup:6.2}×");

    // Assertion 1: the tuned path is fast enough. The Ryzen 9 3900X
    // reference number is ~4 ms; 15 ms is generous to absorb slower CI
    // VMs and noisy runs. Failing this means a real regression in
    // either the runtime's per-frame given assembly or in the
    // apply_solver_tuning hook itself.
    assert!(tuned < 15.0,
        "per-frame solve too slow with default tuning: {tuned:.2} ms/iter \
         (ceiling 15 ms). Investigate: was apply_solver_tuning bypassed? \
         Did the per-frame given grow? Are cache_rebuilds firing every frame?");

    // Assertion 2: the explicit tuning is still buying a real speedup.
    // 2× is the floor; we typically observe 5-7× on Z3 4.8.12. If this
    // shrinks below 2×, the system Z3 has likely been upgraded (4.13+
    // auto-picks a faster default) — flag it so we can revisit whether
    // the explicit tuning is still pulling its weight.
    assert!(speedup > 2.0,
        "smt.arith.solver=2 should be >2× faster than =6 on this Z3 \
         version, got {speedup:.2}×. If the system Z3 is now 4.13+, \
         the explicit tuning in runtime-rust/src/translate/eval.rs \
         can likely be dropped.");
}
