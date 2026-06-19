pub const BODY_MARKERS: &[&str] = &["spawnable_only"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    Schema,
    Claim,
    Type,
    Subclaim,

    Fsm,
}

#[derive(Debug, Clone)]
pub struct SchemaDecl {
    pub keyword: Keyword,
    pub name: String,
    pub body: Vec<BodyItem>,

    pub param_count: usize,

    pub external: bool,
}

#[derive(Debug, Clone)]
pub enum BodyItem {

    Membership { name: String, type_name: String, pins: Pins },

    Passthrough(String),

    SubclaimDecl(SchemaDecl),

    ClaimCall { name: String, mappings: Vec<Mapping> },

    Constraint(Expr),
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub slot: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum Pins {

    None,

    Named(Vec<Mapping>),

    Positional(Vec<Expr>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(String),
    Int(i64),

    Real(f64),
    Bool(bool),
    Str(String),

    SetLit(Vec<Expr>),

    SeqLit(Vec<Expr>),

    Range(Box<Expr>, Box<Expr>),

    InExpr(Box<Expr>, Box<Expr>),

    Tuple(Vec<Expr>),

    Forall(Vec<String>, Box<Expr>, Box<Expr>),
    Exists(Vec<String>, Box<Expr>, Box<Expr>),

    Call(String, Vec<Expr>),

    Cardinality(Box<Expr>),

    Index(Box<Expr>, Box<Expr>),

    Field(Box<Expr>, String),

    Binary(BinOp, Box<Expr>, Box<Expr>),

    Not(Box<Expr>),

    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),

    Match(Box<Expr>, Vec<MatchArm>),

    Matches(Box<Expr>, MatchPattern),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body:    Box<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {

    Ctor { name: String, binds: Vec<Option<String>> },

    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {

    Eq, Neq, Lt, Le, Gt, Ge,

    And, Or, Implies,

    Add, Sub, Mul, Div,

    Concat,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub schemas: Vec<SchemaDecl>,
    pub imports: Vec<String>,
    pub enums: Vec<EnumDecl>,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<EnumField>,
}

#[derive(Debug, Clone)]
pub struct EnumField {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone)]
pub enum Effect {
    NoEffect,
    Print(String),
    Println(String),
    ReadLine,
    Time,
    Exit(i64),

    ParseInt(String),

    ParseReal(String),

    IntToStr(i64),

    RealToStr(f64),

    ShellRun(String),
    FFIOpen(String),
    FFILookup(u64, String),
    FFICall(u64, String, Vec<EffectFfiArg>),
    CloseHandle(u64),

    LibCall(String, String, String, Vec<EffectFfiArg>),

    ReadByte(u64, i64),

    ReadI16(u64, i64),
    ReadI32(u64, i64),
    ReadI64(u64, i64),

    ReadF32(u64, i64),
    ReadF64(u64, i64),

    ReadStr(u64, i64),

    WriteByte(u64, i64, i64),
    WriteI16(u64, i64, i64),
    WriteI32(u64, i64, i64),
    WriteI64(u64, i64, i64),
    WriteF32(u64, i64, f64),
    WriteF64(u64, i64, f64),

    WriteStr(u64, i64, String),

    Malloc(i64),

    MonotonicTime,

    RegisterCallback(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PackedField {
    U8(u8),
    I32(i32),
    F32(f32),
}

impl PackedField {

    pub fn write_le(&self, out: &mut Vec<u8>) {
        match self {
            PackedField::U8(b)  => out.push(*b),
            PackedField::I32(n) => out.extend_from_slice(&n.to_le_bytes()),
            PackedField::F32(f) => out.extend_from_slice(&f.to_le_bytes()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectFfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),

    StrArr(Vec<String>),

    PriorResult(usize),

    I32Buf(Vec<i32>),

    PackedBuf(Vec<PackedField>),

    IntOut,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectResult {
    NoResult,
    Int(i64),
    Str(String),
    Bool(bool),
    Real(f64),
    Handle(u64),
    Error(String),
}
