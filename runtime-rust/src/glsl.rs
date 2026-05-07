//! Evident-shader → GLSL fragment-shader transpiler.
//!
//! Walks a `ShaderDecl`'s body and emits GLSL 330-core source. The
//! transpilation rules are deliberately small: arithmetic on
//! Real/Int, identifier/field/index access, calls into a fixed
//! builtin allowlist, and a constraint-style dispatch (`cond ⇒ var
//! = …`) that desugars to ternaries / `if`-chains.
//!
//! Variables in the shader body fall into three buckets:
//!
//!   - **Uniform**: declared via a sub-record membership
//!     (`state ∈ GameState`). Each leaf field surfaces as a
//!     `uniform float state_hero_x` (etc.). The runtime resolves
//!     `state.hero.x` references to that uniform name.
//!   - **Local**: pinned by some constraint inside the body
//!     (`d = length(...)`). Becomes a GLSL `float` temporary.
//!   - **Noise**: declared (`twinkle ∈ Real`) but not pinned and
//!     not part of a sub-record. The transpiler emits a hash-based
//!     pseudo-random expression seeded on `pixel`.
//!
//! Special vars (must appear in every shader):
//!
//!   - `pixel ∈ Vec2`  — the swept fragment coordinate. Becomes the
//!                       fragment shader's `gl_FragCoord.xy / iRes`
//!                       normalized to [0,1] by the host plugin.
//!   - `output.fragment` — the final color the shader produces.
//!     Either `output.fragment ∈ Color` (RGB) or `output.fragment
//!     ∈ Vec4` (RGBA).
//!
//! What's intentionally not handled in v1: ∀/∃ quantifiers (don't
//! make sense per-pixel), Set / Seq, complex disjunctions, custom
//! function definitions, string types, vertex shaders, multipass.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{BinOp, BodyItem, Expr, ShaderDecl};

