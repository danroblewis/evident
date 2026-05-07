//! Black-box tests for the GLSL transpiler. Each test compiles a tiny
//! Evident shader source and asserts substrings of the emitted GLSL.
//! We don't try to validate against a real GLSL compiler here —
//! that's the plugin's job. These tests pin the shape of the
//! transpiler's output: uniform naming, builtin pass-through, free-
//! var noise, dispatch chains, sub-record expansion.

use std::collections::HashMap;
use evident_runtime::ast::BodyItem;
use evident_runtime::glsl::transpile;
use evident_runtime::EvidentRuntime;

fn compile_shader(src: &str, shader_name: &str) -> String {
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("source must load");
    let shader = rt.shaders().iter().find(|s| s.name == shader_name)
        .expect("shader by name");
    let types = collect_type_leaves(&rt);
    transpile(shader, &types).expect("transpile ok").source
}

fn collect_type_leaves(rt: &EvidentRuntime) -> HashMap<String, Vec<(String, String)>> {
    let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for name in rt.schema_names() {
        let Some(schema) = rt.get_schema(name) else { continue };
        let mut leaves: Vec<(String, String)> = Vec::new();
        for item in &schema.body {
            if let BodyItem::Membership { name: fname, type_name, .. } = item {
                expand_field(rt, fname, type_name, "", &mut leaves);
            }
        }
        if !leaves.is_empty() {
            out.insert(name.to_string(), leaves);
        }
    }
    out
}

fn expand_field(
    rt: &EvidentRuntime, name: &str, type_name: &str, prefix: &str,
    out: &mut Vec<(String, String)>,
) {
    let dotted = if prefix.is_empty() { name.to_string() }
                 else { format!("{prefix}.{name}") };
    match type_name {
        "Real" | "Int" | "Nat" | "Pos" | "Bool" => {
            out.push((dotted, type_name.to_string()));
        }
        _ => {
            if let Some(sub) = rt.get_schema(type_name) {
                for item in &sub.body {
                    if let BodyItem::Membership { name: sn, type_name: st, .. } = item {
                        expand_field(rt, sn, st, &dotted, out);
                    }
                }
            }
        }
    }
}

#[test]
fn emits_version_and_pixel_input() {
    let src = "shader Empty\n    pixel ∈ Vec2\n";
    let g = compile_shader(src, "Empty");
    assert!(g.contains("#version 330 core"), "{g}");
    assert!(g.contains("in vec2 pixel"),     "{g}");
    assert!(g.contains("out vec4 fragColor"),"{g}");
}

#[test]
fn record_uniform_expands_to_per_leaf() {
    let src = "\
type IVec2(x, y ∈ Int)
type GameState
    hero ∈ IVec2
shader S
    pixel ∈ Vec2
    state ∈ GameState
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("uniform int state_hero_x"), "{g}");
    assert!(g.contains("uniform int state_hero_y"), "{g}");
}

#[test]
fn free_var_becomes_noise() {
    let src = "\
shader S
    pixel ∈ Vec2
    twinkle ∈ Real
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("float twinkle = _evhash(pixel)"), "{g}");
}

#[test]
fn pinned_var_becomes_local() {
    let src = "\
shader S
    pixel ∈ Vec2
    d ∈ Real
    d = length(pixel) - 0.5
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    // Declared but not noise-initialized; value computed in body.
    assert!(g.contains("float d;"),  "{g}");
    assert!(!g.contains("d = _evhash"), "should not be noise: {g}");
    assert!(g.contains("d = (length(pixel) - 0.5)"), "{g}");
}

#[test]
fn dispatch_becomes_if_blocks() {
    let src = "\
shader S
    pixel ∈ Vec2
    d ∈ Real
    col ∈ Color
    d = length(pixel) - 0.5
    d < 0.0 ⇒ col = Color(255, 100, 50)
    d ≥ 0.0 ⇒ col = Color(0, 0, 0)
    output.fragment = col
";
    let g = compile_shader(src, "S");
    assert!(g.contains("if ((d < 0.0))"), "{g}");
    assert!(g.contains("if ((d >= 0.0))"), "{g}");
}

#[test]
fn unknown_call_is_rejected() {
    let src = "\
shader S
    pixel ∈ Vec2
    output.fragment = Color(no_such_function(pixel), 0, 0)
";
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();
    let shader = rt.shaders().iter().find(|s| s.name == "S").unwrap();
    let res = transpile(shader, &collect_type_leaves(&rt));
    assert!(res.is_err(), "expected unknown-function error");
}

#[test]
fn constraints_can_be_written_in_any_order() {
    // `r = length(c)` BEFORE `c = ...` — the topo sort should
    // emit them in the right order regardless. If the transpiler
    // ignored deps, the GLSL would have a use-before-define and
    // either fail to compile or produce garbage.
    let src = "\
shader S
    pixel ∈ Vec2
    r ∈ Real
    c ∈ Vec2
    r = length(c)
    c = pixel * 2.0
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    let c_pos = g.find("c = (pixel * 2.0);")
        .expect(&format!("c = ... must be present: {g}"));
    let r_pos = g.find("r = length(c);")
        .expect(&format!("r = ... must be present: {g}"));
    assert!(c_pos < r_pos,
        "c must be defined before r consumes it; got order:\n{g}");
}

