//! Bench classically-symmetric CSPs against Z3 to find out whether
//! Evident programs we'd plausibly write are slow enough to motivate
//! a symmetry-breaking pass (Bliss + lex-leader).
//!
//! The conjecture from the math-foundations research wave: symmetric
//! problems pay an |orbit|× cost for redundant exploration that Z3
//! doesn't break automatically. We need to confirm:
//!   (a) Evident programs have such symmetries; and
//!   (b) they're slow enough that breaking the symmetry would matter.
//!
//! Three benchmarks:
//!   - N-queens (rotation + reflection symmetry, |orbit| = 8)
//!   - k-coloring with interchangeable colors (|orbit| = k!)
//!   - Pigeonhole / interchangeable items (|orbit| = n!)
//!
//! Build + run:  cargo run --release --example bench_symmetric

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

fn solve_inline(source: &str, claim_name: &str, given: HashMap<String, Value>) -> (bool, f64) {
    let tmp = std::env::temp_dir().join(format!("bench_symmetric_{}.ev", std::process::id()));
    fs::write(&tmp, source).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&tmp).unwrap();
    let t0 = Instant::now();
    let r = rt.query(claim_name, &given).unwrap();
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let _ = fs::remove_file(&tmp);
    (r.satisfied, ms)
}

// ── N-queens ─────────────────────────────────────────────────────

fn nqueens_source(n: i64) -> String {
    // Place n queens on n×n board, one per row, distinct columns,
    // no two on same diagonal. Symmetries: 8 (rotations + reflections).
    let mut s = String::new();
    s.push_str(&format!("claim NQueens_{n}\n"));
    s.push_str(&format!("    q ∈ Seq(Int)\n"));
    s.push_str(&format!("    #q = {n}\n"));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : 0 ≤ q[i] ∧ q[i] < {n}\n", n - 1));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : ∀ j ∈ {{0..{}}} : i < j ⇒ q[i] ≠ q[j]\n", n - 1, n - 1));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : ∀ j ∈ {{0..{}}} : i < j ⇒ q[i] - q[j] ≠ i - j\n", n - 1, n - 1));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : ∀ j ∈ {{0..{}}} : i < j ⇒ q[j] - q[i] ≠ j - i\n", n - 1, n - 1));
    s
}

fn bench_nqueens() {
    println!("\n── N-queens (8-fold symmetry per solution) ──");
    println!("  {:<10} {:>10}", "n", "solve");
    for n in [6, 8, 10, 12].iter().copied() {
        let src = nqueens_source(n);
        let (sat, ms) = solve_inline(&src, &format!("NQueens_{n}"), HashMap::new());
        println!("  n={:<8} {:>8.1}ms  {}", n, ms, if sat { "SAT" } else { "UNSAT" });
    }
}

// ── K-color a clique-of-cliques (interchangeable colors) ────────

fn kcolor_source(n: i64, k: i64) -> String {
    // Color n nodes with k colors such that adjacent nodes differ.
    // Edges: a single cycle of length n (graph = C_n). The k colors
    // are interchangeable — |orbit| per solution is k!.
    let mut s = String::new();
    s.push_str(&format!("claim KColor_{n}_{k}\n"));
    s.push_str(&format!("    c ∈ Seq(Int)\n"));
    s.push_str(&format!("    #c = {n}\n"));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : 0 ≤ c[i] ∧ c[i] < {k}\n", n - 1));
    // Cycle edges: c[i] ≠ c[i+1] for i in 0..n-1, plus c[n-1] ≠ c[0].
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : c[i] ≠ c[i+1]\n", n - 2));
    s.push_str(&format!("    c[{}] ≠ c[0]\n", n - 1));
    s
}

fn bench_kcolor() {
    println!("\n── K-coloring a cycle (k! color-permutation symmetry) ──");
    println!("  {:<14} {:>10}", "n,k", "solve");
    let cases: &[(i64, i64)] = &[(6, 3), (8, 3), (10, 3), (12, 3), (8, 4), (10, 4)];
    for &(n, k) in cases {
        let src = kcolor_source(n, k);
        let (sat, ms) = solve_inline(&src, &format!("KColor_{n}_{k}"), HashMap::new());
        println!("  n={n} k={k:<8} {:>8.1}ms  {}", ms, if sat { "SAT" } else { "UNSAT" });
    }
}

// ── Pigeonhole (interchangeable items) ──────────────────────────

fn pigeonhole_source(items: i64, holes: i64) -> String {
    // Each of `items` goes into one of `holes`, no two items share a
    // hole (so SAT iff items ≤ holes). When items < holes, the unused
    // holes are interchangeable; when items = holes-1 the symmetry is
    // most painful for the UNSAT case adjacent (items = holes+1).
    // We test the SAT edge (items = holes) and the just-UNSAT case
    // (items = holes + 1).
    let mut s = String::new();
    s.push_str(&format!("claim Pigeon_{items}_{holes}\n"));
    s.push_str(&format!("    assignment ∈ Seq(Int)\n"));
    s.push_str(&format!("    #assignment = {items}\n"));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : 0 ≤ assignment[i] ∧ assignment[i] < {holes}\n", items - 1));
    s.push_str(&format!("    ∀ i ∈ {{0..{}}} : ∀ j ∈ {{0..{}}} : i < j ⇒ assignment[i] ≠ assignment[j]\n", items - 1, items - 1));
    s
}

fn bench_pigeonhole() {
    println!("\n── Pigeonhole (item-permutation symmetry) ──");
    println!("  {:<18} {:>10}", "items,holes", "solve");
    // The UNSAT cases (items > holes) are where symmetry hurts most —
    // Z3 has to refute every permutation.
    let cases: &[(i64, i64)] = &[(6, 5), (8, 7), (10, 9), (11, 10), (12, 11)];
    for &(items, holes) in cases {
        let src = pigeonhole_source(items, holes);
        let (sat, ms) = solve_inline(&src, &format!("Pigeon_{items}_{holes}"), HashMap::new());
        println!("  items={items} holes={holes:<6} {:>8.1}ms  {}",
            ms, if sat { "SAT" } else { "UNSAT" });
    }
}

fn main() {
    println!("Symmetric-CSP bench — does Z3 actually pay the symmetry tax?");
    bench_nqueens();
    bench_kcolor();
    bench_pigeonhole();
    println!();
}
