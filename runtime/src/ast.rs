//! AST node types — mirrors `parser/src/ast.py` for the v0.1 subset.
//!
//! Only what the v0.1 milestone (`SimpleNat { n ∈ Nat ; n > 5 }`) needs.
//! Add more variants as we expand support.

/// Bare-identifier body items recognized as runtime metadata rather
/// than translatable constraints. When one of these names appears as
/// a `BodyItem::Constraint(Identifier(name))`, the constraint
/// translator skips it (it has no Bool meaning) and runtime layers
/// that care about the marker (currently the multi-FSM scheduler)
/// inspect the body for it directly.
///
/// Add a new entry here ONLY when the meaning of a bare-identifier
/// body item is established at the AST level — i.e. when it's a
/// language convention, not a one-off scheduler hook. The translator
/// MUST NOT reference any specific marker by literal string;
/// scheduler / runtime layers MAY reference specific entries by
/// looking them up against this list.
pub const BODY_MARKERS: &[&str] = &["spawnable_only"];

/// One of the three keywords that all parse to a "schema" header. Kept
/// distinct because some downstream features (subclaim, the type/claim/
/// schema reading convention) check it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    Schema,
    Claim,
    Type,
    Subclaim,
    /// `fsm Name(state ∈ S, …)` — a claim that the multi-FSM scheduler
    /// auto-instantiates. Distinguishes scheduler-eligible FSMs from
    /// ordinary predicates so we don't shape-detect. The runtime
    /// injects `state_next`, `last_results ∈ Seq(Result)`, and
    /// `effects ∈ EffectList` at load time if the body doesn't
    /// already declare them.
    Fsm,
}

/// Top-level schema declaration:
///   schema Name
///       <body>
#[derive(Debug, Clone)]
pub struct SchemaDecl {
    pub keyword: Keyword,
    pub name: String,
    /// Formal type parameters declared with the schema. `type Edge<T>(...)`
    /// captures `["T"]`; `type Pair<A, B>(...)` captures `["A", "B"]`.
    /// Empty for non-generic schemas. At translation time the
    /// `monomorphize_generics` pass produces concrete copies of each
    /// generic schema for every distinct `<args>` instantiation found
    /// in the program; the original generic SchemaDecl is kept as a
    /// template and is never directly translated to Z3.
    ///
    /// Type-parameter names follow the convention of being capitalized
    /// single letters or short capitalized identifiers (`T`, `A`, `B`,
    /// `K`, `V`, `Item`, `Element`, …). They live in a separate
    /// namespace from concrete type names — `T` inside a generic body
    /// is *only* a type variable, never an existing concrete type.
    pub type_params: Vec<String>,
    pub body: Vec<BodyItem>,
    /// Number of leading body items that came from the first-line
    /// param list — `claim Foo(a ∈ X, b ∈ Y) ...` desugars to those
    /// Memberships at the head of `body`. They are the "input/output
    /// slots" of the claim: the runtime treats them as outer-bound
    /// (via slot mapping for positional invocation, via names-match
    /// for guarded `cond ⇒ Foo` invocation), while body Memberships
    /// past this index are helper-LOCALS that get fresh per-call Z3
    /// consts to keep recursive helper invocations isolated.
    /// Zero when the claim has no first-line params.
    pub param_count: usize,
    /// `external` modifier — `external type X(...)` or
    /// `external claim X(...)`. Marks the schema as a boundary-crossing
    /// declaration: only `external` schemas may construct FFI / LibCall
    /// effects in their body. The load-time check in
    /// `runtime::enforce_external_only` rejects non-`external` claims
    /// that try to emit those effects. `external` cannot combine with
    /// `fsm` or `schema` — both are rejected at parse time.
    pub external: bool,
}

