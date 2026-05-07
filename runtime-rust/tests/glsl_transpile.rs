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
