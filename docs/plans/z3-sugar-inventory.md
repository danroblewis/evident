# BuildZ3* sugar inventory — what the compiler pivot needs

Companion to the "build Z3 model, ask Z3 to serialize" architecture
direction. Today the compiler builds SMT-LIB strings by `++`
concatenation; the pivot replaces every text construction with a
libz3 API call that builds a Z3_ast handle in memory, and the
final emit step asks Z3 to stringify (`Z3_ast_to_string` /
`Z3_solver_to_string`).

This document inventories the Z3 C API surface the compiler needs
and proposes the corresponding `BuildZ3*` claim signatures in
`stdlib/kernel.ev`. Each claim is one `LibCall("libz3", "<fn>", …)`
wrapper. Adding a claim is additive — no risk to existing tests.

## What's already in stdlib/kernel.ev (21 claims)

Lifecycle: `BuildZ3MkConfig`, `MkContext`, `DelConfig`, `DelContext`,
`MkSolver`.

Parse / extract: `BuildZ3ParseSmtlib2String`, `AstVectorSize`,
`AstVectorGet`, `SolverAssert`, `SolverCheck`.

AST introspection (wave 5c): `BuildZ3GetAstKind`, `ToApp`,
`GetAppDecl`, `GetDeclKind`, `GetAppNumArgs`, `GetAppArg`.

Tactic API (wave 5c): `BuildZ3MkGoal`, `GoalAssert`, `MkTactic`,
`TacticAndThen`, `TacticApply`.

## What the compiler still needs (grouped)

Each row: Z3 C function — proposed claim signature — note.

### 1. Sorts (the type system)

| Z3 fn                      | Claim                                                            | Note |
| -------------------------- | ---------------------------------------------------------------- | ---- |
| `Z3_mk_int_sort`           | `BuildZ3MkIntSort(ctx_h ∈ Int, eff ∈ Effect)`                   | nullary; cache the result |
| `Z3_mk_bool_sort`          | `BuildZ3MkBoolSort(ctx_h ∈ Int, eff ∈ Effect)`                  | nullary; cache |
| `Z3_mk_real_sort`          | `BuildZ3MkRealSort(ctx_h ∈ Int, eff ∈ Effect)`                  | nullary; cache |
| `Z3_mk_string_sort`        | `BuildZ3MkStringSort(ctx_h ∈ Int, eff ∈ Effect)`                | nullary; cache |
| `Z3_mk_array_sort`         | `BuildZ3MkArraySort(ctx_h, idx_sort, val_sort ∈ Int, eff ∈ Effect)` | for `(Array Int Effect)` etc. |
| `Z3_mk_seq_sort`           | `BuildZ3MkSeqSort(ctx_h, elt_sort ∈ Int, eff ∈ Effect)`         | Z3 sequence theory |
| `Z3_mk_string_symbol`      | `BuildZ3MkStringSymbol(ctx_h ∈ Int, name ∈ String, eff ∈ Effect)` | for naming constants / decls |

### 2. Constants / function decls

| Z3 fn                      | Claim                                                            | Note |
| -------------------------- | ---------------------------------------------------------------- | ---- |
| `Z3_mk_const`              | `BuildZ3MkConst(ctx_h, sym_h, sort_h ∈ Int, eff ∈ Effect)`      | declare-fun () Sort |
| `Z3_mk_func_decl`          | `BuildZ3MkFuncDecl(ctx_h, sym_h, domain_arr_p, domain_len, range_sort ∈ Int, eff ∈ Effect)` | declare-fun with args; domain array via __mem |
| `Z3_mk_app`                | `BuildZ3MkApp(ctx_h, decl_h, args_arr_p, n_args ∈ Int, eff ∈ Effect)` | (decl args…); args array via __mem write_long loop |
| `Z3_mk_numeral`            | `BuildZ3MkNumeral(ctx_h ∈ Int, num ∈ String, sort_h ∈ Int, eff ∈ Effect)` | string form to support arbitrary precision |
| `Z3_mk_int`                | `BuildZ3MkInt(ctx_h, n, sort_h ∈ Int, eff ∈ Effect)`             | small-int convenience |
| `Z3_mk_true` / `Z3_mk_false` | `BuildZ3MkTrue(ctx_h ∈ Int, eff ∈ Effect)` / `BuildZ3MkFalse` | nullary |
| `Z3_mk_string`             | `BuildZ3MkString(ctx_h ∈ Int, s ∈ String, eff ∈ Effect)`        | for string literals |

