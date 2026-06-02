//! Integration test for the decomposition pass.
//!
//! Loads a small synthetic claim that has clearly separable structure
//! (three independent variable clusters), runs the runtime's
//! `analyze_decomposition` entry point, and asserts the components
//! come out as expected.

use evident_runtime::EvidentRuntime;
use std::collections::HashMap;
use std::path::PathBuf;

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(
        format!("decompose_integration_{}_{}.ev", std::process::id(), tag))
}

#[test]
fn three_disjoint_clusters_decompose_independently() {
    // Claim with three obviously-independent variable clusters.
    // a/b are linked; c/d are linked; e is alone.
    let source = "claim Triple\n\
                  \x20\x20\x20\x20a ∈ Int\n\
                  \x20\x20\x20\x20b ∈ Int\n\
                  \x20\x20\x20\x20c ∈ Int\n\
                  \x20\x20\x20\x20d ∈ Int\n\
                  \x20\x20\x20\x20e ∈ Int\n\
                  \x20\x20\x20\x20a = b + 1\n\
                  \x20\x20\x20\x20c = d * 2\n";
    let path = tmp_path("triple");
    std::fs::write(&path, source).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();
    let comps = rt.analyze_decomposition("Triple", &HashMap::new()).unwrap();
    let _ = std::fs::remove_file(&path);

    // Expect at least 3 components: {a, b}, {c, d}, {e}.
    // (Plus possibly singleton components for any synthetic vars the
    // runtime introduces — we only assert lower bound on count.)
    assert!(comps.len() >= 3, "expected ≥3 components, got {}: {:?}",
        comps.len(), comps);

    // Multi-var components: exactly two, sizes 2 and 2.
    let multi: Vec<&evident_runtime::decompose::Component> =
        comps.iter().filter(|c| c.vars.len() > 1).collect();
    assert_eq!(multi.len(), 2, "expected 2 multi-var components, got {}: {:?}",
        multi.len(), multi);
    for m in &multi {
        assert_eq!(m.vars.len(), 2, "expected size 2: {:?}", m);
    }

    // e should be its own singleton.
    let e_alone = comps.iter()
        .any(|c| c.vars.len() == 1 && c.vars.contains(&"e".to_string()));
    assert!(e_alone, "expected singleton component {{e}}; got: {:?}",
        comps);
}

#[test]
fn given_keys_excluded_from_components() {
    // When `given` pins a variable, it shouldn't link other components.
    let source = r#"claim Linked
    pinned ∈ Int
    a ∈ Int
    b ∈ Int
    a = pinned + 1
    b = pinned + 2
"#;
    let path = tmp_path("linked");
    std::fs::write(&path, source).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    // Without pinning, pinned links a and b → one component {a, b, pinned}.
    let no_given = rt.analyze_decomposition("Linked", &HashMap::new()).unwrap();
    let max_no_given = no_given.iter().map(|c| c.vars.len()).max().unwrap();
    assert!(max_no_given >= 3, "without given, expected merged component; got {:?}",
        no_given);

    // With pinned in given, a and b should be SEPARATE.
    let mut given = HashMap::new();
    given.insert("pinned".to_string(), evident_runtime::Value::Int(7));
    let with_given = rt.analyze_decomposition("Linked", &given).unwrap();
    let _ = std::fs::remove_file(&path);

    let max_with_given = with_given.iter().map(|c| c.vars.len()).max().unwrap();
    assert!(max_with_given < 3,
        "with `pinned` in given, a and b should be separate; got {:?}",
        with_given);
}