/// One classified shader var. `name` is the source identifier (no
/// dotted prefix); for record-field uniforms there's one `Bucket`
/// entry per leaf with the leaf's full dotted name.
#[derive(Debug, Clone)]
enum Bucket {
    /// `state.hero.x` → `uniform float state_hero_x`. The
    /// `glsl_name` is the underscored uniform name; `glsl_type` is
    /// the GLSL primitive (`float`, `int`, `bool`).
    Uniform { dotted: String, glsl_name: String, glsl_type: &'static str },
    /// Pinned in the body. The body assignment becomes a GLSL stmt;
    /// the var becomes a GLSL temporary of `glsl_type`.
    Local   { name: String, glsl_type: &'static str },
    /// Free var. Transpiler emits a hash() call seeded on pixel.
    Noise   { name: String, glsl_type: &'static str },
}

/// Result of transpilation. `source` is the full GLSL fragment shader
/// (one file, ready to compile). `uniforms` lists every uniform the
/// shader declares, in declaration order — the host plugin uses this
/// to look up uniform locations once at init and know which `state.*`
/// / `input.*` bindings to upload each frame.
#[derive(Debug, Clone)]
pub struct TranspiledShader {
    pub source:   String,
    pub uniforms: Vec<UniformInfo>,
}

#[derive(Debug, Clone)]
pub struct UniformInfo {
    /// Source-level dotted name (`state.hero.x`). Plugin uses this to
    /// pull the value out of `bindings` each frame.
    pub source_name: String,
    /// GLSL uniform name (`state_hero_x`). Plugin passes this to
    /// `glGetUniformLocation` once at init.
    pub glsl_name:   String,
    /// `float` | `int` | `bool` — picks the right `glUniform*` call.
    pub glsl_type:   &'static str,
}

#[derive(Debug)]
pub struct TranspileError(pub String);

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for TranspileError {}

/// Transpile one shader decl to a complete GLSL 330 core fragment
/// shader. Resolution of sub-record types comes from `types` — the
/// caller passes a map of "user type name → its primitive-leaf
/// declarations", typically built by walking the runtime's schema
/// table.
pub fn transpile(
    shader: &ShaderDecl,
    types: &HashMap<String, Vec<(String, String)>>,
) -> Result<TranspiledShader, TranspileError> {
    // 1. Declarations + locals: walk the body, collect every
    //    Membership. Anything that's a sub-record type gets expanded
    //    into per-leaf uniforms; primitive memberships become
    //    candidates (Local if pinned, Noise otherwise).
    let mut uniforms: Vec<UniformInfo> = Vec::new();
    let mut buckets:  BTreeMap<String, Bucket> = BTreeMap::new();

    // Built-in viewport uniforms, always available. The plugin sets
    // these each frame to the SDL window size; the user references
    // them as `iResolution.x` and `iResolution.y` from inside any
    // shader body (no declaration needed).
    for (axis, glsl_name) in &[("x", "iResolution_x"), ("y", "iResolution_y")] {
        let dotted = format!("iResolution.{axis}");
        uniforms.push(UniformInfo {
            source_name: dotted.clone(),
            glsl_name:   (*glsl_name).to_string(),
            glsl_type:   "float",
        });
        buckets.insert(dotted.clone(), Bucket::Uniform {
            dotted, glsl_name: (*glsl_name).to_string(), glsl_type: "float",
        });
    }

    // First pass: identify which primitive Memberships are
    // referenced ANYWHERE in any constraint. Anything referenced
    // becomes a Local (defined by some constraint via the
    // bidirectional scheduler); anything not referenced becomes
    // Noise. The old test was "appears as LHS Identifier of an
    // = constraint", which under-counted bidirectional cases like
    // `1.0 - vig = X` (vig isn't on a bare LHS but is still
    // defined by that equation).
    let pinned_by_body = referenced_anywhere(&shader.body);

    for item in &shader.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            match type_name.as_str() {
                // Special vars: pixel is the per-fragment input,
                // output.* lives in `output_fragment` (handled later).
                "Vec2" if name == "pixel" => {
                    buckets.insert(name.clone(), Bucket::Local {
                        name: name.clone(), glsl_type: "vec2",
                    });
                }
                "Real"   => bucket_primitive(name, "float", &pinned_by_body, &mut buckets),
                "Int"    => bucket_primitive(name, "int",   &pinned_by_body, &mut buckets),
                "Nat"    => bucket_primitive(name, "int",   &pinned_by_body, &mut buckets),
                "Pos"    => bucket_primitive(name, "int",   &pinned_by_body, &mut buckets),
                "Bool"   => bucket_primitive(name, "bool",  &pinned_by_body, &mut buckets),
                "Color"  => {
                    // Color is itself a record (r, g, b ∈ Nat). When
                    // pinned in the body via `col = Color(…)` it's a
                    // Local vec3; when used as a uniform record it
                    // would be three scalar uniforms. v1 forbids the
                    // latter — colors are always shader-local.
                    buckets.insert(name.clone(), Bucket::Local {
                        name: name.clone(), glsl_type: "vec3",
                    });
                }
                "Vec2" | "Vec3" | "Vec4" => {
                    let gt = match type_name.as_str() {
                        "Vec2" => "vec2", "Vec3" => "vec3", _ => "vec4",
                    };
                    buckets.insert(name.clone(), Bucket::Local {
                        name: name.clone(), glsl_type: gt,
                    });
                }
                other => {
                    // User-defined record: expand into per-leaf
                    // uniforms via the caller-supplied types table.
                    let Some(leaves) = types.get(other) else {
                        return Err(TranspileError(format!(
                            "shader `{}`: variable `{}` has unknown type `{}`",
                            shader.name, name, other
                        )));
                    };
                    for (leaf_name, leaf_type) in leaves {
                        let dotted    = format!("{name}.{leaf_name}");
                        let glsl_type = leaf_to_glsl(leaf_type)?;
                        let glsl_name = dotted.replace('.', "_");
                        uniforms.push(UniformInfo {
                            source_name: dotted.clone(),
                            glsl_name:   glsl_name.clone(),
                            glsl_type,
                        });
                        buckets.insert(dotted.clone(), Bucket::Uniform {
                            dotted, glsl_name, glsl_type,
                        });
                    }
                }
            }
        }
    }

    // 2. Body translation. Skip Memberships (they're declarations);
    //    process Constraints in dependency order so source-line
    //    order doesn't matter — the user can write `r = length(c)`
    //    before `c = …`, and the transpiler emits in the right
    //    order. Cycles are a hard error.
    //
    //    Reject anything other than Membership / Constraint up front
    //    (Passthrough, ClaimCall, etc. don't make sense in a shader
    //    body and would silently fall through the topo step).
    for item in &shader.body {
        if !matches!(item, BodyItem::Membership { .. } | BodyItem::Constraint(_)) {
            return Err(TranspileError(format!(
                "shader `{}`: unsupported body item: {:?}",
                shader.name, item
            )));
        }
    }
    let scheduled = schedule_constraints(&shader.body)?;
    let mut emitter = Emitter {
        out: String::new(), buckets: &buckets, body: &shader.body,
        seen_outputs: HashSet::new(),
    };
    for (idx, unknown, expr) in scheduled {
        // If `unknown` is set and isn't already on the LHS, rewrite
        // the equation so it is. Bidirectional in action: the user
        // could have written `a + b = c`, `c - b = a`, or `c = a + b`
        // — the scheduler picks the unknown, the rewriter puts it
        // on the left, the emitter writes a normal GLSL assignment.
        let rewritten = match (&unknown, &expr) {
            (Some(u), Expr::Binary(BinOp::Eq, lhs, rhs)) if bare_ident(lhs).as_deref() != Some(u) => {
                let isolated = isolate(lhs, rhs, u)?;
                Expr::Binary(
                    BinOp::Eq,
                    Box::new(Expr::Identifier(u.clone())),
                    Box::new(isolated),
                )
            }
            _ => expr,
        };
        emitter.emit_constraint(&rewritten, idx)?;
    }

    // 3. Assemble the source.
    let mut src = String::new();
    src.push_str("#version 330 core\n");
    src.push_str("// Generated by Evident — do not edit by hand.\n\n");
    src.push_str("in vec2 pixel;\n");
    src.push_str("out vec4 fragColor;\n\n");
    for u in &uniforms {
        src.push_str(&format!("uniform {} {};\n", u.glsl_type, u.glsl_name));
    }
    src.push_str("\n");
    src.push_str(NOISE_HELPER);
    src.push_str("\nvoid main() {\n");
    // Local declarations from buckets, in source-declaration order.
    let mut declared_locals: BTreeSet<&str> = BTreeSet::new();
    for item in &shader.body {
        if let BodyItem::Membership { name, .. } = item {
            if name == "pixel" { continue; }
            if let Some(Bucket::Local { name, glsl_type }) = buckets.get(name) {
                if declared_locals.insert(name.as_str()) {
                    src.push_str(&format!("    {} {};\n", glsl_type, name));
                }
            }
            if let Some(Bucket::Noise { name, glsl_type }) = buckets.get(name) {
                if declared_locals.insert(name.as_str()) {
                    src.push_str(&format!("    {} {} = {};\n",
                        glsl_type, name, noise_call(glsl_type)));
                }
            }
        }
    }
    src.push_str(&emitter.out);
    src.push_str("}\n");

    Ok(TranspiledShader { source: src, uniforms })
}

/// Schedule a shader body's `Constraint` items: discover which var
/// each one defines (it can be on either side of `=`), order them
/// so every reference points at an already-defined var, and
/// algebraically isolate the unknown when it isn't already the LHS.
///
/// Returns `(item_idx, unknown_var, isolated_expr)` triples ready
/// for emission. `unknown_var = None` for items that don't define
/// a local (output.fragment writes — they're terminal sinks).
///
/// Wave scheduling instead of pure Kahn's: at each pass, find any
/// constraint with exactly one not-yet-defined var among its
/// references and schedule it. Repeat. If a pass makes no progress
/// and unscheduled constraints remain, the body is either cyclic
/// (a depends on b depends on a) or underdetermined (`a + b = c`
/// when only `c` is known). Both surface as `TranspileError`.
///
/// Guarded constraints (`cond ⇒ var = expr`) are treated
/// non-bidirectionally for v1 — the LHS must still be a bare
/// identifier. Bidirectional rearrangement under a guard is
/// well-defined but adds branching cases that aren't motivated by
/// any concrete shader pattern yet.
fn schedule_constraints(body: &[BodyItem])
    -> Result<Vec<(usize, Option<String>, Expr)>, TranspileError>
{
    // 1. Classify Memberships. Sub-record memberships (state ∈
    //    GameState) become uniforms, available to every constraint
    //    from the start. Primitive memberships (Real, Vec2, …) are
    //    locals that need to be defined by some constraint — except
    //    when no constraint references them at all, in which case
    //    they're free-noise vars (still "defined" for scheduling
    //    purposes — the noise expression initializes them).
    let mut local_to_be_defined: HashSet<String> = HashSet::new();
    let mut local_referenced: HashSet<String> = HashSet::new();
    let mut all_local_names: HashSet<String> = HashSet::new();

    for item in body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            // Anything Real/Int/Nat/Pos/Bool/Color/Vec2/Vec3/Vec4 is
            // a primitive local; sub-records like `state ∈ GameState`
            // are uniform-shaped and never get a constraint defining
            // them in-shader.
            if is_primitive_or_vec_type(type_name) {
                local_to_be_defined.insert(name.clone());
            }
            all_local_names.insert(name.clone());
        }
    }
    // Identify which locals are referenced anywhere — anything
    // unreferenced becomes Noise and is "free" from the scheduler's
    // standpoint.
    for item in body {
        if let BodyItem::Constraint(e) = item {
            let mut refs: HashSet<String> = HashSet::new();
            crate::translate::preprocess_api::collect_referenced_names(e, &mut refs);
            for r in refs {
                if let Some(root) = dep_root(&r) {
                    if local_to_be_defined.contains(&root) {
                        local_referenced.insert(root);
                    }
                }
            }
        }
    }
    // Defined-set seeds: locals that are never referenced (so don't
    // need to be derived — they're noise) start as "defined." Plus
    // `pixel` (varying) is always defined.
    let mut defined: HashSet<String> = local_to_be_defined
        .iter().filter(|n| !local_referenced.contains(*n)).cloned().collect();
    defined.insert("pixel".to_string());

    // Track which constraint index defines which var (for guarded
    // dispatch, multiple constraints can define the same var).
    let mut definer_pending: Vec<(usize, &Expr)> = body.iter().enumerate()
        .filter_map(|(i, item)| match item {
            BodyItem::Constraint(e) => Some((i, e)),
            _ => None,
        })
        .collect();
    let mut emit_order: Vec<(usize, Option<String>, Expr)> = Vec::new();

    // Wave scheduling. In each pass, find every constraint whose
    // unknown can be uniquely identified given the current defined
    // set; schedule and remove. Loop until empty or stuck.
    loop {
        if definer_pending.is_empty() { break; }
        let mut progressed = false;
        let mut still_pending: Vec<(usize, &Expr)> = Vec::new();
        for (idx, e) in definer_pending {
            // Guarded shape always uses LHS-Identifier.
            if let Expr::Binary(BinOp::Implies, ant, cons) = e {
                if let Expr::Binary(BinOp::Eq, lhs, rhs) = cons.as_ref() {
                    if let Some(name) = bare_ident(lhs) {
                        // Defer until both the antecedent and the RHS
                        // are fully resolvable (no unknowns).
                        let mut needed: HashSet<String> = HashSet::new();
                        crate::translate::preprocess_api::collect_referenced_names(ant, &mut needed);
                        crate::translate::preprocess_api::collect_referenced_names(rhs, &mut needed);
                        let unresolved: Vec<String> = needed.into_iter()
                            .filter_map(|r| dep_root(&r))
                            .filter(|r| local_to_be_defined.contains(r))
                            .filter(|r| !defined.contains(r) && r != &name)
                            .collect();
                        if unresolved.is_empty() {
                            emit_order.push((idx, Some(name.clone()), e.clone()));
                            defined.insert(name);
                            progressed = true;
                            continue;
                        }
                    }
                }
                still_pending.push((idx, e));
                continue;
            }
            // Unguarded equation: discover the unknown.
            let Expr::Binary(BinOp::Eq, _, _) = e else {
                // Anything else (a bare bool constraint, e.g.) is
                // emitted as-is at the end with no defined var.
                emit_order.push((idx, None, e.clone()));
                progressed = true;
                continue;
            };
            // Output sink: `output.fragment = expr` is terminal.
            // Treat as a no-unknown emit when its RHS is fully
            // resolvable.
            if let Expr::Binary(BinOp::Eq, lhs, rhs) = e {
                if is_output_fragment(lhs) {
                    let mut needed: HashSet<String> = HashSet::new();
                    crate::translate::preprocess_api::collect_referenced_names(rhs, &mut needed);
                    let unresolved: Vec<String> = needed.into_iter()
                        .filter_map(|r| dep_root(&r))
                        .filter(|r| local_to_be_defined.contains(r))
                        .filter(|r| !defined.contains(r))
                        .collect();
                    if unresolved.is_empty() {
                        emit_order.push((idx, None, e.clone()));
                        progressed = true;
                        continue;
                    } else {
                        still_pending.push((idx, e));
                        continue;
                    }
                }
            }
            // Generic equation. Find the (single) unknown across both sides.
            let mut refs: HashSet<String> = HashSet::new();
            crate::translate::preprocess_api::collect_referenced_names(e, &mut refs);
            let unknowns: Vec<String> = refs.into_iter()
                .filter_map(|r| dep_root(&r))
                .filter(|r| local_to_be_defined.contains(r))
                .filter(|r| !defined.contains(r))
                .collect();
            // Dedup, since an `a + a = c` would list `a` twice.
            let unknowns: Vec<String> = unknowns.into_iter()
                .collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            match unknowns.len() {
                0 => {
                    // Tautology check. Drop with no emission — it's
                    // a runtime-true constraint, useless for shaders.
                    progressed = true;
                }
                1 => {
                    let unk = unknowns[0].clone();
                    emit_order.push((idx, Some(unk.clone()), e.clone()));
                    defined.insert(unk);
                    progressed = true;
                }
                _ => {
                    // Multiple unknowns — wait for another constraint
                    // to define some of them first.
                    still_pending.push((idx, e));
                }
            }
        }
        definer_pending = still_pending;
        if !progressed {
            // Stuck. Either underdetermined or cyclic.
            let stuck: Vec<usize> = definer_pending.iter().map(|(i, _)| *i).collect();
            return Err(TranspileError(format!(
                "shader: can't resolve constraint(s) at body indices {:?} \
                 — {} unknowns remain. Either the constraint set is \
                 underdetermined or you have a cycle.",
                stuck, definer_pending.len()
            )));
        }
    }
    Ok(emit_order)
}

