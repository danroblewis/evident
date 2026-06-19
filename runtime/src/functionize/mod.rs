pub mod cranelift;

pub use crate::core::{CompiledFunction, Functionizer};

pub fn default() -> Box<dyn Functionizer> {
    Box::new(cranelift::CraneliftFunctionizer)
}