/// A single line in a schema body.
#[derive(Debug, Clone)]
pub enum BodyItem {
    /// `x ∈ Type`  — declares a typed variable. For v0.1 the right side
    /// is always a simple identifier (e.g. `Nat`, `Int`, `Bool`).
    ///
    /// `pins` narrows the declared variable to the subset of `Type`
    /// satisfying additional per-field equalities. Two forms:
    ///
    ///   - Named: `x ∈ Type (a mapsto v1, b mapsto v2)` — unambiguous,
    ///     order-independent.
    ///   - Positional: `x ∈ Type (v1, v2, …)` — uses the type's field
    ///     declaration order. Strict: must match the field count.
    ///
    /// Both desugar to a series of `name.field = value` constraints
    /// at translate time. `Pins::None` for the bare `x ∈ Type` form.
    Membership { name: String, type_name: String, pins: Pins },

    /// `..ClaimName` — passthrough composition. The named claim's body
    /// is inlined into the current schema's body using names-match: any
    /// variable declared in the included claim with the same name as a
    /// variable in scope here resolves to the same Z3 const.
    Passthrough(String),

    /// `subclaim Name` — a claim defined inside the parent's body.
    /// Has no effect on the parent at translation time (it doesn't
    /// contribute constraints) but is registered into the runtime's
    /// schemas table at load time so other ClaimCall / passthrough
    /// items can reference it by name.
    SubclaimDecl(SchemaDecl),

    /// `ClaimName(slot mapsto value, …)` — claim composition with
    /// explicit renaming. Each `Mapping` binds the included claim's
    /// `slot` (a variable name in its body) to `value` (a literal or
    /// an existing Z3 binding from the current env).
    ClaimCall { name: String, mappings: Vec<Mapping> },

    /// Any other constraint (comparison, arithmetic equality, etc.).
    Constraint(Expr),
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub slot: String,
    pub value: Expr,
}

/// Optional per-field pinning for a `Membership`. See `Membership` for
/// the source-level forms each variant comes from.
#[derive(Debug, Clone)]
pub enum Pins {
    /// `x ∈ Type` — no pins, bare declaration.
    None,
    /// `x ∈ Type (a mapsto v1, b mapsto v2)` — explicit field names.
    /// Order-independent, partial allowed.
    Named(Vec<Mapping>),
    /// `x ∈ Type (v1, v2, …)` — positional, by field declaration order
    /// in the type's body. Translator looks up the SchemaDecl to resolve
    /// each position to a field name. Strict count match required.
    Positional(Vec<Expr>),
}