/// Whether a type name is a primitive scalar / vector type that
/// shows up as a local in the GLSL `main()` (as opposed to a sub-
/// record type whose Membership becomes uniforms). The transpiler
/// also accepts `Color` here even though it's a record under the
/// hood — Color memberships become local vec3s.
fn is_primitive_or_vec_type(t: &str) -> bool {
    matches!(t, "Real" | "Int" | "Nat" | "Pos" | "Bool" |
                "Color" | "Vec2" | "Vec3" | "Vec4")
}

/// Algebraic isolation: rearrange `lhs = rhs` so `unknown` is on
/// the left. Walks down the side that contains `unknown`, peeling
/// off binary ops and applying inverses to the other side.
///
/// Supported peels: `+`, `-`, `*`, `/`. Anything else (function
/// calls, unary ops, comparisons) where the unknown lives inside
/// is rejected — we'd need `length`/`sin`/etc. inverses, which
/// don't exist in closed form for most cases. Z3 itself can't
/// symbolically invert these either; the limit is fundamental,
/// not a transpiler shortcut.
///
/// Multi-occurrence (`a + a = c`, `a * a = b`) is also rejected —
/// would need quadratic-formula reasoning, out of scope for v1.
fn isolate(lhs: &Expr, rhs: &Expr, unknown: &str) -> Result<Expr, TranspileError> {
    let lhs_count = count_occurrences(lhs, unknown);
    let rhs_count = count_occurrences(rhs, unknown);
    if lhs_count + rhs_count == 0 {
        return Err(TranspileError(format!(
            "shader: tried to isolate `{}` but it doesn't appear in the equation",
            unknown
        )));
    }
    if lhs_count + rhs_count > 1 {
        return Err(TranspileError(format!(
            "shader: `{}` appears multiple times — can't isolate algebraically \
             (would need quadratic-formula reasoning)",
            unknown
        )));
    }
    let (with_unknown, known) = if lhs_count == 1 { (lhs, rhs) } else { (rhs, lhs) };
    isolate_chain(with_unknown, known.clone(), unknown)
}

