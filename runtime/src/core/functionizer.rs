//! `Functionizer` + `CompiledFunction` traits: strategy for compiling `Z3Program` to a callable.
//! Concrete implementations live under `crate::functionize::*`; selected at runtime construction.

use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{DatatypeRegistry, EnumRegistry, Value, Z3Program};

/// Strategy for compiling a `Z3Program` into a callable artifact. Return `Some` on success;
/// `None` to refuse (runtime falls through to full Z3 solve).
pub trait Functionizer {
    /// Short identifier for stats/tracing (e.g. `"cranelift"`).
    fn name(&self) -> &'static str;

    /// Compile `program`. `enums` for enum-typed I/O; `datatypes` for Seq(Record)/composite outputs.
    fn compile(&self,
               program:   &Z3Program,
               enums:     &EnumRegistry,
               datatypes: &DatatypeRegistry)
        -> Option<Rc<dyn CompiledFunction>>;
}

/// Compiled artifact. `call` returns output bindings, or `None` to fall through to Z3.
pub trait CompiledFunction {
    fn call(&self, given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>;
}