/// Expression tree. Compact for v0.1.
#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(String),
    Int(i64),
    /// Real literal. Stored as f64 for ergonomics; converted to a Z3
    /// Real via the numeral's canonical decimal form (Rust's f64
    /// formatter gives `"3.14"` exactly for `3.14`, which Z3's
    /// `from_real_str` parses without precision loss for typical
    /// game/physics constants).
    Real(f64),
    Bool(bool),
    Str(String),
    /// `{e1, e2, …}` set literal — only used as the right side of `∈`
    /// (membership). Not a first-class set value (no Z3 set sort yet).
    SetLit(Vec<Expr>),
    /// `⟨e1, e2, …⟩` sequence literal. Used as the RHS of `=` against a
    /// `Seq(T)` variable. The translator pins both length and per-element
    /// values when it sees `seq_var = SeqLit(items)`.
    SeqLit(Vec<Expr>),
    /// `{lo..hi}` integer range — only used as a quantifier bound.
    Range(Box<Expr>, Box<Expr>),
    /// `lhs ∈ rhs` membership constraint as an expression. We always
    /// reduce this to a disjunction of equalities (lhs = e1 ∨ lhs = e2 ∨ …).
    InExpr(Box<Expr>, Box<Expr>),
    /// `(e1, e2, …)` — a tuple expression. Currently produced by the
    /// parser only as the LHS of `∈ claim_name` — the relational
    /// invocation form for a claim with multiple parameters. See
    /// `inline.rs`'s body-item dispatch for the translation: the
    /// elements bind positionally to the claim's parameters.
    /// Other contexts that see this variant should recurse into
    /// the elements (or reject if a tuple isn't meaningful there).
    Tuple(Vec<Expr>),
    /// `∀ var ∈ range : body` and the existential variant.
    /// Translation requires `range` to be a literal `Range(Int, Int)`
    /// so we can unroll. The `Vec<String>` is the binding: usually
    /// length 1 (`∀ x ∈ …`), length ≥ 2 for tuple destructuring
    /// (`∀ (a, b) ∈ coindexed(A, B) : …` — pair iteration).
    Forall(Vec<String>, Box<Expr>, Box<Expr>),
    Exists(Vec<String>, Box<Expr>, Box<Expr>),
    /// `name(arg, …)` — function-call expression. Used for builtins
    /// like `coindexed(A, B, C)` and `edges(seq)` in quantifier source
    /// position. We don't have user-defined functions yet; the
    /// translator special-cases recognized names and errors on
    /// unrecognized ones.
    Call(String, Vec<Expr>),
    /// `#expr` — cardinality. For Seq translates to Z3 Length.
    Cardinality(Box<Expr>),
    /// `expr[index]` — sequence indexing. Translates to Z3 nth.
    Index(Box<Expr>, Box<Expr>),
    /// `expr.field` — field access on a non-Identifier expression
    /// (e.g. `pts[0].x`). Field access on a bare identifier still
    /// folds into a dotted `Identifier` at parse time; this variant
    /// is for cases where the receiver is itself an expression like
    /// `Index`. The runtime resolves these through the receiver's
    /// Datatype accessors rather than env-key lookup.
    Field(Box<Expr>, String),
    /// Binary operation: `lhs op rhs`.
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Unary `¬e`.
    Not(Box<Expr>),
    /// Ternary conditional: `cond ? then_branch : else_branch`.
    /// Both branches must translate to the same Z3 sort. Maps to
    /// `Bool::ite(cond, then, else)` in the translator. Sits at
    /// lower precedence than `∨` and higher than `⇒`, so:
    ///   `a ∨ b ? c : d` parses as `(a ∨ b) ? c : d`
    ///   `a ⇒ b ? c : d` parses as `a ⇒ (b ? c : d)`
    /// Right-associative: `a ? b : c ? d : e` is `a ? b : (c ? d : e)`.
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Pattern-match expression over an enum-typed scrutinee:
    /// ```text
    /// match r
    ///     Ok(n)  ⇒ n * 10
    ///     Err(_) ⇒ 0
    /// ```
    /// Translates to a nested Z3 ITE chain: `is_Ok(r) ? <arm1 with
    /// n bound to Ok_arg0(r)> : (is_Err(r) ? <arm2> : ...)`. All arm
    /// bodies must share a sort (same as ternary). Either all enum
    /// variants are covered or there's a wildcard `_ ⇒ ...` arm.
    Match(Box<Expr>, Vec<MatchArm>),
    /// Constructor-recognizer test: `e matches Ctor(_, _, ...)`.
    /// Returns Bool — true iff `e`'s variant tag is `Ctor`. Payload
    /// bindings are IGNORED in this form (`_` and bare names alike
    /// act as wildcards). For payload-aware extraction, use a `match`
    /// expression. For literal-payload comparison, use `e = Ctor(7)`.
    Matches(Box<Expr>, MatchPattern),
}

/// One arm of a `match` expression — a pattern + the body that fires
/// when the scrutinee matches.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body:    Box<Expr>,
}

/// Match pattern. Either a constructor with positional bindings
/// (`Ctor(name, _, name2)`) or a wildcard `_` that catches any value.
/// A binding of `_` discards the corresponding payload field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    /// `Ctor(b0, b1, ...)`. `binds[i]` is the variable name to bind
    /// the i-th payload field to, or `None` if the binding was `_`
    /// (discard). Length must match the constructor's arity.
    Ctor { name: String, binds: Vec<Option<String>> },
    /// `_` — matches any value, no bindings.
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {
    // Comparisons → Bool
    Eq, Neq, Lt, Le, Gt, Ge,
    // Bool ops
    And, Or, Implies,
    // Arithmetic → Int
    Add, Sub, Mul, Div,
    // String concatenation (`++`) → String
    Concat,
}

