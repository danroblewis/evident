# Phase 6.2.0: Seq-typed enum-variant payloads

## Goal

Let an enum variant carry a `Seq(T)`-typed payload field, e.g.
`enum Bag = Empty | OfInts(Seq(Int))`. Today every layer of the
pipeline rejects this: parser bails on the inner `(`, AST has no
Seq concept in payload types, and the Z3 Datatype builder has no
way to put a (Array, Int) pair into a constructor argument.

Phase 6.2 (FFI arg migration) cannot proceed without this — the
target shapes `Effect::FFICall(Int, String, Seq(FFIArg))`,
`ArgStrArr(Seq(String))`, `ArgI32Buf(Seq(Int))`,
`ArgPackedBuf(Seq(PackedField))` all put Seqs inside enum
payloads.

## Approach

For each `Seq(T)` field of an enum variant, build a **wrapper
Datatype** with one constructor and two accessors: `arr: Array(Int → T)`,
`len: Int`. The enum's main Datatype's variant takes the wrapper
Datatype as its field type. Reading the field gives the wrapper
value; calling its accessors yields the (arr, len) pair that
extract_seq already knows how to walk.

Wrapper datatypes are interned in a `SeqWrapperRegistry` keyed on
element-type-name so the same `Seq(Int)` field anywhere in an
enum payload reuses the same wrapper sort. Built lazily at
enum-load time when a Seq-typed field is first encountered.

## Files touched

- `runtime/src/parser.rs` — accept compound types
  (`Seq(...)`, `Set(...)`) in `parse_enum_decl`'s payload loop.
- `runtime/src/translate/types.rs` — add `FieldKind::Seq` (later;
  fields metadata is for struct fields, not enum payloads — may
  not need changes here).
- `runtime/src/runtime.rs` (enum loading) — when a variant field's
  type_name starts with `Seq(`, look up / build the wrapper
  Datatype and use it as the field's Z3 sort.
- `runtime/src/translate/exprs.rs` — when constructing an enum
  value with a Seq-typed field, pin the wrapper's accessors via
  the existing Seq-pinning path.
- `runtime/src/translate/extract.rs` + eval.rs's
  `extract_enum_value` — when a variant field is Seq-typed, unwrap
  the (arr, len) and produce `Value::SeqInt/Bool/Str/SeqEnum`
  inline.

## Acceptance

- `enum Bag = Empty | OfInts(Seq(Int))` parses cleanly.
- `b = OfInts(⟨1, 2, 3⟩)` translates to a satisfiable constraint.
- `query` returns `Value::Enum { variant: "OfInts", fields:
  [Value::SeqInt(vec![1,2,3])] }`.
- Same for `Seq(String)`, `Seq(Bool)`, `Seq(SomeOtherEnum)`.
- All existing tests pass (no regression).