### 3. Arithmetic (translate_arith.ev — SUBAGENT TERRITORY this session)

| Z3 fn                                                                                                       | Claim |
| ---------------------------------------------------------------------------------------------------------- | ----- |
| `Z3_mk_add` `Z3_mk_sub` `Z3_mk_mul`                                                                          | `BuildZ3MkAdd/Sub/Mul(ctx_h, args_arr_p, n_args ∈ Int, eff ∈ Effect)` |
| `Z3_mk_div` `Z3_mk_mod` `Z3_mk_rem` `Z3_mk_power`                                                            | `BuildZ3MkDiv/Mod/Rem/Power(ctx_h, l, r ∈ Int, eff ∈ Effect)`   |
| `Z3_mk_unary_minus`                                                                                          | `BuildZ3MkUnaryMinus(ctx_h, x ∈ Int, eff ∈ Effect)`             |
| `Z3_mk_lt` `Z3_mk_gt` `Z3_mk_le` `Z3_mk_ge`                                                                  | `BuildZ3MkLt/Gt/Le/Ge(ctx_h, l, r ∈ Int, eff ∈ Effect)` |

### 4. Boolean (covers translate_bool.ev)

| Z3 fn                | Claim                                                            | Note |
| -------------------- | ---------------------------------------------------------------- | ---- |
| `Z3_mk_eq`           | `BuildZ3MkEq(ctx_h, l, r ∈ Int, eff ∈ Effect)`                 | sort-polymorphic |
| `Z3_mk_distinct`     | `BuildZ3MkDistinct(ctx_h, args_arr_p, n_args ∈ Int, eff ∈ Effect)` | variadic |
| `Z3_mk_not`          | `BuildZ3MkNot(ctx_h, x ∈ Int, eff ∈ Effect)`                   | |
| `Z3_mk_and`/`Z3_mk_or` | `BuildZ3MkAnd/Or(ctx_h, args_arr_p, n_args ∈ Int, eff ∈ Effect)` | variadic |
| `Z3_mk_xor` `Z3_mk_iff` `Z3_mk_implies` | `BuildZ3MkXor/Iff/Implies(ctx_h, l, r ∈ Int, eff ∈ Effect)` | 2-arity |
| `Z3_mk_ite`          | `BuildZ3MkIte(ctx_h, cond, then_h, else_h ∈ Int, eff ∈ Effect)` | covers translate_ternary.ev |

### 5. Arrays (covers translate_seq.ev's Array+len encoding)

| Z3 fn              | Claim                                                            |
| ------------------ | ---------------------------------------------------------------- |
| `Z3_mk_select`     | `BuildZ3MkSelect(ctx_h, arr, idx ∈ Int, eff ∈ Effect)`         |
| `Z3_mk_store`      | `BuildZ3MkStore(ctx_h, arr, idx, val ∈ Int, eff ∈ Effect)`     |
| `Z3_mk_const_array` | `BuildZ3MkConstArray(ctx_h, idx_sort, val ∈ Int, eff ∈ Effect)` |

### 6. Sequences / strings (covers translate_seq.ev, translate_string.ev, translate_concat.ev)