/// A parsed program (one or more top-level declarations).
///
/// `imports` is captured at parse time but consumed during loading
/// (see `EvidentRuntime::load_source` / `load_file`). After loading,
/// only the `schemas` survive into the runtime's IR.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub schemas: Vec<SchemaDecl>,
    pub imports: Vec<String>,
    pub enums: Vec<EnumDecl>,
}

/// An enum declaration: `enum Name = Variant1 | Variant2(T1, T2) | …`.
/// Variants are either nullary (no payload) or carry an ordered tuple
/// of typed fields. Self-references are allowed inside payload field
/// types (the enum being declared can reference itself), enabling
/// recursive types like `enum Expr = Int(Int) | Binary(BinOp, Expr, Expr)`.
/// Translates to a Z3 algebraic datatype with one constructor per
/// variant; payload field types resolve to either a primitive Z3 sort
/// or a `DatatypeAccessor::Datatype(self_name)` for recursion.
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// One variant of an `EnumDecl`. Payload field names are auto-generated
/// (`f0`, `f1`, …) when callers don't supply them — sufficient for
/// positional Variant(T1, T2) syntax. Named-payload variants
/// (`Variant { x ∈ Int, y ∈ Int }`) are out of scope for v0.1.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<EnumField>,
}

/// One field of an enum payload. `type_name` is a raw textual type
/// reference (validated at registration time by looking it up against
/// primitive sorts, the EnumRegistry itself for self-references, or
/// future user types).
#[derive(Debug, Clone)]
pub struct EnumField {
    pub name: String,
    pub type_name: String,
}

// TraceDecl, TraceStep, TraceAssertion, AssertOp, ShaderDecl removed
// in Phase 2 plugin removal — the runners that consumed them are gone.

// ── Effect / Result / FfiArg ─────────────────────────────────────
//
// Mirror of `stdlib/runtime.ev`'s Effect/Result/FFIArg enums. The
// effect dispatcher in `effect_dispatch.rs` walks Vec<Effect> and
// produces Vec<EffectResult>. Decoded from Z3 datatype values by
// `decode_ast::decode_effect_list` (which now reads a Value::SeqEnum
// — the Seq(Effect) shape — rather than a Cons chain).

