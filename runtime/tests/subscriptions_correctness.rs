//! Correctness of the self-hosted subscriptions analysis — the runtime's
//! SOLE world-access-set implementation since session XX.
//!
//! This replaces `subscriptions_equivalence.rs`, which compared the
//! Evident pass against a canonical Rust walk. That Rust walk is now
//! deleted, so there is no oracle to compare against: instead, this test
//! pins the EXPECTED `(reads, writes)` for every FSM-shaped claim across
//! the demo corpus + Mario as direct expectations. The expectations were
//! captured from the Rust walk before its deletion (the cases the
//! equivalence test pinned), and now stand on their own.
//!
//! A regression in `stdlib/passes/subscriptions.ev` (or the marshaler it
//! consumes) surfaces here as a failing assertion against the pinned set.
//!
//! Three concerns:
//!   1. `corpus_access_sets` — the per-claim expectations across the corpus.
//!   2. `production_entry_matches` — the scheduler's actual entry point,
//!      `portable::subscriptions::access_sets` (cached engine + WW
//!      resolver), agrees with the explicitly-constructed engine.
//!   3. `bootstrap_*` — the bootstrap-cycle resolution made concrete.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use evident_runtime::portable::subscriptions::{
    self, EvidentSubscriptions, SubscriptionsImpl,
};
use evident_runtime::subscriptions::AccessSets;
use evident_runtime::EvidentRuntime;

const STDLIB: &str = "../stdlib";

fn evident_impl() -> EvidentSubscriptions {
    EvidentSubscriptions::new(Path::new(STDLIB))
        .expect("load stdlib/passes/subscriptions.ev")
}

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn sorted(set: &HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = set.iter().cloned().collect();
    v.sort();
    v
}

/// One pinned expectation: `(file, claim, expected_reads, expected_writes)`.
struct Expect {
    file:   &'static str,
    claim:  &'static str,
    reads:  &'static [&'static str],
    writes: &'static [&'static str],
}

/// Expected access sets for every FSM-shaped, world-touching claim in the
/// corpus. Captured from the canonical Rust walk before session XX deleted
/// it; these are now the contract the Evident pass must reproduce.
const EXPECTED: &[Expect] = &[
    Expect { file: "../examples/test_09_two_fsms.ev", claim: "producer",
             reads: &[], writes: &["n"] },
    Expect { file: "../examples/test_09_two_fsms.ev", claim: "consumer",
             reads: &["n"], writes: &[] },
    Expect { file: "../examples/test_14_stdin.ev", claim: "echo",
             reads: &["stdin_line", "stdin_seq"], writes: &[] },
    Expect { file: "../examples/test_15_signal.ev", claim: "guard",
             reads: &["signal_received"], writes: &[] },
    Expect { file: "../examples/test_18_reflection.ev", claim: "reflect_demo",
             reads: &["program"], writes: &[] },
    Expect { file: "../examples/test_25_per_component_jit.ev", claim: "sim",
             reads: &["distance", "frame", "score", "trail"],
             writes: &["distance", "frame", "score", "seen", "trail"] },
    Expect { file: "../examples/test_26_value_cache.ev", claim: "driver",
             reads: &[], writes: &["signal"] },
    Expect { file: "../examples/test_26_value_cache.ev", claim: "expensive",
             reads: &["signal"], writes: &[] },
    Expect { file: "../examples/test_30_jit_gap_closures.ev", claim: "gaps",
             reads: &["n", "trail"],
             writes: &["digest", "half", "n", "quad", "trail"] },
    Expect { file: "../examples/test_31_symbolic_regression.ev", claim: "regressor",
             reads: &["x"], writes: &["x", "y"] },
    Expect { file: "../examples/test_32_llm_functionizer.ev", claim: "classifier",
             reads: &["stdin_line"], writes: &["classified_line"] },
    Expect { file: "../examples/test_32_llm_functionizer.ev", claim: "printer",
             reads: &["classified_line", "stdin_line", "stdin_seq"], writes: &[] },
    // Mario — the typical multi-FSM target. `game` is the major writer,
    // `keyboard` writes input, `display` only reads (plus a tick write).
    Expect { file: "../examples/test_21_mario/main.ev", claim: "game",
             reads: &["coin_count", "coins", "dead", "enemies", "is_big",
                      "keys", "lives", "player", "tick", "won"],
             writes: &["camera_x", "coin_count", "coins", "dead", "enemies",
                       "is_big", "lives", "player", "won"] },
    Expect { file: "../examples/test_21_mario/main.ev", claim: "keyboard",
             reads: &["tick"], writes: &["keys"] },
    Expect { file: "../examples/test_21_mario/main.ev", claim: "display",
             reads: &["camera_x", "coin_count", "coins", "enemies", "is_big",
                      "lives", "player", "won"],
             writes: &["tick"] },
];

