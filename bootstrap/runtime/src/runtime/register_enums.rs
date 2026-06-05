//! Z3 datatype registration for `enum` declarations.

use crate::core::{RuntimeError, internal_cons_helper_name, parse_seq_type};
use z3::Context;

/// Build Z3 DatatypeSorts for `decls` via `create_datatypes` (handles forward/mutual refs).
/// Validates variant-name global uniqueness; errors on unknown payload types.
pub(super) fn register_enums(
    decls: &[crate::core::ast::EnumDecl],
    ctx: &'static Context,
    registry: &crate::core::EnumRegistry,
) -> Result<(), RuntimeError> {
    use z3::{DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};
    if decls.is_empty() { return Ok(()); }

    // Pre-flight: variant + enum-name uniqueness across batch and previously-loaded enums.
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();
    {
        if batch_names.len() != decls.len() {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for d in decls {
                if !seen.insert(d.name.as_str()) {
                    return Err(RuntimeError::Parse(format!(
                        "enum `{}` declared more than once in the same load",
                        d.name)));
                }
            }
        }
        let existing_by_name = registry.by_name.borrow();
        for d in decls {
            if existing_by_name.contains_key(&d.name) {
                return Err(RuntimeError::Parse(format!(
                    "enum `{}` declared more than once", d.name)));
            }
            if d.variants.is_empty() {
                return Err(RuntimeError::Parse(
                    format!("enum {} has no variants", d.name)));
            }
        }
        let by_variant = registry.by_variant.borrow();
        let mut batch_seen: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for d in decls {
            for v in &d.variants {
                if let Some((existing_enum, _)) = by_variant.get(&v.name) {
                    return Err(RuntimeError::Parse(format!(
                        "enum variant `{}` is declared twice — once in `{}` and once in `{}`",
                        v.name, existing_enum, d.name,
                    )));
                }
                if let Some(prev_in_batch) = batch_seen.get(&v.name) {
                    return Err(RuntimeError::Parse(format!(
                        "enum variant `{}` is declared twice — once in `{}` and once in `{}`",
                        v.name, prev_in_batch, d.name,
                    )));
                }
                batch_seen.insert(v.name.clone(), d.name.clone());
            }
        }
    }

    // Seq(T) where T is batch-local can't use Array(Int→T) — Z3 needs a concrete element sort.
    // Generate a Cons-shaped __SeqOf_T helper per such T and use a forward Datatype ref instead.
    let decls_owned: Vec<crate::core::ast::EnumDecl>;
    let internal_cons_set: std::collections::HashSet<String>;
    let decls: &[crate::core::ast::EnumDecl] = {
        let (rewritten, set) = generate_internal_cons_helpers(decls);
        if set.is_empty() {
            internal_cons_set = set;
            decls
        } else {
            decls_owned = rewritten;
            internal_cons_set = set;
            &decls_owned
        }
    };
    // Recompute after possible helper injection so helpers are in-batch for forward-ref.
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();

    // Seq-in-payload needs Y's sort concrete before X's batch; order stages accordingly.
    let stages = topo_stage_enums(decls, &batch_names, &internal_cons_set)?;

    for stage in stages {
        let stage_names: std::collections::HashSet<&str> =
            stage.iter().map(|&i| decls[i].name.as_str()).collect();

        let mut builders: Vec<DatatypeBuilder<'static>> = Vec::with_capacity(stage.len());
        for &i in &stage {
            let d = &decls[i];
            let mut builder = DatatypeBuilder::new(ctx, d.name.as_str());
            for v in &d.variants {
                let mut accessors: Vec<(&str, DatatypeAccessor)> = Vec::new();
                // owned_names keeps `f_arr`/`f_len` strings alive while &str refs live in `accessors`.
                let mut owned_names: Vec<String> = Vec::new();
                for f in &v.fields {
                    if let Some(inner) = parse_seq_type(&f.type_name) {
                        // Batch-local T: use single accessor → __SeqOf_T helper (forward-ref by name).
                        if internal_cons_set.contains(inner) {
                            let helper = internal_cons_helper_name(inner);
                            owned_names.push(helper);
                            let nm_idx = owned_names.len() - 1;
                            let nm: &str = unsafe {
                                &*(owned_names[nm_idx].as_str() as *const str)
                            };
                            accessors.push((f.name.as_str(),
                                DatatypeAccessor::Datatype(nm.into())));
                            continue;
                        }
                        // Seq(T) with concrete T → two accessors: arr: Array(Int→T), len: Int.
                        let elem_sort = resolve_concrete_sort(
                            inner, ctx, &stage_names, registry, &d.name, &v.name)?;
                        if elem_sort.is_none() {
                            return Err(RuntimeError::Parse(format!(
                                "internal: Seq({}) field in `{}::{}` references \
                                 an in-stage enum without an internal-Cons helper",
                                inner, d.name, v.name)));
                        }
                        let arr_sort = Sort::array(ctx, &Sort::int(ctx), &elem_sort.unwrap());
                        owned_names.push(format!("{}_arr", f.name));
                        let arr_name_idx = owned_names.len() - 1;
                        owned_names.push(format!("{}_len", f.name));
                        let len_name_idx = owned_names.len() - 1;
                        let arr_name: &str = unsafe {
                            &*(owned_names[arr_name_idx].as_str() as *const str)
                        };
                        let len_name: &str = unsafe {
                            &*(owned_names[len_name_idx].as_str() as *const str)
                        };
                        accessors.push((arr_name, DatatypeAccessor::Sort(arr_sort)));
                        accessors.push((len_name, DatatypeAccessor::Sort(Sort::int(ctx))));
                        continue;
                    }
                    let acc = match f.type_name.as_str() {
                        "Int" | "Nat" | "Pos" =>
                            DatatypeAccessor::Sort(Sort::int(ctx)),
                        "Bool"   => DatatypeAccessor::Sort(Sort::bool(ctx)),
                        "Real"   => DatatypeAccessor::Sort(Sort::real(ctx)),
                        "String" => DatatypeAccessor::Sort(Sort::string(ctx)),
                        other if stage_names.contains(other) => {
                            // In-stage forward-ref via Z3's resolver.
                            DatatypeAccessor::Datatype(other.into())
                        }
                        other => {
                            // Previously-loaded enum (earlier stage or
                            // earlier load batch). Resolve to concrete.
                            if let Some((prev, _)) = registry.by_name.borrow().get(other) {
                                DatatypeAccessor::Sort(prev.sort.clone())
                            } else {
                                return Err(RuntimeError::Parse(format!(
                                    "unknown payload type `{}` in variant `{}::{}` \
                                     (must be a primitive or a declared enum)",
                                    other, d.name, v.name,
                                )));
                            }
                        }
                    };
                    accessors.push((f.name.as_str(), acc));
                }
                builder = builder.variant(v.name.as_str(), accessors);
                drop(owned_names); // builder copies accessor names; safe to drop
            }
            builders.push(builder);
        }

        let sorts: Vec<DatatypeSort<'static>> =
            z3::datatype_builder::create_datatypes(builders);
        assert_eq!(sorts.len(), stage.len());

        // Stash each built sort + its variant decl list.
        {
            let mut by_name = registry.by_name.borrow_mut();
            let mut by_variant = registry.by_variant.borrow_mut();
            for (&i, dt) in stage.iter().zip(sorts.into_iter()) {
                let d = &decls[i];
                let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
                by_name.insert(d.name.clone(), (leaked, d.variants.clone()));
                for (idx, v) in d.variants.iter().enumerate() {
                    by_variant.insert(v.name.clone(), (d.name.clone(), idx));
                }
            }
        }
    }
    Ok(())
}