/// One side-effect to perform between solver steps. Variants align
/// with the `Effect` enum in stdlib/runtime.ev.
#[derive(Debug, Clone)]
pub enum Effect {
    NoEffect,
    Print(String),
    Println(String),
    ReadLine,
    Time,
    Exit(i64),
    /// Parse a decimal integer string. On success the result is an
    /// IntResult. On parse failure, an ErrorResult with the parse
    /// error message. Empty string and trailing whitespace both
    /// fail (use trim/strip in user code if needed).
    ParseInt(String),
    /// Parse a decimal real (float) string. IEEE-754 double precision.
    /// On success: RealResult. On failure: ErrorResult.
    ParseReal(String),
    /// Format an Int as decimal string. StringResult always.
    IntToStr(i64),
    /// Format a Real (f64) as decimal string. Uses Rust's default
    /// f64 formatter — sufficient for typical numeric output.
    /// StringResult always.
    RealToStr(f64),
    /// Run a shell command synchronously, capture stdout. Result
    /// is StringResult(stdout) on success (exit code 0); otherwise
    /// ErrorResult with exit code + stderr. Synchronous (blocks
    /// dispatch until the command returns) — fine for short-lived
    /// commands; for long-running programs use a custom plugin.
    /// Trailing newline is stripped from stdout for convenience.
    ShellRun(String),
    /// Spawn a new FSM instance of the named claim with an Int
    /// argument that gets pinned into the new FSM's first
    /// state-variant payload. The new FSM joins the scheduler
    /// from the next tick; result is IntResult(instance_id).
    ///
    /// Convention: define the spawnable claim's state enum with
    /// a payload first variant taking an Int (the instance ID
    /// or a parameter chosen by the parent). The runtime seeds
    /// the new FSM's state to `FirstVariant(arg)`. Subsequent
    /// state transitions are the FSM's own.
    ///
    /// Example:
    ///   enum WState = WInit(Int) | WGo
    ///   claim worker(state, state_next ∈ WState, ...)
    ///       my_id ∈ Int
    ///       my_id = match state
    ///           WInit(id) ⇒ id
    ///           WGo       ⇒ -1
    ///       state_next = match state
    ///           WInit(_) ⇒ WGo
    ///           WGo      ⇒ WGo
    ///
    /// v1 limitation: shares parent's world; no per-instance
    /// world or parent-child message passing beyond shared world.
    /// See docs/design/fsm-spawning.md.
    SpawnFsm(String, i64),
    FFIOpen(String),
    FFILookup(u64, String),
    FFICall(u64, String, Vec<EffectFfiArg>),
    CloseHandle(u64),
    /// Cached one-shot FFI call: `LibCall(library, symbol, signature, args)`.
    /// The runtime lazily resolves and caches `library` + `symbol` so
    /// repeated calls amortize dlopen/dlsym to once. See
    /// `effect_dispatch::dispatch_one` for the cache implementation.
    LibCall(String, String, String, Vec<EffectFfiArg>),
    /// `ReadByte(handle, offset)` — read a single u8 at the byte
    /// offset from the registered pointer. Result is `IntResult(byte)`
    /// with byte ∈ 0..255. The handle must be a registered pointer
    /// from a prior FFI call. Caller is responsible for in-bounds
    /// offsets; out-of-bounds is undefined behavior.
    ///
    /// Primary use: reading flat memory exposed by C libraries —
    /// the SDL_GetKeyboardState array is the motivating case
    /// (`ReadByte(state_ptr, SDL_SCANCODE_LEFT)` → 0 or 1).
    ReadByte(u64, i64),
    /// Signed wide reads. All use unaligned access so the offset
    /// doesn't need to be aligned. Result is sign-extended into i64
    /// (`Read{Width} == read_unaligned::<i{Width}>` as i64).
    ReadI16(u64, i64),
    ReadI32(u64, i64),
    ReadI64(u64, i64),
    /// Float reads (unaligned). f32 result is widened to f64.
    ReadF32(u64, i64),
    ReadF64(u64, i64),
    /// Null-terminated UTF-8 string read. Walks bytes from
    /// `(ptr + offset)` to the first 0 byte, returns the result
    /// as StringResult. Invalid UTF-8 returns ErrorResult; no
    /// length cap, so callers must trust the buffer contents.
    ReadStr(u64, i64),
    /// Memory writes. Mirror of the Read variants; same
    /// alignment-tolerant unaligned access. All return NoResult.
    /// Caller must hold a handle to writable memory (typically
    /// from Malloc); writing into pointers backed by const-
    /// qualified C buffers is undefined behavior.
    WriteByte(u64, i64, i64),         // value's low byte stored
    WriteI16(u64, i64, i64),
    WriteI32(u64, i64, i64),
    WriteI64(u64, i64, i64),
    WriteF32(u64, i64, f64),          // narrowed to f32
    WriteF64(u64, i64, f64),
    /// Writes the UTF-8 bytes followed by a 0 terminator. Caller
    /// must have allocated at least `bytes + 1` bytes at the
    /// offset.
    WriteStr(u64, i64, String),
    /// Allocate `size` bytes (zeroed) and register the resulting
    /// pointer in the HandleRegistry with `libc::free` as its
    /// drop fn. Result is `IntResult(handle)`. The handle's
    /// memory is freed when CloseHandle(handle) fires OR when
    /// the runtime exits.
    Malloc(i64),
    /// Monotonic clock in nanoseconds since an arbitrary epoch.
    /// `Time` returns Unix-epoch ms (subject to NTP jumps); this
    /// returns a strictly-increasing nanosecond counter suitable
    /// for benchmarking and rate-limiting. Result is `IntResult`.
    MonotonicTime,
    /// Register an Evident claim as a C-callable function. The
    /// runtime builds a libffi closure that, when invoked from C,
    /// dispatches the named claim with the unmarshalled args and
    /// returns the claim's computed value.
    ///
    /// Result is `IntResult(handle)` — the handle's pointer is
    /// the C function address. Pass it to C via `ArgHandle(h)`.
    ///
    /// **NOT YET IMPLEMENTED.** Dispatch currently returns an
    /// Error. Implementing this needs: libffi closure setup, a
    /// thread-safe path for C-thread → scheduler dispatch,
    /// decisions on whether the callback body may emit Effects.
    /// See `docs/design/ffi-os-evolution.md` § Tier 4 for the
    /// design and constraints.
    RegisterCallback(String, String),
}

