//! `generics` — generic-type monomorphization, driven by the [`super`]
//! swap interface. **NOT cut over: the canonical Rust pass stays the
//! production load path** (honest-fallback — see below).
//!
//! Monomorphization (`runtime/src/runtime/generics.rs`) expands every
//! `type Edge<T>` / `claim Toposort<T>` reference into a concrete copy
//! (`Edge<Rect>`, `Toposort<Int>`, …) before translation. It runs in the
//! load pipeline (`runtime/src/runtime/load.rs`). The pass decomposes
//! into two halves:
//!
//!   * **WALK** — find every type-position string that could name a
//!     generic instantiation (a `Membership`'s type_name, a `ClaimCall`'s
//!     / `Passthrough`'s name, every `Call` name in a constraint). This
//!     is `collect_generic_uses`'s `walk` / `walk_expr`.
//!   * **PARSE + SUBSTITUTE + CONSTRUCT** — split `"Edge<Rect>"` into
//!     head + args, substitute the type params through the generic body's
//!     type_name strings, build the concrete copy, iterate to a fixed
//!     point. This is `collect_from_type_name` + `substitute_idents` +
//!     `monomorphize_generics_with`'s loop.
//!
//! ## What self-hosts, and the gap that blocks a cutover
//!
//! The **WALK is faithfully self-hosted** here — [`EvidentGenerics`] runs
//! `stdlib/passes/generics.ev`, a stack-FSM over the SHARED marshaler
//! (session UU), exactly the recipe `subscriptions` proved. The
//! [`generics_equivalence`](../../../tests/generics_equivalence.rs) test
//! pins it byte-identical to the Rust walk on the full corpus.
//!
//! The **PARSE + SUBSTITUTE half cannot be expressed in Evident.**
//! `split_generic_head("Edge<Rect>")` scans for `<` / `>`;
//! `substitute_idents("Seq(T)", T↦Rect)` tokenizes the string into
//! identifier runs and rebuilds it. Both are substring / character-level
//! operations, and Evident's only string ops are `=`, `≠`, and `++` —
//! there is no substring, char-access, split, strip, or tokenize. This is
//! the SAME limit `subscriptions` documented for its one-line `world.`
//! prefix classifier, except for generics it is the WHOLE transformation,
//! not a leaf decision. So the FSM emits RAW type-position strings and
//! this shim runs the (unavoidable) Rust parse + substitution.
//!
//! Per the honest-fallback policy: because the rewrite half can't move,
//! **generics is NOT cut over**. The two impls share the exact subst +
//! construct + fixed-point body (`monomorphize_generics_with`) and differ
//! only in the swappable *collector* — the same "shared transform,
//! swappable sub-step" shape `portable/validate.rs` uses. `RustGenerics`
//! is and remains the production default; `EvidentGenerics` + the
//! equivalence test prove the walk half is self-hostable. Routing the
//! load path through Evident would add a per-load FSM cost (the collector
//! runs up to 50× in the fixed point) while deleting nothing — the
//! inexpressible subst + construct + the parse all stay in Rust — so net
//! Rust LOC would not fall. The recipe generalizes; the language gap is
//! the blocker. See `docs/self-hosting.md`, `docs/jit-codegen-gaps.md`,
//! and `examples/COUNTEREXAMPLES.md`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::core::ast::SchemaDecl;
use crate::core::{RuntimeError, Value};
use crate::runtime::generics::{collect_from_type_name, collect_generic_uses};
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_list, decode_str};
use crate::translate::ast_encoder::body_item_to_value;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// Monomorphization's Rust-level signature, independent of which impl
/// backs it. The swappable unit is [`collect_uses`](Self::collect_uses) —
/// the AST WALK that locates generic uses. [`monomorphize`](Self::monomorphize)
/// drives the shared fixed-point + substitution + copy construction
/// (`monomorphize_generics_with`) using that collector, so the only thing
/// that differs between impls — and the only thing the equivalence test
/// has to pin — is the walk.
pub trait GenericsImpl: Portable {
    /// Collect every `(composite_name, generic_head, args_str)` tuple
    /// referenced anywhere in the schema map. Same contract as the
    /// canonical `runtime::generics::collect_generic_uses`. Order is
    /// unspecified (the canonical iterates a `HashMap`); compare as a set.
    fn collect_uses(&self, schemas: &HashMap<String, SchemaDecl>) -> Vec<(String, String, String)>;

    /// Run the full monomorphization to a fixed point using this impl's
    /// [`collect_uses`](Self::collect_uses). The fixed-point loop,
    /// type-param substitution, copy construction, and every error case
    /// are the shared canonical body — only the collector is swapped, so
    /// two impls whose collectors agree produce byte-identical schema maps.
    fn monomorphize(
        &self,
        schemas: &mut HashMap<String, SchemaDecl>,
        schema_order: &mut Vec<String>,
    ) -> Result<(), RuntimeError> {
        crate::runtime::generics::monomorphize_generics_with(
            schemas, schema_order, |s| self.collect_uses(s))
    }
}

// ─────────────────────────────────────────────────────────────────────
// Rust impl — wraps the canonical walk. The production default.
// ─────────────────────────────────────────────────────────────────────

