//! Cross-validation: the Rust `subscriptions` impl vs the Evident
//! `stdlib/passes/subscriptions.ev` impl, both reached through the
//! `portable::subscriptions` swap interface.
//!
//! For every FSM-shaped claim across the demo corpus, both impls must
//! produce *byte-identical* `AccessSets` (same HashSet of read field
//! names, same HashSet of write field names). The Rust impl is canonical;
//! a divergence is an Evident-pass bug.
//!
//! Corpus = every `examples/test_NN_*.ev` whose source references
//! `world.` or `world_next.` (i.e. participates in subscription
//! inference), plus the Mario demo as the typical multi-FSM case. The
//! `test_NN_*` files marked here are exactly those filtered down from
//! the demo set.
//!
//! Unlike `pretty_equivalence`, there is no "known-divergence" tier:
//! the subscriptions analysis lives entirely on string-prefix
//! classification, which the Evident pass reproduces faithfully. If a
//! future demo introduces a new world-access shape that breaks the
//! Evident impl, it surfaces here as a failing assertion — not a
//! sentinel.

use std::path::{Path, PathBuf};

use evident_runtime::portable::subscriptions::{
    EvidentSubscriptions, RustSubscriptions, SubscriptionsImpl,
};
use evident_runtime::EvidentRuntime;

const STDLIB: &str = "../stdlib";

/// FSM-shape demos that use `world.` / `world_next.` field access.
/// Surveyed by `grep -l 'world\.\|world_next\.' examples/test_*.ev`.
/// Each entry is a relative path from `runtime/`.
const CORPUS: &[&str] = &[
    "../examples/test_09_two_fsms.ev",
    "../examples/test_14_stdin.ev",
    "../examples/test_15_signal.ev",
    "../examples/test_18_reflection.ev",
    "../examples/test_25_per_component_jit.ev",
    "../examples/test_26_value_cache.ev",
    "../examples/test_30_jit_gap_closures.ev",
    "../examples/test_31_symbolic_regression.ev",
    "../examples/test_32_llm_functionizer.ev",
    "../examples/test_21_mario/main.ev",
];

fn evident_impl() -> EvidentSubscriptions {
    EvidentSubscriptions::new(Path::new(STDLIB))
        .expect("load stdlib/passes/subscriptions.ev")
}

/// Load `path` into a fresh runtime, walk each top-level claim, and
/// assert the two impls produce identical access sets.
fn assert_corpus_file_equiv(path: &Path, ev: &EvidentSubscriptions) -> usize {
    let mut rt = EvidentRuntime::new();
    // Some demos `import` packages/SDL; loading those is fine — we only
    // care about claim bodies, not running effects.
    rt.load_file(Path::new("../stdlib/runtime.ev")).ok();
    rt.load_file(path)
        .unwrap_or_else(|e| panic!("loading {path:?} failed: {e}"));

    let rs = RustSubscriptions;
    let mut compared = 0;

    let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    for name in &names {
        // sat_* / unsat_* are static-assertion claims, not FSMs — skip
        // them; the runtime itself skips them in the multi-FSM scheduler.
        if name.starts_with("sat_") || name.starts_with("unsat_") {
            continue;
        }
        let Some(schema) = rt.get_schema(name) else { continue };
        let r = rs.access_sets(schema);
        let e = ev.access_sets(schema);
        assert_eq!(r.reads, e.reads,
            "claim `{name}` in {path:?}: read-set mismatch.\n  rust    = {:?}\n  evident = {:?}",
            sorted(&r.reads), sorted(&e.reads));
        assert_eq!(r.writes, e.writes,
            "claim `{name}` in {path:?}: write-set mismatch.\n  rust    = {:?}\n  evident = {:?}",
            sorted(&r.writes), sorted(&e.writes));
        compared += 1;
    }
    compared
}

fn sorted(set: &std::collections::HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = set.iter().cloned().collect();
    v.sort();
    v
}

// ── 1. Demo corpus — every FSM-shaped claim ──

