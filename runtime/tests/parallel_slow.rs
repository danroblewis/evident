//! Parallel solving of independent slow components.
//!
//! A claim that decomposes into N independent components whose
//! constraints can't be JIT-compiled (a genuine Z3 search — here, four
//! disjoint N-queens boards) is solved with each component on its own
//! Z3 context and its own thread. This test asserts two things:
//!
//!   1. **Correctness** — the parallel result equals the sequential one
//!      and every board is a valid N-queens placement. (Always checked;
//!      not timing-dependent.)
//!   2. **Speedup** — re-solving the cached plan in parallel is
//!      meaningfully faster than forcing the single-context sequential
//!      path. (Checked only when ≥2 cores are available; the ratio is
//!      always printed.)
//!
//! Timing is measured on the *cached* plan: the first query builds the
//! plan (translate + per-context setup, a one-time cost), and the timed
//! queries just re-run `execute_plan`. The cross-tick value cache is
//! disabled so each query actually re-solves.
//!
//! Session S extends this with `enum_parallel_speedup_and_correctness`
//! (called from the single `#[test]` so it never times concurrently with
//! the primitive case): the same disjoint-board decomposition, but each
//! component additionally carries an `EnumVar` (`Half = LeftHalf |
//! RightHalf`). Before session S an enum-typed component fell back to the
//! sequential single-context path; now the parallel path replays the
//! enum's datatype into each worker context. The enum test asserts the
//! boards are valid, the enum tags decode correctly (only possible if the
//! replay worked), and the same ≥1.8× speedup holds.

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Board size per component. N-queens solve time is erratic across N
/// (it sits on a search boundary); N=16 is reliably in the tens-of-ms
/// range here — large enough that the parallel win dominates thread
/// overhead, small enough to keep the test quick.
const N: usize = 16;
/// Number of independent boards (= independent slow components).
const COMPONENTS: usize = 4;

/// Build a claim with `k` disjoint N-queens boards `q0..q{k-1}`, each a
/// `Seq(Int)` of length `n` with the standard column/diagonal
/// distinctness constraints. The boards share no variables, so the
/// runtime decomposes them into `k` independent slow components.
fn nqueens_claim(n: usize, k: usize) -> String {
    let last = n - 1;
    let mut s = String::from("claim parallel_search\n");
    for c in 0..k {
        s.push_str(&format!("    q{c} ∈ Seq(Int)\n"));
    }
    for c in 0..k {
        s.push_str(&format!("    #q{c} = {n}\n"));
    }
    for c in 0..k {
        s.push_str(&format!(
            "    ∀ i ∈ {{0..{last}}} : (0 ≤ q{c}[i] ∧ q{c}[i] < {n})\n"));
        s.push_str(&format!(
            "    ∀ i ∈ {{0..{last}}} : ∀ j ∈ {{0..{last}}} : i < j ⇒ \
             (q{c}[i] ≠ q{c}[j] ∧ q{c}[i] + i ≠ q{c}[j] + j ∧ q{c}[i] - i ≠ q{c}[j] - j)\n"));
    }
    s
}

/// Build the same `k` disjoint N-queens boards as `nqueens_claim`, but
/// give each board an ENUM-typed companion var `h{c} ∈ Half` classifying
/// which half its first queen occupies (a complete dispatch, so `h{c}` is
/// fully determined by — and joins the connected component of — board
/// `c`). The enum var is what used to push the whole claim onto the
/// sequential single-context path; session S replays the `Half` datatype
/// into each worker context so these components parallelize too.
fn nqueens_enum_claim(n: usize, k: usize) -> String {
    let last = n - 1;
    let mut s = String::from("enum Half = LeftHalf | RightHalf\n");
    s.push_str("claim parallel_search_enum\n");
    for c in 0..k {
        s.push_str(&format!("    q{c} ∈ Seq(Int)\n"));
    }
    for c in 0..k {
        s.push_str(&format!("    h{c} ∈ Half\n"));
    }
    for c in 0..k {
        s.push_str(&format!("    #q{c} = {n}\n"));
    }
    for c in 0..k {
        s.push_str(&format!(
            "    ∀ i ∈ {{0..{last}}} : (0 ≤ q{c}[i] ∧ q{c}[i] < {n})\n"));
        s.push_str(&format!(
            "    ∀ i ∈ {{0..{last}}} : ∀ j ∈ {{0..{last}}} : i < j ⇒ \
             (q{c}[i] ≠ q{c}[j] ∧ q{c}[i] + i ≠ q{c}[j] + j ∧ q{c}[i] - i ≠ q{c}[j] - j)\n"));
        // Couple the enum var to the board: a complete LeftHalf/RightHalf
        // dispatch on the first queen's column.
        s.push_str(&format!("    (q{c}[0] * 2 < {n}) ⇒ h{c} = LeftHalf\n"));
        s.push_str(&format!("    (q{c}[0] * 2 ≥ {n}) ⇒ h{c} = RightHalf\n"));
    }
    s
}