/// Recursive peel: `(side OP other) = acc` → `side = acc OP_INV other`
/// (or symmetric for non-commutative). When `side` is the bare
/// unknown identifier, return `acc` directly.
fn isolate_chain(side: &Expr, acc: Expr, unknown: &str) -> Result<Expr, TranspileError> {
    if let Expr::Identifier(n) = side {
        if n == unknown { return Ok(acc); }
    }
    let Expr::Binary(op, a, b) = side else {
        return Err(TranspileError(format!(
            "shader: can't isolate `{}` through {:?} — only +, -, *, / chains \
             are supported (function calls and other ops have no closed-form \
             inverse)",
            unknown, side
        )));
    };
    let a_has = count_occurrences(a, unknown) == 1;
    let b_has = count_occurrences(b, unknown) == 1;
    if !a_has && !b_has {
        return Err(TranspileError(format!(
            "shader: lost track of `{}` while isolating", unknown
        )));
    }
    let (with_unknown, known) = if a_has {
        (a.as_ref(), b.as_ref().clone())
    } else {
        (b.as_ref(), a.as_ref().clone())
    };
    let new_acc = match op {
        // (x + k) = acc       → x = acc - k         (commutative)
        // (k + x) = acc       → x = acc - k
        BinOp::Add => Expr::Binary(
            BinOp::Sub, Box::new(acc), Box::new(known)),
        // (x - k) = acc       → x = acc + k
        // (k - x) = acc       → x = k - acc
        BinOp::Sub => if a_has {
            Expr::Binary(BinOp::Add, Box::new(acc), Box::new(known))
        } else {
            Expr::Binary(BinOp::Sub, Box::new(known), Box::new(acc))
        },
        // (x * k) = acc       → x = acc / k         (commutative)
        BinOp::Mul => Expr::Binary(
            BinOp::Div, Box::new(acc), Box::new(known)),
        // (x / k) = acc       → x = acc * k
        // (k / x) = acc       → x = k / acc
        BinOp::Div => if a_has {
            Expr::Binary(BinOp::Mul, Box::new(acc), Box::new(known))
        } else {
            Expr::Binary(BinOp::Div, Box::new(known), Box::new(acc))
        },
        other => return Err(TranspileError(format!(
            "shader: can't isolate `{}` through operator {:?}", unknown, other
        ))),
    };
    isolate_chain(with_unknown, new_acc, unknown)
}