/// Walk `decls` for `Seq(T)` enum-variant fields where T is also in
/// `decls` (batch-local). For each such T, generate a Cons-shaped
/// helper enum:
///
/// ```text
/// enum __SeqOf_T =
///     __Empty_T
///     __Cell_T(T, __SeqOf_T)
/// ```
///
/// Returns the augmented decl list (original + helpers) and the set
/// of T-names that got helpers. Caller uses the set to route Seq
/// fields through the Cons helper in register_enums.
///
/// When no Seq-of-batch-local fields exist, returns (empty vec,
/// empty set) and the caller uses the original `decls` unchanged.
fn generate_internal_cons_helpers(
    decls: &[crate::core::ast::EnumDecl],
) -> (Vec<crate::core::ast::EnumDecl>, std::collections::HashSet<String>) {
    use crate::core::ast::{EnumDecl, EnumField, EnumVariant};
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();
    let mut needs_helper: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for d in decls {
        for v in &d.variants {
            for f in &v.fields {
                if let Some(inner) = parse_seq_type(&f.type_name) {
                    if batch_names.contains(inner) {
                        needs_helper.insert(inner.to_string());
                    }
                }
            }
        }
    }
    if needs_helper.is_empty() {
        return (Vec::new(), needs_helper);
    }
    let mut out: Vec<EnumDecl> = decls.to_vec();
    for t in &needs_helper {
        let helper_name = internal_cons_helper_name(t);
        let empty = EnumVariant {
            name: format!("__Empty_{}", t),
            fields: Vec::new(),
        };
        let cell = EnumVariant {
            name: format!("__Cell_{}", t),
            fields: vec![
                EnumField {
                    name: "head".to_string(),
                    type_name: t.clone(),
                },
                EnumField {
                    name: "tail".to_string(),
                    type_name: helper_name.clone(),
                },
            ],
        };
        out.push(EnumDecl {
            name: helper_name,
            variants: vec![empty, cell],
        });
    }
    (out, needs_helper)
}

