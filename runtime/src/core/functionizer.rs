use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{DatatypeRegistry, EnumRegistry, Value, Z3Program};

pub trait Functionizer {

    fn name(&self) -> &'static str;

    fn compile(&self,
               program:   &Z3Program,
               enums:     &EnumRegistry,
               datatypes: &DatatypeRegistry)
        -> Option<Rc<dyn CompiledFunction>>;
}

pub trait CompiledFunction {
    fn call(&self, given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>;
}