#[test]
fn cyclic_constraints_error() {
    // a = b + 1; b = a + 1 — true cycle, can't be unrolled to GLSL.
    let src = "\
shader S
    pixel ∈ Vec2
    a, b ∈ Real
    a = b + 1.0
    b = a + 1.0
    output.fragment = Color(255, 0, 0)
";
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();
    let shader = rt.shaders().iter().find(|s| s.name == "S").unwrap();
    let res = transpile(shader, &collect_type_leaves(&rt));
    let err = res.expect_err("cycle should be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("cycle") || msg.contains("underdetermined"),
        "expected cycle/underdetermined error, got: {err}");
}

#[test]
fn dispatch_chain_keeps_source_order() {
    // Two `if` blocks targeting the same var should appear in the
    // same source order regardless of topo-sort tie-breaking.
    let src = "\
shader S
    pixel ∈ Vec2
    d ∈ Real
    col ∈ Color
    d = length(pixel) - 0.5
    d < 0.0 ⇒ col = Color(255, 0, 0)
    d ≥ 0.0 ⇒ col = Color(0, 0, 255)
    output.fragment = col
";
    let g = compile_shader(src, "S");
    let red_pos  = g.find("if ((d < 0.0))")
        .expect(&format!("first branch missing: {g}"));
    let blue_pos = g.find("if ((d >= 0.0))")
        .expect(&format!("second branch missing: {g}"));
    assert!(red_pos < blue_pos,
        "dispatch order must match source order:\n{g}");
}

#[test]
fn bidirectional_lhs_unknown() {
    // Standard direction — unknown is on the LHS, transpiler emits
    // straight assignment.
    let src = "\
shader S
    pixel ∈ Vec2
    a, b, c ∈ Real
    a = pixel.x
    b = 5.0
    c = a + b
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("c = (a + b);"), "{g}");
}

#[test]
fn bidirectional_rhs_unknown() {
    // Unknown is on the RHS — transpiler should flip.
    let src = "\
shader S
    pixel ∈ Vec2
    a, b, c ∈ Real
    a = pixel.x
    b = 5.0
    a + b = c
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("c = (a + b);"),
        "RHS unknown should flip to LHS:\n{g}");
}

#[test]
fn bidirectional_subtraction_left() {
    // a + b = c with c, b known: solve for a → a = c - b.
    let src = "\
shader S
    pixel ∈ Vec2
    a, b, c ∈ Real
    b = 5.0
    c = pixel.x
    a + b = c
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("a = (c - b);"),
        "should isolate `a` via subtraction:\n{g}");
}

#[test]
fn bidirectional_division() {
    // 2 * x = y with y known: x = y / 2.
    let src = "\
shader S
    pixel ∈ Vec2
    x, y ∈ Real
    y = pixel.x
    2.0 * x = y
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("x = (y / 2.0);"),
        "should isolate `x` via division:\n{g}");
}

#[test]
fn bidirectional_subtract_with_unknown_on_right() {
    // k - x = y with k, y known: x = k - y. Tests the
    // non-commutative branch.
    let src = "\
shader S
    pixel ∈ Vec2
    k, x, y ∈ Real
    k = 10.0
    y = pixel.x
    k - x = y
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("x = (k - y);"),
        "should isolate `x = k - y`:\n{g}");
}

#[test]
fn bidirectional_nested_chain() {
    // (a + b) * c = d, isolating `a` with b, c, d known.
    // → a = (d / c) - b
    let src = "\
shader S
    pixel ∈ Vec2
    a, b, c, d ∈ Real
    b = 1.0
    c = 2.0
    d = pixel.x
    (a + b) * c = d
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    assert!(g.contains("a = ((d / c) - b);"),
        "should isolate `a` through chain:\n{g}");
}

#[test]
fn bidirectional_multi_occurrence_rejected() {
    // a + a = b — quadratic-ish, can't isolate.
    let src = "\
shader S
    pixel ∈ Vec2
    a, b ∈ Real
    b = pixel.x
    a + a = b
    output.fragment = Color(255, 0, 0)
";
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();
    let shader = rt.shaders().iter().find(|s| s.name == "S").unwrap();
    let err = transpile(shader, &collect_type_leaves(&rt))
        .expect_err("multi-occurrence should be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("multiple times"),
        "expected multi-occurrence error, got: {err}");
}

#[test]
fn bidirectional_nonlinear_rejected() {
    // length(c) = r — c is the unknown; can't invert length.
    let src = "\
shader S
    pixel ∈ Vec2
    c ∈ Vec2
    r ∈ Real
    r = pixel.x
    length(c) = r
    output.fragment = Color(255, 0, 0)
";
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();
    let shader = rt.shaders().iter().find(|s| s.name == "S").unwrap();
    let err = transpile(shader, &collect_type_leaves(&rt))
        .expect_err("nonlinear should be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("can't isolate") || msg.contains("only +"),
        "expected isolation error, got: {err}");
}

#[test]
fn subrecord_synthesizes_vec_constructor() {
    let src = "\
type IVec2(x, y ∈ Int)
type GameState
    hero ∈ IVec2
shader S
    pixel ∈ Vec2
    state ∈ GameState
    d ∈ Real
    d = length(pixel - state.hero)
    output.fragment = Color(255, 0, 0)
";
    let g = compile_shader(src, "S");
    // state.hero must expand to vec2(state_hero_x, state_hero_y),
    // with int → float coercion since the leaves are int.
    assert!(
        g.contains("vec2(float(state_hero_x), float(state_hero_y))"),
        "{g}"
    );
}