/// Resolve a payload element type to a concrete Z3 Sort. Returns
/// `Ok(Some(sort))` for primitives + previously-loaded enums,
/// `Ok(None)` when the type is in the current stage (caller decides
/// how to handle — Seq fields error out, plain Datatype refs use
/// forward-ref). Returns `Err` on unknown types.
fn resolve_concrete_sort<'ctx>(
    type_name: &str,
    ctx: &'ctx z3::Context,
    stage_names: &std::collections::HashSet<&str>,
    registry: &crate::core::EnumRegistry,
    enclosing_enum: &str,
    enclosing_variant: &str,
) -> Result<Option<z3::Sort<'ctx>>, RuntimeError> {
    use z3::Sort;
    match type_name {
        "Int" | "Nat" | "Pos" => Ok(Some(Sort::int(ctx))),
        "Bool"   => Ok(Some(Sort::bool(ctx))),
        "Real"   => Ok(Some(Sort::real(ctx))),
        "String" => Ok(Some(Sort::string(ctx))),
        other if stage_names.contains(other) => Ok(None),
        other => {
            if let Some((prev, _)) = registry.by_name.borrow().get(other) {
                Ok(Some(prev.sort.clone()))
            } else {
                Err(RuntimeError::Parse(format!(
                    "unknown element type `{}` in Seq payload of `{}::{}` \
                     (must be a primitive or a declared enum)",
                    other, enclosing_enum, enclosing_variant,
                )))
            }
        }
    }
}