| Z3 fn                  | Claim                                                                 |
| ---------------------- | --------------------------------------------------------------------- |
| `Z3_mk_seq_empty`      | `BuildZ3MkSeqEmpty(ctx_h, seq_sort ∈ Int, eff ∈ Effect)`            |
| `Z3_mk_seq_unit`       | `BuildZ3MkSeqUnit(ctx_h, x ∈ Int, eff ∈ Effect)`                    |
| `Z3_mk_seq_concat`     | `BuildZ3MkSeqConcat(ctx_h, args_arr_p, n_args ∈ Int, eff ∈ Effect)` |
| `Z3_mk_seq_length`     | `BuildZ3MkSeqLength(ctx_h, s ∈ Int, eff ∈ Effect)`                  |
| `Z3_mk_seq_at`         | `BuildZ3MkSeqAt(ctx_h, s, i ∈ Int, eff ∈ Effect)`                   |
| `Z3_mk_seq_nth`        | `BuildZ3MkSeqNth(ctx_h, s, i ∈ Int, eff ∈ Effect)`                  |
| `Z3_mk_seq_extract`    | `BuildZ3MkSeqExtract(ctx_h, s, off, len ∈ Int, eff ∈ Effect)`       |
| `Z3_mk_seq_prefix` / `_suffix` / `_contains` / `_index` | `BuildZ3MkSeqPrefix/Suffix/Contains/Index(...)` |
| `Z3_mk_int_to_str` / `_str_to_int` | `BuildZ3MkIntToStr/StrToInt(ctx_h, x ∈ Int, eff ∈ Effect)` |

### 7. Datatypes (covers translate.ev's enum→declare-datatypes + translate_ctor.ev)

This is the spikiest section because Z3's datatype API uses
out-arrays of constructors and returns recognizer/accessor decls
through more out-arrays. The full API:

| Z3 fn                              | Claim                                                                          | Note |
| ---------------------------------- | ------------------------------------------------------------------------------ | ---- |
| `Z3_mk_constructor`                | `BuildZ3MkConstructor(ctx_h, name_sym, recognizer_sym, n_fields, field_names_arr_p, field_sorts_arr_p, field_sort_refs_arr_p ∈ Int, eff ∈ Effect)` | one variant; need three i64 arrays via __mem |
| `Z3_mk_datatypes` (multi-sort)     | `BuildZ3MkDatatypes(ctx_h, n_sorts, sort_names_arr_p, sort_out_arr_p, ctor_lists_arr_p ∈ Int, eff ∈ Effect)` | for mutual recursion; multiple out arrays |
| `Z3_query_constructor`             | `BuildZ3QueryConstructor(ctx_h, ctor_h, n_fields, ctor_decl_out_p, recognizer_decl_out_p, accessor_decls_arr_p ∈ Int, eff ∈ Effect)` | to retrieve recognizer + accessor decls per variant |
| `Z3_del_constructor`               | `BuildZ3DelConstructor(ctx_h, ctor_h ∈ Int, eff ∈ Effect)`                    | cleanup |

These rows ALL need `__mem` write_long loops to build the input
arrays, and `__mem` read_long to harvest the output arrays. The
wave-5b __mem primitive covers it; this is the same shape as G2-5b's
ffi_prep_cif. Document the malloc + write pattern as a sub-claim
helper (e.g. `BuildAllocAndWriteHandles(handles ∈ Seq(Int))` →
pointer).

### 8. Recognizers / accessors (datatype use, covers translate_match.ev)

Once `Z3_query_constructor` has retrieved a recognizer decl and
accessor decls, they're applied via `Z3_mk_app` (§2). So no new
sugar — translate_match.ev composes `MkApp(recognizer_decl, [val])`
to get `((_ is Ctor) val)`, and `MkApp(accessor_decl, [val])` to
project a field.

### 9. Quantifiers (covers translate_quant.ev)

| Z3 fn                  | Claim                                                                                                    | Note |
| ---------------------- | -------------------------------------------------------------------------------------------------------- | ---- |
| `Z3_mk_bound`          | `BuildZ3MkBound(ctx_h, index, sort_h ∈ Int, eff ∈ Effect)`                                              | de Bruijn index |
| `Z3_mk_forall_const`   | `BuildZ3MkForallConst(ctx_h, weight, num_bound, bound_consts_arr_p, num_patterns, patterns_arr_p, body ∈ Int, eff ∈ Effect)` | the const-bound form is friendlier than the de-Bruijn form |
| `Z3_mk_exists_const`   | `BuildZ3MkExistsConst(...)`                                                                              | same shape |

