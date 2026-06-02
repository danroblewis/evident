//! AST `Expr` → Z3 expression translators (Int / Bool / String / Real) and
//! thread-local translation context (active EnumRegistry, SeqLit-target hint).

use z3::DatatypeSort;

use crate::core::EnumRegistry;

mod mapping;
mod enums;
mod seq_field;
mod scalar;
mod record_lift;
mod seq_eq;
mod bool;
mod quant;
mod match_expr;
mod range;
mod string_ops;

pub(super) use mapping::resolve_mapping;
pub(super) use bool::translate_bool;

thread_local! {
    /// Active EnumRegistry pointer for the current translation; set/cleared via `EnumRegistryGuard`.
    /// Raw pointer because the registry's lifetime is tied to `EvidentRuntime`, not expressible via thread-local.
    static ACTIVE_ENUMS: std::cell::Cell<Option<*const EnumRegistry>> =
        const { std::cell::Cell::new(None) };
}

/// RAII guard: installs an EnumRegistry pointer in thread-local; restores the previous on drop.
pub struct EnumRegistryGuard {
    prev: Option<*const EnumRegistry>,
}

impl EnumRegistryGuard {
    pub fn new(enums: Option<&EnumRegistry>) -> Self {
        let new_ptr = enums.map(|r| r as *const EnumRegistry);
        let prev = ACTIVE_ENUMS.with(|c| {
            let was = c.get();
            c.set(new_ptr);
            was
        });
        Self { prev }
    }
}

impl Drop for EnumRegistryGuard {
    fn drop(&mut self) {
        ACTIVE_ENUMS.with(|c| c.set(self.prev));
    }
}

pub(super) fn with_active_enums<R>(f: impl FnOnce(Option<&EnumRegistry>) -> R) -> R {
    let ptr = ACTIVE_ENUMS.with(|c| c.get());
    // SAFETY: ptr was set by an EnumRegistryGuard that outlives this call (single-threaded translation).
    let opt = ptr.map(|p| unsafe { &*p });
    f(opt)
}

thread_local! {
    /// Expected enum type for SeqLit-as-Cons-chain lowering; set by `translate_bool` Eq path.
    static TARGET_ENUM_HINT: std::cell::RefCell<Option<(String, &'static DatatypeSort<'static>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with `target` as the current SeqLit-target hint; restores previous on return.
pub(super) fn with_target_enum_hint<R>(
    target: Option<(String, &'static DatatypeSort<'static>)>,
    f: impl FnOnce() -> R,
) -> R {
    let prev = TARGET_ENUM_HINT.with(|c| c.replace(target));
    let r = f();
    TARGET_ENUM_HINT.with(|c| { *c.borrow_mut() = prev; });
    r
}

pub(super) fn current_target_enum() -> Option<(String, &'static DatatypeSort<'static>)> {
    TARGET_ENUM_HINT.with(|c| c.borrow().clone())
}