#[test]
fn corpus_equiv() {
    let ev = evident_impl();
    let mut total = 0;
    for rel in CORPUS {
        let path: PathBuf = rel.into();
        if !path.exists() {
            // Surface as a real failure — a missing corpus file means
            // the demo was renamed/deleted and the list needs updating.
            panic!("corpus file {path:?} not found; update CORPUS");
        }
        total += assert_corpus_file_equiv(&path, &ev);
    }
    assert!(total >= 10,
        "expected ≥10 claims compared across the corpus; got {total}");
}

// ── 2. Mario specifically — the typical multi-FSM target ──

#[test]
fn mario_writer_reader_separation() {
    // Mario's `game` claim is the world-writer; `keyboard` writes input
    // fields; `display` only reads. Codifies the expected shape so a
    // regression in inference surfaces here even if all classifiers agree.
    let ev = evident_impl();
    let rs = RustSubscriptions;

    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(Path::new("../examples/test_21_mario/main.ev"))
        .expect("load mario");

    for claim_name in ["game", "keyboard", "display"] {
        let schema = rt.get_schema(claim_name)
            .unwrap_or_else(|| panic!("`{claim_name}` claim missing"));
        let r = rs.access_sets(schema);
        let e = ev.access_sets(schema);
        assert_eq!(r.reads, e.reads,
            "mario `{claim_name}` read-set:\n  rust    = {:?}\n  evident = {:?}",
            sorted(&r.reads), sorted(&e.reads));
        assert_eq!(r.writes, e.writes,
            "mario `{claim_name}` write-set:\n  rust    = {:?}\n  evident = {:?}",
            sorted(&r.writes), sorted(&e.writes));
    }

    // Game IS the major writer.
    let game = rt.get_schema("game").unwrap();
    let g = rs.access_sets(game);
    assert!(g.writes.len() >= 5,
        "expected `game` to write multiple world fields; got {:?}", sorted(&g.writes));

    // Display reads many fields (camera, player, coins, enemies, …).
    let display = rt.get_schema("display").unwrap();
    let d = rs.access_sets(display);
    assert!(d.reads.len() >= 3,
        "expected `display` to read multiple world fields; got {:?}", sorted(&d.reads));
}

// ── 3. Trivial sanity — impl-name plumbing ──

#[test]
fn impl_names() {
    use evident_runtime::portable::Portable;
    assert_eq!(RustSubscriptions.impl_name(), "rust");
    assert_eq!(evident_impl().impl_name(), "evident");
}

// ── 4. Edge cases for the classifier itself ──

#[test]
fn classifier_unicode_identifier_is_neither() {
    // An identifier with no `world.` / `world_next.` prefix — including
    // a bare name with no dot — must be classified as neither. This
    // exercises the "no-prefix" UNSAT-then-default path in the shim.
    let ev = evident_impl();

    let src = r#"
type World
    a ∈ Int

claim w
    world, world_next ∈ World
    counter ∈ Int
    counter = 7
"#;
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("load");
    let schema = rt.get_schema("w").unwrap();

    let rs = RustSubscriptions;
    assert_eq!(rs.access_sets(schema), ev.access_sets(schema));
}

#[test]
fn classifier_world_passthrough_idiom() {
    // The `world_next.X = world.X` passthrough idiom — same field
    // appears in both sets. Subscriptions.rs's `passthrough_writer_passes_through_field`
    // test covers this for the Rust impl; this version asserts both
    // impls agree on the shape.
    let ev = evident_impl();

    let src = r#"
type World
    a ∈ Int
    b ∈ Int

enum S = X | Y

claim w
    world, world_next ∈ World
    state ∈ S
    world_next.a = (state matches X ? 99 : world.a)
    world_next.b = world.b
"#;
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("load");
    let schema = rt.get_schema("w").unwrap();

    let rs = RustSubscriptions;
    let r = rs.access_sets(schema);
    let e = ev.access_sets(schema);
    assert_eq!(r.reads, e.reads);
    assert_eq!(r.writes, e.writes);
    assert_eq!(sorted(&r.reads), vec!["a".to_string(), "b".to_string()]);
    assert_eq!(sorted(&r.writes), vec!["a".to_string(), "b".to_string()]);
}