/// Count how many times `unknown` (as a leaf identifier) appears
/// inside `e`. Walks Binary, Field, Call, Not, etc. — anywhere a
/// reference can hide. Returns 0/1/2+; the isolator only handles
/// the count-1 case.
fn count_occurrences(e: &Expr, unknown: &str) -> usize {
    match e {
        Expr::Identifier(n) => {
            // Match either the bare name or a dotted form rooted at
            // the unknown (so `a.x` counts when isolating `a` —
            // though we'd then reject the isolation since vec
            // swizzles aren't invertible).
            let root = n.split('.').next().unwrap_or(n);
            if root == unknown { 1 } else { 0 }
        }
        Expr::Binary(_, a, b) => count_occurrences(a, unknown) + count_occurrences(b, unknown),
        Expr::Not(inner) | Expr::Cardinality(inner) =>
            count_occurrences(inner, unknown),
        Expr::Field(r, _) => count_occurrences(r, unknown),
        Expr::Index(s, i) =>
            count_occurrences(s, unknown) + count_occurrences(i, unknown),
        Expr::Call(_, args) | Expr::SetLit(args) | Expr::SeqLit(args) =>
            args.iter().map(|a| count_occurrences(a, unknown)).sum(),
        Expr::Range(lo, hi) =>
            count_occurrences(lo, unknown) + count_occurrences(hi, unknown),
        Expr::InExpr(l, r) =>
            count_occurrences(l, unknown) + count_occurrences(r, unknown),
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) =>
            count_occurrences(range, unknown) + count_occurrences(body, unknown),
        _ => 0,
    }
}

/// Bare-identifier extractor. `Identifier("c")` → Some("c");
/// `Identifier("c.y")` (parser-folded swizzle) → Some("c"); Field
/// chains like `output.fragment` → None (they're not local-var
/// definitions).
fn bare_ident(e: &Expr) -> Option<String> {
    if let Expr::Identifier(n) = e {
        // Strip the swizzle suffix so `c.y = ...` (if it ever
        // appeared as an LHS) would be classified as defining `c`.
        // Today the transpiler doesn't accept LHS swizzles, but
        // being lenient here costs nothing.
        return Some(n.split('.').next().unwrap_or(n).to_string());
    }
    None
}

/// Map a referenced-name string back to its root variable name. A
/// reference to `c.y` depends on `c`; `state.hero.pos.x` depends on
/// `state` (which the shader treats as opaque uniforms — the topo
/// pass filters by local_names so non-local refs drop out).
fn dep_root(name: &str) -> Option<String> {
    let root = name.split('.').next()?;
    if root.is_empty() { None } else { Some(root.to_string()) }
}

