fn main() {
    println!("sizeof Value = {}", std::mem::size_of::<evident_runtime::Value>());
    println!("align Value  = {}", std::mem::align_of::<evident_runtime::Value>());
    let v = evident_runtime::Value::Int(0);
    println!("Int(0)   = {v:?}");
    let v = evident_runtime::Value::Enum {
        enum_name: "Effect".into(),
        variant: "Println".into(),
        fields: vec![evident_runtime::Value::Str("hello".into())],
    };
    println!("Enum     = {v:?}");
}
