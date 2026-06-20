//! Differential testing: the functionizer (Cranelift JIT) is a lossy
//! optimization; the slow Z3 path is the correctness oracle. For any query the
//! JIT compiles, its bindings must match the oracle's exactly. This harness
//! runs the same query both ways in one process (via `set_functionize_enabled`)
//! and diffs the bindings — a mismatch is a functionizer correctness bug.

use std::collections::HashMap;
use std::path::Path;
use evident_runtime::{EvidentRuntime, Value};

/// Run `claim` both ways with the same `given`; return `Some(diff)` if the JIT
/// disagrees with the oracle. Handles errors gracefully: if BOTH paths reject
/// the program (same translator gap) that's agreement; if only one does, that
/// itself is a divergence.
fn diff_both_ways(rt: &EvidentRuntime, claim: &str, given: &HashMap<String, Value>) -> Option<String> {
    rt.set_functionize_enabled(false);
    let oracle = rt.query_with_pins_and_given(claim, &[], given);
    rt.set_functionize_enabled(true);
    let jit = rt.query_with_pins_and_given(claim, &[], given);
    rt.set_functionize_enabled(true);

    match (oracle, jit) {
        (Err(_), Err(_)) => None, // both reject the same way — agreement
        (Ok(o), Ok(j)) => {
            if o.satisfied != j.satisfied {
                return Some(format!("SAT mismatch: oracle={} jit={}", o.satisfied, j.satisfied));
            }
            let keys: std::collections::BTreeSet<&String> =
                o.bindings.keys().chain(j.bindings.keys()).collect();
            let mut diffs = Vec::new();
            for k in keys {
                if o.bindings.get(k) != j.bindings.get(k) {
                    diffs.push(format!("  {k}: oracle={:?}  jit={:?}",
                        o.bindings.get(k), j.bindings.get(k)));
                }
            }
            if diffs.is_empty() { None } else { Some(diffs.join("\n")) }
        }
        (o, j) => Some(format!("one path errored, the other didn't: \
            oracle_ok={} jit_ok={}", o.is_ok(), j.is_ok())),
    }
}

/// Sanity: a fully-determined enum transition must agree both ways.
#[test]
fn transition_agrees_both_ways() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum S = A | B
claim trans
    _s ∈ S
    s ∈ S
    s = match _s
        A ⇒ B
        B ⇒ B
    _s = A
").unwrap();
    if let Some(d) = diff_both_ways(&rt, "trans", &HashMap::new()) {
        panic!("JIT diverged from oracle on a plain transition:\n{d}");
    }
}

/// A `Seq(Int)` built from reads of a `Seq` element's fields.
#[test]
fn seqlit_from_seq_element_agrees_both_ways() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
type P(x, y ∈ Int)
claim build
    pts ∈ Seq(P)
    #pts = 3
    pts[0] = P(10, 20)
    pts[1] = P(30, 40)
    pts[2] = P(50, 60)
    rb ∈ Seq(Int)
    rb = ⟨pts[1].x, pts[1].y, 5, 5⟩
").unwrap();
    if let Some(d) = diff_both_ways(&rt, "build", &HashMap::new()) {
        panic!("JIT diverged from oracle building a Seq(Int) from Seq elements:\n{d}");
    }
}

/// A WHOLE-record `Seq` element extracted to a named var, then read.
#[test]
fn whole_record_seq_element_agrees_both_ways() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
type P(x, y ∈ Int)
claim build
    pts ∈ Seq(P)
    #pts = 3
    pts[0] = P(10, 20)
    pts[1] = P(30, 40)
    pts[2] = P(50, 60)
    v ∈ P
    v = pts[1]
    rb ∈ Seq(Int)
    rb = ⟨v.x, v.y, 5, 5⟩
").unwrap();
    if let Some(d) = diff_both_ways(&rt, "build", &HashMap::new()) {
        panic!("JIT diverged from oracle on whole-record Seq-element extract:\n{d}");
    }
}

/// The draw-path probe: an `ArgI32Buf` (the SDL rect buffer) built from a
/// `Seq(Int)` that came from Seq-element reads, wrapped in a `LibCall` effect —
/// the exact value the renderer marshals. Confirms the JIT and oracle agree on
/// the rect-buffer shape (they do — this is the kind of draw the functionizer
/// handles correctly, contrary to an earlier misdiagnosis).
#[test]
fn libcall_argi32buf_from_seq_element_agrees_both_ways() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_source("\
type P(x, y ∈ Int)
claim build
    pts ∈ Seq(P)
    #pts = 2
    pts[0] = P(11, 22)
    pts[1] = P(33, 44)
    rb ∈ Seq(Int)
    rb = ⟨pts[1].x, pts[1].y, 7, 7⟩
    eff ∈ Effect
    eff = LibCall(\"lib\", \"sym\", \"i(p)\", ⟨ArgI32Buf(rb)⟩)
").unwrap();
    if let Some(d) = diff_both_ways(&rt, "build", &HashMap::new()) {
        panic!("JIT diverged from oracle on a LibCall/ArgI32Buf from a Seq element \
                (the rect-buffer rendering bug):\n{d}");
    }
}