fn bucket_primitive(
    name: &str, glsl_type: &'static str,
    pinned: &HashSet<String>,
    buckets: &mut BTreeMap<String, Bucket>,
) {
    if pinned.contains(name) {
        buckets.insert(name.to_string(), Bucket::Local {
            name: name.to_string(), glsl_type,
        });
    } else {
        buckets.insert(name.to_string(), Bucket::Noise {
            name: name.to_string(), glsl_type,
        });
    }
}

/// Set of identifier names referenced by any constraint in the
/// shader body, anywhere. Used to decide Local vs Noise
/// classification: a primitive Membership becomes Local if
/// referenced (some constraint will define it), Noise if not.
/// Because we're using the bidirectional scheduler, we don't care
/// which side of `=` the var appears on — just whether it's
/// reachable at all.
fn referenced_anywhere(body: &[BodyItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for item in body {
        if let BodyItem::Constraint(e) = item {
            let mut refs: HashSet<String> = HashSet::new();
            crate::translate::preprocess_api::collect_referenced_names(e, &mut refs);
            for r in refs {
                if let Some(root) = r.split('.').next() {
                    out.insert(root.to_string());
                }
            }
        }
    }
    out
}

/// Map an Evident leaf type name to the GLSL primitive used in
/// uniforms (no `vec2` here — those are declared as record types via
/// `IVec2`, which surfaces as two scalar uniforms).
fn leaf_to_glsl(t: &str) -> Result<&'static str, TranspileError> {
    match t {
        "Real"               => Ok("float"),
        "Int" | "Nat" | "Pos" => Ok("int"),
        "Bool"               => Ok("bool"),
        other => Err(TranspileError(format!(
            "uniform leaf type `{}` not supported (allowed: Real/Int/Nat/Pos/Bool)",
            other
        ))),
    }
}

fn noise_call(glsl_type: &str) -> String {
    match glsl_type {
        "float" => "_evhash(pixel)".into(),
        "vec2"  => "vec2(_evhash(pixel), _evhash(pixel + 1.7))".into(),
        "vec3"  => "vec3(_evhash(pixel), _evhash(pixel + 1.7), _evhash(pixel + 3.1))".into(),
        "int"   => "int(_evhash(pixel) * 256.0)".into(),
        "bool"  => "(_evhash(pixel) > 0.5)".into(),
        _       => "0.0".into(),
    }
}

const NOISE_HELPER: &str = r#"
// Hash-based pseudo-random for free-variable noise. Cheap, not
// statistically great — fine for shadertoy-style visual jitter.
float _evhash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}
"#;

/// Translates one body item into one GLSL `main()` statement.
struct Emitter<'a> {
    out:           String,
    buckets:       &'a BTreeMap<String, Bucket>,
    /// Full body for two-pass dispatch detection (see `emit_constraint`).
    #[allow(dead_code)]
    body:          &'a [BodyItem],
    /// Track which `output.<name>` LHS we've emitted so we can warn
    /// on a missing one at the end.
    seen_outputs:  HashSet<String>,
}

impl<'a> Emitter<'a> {
    fn emit_constraint(&mut self, e: &Expr, _idx: usize)
        -> Result<(), TranspileError>
    {
        match e {
            // `output.fragment = <expr>` — terminal; emit fragColor
            // assignment.
            Expr::Binary(BinOp::Eq, lhs, rhs) if is_output_fragment(lhs) => {
                let v = self.expr_glsl(rhs)?;
                self.out.push_str(&format!(
                    "    fragColor = vec4({}, 1.0);\n", coerce_to_vec3(&v)
                ));
                self.seen_outputs.insert("fragment".into());
                Ok(())
            }
            // `var = <expr>` — local assignment.
            Expr::Binary(BinOp::Eq, lhs, rhs) => {
                let lhs_g = self.expr_glsl(lhs)?;
                let rhs_g = self.expr_glsl(rhs)?;
                self.out.push_str(&format!("    {} = {};\n", lhs_g, rhs_g));
                Ok(())
            }
            // `cond ⇒ var = <expr>` — guarded assignment via if-stmt.
            // GLSL's `if` mutates the local; mutually-exclusive
            // partners on the same var compose into `else if` /
            // `else`. v1 emits each guard as a standalone if; the
            // mutex pairing is left as a transpiler refinement.
            Expr::Binary(BinOp::Implies, ant, cons) => {
                let cond = self.expr_glsl(ant)?;
                self.out.push_str(&format!("    if ({}) {{\n", cond));
                if let Expr::Binary(BinOp::Eq, lhs, rhs) = cons.as_ref() {
                    let lhs_g = self.expr_glsl(lhs)?;
                    let rhs_g = self.expr_glsl(rhs)?;
                    if is_output_fragment(lhs) {
                        self.out.push_str(&format!(
                            "        fragColor = vec4({}, 1.0);\n",
                            coerce_to_vec3(&rhs_g)
                        ));
                        self.seen_outputs.insert("fragment".into());
                    } else {
                        self.out.push_str(&format!("        {} = {};\n", lhs_g, rhs_g));
                    }
                } else {
                    return Err(TranspileError(format!(
                        "shader: `⇒` consequent must be an assignment, got {:?}", cons
                    )));
                }
                self.out.push_str("    }\n");
                Ok(())
            }
            other => Err(TranspileError(format!(
                "shader: unsupported constraint shape: {:?}", other
            ))),
        }
    }

