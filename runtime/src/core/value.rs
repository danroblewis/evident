use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),

    Real(f64),
    Bool(bool),
    Str(String),

    SeqInt(Vec<i64>),
    SeqBool(Vec<bool>),
    SeqStr(Vec<String>),

    Composite(HashMap<String, Value>),

    SeqComposite(Vec<HashMap<String, Value>>),

    SeqEnum(Vec<Value>),

    SetInt(Vec<i64>),
    SetBool(Vec<bool>),
    SetStr(Vec<String>),

    Enum {
        enum_name: String,
        variant: String,
        fields: Vec<Value>,
    },
}