/// One field of a packed C struct passed through `ArgPackedBuf`.
/// Each variant carries its natural-width value; the marshaller
/// writes the bytes contiguously in declaration order. The runtime
/// has no opinion about which library or struct a sequence of
/// `PackedField`s is destined for — the stdlib wrapper that declares
/// the foreign function knows the layout and emits a matching field
/// sequence.
///
/// Extend as new bridges need new widths. Today's set covers
/// SDL_Vertex (`[F32, F32, U8, U8, U8, U8, F32, F32]`). A future
/// GL_Float vertex array would add F32 only; an audio s16 buffer
/// would add I16; etc.
#[derive(Debug, Clone, PartialEq)]
pub enum PackedField {
    U8(u8),
    I32(i32),
    F32(f32),
}

impl PackedField {
    /// Append this field's bytes (little-endian, natural width) to
    /// `out`. Used by the FFI marshaller to build a contiguous heap
    /// buffer for `ArgPackedBuf`.
    pub fn write_le(&self, out: &mut Vec<u8>) {
        match self {
            PackedField::U8(b)  => out.push(*b),
            PackedField::I32(n) => out.extend_from_slice(&n.to_le_bytes()),
            PackedField::F32(f) => out.extend_from_slice(&f.to_le_bytes()),
        }
    }
}

/// One argument to an FFICall effect. Distinct name from
/// `ffi::FfiArg` to avoid the cross-module type clash; the
/// dispatcher converts when handing off to libffi.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectFfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
    /// `ArgStrArr(StrList)` — array of strings, marshalled as
    /// `const char * const *`. Needed for `glShaderSource` and any
    /// other API that wants a multi-string buffer.
    StrArr(Vec<String>),
    /// `ArgPriorResult(N)` — within an `Effect::Seq`, refers to the
    /// Nth prior call's result. Resolved at marshal time to a typed
    /// FfiArg matching the result's variant (Handle/Int/Bool/Str/
    /// Real). Index is local to the enclosing Seq (0 = first call's
    /// result). Out of range → error.
    PriorResult(usize),
    /// Pack N i32s into a contiguous heap buffer, pass its pointer
    /// (`p` slot). Used for fixed-layout homogeneous-int32 structs:
    /// `SDL_Rect { x, y, w, h }` (4 i32s = 16 bytes) and
    /// `SDL_Point { x, y }` (2 i32s = 8 bytes). The buffer lives for
    /// the duration of the call only — C side must not retain it.
    I32Buf(Vec<i32>),
    /// Pack a sequence of `PackedField`s into a contiguous heap
    /// buffer (each field at its natural width, little-endian) and
    /// pass the pointer (`p` slot). The stdlib wrapper that declares
    /// the foreign function chooses the field sequence to match the
    /// C struct layout. Replaces what would otherwise be one
    /// runtime variant per packed-struct shape.
    PackedBuf(Vec<PackedField>),
    /// `ArgIntOut` — 0-arity marker for "writes one i32 into the
    /// pointed-to slot" output args (`glGenVertexArrays(1, &vao)`,
    /// `glGetShaderiv(...)`, etc.). The dispatcher allocates a
    /// stable i32, passes its pointer, then surfaces the read-back
    /// value as the call's `IntResult` (replacing the function's
    /// void return).
    IntOut,
}

/// Outcome of one performed effect. Position-aligned with the
/// previous step's effect list.
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