    /// If `prefix` is a parent of one or more leaf uniforms (e.g.
    /// `state.hero` whose children are `state.hero.x`, `state.hero.y`),
    /// synthesize a `vecN(child1, child2, …)` GLSL constructor. Int
    /// leaves are coerced to float to keep arithmetic in shadertoy
    /// idiom. Returns None when no children exist (caller errors).
    fn synthesize_subrecord(&self, prefix: &str) -> Option<String> {
        let prefix_dot = format!("{prefix}.");
        let mut children: Vec<&Bucket> = self.buckets.values().filter(|b| {
            if let Bucket::Uniform { dotted, .. } = b {
                dotted.starts_with(&prefix_dot)
                    // Exactly one path level deeper — `state.hero.x`,
                    // not `state.hero.pos.x`. Deeper records require
                    // a deeper synthesize call from a Field walk.
                    && !dotted[prefix_dot.len()..].contains('.')
            } else { false }
        }).collect();
        if children.is_empty() { return None; }
        // Sort by leaf name for stable order — caller-side records
        // declared (x, y) come back as (x, y) because alphabetical
        // matches the IVec2/Vec2/Vec3 convention.
        children.sort_by_key(|b| match b {
            Bucket::Uniform { dotted, .. } => dotted.clone(),
            _ => String::new(),
        });
        let parts: Vec<String> = children.iter().map(|b| match b {
            Bucket::Uniform { glsl_name, glsl_type, .. } => {
                if *glsl_type == "int" { format!("float({glsl_name})") }
                else                   { glsl_name.clone() }
            }
            _ => unreachable!(),
        }).collect();
        let ctor = match parts.len() {
            2 => "vec2", 3 => "vec3", 4 => "vec4",
            _ => return None,
        };
        Some(format!("{}({})", ctor, parts.join(", ")))
    }

    /// Translate one Expr into a GLSL expression string.
    fn expr_glsl(&self, e: &Expr) -> Result<String, TranspileError> {
        match e {
            Expr::Int(n)  => Ok(n.to_string()),
            Expr::Real(r) => Ok(format_real(*r)),
            Expr::Bool(b) => Ok(b.to_string()),
            Expr::Identifier(name) => {
                if name == "pixel" { return Ok("pixel".into()); }
                // Pixel swizzles (`pixel.x`, `pixel.y`) — pass
                // through as GLSL vec2 component access. The
                // parser folds `<bare>.<field>` into a single
                // dotted Identifier, so this case never reaches
                // the Field arm.
                if let Some(rest) = name.strip_prefix("pixel.") {
                    if is_glsl_swizzle(rest) {
                        return Ok(format!("pixel.{rest}"));
                    }
                }
                if let Some(b) = self.buckets.get(name) {
                    return Ok(bucket_glsl(b));
                }
                // Dotted swizzle on a local vec/color: `c.y`,
                // `pal.rgb`, `mouse.xx`. The parser folds the dot
                // into the identifier so we never reach the Field
                // arm — split here and check the prefix against
                // the bucket map.
                if let Some(idx) = name.find('.') {
                    let (prefix, dotted_rest) = name.split_at(idx);
                    let suffix = &dotted_rest[1..];
                    if is_glsl_swizzle(suffix) {
                        if let Some(b) = self.buckets.get(prefix) {
                            return Ok(format!("{}.{}", bucket_glsl(b), suffix));
                        }
                    }
                }
                // Dotted: try as a sub-record prefix
                // (e.g. `state.hero` over leaves x, y).
                if let Some(s) = self.synthesize_subrecord(name) {
                    return Ok(s);
                }
                Err(TranspileError(format!(
                    "shader: unknown identifier `{}`", name
                )))
            }
            Expr::Binary(op, lhs, rhs) => {
                let l = self.expr_glsl(lhs)?;
                let r = self.expr_glsl(rhs)?;
                let op_s = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Lt => "<", BinOp::Le => "<=", BinOp::Gt => ">", BinOp::Ge => ">=",
                    BinOp::Eq => "==", BinOp::Neq => "!=",
                    BinOp::And => "&&", BinOp::Or => "||",
                    BinOp::Implies | BinOp::Concat => return Err(TranspileError(format!(
                        "shader: operator {:?} not supported in expression position", op
                    ))),
                };
                Ok(format!("({} {} {})", l, op_s, r))
            }
            Expr::Not(inner) => {
                let v = self.expr_glsl(inner)?;
                Ok(format!("(!{})", v))
            }
            Expr::Field(receiver, field) => {
                // Walk back to get the full dotted path so we can
                // look up the corresponding uniform / local.
                let dotted = dotted_path(e)
                    .ok_or_else(|| TranspileError(format!(
                        "shader: cannot resolve field access {:?}.{}", receiver, field
                    )))?;
                if let Some(b) = self.buckets.get(&dotted) {
                    return Ok(bucket_glsl(b));
                }
                // Sub-record at this level (e.g. `state.hero` whose
                // leaves are x, y) — synthesize a vecN constructor
                // from the leaves.
                if let Some(s) = self.synthesize_subrecord(&dotted) {
                    return Ok(s);
                }
                // Otherwise it's a swizzle on a local — single
                // component (`pixel.x`), color access (`col.r`), or
                // a multi-char swizzle (`v.xy`, `c.rgb`, `pos.xx`).
                // GLSL accepts any 1-4 char combination from one of
                // the {x,y,z,w} / {r,g,b,a} / {s,t,p,q} sets; we
                // permit the first two (the geometry + color sets,
                // which is what users actually write).
                let recv = self.expr_glsl(receiver)?;
                if !is_glsl_swizzle(field) {
                    return Err(TranspileError(format!(
                        "shader: unknown field `.{}`", field
                    )));
                }
                Ok(format!("{}.{}", recv, field))
            }
            Expr::Call(name, args) => {
                if !is_glsl_builtin(name) && !is_constructor(name) {
                    return Err(TranspileError(format!(
                        "shader: function `{}` not in the GLSL builtin allowlist",
                        name
                    )));
                }
                let parts: Result<Vec<String>, _> = args.iter()
                    .map(|a| self.expr_glsl(a)).collect();
                let joined = parts?.join(", ");
                let glsl_name = match name.as_str() {
                    "Color" => "vec3",
                    "Vec2"  => "vec2",
                    "Vec3"  => "vec3",
                    "Vec4"  => "vec4",
                    "IVec2" => "vec2",
                    other   => other,
                };
                // Color(255, 100, 50) → vec3(255.0/255.0, 100.0/255.0, 50.0/255.0)
                if name == "Color" {
                    let scaled: Vec<String> = args.iter()
                        .map(|a| self.expr_glsl(a).map(|s| format!("({})/255.0", s)))
                        .collect::<Result<_, _>>()?;
                    return Ok(format!("vec3({})", scaled.join(", ")));
                }
                Ok(format!("{}({})", glsl_name, joined))
            }
            other => Err(TranspileError(format!(
                "shader: unsupported expression: {:?}", other
            ))),
        }
    }
}