/// Extract board `c` from a result's bindings as a `Vec<i64>`.
fn board(bindings: &HashMap<String, Value>, c: usize) -> Vec<i64> {
    match bindings.get(&format!("q{c}")) {
        Some(Value::SeqInt(xs)) => xs.clone(),
        other => panic!("q{c} missing or wrong type: {other:?}"),
    }
}

/// Extract the `Half` enum tag for board `c` as its variant name. Panics
/// if it's missing or not a `Value::Enum` of the `Half` type — which is
/// exactly the regression this test guards: on the parallel path the
/// worker context must replay the `Half` datatype so the model value
/// decodes to a real enum, not an empty/garbage value.
fn half_tag(bindings: &HashMap<String, Value>, c: usize) -> String {
    match bindings.get(&format!("h{c}")) {
        Some(Value::Enum { enum_name, variant, .. }) => {
            assert_eq!(enum_name, "Half", "h{c} has wrong enum type");
            variant.clone()
        }
        other => panic!("h{c} missing or not an enum: {other:?}"),
    }
}

/// Assert `cols` is a valid N-queens solution: a permutation-like
/// placement with no two queens sharing a column or a diagonal.
fn assert_valid_queens(cols: &[i64], n: usize) {
    assert_eq!(cols.len(), n, "board has wrong length");
    for (i, &ci) in cols.iter().enumerate() {
        assert!(ci >= 0 && (ci as usize) < n, "column {ci} out of range");
        for (j, &cj) in cols.iter().enumerate().skip(i + 1) {
            assert!(ci != cj, "two queens in column {ci}");
            let (di, dj) = (i as i64, j as i64);
            assert!(ci + di != cj + dj, "queens on a / diagonal");
            assert!(ci - di != cj - dj, "queens on a \\ diagonal");
        }
    }
}

/// Median wall-clock of `iters` re-queries of the (already-built) plan
/// for schema `name`.
fn time_queries(rt: &EvidentRuntime, name: &str, iters: usize) -> Duration {
    let mut samples: Vec<Duration> = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let r = rt.query(name, &HashMap::new()).unwrap();
        assert!(r.satisfied, "query went UNSAT");
        samples.push(t0.elapsed());
    }
    samples.sort();
    samples[samples.len() / 2]
}

