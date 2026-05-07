//! `evident transpile-shader <file> <shader_name>` — load a file,
//! find a `shader` decl by name, transpile it to GLSL, print to
//! stdout. The output is a complete `#version 330 core` fragment
//! shader, ready to compile.
//!
//! Useful for inspecting the transpiler's output before wiring up
//! a GPU plugin: pipe the output into `glslangValidator` (or just
//! eyeball it) to make sure the generated GLSL is what you expect.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use evident_runtime::ast::{BodyItem, Expr};
use evident_runtime::glsl::transpile;
use evident_runtime::EvidentRuntime;

pub fn cmd_transpile_shader(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: evident transpile-shader <file> <shader_name>");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[0]);
    let name = &args[1];

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(&path) {
        eprintln!("transpile-shader: load {}: {e}", path.display());
        return ExitCode::from(2);
    }

    let Some(shader) = rt.shaders().iter().find(|s| s.name == *name) else {
        let names: Vec<&str> = rt.shaders().iter().map(|s| s.name.as_str()).collect();
        eprintln!("transpile-shader: no shader named {:?} in {}", name, path.display());
        if !names.is_empty() {
            eprintln!("  available: {}", names.join(", "));
        }
        return ExitCode::from(2);
    };

    // Build the type-leaves map the transpiler needs to expand
    // `state ∈ GameState` into per-leaf uniforms. Only flat
    // (non-recursive) Memberships are supported in v1.
    let types = collect_type_leaves(&rt);

    match transpile(shader, &types) {
        Ok(t) => {
            print!("{}", t.source);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("transpile-shader: {e}");
            ExitCode::from(1)
        }
    }
}

/// For every `type` decl in the runtime, collect its primitive-leaf
/// fields as `(field_name, field_type_name)` pairs. Skips fields
/// whose type isn't a primitive (the transpiler errors loudly if
/// it later encounters such a field). Recursive sub-records aren't
/// flattened in v1 — `Hero { pos ∈ IVec2 }` exposes only `pos` at
/// this level; the transpiler walks one more step internally.
fn collect_type_leaves(rt: &EvidentRuntime) -> HashMap<String, Vec<(String, String)>> {
    let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for name in rt.schema_names() {
        let Some(schema) = rt.get_schema(name) else { continue };
        let mut leaves: Vec<(String, String)> = Vec::new();
        // Walk one level: each Membership becomes a leaf. If the
        // type is itself a record, recursively expand.
        for item in &schema.body {
            if let BodyItem::Membership { name: fname, type_name, .. } = item {
                expand_field(rt, fname, type_name, "", &mut leaves);
            }
        }
        if !leaves.is_empty() {
            out.insert(name.to_string(), leaves);
        }
    }
    let _ = Expr::Bool(true); // silence unused-import in case
    out
}

fn expand_field(
    rt: &EvidentRuntime,
    name: &str,
    type_name: &str,
    prefix: &str,
    out: &mut Vec<(String, String)>,
) {
    let dotted = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };
    match type_name {
        "Real" | "Int" | "Nat" | "Pos" | "Bool" => {
            out.push((dotted, type_name.to_string()));
        }
        _ => {
            // User-defined record — recurse one more level.
            if let Some(sub) = rt.get_schema(type_name) {
                for item in &sub.body {
                    if let BodyItem::Membership { name: sub_name, type_name: sub_type, .. } = item {
                        expand_field(rt, sub_name, sub_type, &dotted, out);
                    }
                }
            }
        }
    }
}