/// Partition `decls` into stages. Two kinds of dependencies:
///
///   * **Hard** (regular Datatype payload ref like `EffCons(Effect,
///     EffectList)`): the referenced enum must be in the SAME stage
///     as the referencer, so Z3's batch forward-ref machinery can
///     resolve it. Hard edges are transitive — they merge enums
///     into one stage via union-find.
///   * **Soft** (Seq-in-payload like `FFICall(Int, String, Seq(FFIArg))`):
///     the Seq element's sort must be concrete when the referencer's
///     batch is built. Soft edges order stages: the referencer's
///     group must come AFTER the element type's group.
///
/// Returns a list of stages, each containing indices into `decls`.
/// Errors if Seq-in-payload references form a cycle across hard-edge
/// groups (a single group requiring Seq into itself).
fn topo_stage_enums(
    decls: &[crate::core::ast::EnumDecl],
    _batch_names: &std::collections::HashSet<&str>,
    internal_cons_set: &std::collections::HashSet<String>,
) -> Result<Vec<Vec<usize>>, RuntimeError> {
    use std::collections::{HashMap, HashSet};

    let n = decls.len();
    let name_to_idx: HashMap<&str, usize> =
        decls.iter().enumerate().map(|(i, d)| (d.name.as_str(), i)).collect();

    // Union-find over enum indices for hard-edge merging.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r { r = parent[r]; }
        // Path compression.
        let mut cur = x;
        while parent[cur] != r {
            let next = parent[cur];
            parent[cur] = r;
            cur = next;
        }
        r
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb { parent[ra] = rb; }
    }

    // Walk every variant field; collect hard + soft edges.
    let mut soft: Vec<(usize, usize)> = Vec::new();  // (src_idx, dst_idx) — src needs dst earlier
    for (i, d) in decls.iter().enumerate() {
        for v in &d.variants {
            for f in &v.fields {
                if let Some(inner) = parse_seq_type(&f.type_name) {
                    // Internal-Cons backing: the field becomes a hard
                    // ref to `__SeqOf_T`, NOT a Seq-soft-edge to T.
                    // Without this, the soft-cycle check below would
                    // erroneously reject the mutually-recursive AST
                    // even though the runtime now handles it via the
                    // generated helper.
                    if internal_cons_set.contains(inner) {
                        let helper = internal_cons_helper_name(inner);
                        if let Some(&j) = name_to_idx.get(helper.as_str()) {
                            if j != i { union(&mut parent, i, j); }
                        }
                        continue;
                    }
                    if let Some(&j) = name_to_idx.get(inner) {
                        soft.push((i, j));
                    }
                    continue;
                }
                if let Some(&j) = name_to_idx.get(f.type_name.as_str()) {
                    if j != i {  // self-ref doesn't merge
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }

    // Group indices by their union-find root.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }

    // Group-level soft deps.
    let mut group_deps: HashMap<usize, HashSet<usize>> = HashMap::new();
    for &(src, dst) in &soft {
        let rs = find(&mut parent, src);
        let rd = find(&mut parent, dst);
        if rs == rd {
            // Seq inside its own hard-edge group: would need a
            // forward-ref Array sort, which Z3 doesn't support.
            return Err(RuntimeError::Parse(format!(
                "Seq-in-payload references a type in the same hard-edge group: \
                 `{}` has Seq(`{}`) and they're in one mutually-recursive batch",
                decls[src].name, decls[dst].name,
            )));
        }
        group_deps.entry(rs).or_default().insert(rd);
    }

    // Topologically order groups.
    let group_roots: Vec<usize> = groups.keys().copied().collect();
    let mut remaining: Vec<usize> = group_roots.clone();
    let mut built: HashSet<usize> = HashSet::new();
    let mut stages: Vec<Vec<usize>> = Vec::new();
    while !remaining.is_empty() {
        let mut this_round: Vec<usize> = Vec::new();
        let mut next: Vec<usize> = Vec::new();
        for &g in &remaining {
            let deps = group_deps.get(&g);
            let ready = deps.map(|d| d.iter().all(|x| built.contains(x))).unwrap_or(true);
            if ready { this_round.push(g); } else { next.push(g); }
        }
        if this_round.is_empty() {
            let names: Vec<&str> = remaining.iter()
                .flat_map(|g| groups[g].iter().map(|&i| decls[i].name.as_str()))
                .collect();
            return Err(RuntimeError::Parse(format!(
                "circular Seq-in-payload dependency across groups: {:?}", names)));
        }
        for &g in &this_round {
            built.insert(g);
            let mut stage: Vec<usize> = groups[&g].clone();
            stage.sort();
            stages.push(stage);
        }
        remaining = next;
    }
    Ok(stages)
}