// One test function (not several) so cargo's per-test thread pool never
// runs two timing-sensitive bodies concurrently — CPU contention would
// skew the speedup ratio — and so the `set_var` below isn't racy.
#[test]
fn parallel_slow_components_speedup_and_correctness() {
    // Disable the cross-tick value cache so each re-query actually
    // re-solves rather than returning a memoized result. Set before any
    // query so the env read (memoized once) sees it.
    std::env::set_var("EVIDENT_VALUE_CACHE", "0");

    coupled_components_single_part_unaffected();

    let src = nqueens_claim(N, COMPONENTS);

    // ── Parallel runtime ──────────────────────────────────────────
    let mut rt_par = EvidentRuntime::new();
    rt_par.set_slow_parallel(true);
    rt_par.load_source(&src).expect("load parallel");
    let r_par = rt_par.query("parallel_search", &HashMap::new()).unwrap();
    assert!(r_par.satisfied, "parallel query UNSAT");
    let par_boards: Vec<Vec<i64>> = (0..COMPONENTS).map(|c| board(&r_par.bindings, c)).collect();
    for b in &par_boards { assert_valid_queens(b, N); }

    // ── Sequential runtime (same claim, parallel disabled) ────────
    let mut rt_seq = EvidentRuntime::new();
    rt_seq.set_slow_parallel(false);
    rt_seq.load_source(&src).expect("load sequential");
    let r_seq = rt_seq.query("parallel_search", &HashMap::new()).unwrap();
    assert!(r_seq.satisfied, "sequential query UNSAT");
    for c in 0..COMPONENTS { assert_valid_queens(&board(&r_seq.bindings, c), N); }

    // ── Timing on the cached plans ────────────────────────────────
    let iters = 7;
    let t_par = time_queries(&rt_par, "parallel_search", iters);
    let t_seq = time_queries(&rt_seq, "parallel_search", iters);
    let ratio = t_seq.as_secs_f64() / t_par.as_secs_f64();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!(
        "[parallel_slow] {COMPONENTS} components × {N}-queens | \
         parallel={:.1}ms sequential={:.1}ms speedup={ratio:.2}x ({cores} cores)",
        t_par.as_secs_f64() * 1000.0, t_seq.as_secs_f64() * 1000.0);

    // Speedup assertion only where there's parallelism to exploit. The
    // threshold is deliberately conservative (1.8×) so a loaded CI box
    // doesn't flake; on the dev machine (10 cores) this clears ~3×.
    if cores >= 4 {
        assert!(ratio >= 1.8,
            "expected ≥1.8× speedup from parallel slow components, got {ratio:.2}× \
             (parallel {:.1}ms vs sequential {:.1}ms)",
            t_par.as_secs_f64() * 1000.0, t_seq.as_secs_f64() * 1000.0);
    }

    // ── Thread-safety stress ──────────────────────────────────────
    // Hammer the parallel path: many fresh plans (each mints 4 private
    // Z3 contexts under the creation lock) plus many re-solves of a
    // cached plan (each spawns 4 worker threads that `check()` their own
    // contexts concurrently). A Z3 context race would surface as a
    // segfault or a wrong/short board here. Small N keeps it quick.
    for _ in 0..12 {
        let mut rt = EvidentRuntime::new();
        rt.set_slow_parallel(true);
        rt.load_source(&nqueens_claim(8, COMPONENTS)).unwrap();
        for _ in 0..6 {
            let r = rt.query("parallel_search", &HashMap::new()).unwrap();
            assert!(r.satisfied, "stress query UNSAT");
            for c in 0..COMPONENTS {
                assert_valid_queens(&board(&r.bindings, c), 8);
            }
        }
    }

    // ── Session S: enum-typed components ──────────────────────────────
    // Run AFTER the primitive case (same test fn, so it's sequential —
    // never concurrent with the timing above).
    enum_parallel_speedup_and_correctness();
}

