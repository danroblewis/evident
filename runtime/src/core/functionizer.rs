//! Functionizer trait interface: a strategy for turning an extracted
//! `Z3Program` into a callable function.
//!
//! The runtime extracts a `Z3Program` from a claim body (via Z3's
//! tactic chain), then hands it to a `Functionizer` for compilation.
//! The compiled artifact is a `CompiledFunction` that the runtime
//! calls once per query.
//!
//! Concrete implementations live under `crate::functionize::*` (the
//! Cranelift JIT is the only one today). Selection happens at runtime
//! construction via `EvidentRuntime::with_functionizer`.

use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{EnumRegistry, Value, Z3Program};

/// A strategy for compiling an extracted `Z3Program` into a
/// callable artifact. Implementations decide what "compile" means
/// (emit native code, build an op tree, transpile to another
/// language, …). On success, return `Some(Rc<dyn CompiledFunction>)`;
/// on refusal (program uses constructs this strategy can't handle),
/// return `None` and the runtime falls through to a full Z3 solve.
pub trait Functionizer {
    /// Short identifier used in stats output and tracing.
    /// Examples: `"cranelift"`, `"interpreter"`, `"c-transpiler"`.
    fn name(&self) -> &'static str;

    /// Compile a `Z3Program` into a callable function. `enums` is
    /// the runtime's enum registry — strategies that need to encode
    /// enum-typed inputs / outputs read variant tags from it.
    fn compile(&self,
               program: &Z3Program,
               enums:   &EnumRegistry)
        -> Option<Rc<dyn CompiledFunction>>;
}

/// A compiled artifact produced by a `Functionizer`. The runtime
/// calls `call` once per query with the input bindings; the
/// implementation returns the output bindings (or `None` if the
/// inputs violated a runtime predicate, in which case the runtime
/// falls through to a full Z3 solve).
pub trait CompiledFunction {
    fn call(&self, given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>;
}