### 10. Output / serialization (the emit step)

| Z3 fn                  | Claim                                                                          | Note |
| ---------------------- | ------------------------------------------------------------------------------ | ---- |
| `Z3_ast_to_string`     | returns `const char*` — needs C-string → Evident-String marshaling             | KEY: add `__cstr.copy` pseudo-library in kernel/src/libcall.rs (see §11) |
| `Z3_solver_to_string`  | returns `const char*` for a whole solver's assertions                          | same marshaling |
| `Z3_solver_assert_and_track` |                                                                          | tracking lables for unsat cores; future |
| `Z3_set_ast_print_mode` | `BuildZ3SetAstPrintMode(ctx_h, mode ∈ Int, eff ∈ Effect)`                    | for `Z3_PRINT_SMTLIB2_COMPLIANT` mode |

### 11. Kernel-side helper needed: `__cstr.copy`

`Z3_ast_to_string` and `Z3_solver_to_string` return `const char*`.
The kernel's libcall path returns `i64` (a pointer). The wave-5a
plan flagged this as one of two named blockers (B2 — char*→Evident
String marshaling).

Proposed kernel addition (small, additive, peer to `__mem` and
`__dlsym`):

```rust
fn cstr_call(fn_name: &str, args: &[LibArg]) -> Result<i64, String> {
    match fn_name {
        // Returns the StringResult containing the contents of the
        // C string at addr, copied into a heap Evident String.
        "copy" => { let addr = ...; let s = CStr::from_ptr(addr).to_str(); ... }
        // Optional: length-only probe.
        "len"  => { let addr = ...; let n = strlen(addr); Ok(n as i64) }
        _ => Err(...)
    }
}
```

But the return shape of `__cstr.copy` differs from `__mem.read_long`:
this one needs to return a `StringResult` so the FSM can capture
the value as an Evident String. Two options:

  - **(a) Kernel returns `StringResult(s)` automatically when the
    pseudo-lib is named `__cstr` and fn is `copy`.** Cleanest. The
    dispatch site has to know to return Str instead of Int.
  - **(b) Add a new Result variant `StringResultFromC(...)`.** Avoids
    overloading meaning of the existing `StringResult`. Cost: every
    FSM that captures a C string match-destructures the new variant.

Recommend (a). Implement by branching at the libcall dispatch in
tick.rs: when `lib_name == "__cstr"`, build `Res::Str` instead of
`Res::Int`. One ~10-line change.

### 12. Refcount discipline

Every Z3_mk_* returns an AST with refcount 0. Without immediate
`Z3_inc_ref` the next libz3 call may GC it (wave-5a learned this
the hard way). The standard claim shape:

```
BuildZ3IncRef(ctx_h, ast_h ∈ Int, eff ∈ Effect)
BuildZ3DecRef(ctx_h, ast_h ∈ Int, eff ∈ Effect)
```

Already partly present: `Z3_solver_inc_ref`, `Z3_ast_vector_inc_ref`,
`Z3_inc_ref` are used in tests/kernel/wave-5a/z3_solve_x42.smt2.
Add wrappers for them in stdlib/kernel.ev.

Convention: every BuildZ3Mk* sugar that returns a handle assumes
the caller will inc_ref one tick after capture. Document this in
stdlib/kernel.ev's header comment.

### 13. NOT needed (kept as text for now)

- The `;; manifest:` header lines — these are the kernel's wire
  contract, not part of the Z3 model. Keep emitting them as text.
- `(push)` / `(pop)` block delimiters in sample.smt2's per-claim
  blocks. Z3 has push/pop methods we could use, but the current
  shape works and the savings aren't load-bearing.
- Comments like `;; claim: <name>`. Text.

## Implementation order

1. **Lifecycle + emit (§1, §2, §10, §11, §12).** Without `__cstr.copy`
   we can't see the output at all; without sorts + const-decl we
   can't build anything. ~10 claims + the kernel cstr addition.

