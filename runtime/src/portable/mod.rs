//! Swap-interface pattern: a pure transformation that has BOTH a Rust
//! implementation and an Evident-pass implementation, selectable at
//! runtime. This is the seam by which the runtime is progressively
//! self-hosted — pure passes move out of Rust into `stdlib/passes/*.ev`
//! while callers keep a stable Rust signature.
//!
//! # The pattern
//!
//! Each swappable function gets its own module here (`portable/pretty.rs`;
//! later `portable/desugar.rs`, `portable/inject.rs`, …). A module owns
//! three things:
//!
//!   1. A **typed trait** (e.g. [`pretty::PrettyImpl`]) — the function's
//!      Rust-level signature, independent of which impl backs it.
//!   2. The **Rust impl** (`RustPretty`) — the original native code; the
//!      default. Fast, total, always correct.
//!   3. The **Evident impl** (`EvidentPretty`) — owns an
//!      [`crate::EvidentRuntime`] with the stdlib pass loaded, marshals
//!      the Rust input into a [`crate::Value`], runs `rt.query`, and
//!      decodes the output binding.
//!
//! Every impl is also [`Portable`] — a short name used for tracing and
//! impl-selection assertions.
//!
//! # Selecting an impl
//!
//! Construction *is* selection: build a `RustPretty` or an
//! `EvidentPretty` and call the trait method. Each module also exposes a
//! `default_impl()` returning a `Box<dyn …>` chosen by an env var
//! (`EVIDENT_PRETTY_IMPL=rust|evident`), defaulting to the Rust impl.
//!
//! We deliberately do NOT thread a registry slot through
//! `EvidentRuntime`: the impls are standalone, so a caller — or a
//! cross-validation test — picks one without mutating shared runtime
//! state, and the choice can't leak across queries. (A registry on
//! `EvidentRuntime` is a viable future refinement once a pass needs to
//! be the *production* default; until then standalone keeps the seam
//! small and side-effect-free.)
//!
//! # Marshaling
//!
//! Input is encoded as a `Value::Enum` tree whose enum/variant names
//! match `stdlib/ast.ev`. Output is read from
//! `QueryResult.bindings["out"]`. v1 uses a per-port hand-written
//! marshaler (`pretty::encode`); it mirrors the private `*_to_value`
//! family in `translate/encode_ast.rs` and can be unified with it once
//! that surface is made public. Keeping the marshaler local to the port
//! means each port is self-contained.
//!
//! # Cost
//!
//! The Evident path runs through `EvidentRuntime::query`, which JIT-caches
//! the compiled claim after the first call — steady-state cost is a JIT
//! function call (~µs) plus marshaling, not a full Z3 solve. Loading the
//! pass file is the one-time construction cost, so hold an
//! `EvidentPretty` across calls rather than rebuilding it per call.
//!
//! See `docs/self-hosting.md` for the porting checklist and the current
//! runtime gaps (recursion, Unicode-in-strings) that bound what a pass
//! can faithfully reproduce.

/// A transformation impl that can be swapped between a Rust and an
/// Evident backing. The supertrait of every per-function impl trait
/// (`PrettyImpl`, future `DesugarImpl`, …). `impl_name` returns a short
/// identifier — `"rust"` / `"evident"` — for tracing and test
/// assertions.
pub trait Portable {
    fn impl_name(&self) -> &'static str;
}

pub mod inject;
pub mod pretty;
pub mod subscriptions;
pub mod validate;