/// Load `path` into a fresh runtime (with stdlib/runtime.ev for the demos
/// that import it).
fn load(path: &Path) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).ok();
    rt.load_file(path)
        .unwrap_or_else(|e| panic!("loading {path:?} failed: {e}"));
    rt
}

// ── 1. Corpus — pinned expectations, no Rust oracle ──

#[test]
fn corpus_access_sets() {
    let ev = evident_impl();
    let mut checked = 0;
    for exp in EXPECTED {
        let path: PathBuf = exp.file.into();
        assert!(path.exists(), "corpus file {path:?} not found; update EXPECTED");
        let rt = load(&path);
        let schema = rt.get_schema(exp.claim).unwrap_or_else(||
            panic!("claim `{}` not found in {}", exp.claim, exp.file));
        let got = ev.access_sets(schema);
        let want = AccessSets { reads: set(exp.reads), writes: set(exp.writes) };
        assert_eq!(got, want,
            "{}::{}:\n  expected reads={:?} writes={:?}\n  got      reads={:?} writes={:?}",
            exp.file, exp.claim, exp.reads, exp.writes,
            sorted(&got.reads), sorted(&got.writes));
        checked += 1;
    }
    assert!(checked >= 15, "expected ≥15 pinned claims; checked {checked}");
}

// ── 2. The production entry point agrees with the explicit engine ──

#[test]
fn production_entry_matches() {
    // `portable::subscriptions::access_sets` is what the scheduler calls:
    // it builds a cached engine via the WW stdlib resolver. Assert it
    // produces the same sets as an explicitly-constructed engine on Mario's
    // three FSMs — exercising the resolver path + the thread-local cache.
    let ev = evident_impl();
    let rt = load(Path::new("../examples/test_21_mario/main.ev"));
    for claim in ["game", "keyboard", "display"] {
        let schema = rt.get_schema(claim).unwrap();
        let direct = ev.access_sets(schema);
        let prod = subscriptions::access_sets(schema);
        assert_eq!(direct.reads, prod.reads, "mario `{claim}` reads via production entry");
        assert_eq!(direct.writes, prod.writes, "mario `{claim}` writes via production entry");
    }
}

// ── 3. Bootstrap-cycle resolution, made concrete ──

#[test]
fn bootstrap_walk_fsm_has_empty_world_access() {
    // The pass FSM `subscriptions_walk` reads no `world.X` — its state is
    // the plain `SW` stack machine. So even if the scheduler tried to
    // schedule it, computing ITS subscriptions needs nothing: the analysis
    // does not recurse into needing subscriptions for the analysis. Load
    // the pass into a runtime and walk the FSM's own declaration: empty.
    let ev = evident_impl();
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/passes/subscriptions.ev"))
        .expect("load the pass");
    let walk = rt.get_schema("subscriptions_walk")
        .expect("subscriptions_walk declared");
    let got = ev.access_sets(walk);
    assert!(got.reads.is_empty(),
        "subscriptions_walk should read no world fields; got {:?}", sorted(&got.reads));
    assert!(got.writes.is_empty(),
        "subscriptions_walk should write no world fields; got {:?}", sorted(&got.writes));
}

#[test]
fn bootstrap_idempotent_across_calls() {
    // The cached engine is reused across calls (thread-local). Two calls on
    // the same claim must return identical sets — the per-tick solve that
    // drives the walk is a pure function of the marshaled body, and reusing
    // the JIT-cached runtime doesn't perturb it.
    let rt = load(Path::new("../examples/test_21_mario/main.ev"));
    let game = rt.get_schema("game").unwrap();
    let a = subscriptions::access_sets(game);
    let b = subscriptions::access_sets(game);
    assert_eq!(a.reads, b.reads);
    assert_eq!(a.writes, b.writes);
}

// ── 4. Trivial sanity — impl-name plumbing ──

#[test]
fn impl_name_is_evident() {
    use evident_runtime::portable::Portable;
    assert_eq!(evident_impl().impl_name(), "evident");
}