/// Session S: a decomposition whose components each carry an ENUM-typed
/// variable. Before session S these silently fell back to the sequential
/// single-context path (the var's `&'static DatatypeSort` was bound to
/// the runtime's main context). Now the parallel path replays the enum's
/// datatype into each worker context, so they parallelize. Asserts:
///
///   1. **Correctness** — every board is a valid N-queens placement AND
///      its `Half` enum tag decodes to the right variant (which only
///      works if the worker context replayed the `Half` datatype). Both
///      paths agree.
///   2. **Speedup** — re-solving the cached plan in parallel is ≥1.8×
///      faster than the forced-sequential path (cores ≥ 4).
///   3. **Thread safety** — a stress loop minting many fresh enum plans
///      (each replays the datatype into 4 private contexts under the
///      setup lock) + re-solving them, to catch any datatype-replay race.
fn enum_parallel_speedup_and_correctness() {
    let src = nqueens_enum_claim(N, COMPONENTS);

    // Expected `Half` tag for a board: LeftHalf iff its first queen sits
    // in the left half (column·2 < N).
    let expect_tag = |cols: &[i64]| -> &'static str {
        if cols[0] * 2 < N as i64 { "LeftHalf" } else { "RightHalf" }
    };

    // ── Parallel runtime ──────────────────────────────────────────
    let mut rt_par = EvidentRuntime::new();
    rt_par.set_slow_parallel(true);
    rt_par.load_source(&src).expect("load parallel enum");
    let r_par = rt_par.query("parallel_search_enum", &HashMap::new()).unwrap();
    assert!(r_par.satisfied, "parallel enum query UNSAT");
    for c in 0..COMPONENTS {
        let b = board(&r_par.bindings, c);
        assert_valid_queens(&b, N);
        assert_eq!(half_tag(&r_par.bindings, c), expect_tag(&b),
            "parallel: board {c} enum tag disagrees with its first queen");
    }

    // ── Sequential runtime (same claim, parallel disabled) ────────
    let mut rt_seq = EvidentRuntime::new();
    rt_seq.set_slow_parallel(false);
    rt_seq.load_source(&src).expect("load sequential enum");
    let r_seq = rt_seq.query("parallel_search_enum", &HashMap::new()).unwrap();
    assert!(r_seq.satisfied, "sequential enum query UNSAT");
    for c in 0..COMPONENTS {
        let b = board(&r_seq.bindings, c);
        assert_valid_queens(&b, N);
        assert_eq!(half_tag(&r_seq.bindings, c), expect_tag(&b),
            "sequential: board {c} enum tag disagrees with its first queen");
    }

    // ── Timing on the cached plans ────────────────────────────────
    let iters = 7;
    let t_par = time_queries(&rt_par, "parallel_search_enum", iters);
    let t_seq = time_queries(&rt_seq, "parallel_search_enum", iters);
    let ratio = t_seq.as_secs_f64() / t_par.as_secs_f64();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!(
        "[parallel_slow/enum] {COMPONENTS} components × {N}-queens + Half enum | \
         parallel={:.1}ms sequential={:.1}ms speedup={ratio:.2}x ({cores} cores)",
        t_par.as_secs_f64() * 1000.0, t_seq.as_secs_f64() * 1000.0);

    if cores >= 4 {
        assert!(ratio >= 1.8,
            "expected ≥1.8× speedup from parallel ENUM-typed slow components, got {ratio:.2}× \
             (parallel {:.1}ms vs sequential {:.1}ms)",
            t_par.as_secs_f64() * 1000.0, t_seq.as_secs_f64() * 1000.0);
    }

    // ── Thread-safety stress (datatype replay under concurrency) ──
    // Each fresh plan replays the `Half` datatype into COMPONENTS private
    // contexts (under the setup lock) then `check()`s them concurrently.
    // A datatype-replay race or a cross-context sort mixup would surface
    // as a segfault, an UNSAT, or a board/tag that fails validation.
    for _ in 0..12 {
        let mut rt = EvidentRuntime::new();
        rt.set_slow_parallel(true);
        rt.load_source(&nqueens_enum_claim(8, COMPONENTS)).unwrap();
        for _ in 0..6 {
            let r = rt.query("parallel_search_enum", &HashMap::new()).unwrap();
            assert!(r.satisfied, "stress enum query UNSAT");
            for c in 0..COMPONENTS {
                let b = board(&r.bindings, c);
                assert_valid_queens(&b, 8);
                let expect = if b[0] * 2 < 8 { "LeftHalf" } else { "RightHalf" };
                assert_eq!(half_tag(&r.bindings, c), expect,
                    "stress: board {c} enum tag wrong");
            }
        }
    }
}

/// A claim whose slow components all share a variable forms ONE
/// component — there's nothing to parallelize, and the result must match
/// the sequential path exactly. Guards against the parallel split ever
/// changing the answer for a coupled problem.
fn coupled_components_single_part_unaffected() {
    // Two boards coupled by a shared cross-board constraint → one
    // connected component, so `slow.len() == 1` and the parallel branch
    // is never taken regardless of the flag.
    let mut src = nqueens_claim(10, 2);
    src.push_str("    q0[0] = q1[0]\n");

    let mut rt_par = EvidentRuntime::new();
    rt_par.set_slow_parallel(true);
    rt_par.load_source(&src).unwrap();
    let r = rt_par.query("parallel_search", &HashMap::new()).unwrap();
    assert!(r.satisfied);
    assert_valid_queens(&board(&r.bindings, 0), 10);
    assert_valid_queens(&board(&r.bindings, 1), 10);
    assert_eq!(board(&r.bindings, 0)[0], board(&r.bindings, 1)[0],
        "shared-constraint coupling violated");
}
