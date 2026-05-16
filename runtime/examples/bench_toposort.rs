//! Performance benchmark for the self-hosted `Toposort<Int>` claim.
//!
//! Runs the stdlib toposort against a matrix of graph shapes and
//! sizes, captures wall-clock time per query, prints a table.
//!
//! Build + run:
//!     cargo run --release --example bench_toposort
//!
//! Optional filter: pass shape names on argv to restrict the run,
//! e.g. `cargo run --release --example bench_toposort -- linear empty`.
//!
//! Each cell measures one `query` call: parse + translate + Z3 solve
//! + extract. Runtime loading (parse stdlib) happens once per cell
//! because the runtime is created fresh inside `run_toposort` — that
//! is the realistic cost of "use toposort as a self-host primitive".

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::time::{Duration, Instant};

fn edges_given(pairs: &[(i64, i64)]) -> Value {
    Value::SeqComposite(pairs.iter().map(|(f, t)| {
        let mut m = HashMap::new();
        m.insert("from".to_string(), Value::Int(*f));
        m.insert("to".to_string(),   Value::Int(*t));
        m
    }).collect())
}

#[derive(Debug)]
struct CellResult {
    load:       Duration,
    solve:      Duration,
    satisfied:  bool,
}

fn run_toposort(items: &[i64], edges: &[(i64, i64)]) -> CellResult {
    let t0 = Instant::now();
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();
    let load = t0.elapsed();
    let t1 = Instant::now();
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("items".into(), Value::SetInt(items.to_vec()));
    given.insert("edges".into(), edges_given(edges));
    let r = rt.query("Toposort<Int>", &given).unwrap();
    CellResult { load, solve: t1.elapsed(), satisfied: r.satisfied }
}

// ── Graph shape generators ────────────────────────────────────────

fn empty(n: usize) -> (Vec<i64>, Vec<(i64, i64)>) {
    ((0..n as i64).collect(), vec![])
}

fn linear_chain(n: usize) -> (Vec<i64>, Vec<(i64, i64)>) {
    let items: Vec<i64> = (0..n as i64).collect();
    let edges: Vec<(i64, i64)> = (0..n as i64 - 1).map(|i| (i, i + 1)).collect();
    (items, edges)
}

fn fanout_tree(n: usize) -> (Vec<i64>, Vec<(i64, i64)>) {
    // Node 0 is root; every other node is a child of 0.
    // n-1 edges, all parallel-orderable.
    let items: Vec<i64> = (0..n as i64).collect();
    let edges: Vec<(i64, i64)> = (1..n as i64).map(|i| (0, i)).collect();
    (items, edges)
}

fn dense_dag(n: usize, edges_per_node: usize, seed: u64) -> (Vec<i64>, Vec<(i64, i64)>) {
    // Pseudo-random DAG: for each node, pick k forward edges into
    // nodes with strictly higher indices. Guaranteed acyclic.
    let items: Vec<i64> = (0..n as i64).collect();
    let mut rng_state = seed;
    let mut next = || {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        rng_state >> 33
    };
    let mut edges = Vec::new();
    for i in 0..n as i64 {
        let remaining = (n as i64 - 1 - i) as usize;
        if remaining == 0 { continue; }
        let k = edges_per_node.min(remaining);
        // Sample k distinct targets in (i, n)
        let mut chosen: Vec<i64> = (i + 1..n as i64).collect();
        for j in 0..k {
            let idx = (next() as usize) % (chosen.len() - j) + j;
            chosen.swap(j, idx);
            edges.push((i, chosen[j]));
        }
    }
    (items, edges)
}

// ── Runner ────────────────────────────────────────────────────────

fn fmt_ms(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1.0       { format!("{:.2}ms",  ms) }
    else if ms < 100.0 { format!("{:.1}ms",  ms) }
    else if ms < 10_000.0 { format!("{:.0}ms", ms) }
    else { format!("{:.1}s", d.as_secs_f64()) }
}

fn bench_cell(name: &str, n: usize, edges: usize,
              items: &[i64], edges_vec: &[(i64, i64)]) {
    let r = run_toposort(items, edges_vec);
    println!("  {:<14} n={:<4} edges={:<4} load={:>8}  solve={:>8}  {}",
             name, n, edges, fmt_ms(r.load), fmt_ms(r.solve),
             if r.satisfied { "SAT" } else { "UNSAT" });
}

fn main() {
    let filter: Vec<String> = env::args().skip(1).collect();
    let want = |name: &str| filter.is_empty() || filter.iter().any(|f| f == name);
    let max_n: usize = env::var("MAX_N").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(64);

    println!("Toposort<Int> bench — release build");
    println!("  shape          n     edges   load      solve     result");
    println!("  ----------     ----  -----   --------  --------  ------");

    let sizes: Vec<usize> = [4usize, 8, 16, 32, 64].into_iter()
        .filter(|&n| n <= max_n).collect();

    if want("empty") {
        for &n in &sizes {
            let (items, edges) = empty(n);
            bench_cell("empty",          n, edges.len(), &items, &edges);
        }
    }

    if want("linear") {
        for &n in &sizes {
            let (items, edges) = linear_chain(n);
            bench_cell("linear_chain",   n, edges.len(), &items, &edges);
        }
    }

    if want("fanout") {
        for &n in &sizes {
            let (items, edges) = fanout_tree(n);
            bench_cell("fanout_tree",    n, edges.len(), &items, &edges);
        }
    }

    if want("dense") {
        for &n in &sizes {
            let (items, edges) = dense_dag(n, n / 4, 0xC0FFEE);
            bench_cell("dense_dag",      n, edges.len(), &items, &edges);
        }
    }
}