/// Reconstruct the source-level dotted path from a chain of
/// `Field(Field(Identifier(name), …), …)` nodes. Returns None if
/// the chain isn't purely Identifier + Field.
fn dotted_path(e: &Expr) -> Option<String> {
    match e {
        Expr::Identifier(n) => Some(n.clone()),
        Expr::Field(recv, field) => {
            dotted_path(recv).map(|s| format!("{s}.{field}"))
        }
        _ => None,
    }
}

fn is_output_fragment(e: &Expr) -> bool {
    dotted_path(e).map(|s| s == "output.fragment").unwrap_or(false)
}

fn bucket_glsl(b: &Bucket) -> String {
    match b {
        Bucket::Uniform { glsl_name, .. } => glsl_name.clone(),
        Bucket::Local   { name, .. }      => name.clone(),
        Bucket::Noise   { name, .. }      => name.clone(),
    }
}

fn coerce_to_vec3(s: &str) -> String {
    // Heuristic: if the expression already looks like vec3(...) or
    // mix(...) of vec3s, pass through; otherwise wrap. Cheap and
    // correct for the common case.
    s.to_string()
}

fn is_glsl_builtin(name: &str) -> bool {
    matches!(name,
        "length" | "distance" | "dot" | "cross" | "normalize" |
        "min" | "max" | "clamp" | "mix" | "smoothstep" | "step" |
        "abs" | "sign" | "floor" | "ceil" | "fract" | "mod" | "pow" | "sqrt" |
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "log" |
        "reflect" | "refract"
    )
}

/// Whether `s` is a valid GLSL swizzle: 1-4 chars, all from one of
/// the geometry set (`xyzw`) or the color set (`rgba`). The third
/// GLSL swizzle set (`stpq`, for textures) is intentionally not
/// supported — texture coords aren't a thing in shader bodies yet.
fn is_glsl_swizzle(s: &str) -> bool {
    if s.is_empty() || s.len() > 4 { return false; }
    let geom  = s.chars().all(|c| matches!(c, 'x' | 'y' | 'z' | 'w'));
    let color = s.chars().all(|c| matches!(c, 'r' | 'g' | 'b' | 'a'));
    geom || color
}

fn is_constructor(name: &str) -> bool {
    // Vec/Color constructors plus the GLSL primitive casts (`float(x)`,
    // `int(x)`). The casts let users bring an `int` uniform into
    // float arithmetic without an Evident-side conversion (which
    // would force Z3 to do mixed Int/Real math, sometimes slow and
    // sometimes outright unsupported).
    matches!(name,
        "Vec2" | "Vec3" | "Vec4" | "Color" | "IVec2" |
        "float" | "int"
    )
}

/// GLSL's float literal must contain a `.` to be parsed as float.
/// `3` would be int; `3.0` is float. f64::to_string drops the `.0`
/// for whole numbers, so we patch it back on.
fn format_real(r: f64) -> String {
    let s = r.to_string();
    if s.contains('.') || s.contains('e') || s.contains('E') { s }
    else { format!("{}.0", s) }
}
