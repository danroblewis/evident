//! Functionizer interface: a strategy for turning an extracted
//! `Z3Program` into a callable function.
//!
//! The runtime extracts a `Z3Program` from a claim body (via Z3's
//! tactic chain), then hands it to a `Functionizer` for compilation.
//! The compiled artifact is a `CompiledFunction` that the runtime
//! calls once per query.
//!
//! Today the only implementation is Cranelift JIT (native code).
//! Future strategies (an interpreter, a transpiler to C / GLSL,
//! a remote compile cache) plug in by implementing `Functionizer`.
//!
//! Selection happens at runtime construction:
//!
//! ```ignore
//! let rt = EvidentRuntime::new();                 // default: Cranelift
//! let rt = EvidentRuntime::with_functionizer(    // custom
//!     Box::new(my_strategy));
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use crate::translate::{EnumRegistry, Value};
use crate::z3_eval::Z3Program;

pub mod cranelift;

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

/// Default functionizer (Cranelift JIT). Used by `EvidentRuntime::new()`.
pub fn default() -> Box<dyn Functionizer> {
    Box::new(cranelift::CraneliftFunctionizer)
}