/// Native collector — the canonical `collect_generic_uses`. Total, fast,
/// always correct; this is what the load pipeline runs.
pub struct RustGenerics;

impl Portable for RustGenerics {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl GenericsImpl for RustGenerics {
    fn collect_uses(&self, schemas: &HashMap<String, SchemaDecl>) -> Vec<(String, String, String)> {
        collect_generic_uses(schemas)
    }
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — runs stdlib/passes/generics.ev as a stack-FSM
// ─────────────────────────────────────────────────────────────────────

/// Runs the WALK by encoding each body item with the shared marshaler and
/// driving the `generics_walk` FSM to halt; the Rust shim then parses the
/// raw type-position strings the FSM emits (the substring-dependent half
/// that can't move). Holds its own runtime with the pass loaded; build
/// once and reuse so the FSM's per-tick solve is JIT-cached across calls.
pub struct EvidentGenerics {
    rt: EvidentRuntime,
}

impl EvidentGenerics {
    /// The whole-walk FSM in `stdlib/passes/generics.ev`.
    const WALK_FSM: &'static str = "generics_walk";

    /// Max-iteration guard for the nested walk. One AST node costs a small
    /// constant number of FSM ticks, so a body of N nodes halts in O(N)
    /// ticks; the cap sits far above any realistic claim (a runaway walk
    /// would be a pass bug, surfaced as a loud `MaxItersExceeded`).
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/generics.ev` from `stdlib_dir` into a fresh runtime.
    /// The pass is self-contained (it declares its own cons-list copy of
    /// the AST enums matching the shared marshaler), so no other stdlib
    /// file is needed.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("generics.ev"))
            .map_err(|e| format!("load passes/generics.ev: {e}"))?;
        Ok(Self { rt })
    }

    /// Drive `generics_walk` over one seeded `Work` node and return the
    /// RAW type-position strings it reaches (head-first). The FSM returns
    /// `GWDone(NameList)` — a cons-list of unparsed type-name strings.
    fn walk_item_raw(&self, seed: &Value, claim_name: &str) -> Vec<String> {
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed.clone(), Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "GWDone" && fields.len() == 1 => {
                match decode_list(&fields[0], "NameList", "NameNil", "NameCons", decode_str) {
                    Ok(names) => names,
                    Err(e) => {
                        eprintln!("[generics/evident] decode of `{claim_name}` result failed: {e}");
                        Vec::new()
                    }
                }
            }
            Ok(other) => {
                eprintln!("[generics/evident] walk of `{claim_name}` returned a \
                    non-GWDone state: {other:?}");
                Vec::new()
            }
            Err(e) => {
                eprintln!("[generics/evident] walk of `{claim_name}` failed: {e}");
                Vec::new()
            }
        }
    }
}

impl Portable for EvidentGenerics {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl GenericsImpl for EvidentGenerics {
    fn collect_uses(&self, schemas: &HashMap<String, SchemaDecl>) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        // Iterate schema keys in sorted order for run-to-run reproducibility.
        // The canonical `collect_generic_uses` iterates `schemas.values()`
        // (HashMap order) and dedups via the shared `seen`; the RESULT SET
        // is order-independent, so the equivalence test compares as a set.
        let mut keys: Vec<&String> = schemas.keys().collect();
        keys.sort();
        for k in keys {
            let s = &schemas[k];
            // Drive the walk-FSM once per top-level body item (a flat
            // driver, NOT a tree walk — every recursion, into sub-exprs AND
            // into subclaim bodies, happens inside the FSM). Each emitted
            // raw string is parsed through the SHARED `collect_from_type_name`
            // with one shared `seen` — byte-identical to the canonical's
            // single-`seen` walk-and-parse.
            for item in &s.body {
                let seed = work_node("WBody", body_item_to_value(item));
                for raw in self.walk_item_raw(&seed, &s.name) {
                    collect_from_type_name(&raw, &mut out, &mut seen);
                }
            }
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point + selection
// ─────────────────────────────────────────────────────────────────────

/// Wrap an already-marshaled AST `Value` as the FSM's unified `Work` node.
fn work_node(variant: &str, inner: Value) -> Value {
    Value::Enum {
        enum_name: "Work".to_string(),
        variant: variant.to_string(),
        fields: vec![inner],
    }
}

/// Pick a collector impl by `EVIDENT_GENERICS_IMPL` (`rust` | `evident`),
/// defaulting to the Rust impl. `evident` locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver (honoring `EVIDENT_STDLIB`);
/// if locating or loading fails it falls back to Rust.
///
/// **The load pipeline does NOT call this** — `runtime/src/runtime/load.rs`
/// uses the canonical `monomorphize_generics` directly (generics isn't cut
/// over; see the module doc). This selector exists for parity with the
/// rest of the `portable/` family and for opt-in experimentation.
pub fn default_impl() -> Box<dyn GenericsImpl> {
    if std::env::var("EVIDENT_GENERICS_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = crate::stdlib_path::stdlib_dir() {
            if let Ok(ev) = EvidentGenerics::new(&dir) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustGenerics)
}
