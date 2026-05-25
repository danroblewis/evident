//! String-manipulation builtins lowered to Z3 string (seq) theory
//! (session GAPC). Covers each op end-to-end: load → translate → Z3 solve
//! → model extraction. These are the operations that let generics'
//! `split_generic_head` + `substitute_idents` (and subscriptions'
//! `world.`-prefix test) become expressible in Evident.

use evident_runtime::{EvidentRuntime, Value};

/// `#text` and `str_len(text)` both lower to `str.len`.
#[test]
fn str_len_and_cardinality() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"hello\"\n    \
         n ∈ Int = #s\n    m ∈ Int = str_len(s)\n",
    )
    .unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(5)));
    assert_eq!(r.bindings.get("m"), Some(&Value::Int(5)));
}

/// `substr(text, off, len)` → `str.substr` (seq.extract).
#[test]
fn substr_slice() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"Edge<Rect>\"\n    \
         head ∈ String = substr(s, 0, 4)\n",
    )
    .unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("head"), Some(&Value::Str("Edge".into())));
}

/// `replace(text, src, dst)` → `str.replace` (first occurrence).
#[test]
fn replace_first_occurrence() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    t ∈ String = \"Seq(T)\"\n    \
         out ∈ String = replace(t, \"T\", \"Rect\")\n",
    )
    .unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("Seq(Rect)".into())));
}

/// `index_of(text, sub)` → `str.indexof` (offset 0); `-1` when absent.
#[test]
fn index_of_present_and_absent() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"Edge<Rect>\"\n    \
         lt ∈ Int = index_of(s, \"<\")\n    \
         gt ∈ Int = index_of(s, \">\")\n    \
         miss ∈ Int = index_of(s, \"@\")\n",
    )
    .unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("lt"), Some(&Value::Int(4)));
    assert_eq!(r.bindings.get("gt"), Some(&Value::Int(9)));
    assert_eq!(r.bindings.get("miss"), Some(&Value::Int(-1)));
}

/// 3-arg `index_of(text, sub, offset)` searches from `offset`.
#[test]
fn index_of_with_offset() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"a.b.c\"\n    \
         first ∈ Int = index_of(s, \".\", 0)\n    \
         second ∈ Int = index_of(s, \".\", 2)\n",
    )
    .unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("first"), Some(&Value::Int(1)));
    assert_eq!(r.bindings.get("second"), Some(&Value::Int(3)));
}

/// `char_at(text, i)` → `str.at` (length-1 substring).
#[test]
fn char_at_index() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"abc\"\n    c ∈ String = char_at(s, 1)\n",
    )
    .unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("c"), Some(&Value::Str("b".into())));
}

/// `str_contains` and the infix `sub ∈ text` form both lower to
/// `str.contains`. The SAT case holds; the UNSAT case proves it really
/// checks containment (closes COUNTEREXAMPLES.md #18's infix gap).
#[test]
fn contains_call_and_infix() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"world.pos\"\n    \
         str_contains(s, \"pos\")\n    \"world\" ∈ s\n",
    )
    .unwrap();
    assert!(rt.query_free("S").unwrap().satisfied);

    let mut rt2 = EvidentRuntime::new();
    rt2.load_source("claim U\n    s ∈ String = \"abc\"\n    \"xyz\" ∈ s\n")
        .unwrap();
    assert!(!rt2.query_free("U").unwrap().satisfied);
}

/// `starts_with` / `ends_with` → `str.prefixof` / `str.suffixof`.
#[test]
fn prefix_and_suffix_tests() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    s ∈ String = \"world.pos\"\n    \
         starts_with(s, \"world.\")\n    ends_with(s, \".pos\")\n",
    )
    .unwrap();
    assert!(rt.query_free("S").unwrap().satisfied);

    let mut rt2 = EvidentRuntime::new();
    rt2.load_source(
        "claim U\n    s ∈ String = \"world.pos\"\n    starts_with(s, \"local.\")\n",
    )
    .unwrap();
    assert!(!rt2.query_free("U").unwrap().satisfied);
}

/// Wrong-value UNSAT: substr must produce exactly "Edge", not anything
/// else — guards against the op leaving the output free.
#[test]
fn substr_is_exact_unsat_on_wrong_value() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim U\n    s ∈ String = \"Edge<Rect>\"\n    \
         head ∈ String = substr(s, 0, 4)\n    head = \"Edgz\"\n",
    )
    .unwrap();
    assert!(!rt.query_free("U").unwrap().satisfied);
}

/// The generics unblock: `substitute_idents` needs `"Seq(T)"` → `"Seq(Rect)"`,
/// and `split_generic_head` needs `"Edge<Rect>"` split on `<`/`>` into
/// `"Edge"` + `"Rect"`. Both are now expressible in Evident. This is the
/// load-time string manipulation PORT-generics couldn't write.
#[test]
fn generics_split_and_substitute() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim Generics\n    \
         g ∈ String = \"Edge<Rect>\"\n    \
         lt ∈ Int = index_of(g, \"<\")\n    \
         gt ∈ Int = index_of(g, \">\")\n    \
         head ∈ String = substr(g, 0, lt)\n    \
         arg ∈ String = substr(g, lt + 1, gt - lt - 1)\n    \
         tmpl ∈ String = \"Seq(T)\"\n    \
         mono ∈ String = replace(tmpl, \"T\", arg)\n",
    )
    .unwrap();
    let r = rt.query_free("Generics").unwrap();
    assert!(r.satisfied);
    // split_generic_head("Edge<Rect>") → ("Edge", "Rect")
    assert_eq!(r.bindings.get("head"), Some(&Value::Str("Edge".into())));
    assert_eq!(r.bindings.get("arg"), Some(&Value::Str("Rect".into())));
    // substitute_idents("Seq(T)", {T → "Rect"}) → "Seq(Rect)"
    assert_eq!(r.bindings.get("mono"), Some(&Value::Str("Seq(Rect)".into())));
}