2. **Arithmetic + bool (§3, §4).** Smallest translate file (arith)
   first — this is what the subagent is working on this session.

3. **Sequences / strings (§5, §6).** Sample.smt2's `(seq.unit …)` /
   `(__Cell_LibArg …)` shapes need this.

4. **Datatypes (§7, §8).** The big one. Wide API surface (mutual
   recursion via `Z3_mk_datatypes`, out-array marshaling). Plan
   on the spike.

5. **Quantifiers (§9).** Last because translate_quant.ev is the
   smallest user.

## File-by-file pivot impact (excluding translate_arith.ev)

| File                          | Today's ops              | New BuildZ3* calls |
| ----------------------------- | ------------------------ | ------------------ |
| translate_bool.ev             | and, or, not, ite, =, ≠  | §4 |
| translate_ternary.ev          | ite                      | §4 (MkIte alone) |
| translate_record.ev           | and + accessor app       | §4 + §8 (MkApp on accessor decl) |
| translate_ctor.ev             | datatype variant app + seq cons | §2 (MkApp) + §6 (seq) |
| translate_match.ev            | nested ite + (_ is Ctor) + accessor app | §4 + §7 + §8 |
| translate_seq.ev              | (Array Int T)+__len, seq.unit, seq.++ | §5 + §6 |
| translate_string.ev           | str.substr, str.len, str.prefixof, str.++ | §6 |
| translate_concat.ev           | seq.++ over Seq(...)     | §6 |
| translate_quant.ev            | forall, exists           | §9 |
| translate_compose.ev          | (no Z3 emit; routes between passes) | n/a |
| translate_declare.ev          | (declare-fun name () T)  | §2 (MkConst) + needs Solver to host it (next §) |
| translate_generics.ev         | (string substitution; not Z3-shaped) | n/a |
| translate_infer.ev            | (text analysis; not Z3-shaped) | n/a |
| translate_manifest.ev         | (manifest text; kept as text per §13) | n/a |
| translate.ev (enum emit)      | declare-datatypes        | §7 (MkConstructor + MkDatatypes + QueryConstructor) |

## Cross-cutting: where do top-level assertions go?

The compiler currently outputs each assertion as `"(assert " ++ body ++ ")"`.
With the pivot, every body becomes a `Z3_ast` handle. We then need
either:
- a single `Z3_solver`, and we call `Z3_solver_assert` per top-level
  assertion, then ask `Z3_solver_to_string` at the end; OR
- accumulate ASTs into a `Z3_ast_vector`, conjoin with `Z3_mk_and`,
  and stringify the whole thing.

Recommend the Solver path — it gives push/pop discipline for the
sample.smt2 per-claim blocks for free (`Z3_solver_push` /
`Z3_solver_pop`).

| Z3 fn                  | Claim                                                  |
| ---------------------- | ------------------------------------------------------ |
| `Z3_solver_push`       | `BuildZ3SolverPush(ctx_h, sol_h ∈ Int, eff ∈ Effect)` |
| `Z3_solver_pop`        | `BuildZ3SolverPop(ctx_h, sol_h, n_levels ∈ Int, eff ∈ Effect)` |
| `Z3_solver_reset`      | `BuildZ3SolverReset(ctx_h, sol_h ∈ Int, eff ∈ Effect)` |

Combined with `Z3_solver_to_string` (§10) this gives sample.smt2's
push/pop block shape directly, no text concatenation.

## Summary

Total new claims needed: roughly **55 sugar claims** in
stdlib/kernel.ev across §§1-10, plus **one ~10-line kernel
addition** (`__cstr` pseudo-library in kernel/src/libcall.rs).

The work is mechanical once §11's C-string marshaling lands — every
claim is one `LibCall("libz3", "<fn>", ⟨ArgInt(…)⟩)` + a header
comment naming the Z3 function it wraps. The hard parts are §7
(datatypes) and the out-array marshaling pattern in
§2's `Z3_mk_func_decl` / `Z3_mk_app`.
