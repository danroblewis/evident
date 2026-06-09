# Wave 5a — feasibility: Z3 wrapper in Evident (FFI to libz3)

**Verdict: MEDIUM, split.** The *solve* half (parse SMT-LIB → assert
→ check → read sat int) is HIGH. The *model-readback* half (decode
the model AST into effect values — the bulk of `kernel/src/tick.rs`)
is BLOCKED on two named capabilities. The functionizer Z3 use is
separable JIT-only code, out of an MVP's scope.

Diagnostic only. No `kernel/` / `compiler/` / `stdlib/` edits.

Cites: `CLAUDE.md` (kernel spec, FFI floor, `__mem`),
`legacy-python/docs/fti-z3.md` (the canonical Z3-via-LibCall design),
`kernel/src/libcall.rs`, `kernel/src/tick.rs`,
[[reference-z3-cross-parse-interning]],
[[project-smtlib-compile-target]], [[feedback-aot-over-runtime-disk-cache]],
[[project-constraint-model-compilation]].

---

## Section 1 — Z3 API surface today

`grep -rE 'Z3_[A-Z][a-z]' kernel/src/` → ~70 distinct functions in
4 files. All in `kernel/src/`: `tick.rs` (core), `functionize/{mod,jit,eval}.rs`.
Grouped by role (refcount behavior noted; `ctx` is first arg of nearly all):

**A. Lifecycle / solve — `tick.rs:139-490, 600-715`**
| fn | sig (→ ret) | refs |
|---|---|---|
| `Z3_mk_config` | `() → Z3_config` | — |
| `Z3_mk_context` | `(Z3_config) → Z3_context` | owns all |
| `Z3_del_config` / `Z3_del_context` | `(h) → void` | frees subtree |
| `Z3_parse_smtlib2_string` | `(ctx, str, n,sym*,sort*, n,sym*,decl*) → Z3_ast_vector` | — |
| `Z3_mk_solver` | `(ctx) → Z3_solver` | needs inc_ref |
| `Z3_solver_inc_ref`/`dec_ref` | `(ctx, solver) → void` | **refcount** |
| `Z3_solver_assert` | `(ctx, solver, ast) → void` | — |
| `Z3_solver_check` | `(ctx, solver) → Z3_lbool` (i32: 1/0/−1) | — |
| `Z3_solver_check_assumptions` | `(ctx, solver, n, ast[]) → Z3_lbool` | array arg |
| `Z3_solver_get_model` | `(ctx, solver) → Z3_model` | needs inc_ref |
| `Z3_ast_vector_{inc,dec}_ref`/`size`/`get` | `(ctx, vec[,i]) → void/unsigned/ast` | **refcount** |
| `Z3_model_{inc,dec}_ref` | `(ctx, model) → void` | **refcount** |
| `Z3_get_error_code`/`Z3_get_error_msg` | `(ctx) → i32` / `(ctx, code) → Z3_string` | — |

**B. Model readback + AST introspection — `tick.rs:412-908, 1067, 1172`**
`Z3_model_eval(ctx, model, ast, bool, *Z3_ast out) → bool` (**out-pointer**),
`Z3_get_ast_kind → i32 enum`, `Z3_to_app`, `Z3_get_app_decl`,
`Z3_get_decl_kind → i32`, `Z3_get_app_num_args → unsigned`,
`Z3_get_app_arg(ctx,app,i) → Z3_ast` (child), `Z3_get_decl_name → Z3_symbol`,
`Z3_get_symbol_{string,kind,int}`, `Z3_get_string(ctx,ast) → Z3_string`
(**char\***), `Z3_is_string`, `Z3_get_numeral_int(ctx,ast,*out) → bool`
(**out-ptr**), `Z3_get_sort`/`Z3_get_sort_kind`/`Z3_get_sort_name`,
`Z3_ast_to_string → Z3_string` (**char\***), `Z3_get_numerator`/`denominator`,
`Z3_get_bool_value`, `Z3_get_domain_size`. Plus probe-AST builders
`Z3_mk_app`/`mk_select`/`mk_int`/`mk_int_sort`/`mk_not`/`simplify`.

**C. Functionizer / JIT (separable) — `functionize/mod.rs:428-458` etc.**
`Z3_mk_goal`, `Z3_goal_{inc,dec}_ref`/`assert`/`size`/`formula`,
`Z3_mk_tactic`, `Z3_tactic_{inc,dec}_ref`/`and_then`/`apply`,
`Z3_apply_result_*`, `Z3_get_datatype_sort_*`. Used only by the
Cranelift/interp fast path; not on the minimal solve loop.

---

## Section 2 — LibCall feasibility per signature shape

`libcall.rs` today: arg grammar `ArgInt(i64)`/`ArgStr(String)`/`ArgReal(f64)`,
**return assumed `i64`** (whatever the integer register holds), dlopen
cached per-lib. Pointer returns already come back as an `i64` handle
(`libcall.rs:6-8`). Library named by string ⇒ `LibCall("libz3", …)`
"just works" once `libz3.dylib` is on the dlopen path.

- **Easy (scalar→scalar).** `Z3_solver_check`, `Z3_get_ast_kind`,
  `Z3_get_decl_kind`, `Z3_get_app_num_args`, `Z3_solver_assert`,
  `Z3_*_inc_ref`/`dec_ref`. Handle in (ArgInt), int/void out. **Direct.**
- **Pointer.** `Z3_mk_context`, `Z3_mk_solver`, `Z3_parse_smtlib2_string`,
  `Z3_to_app`, `Z3_get_app_decl`, `Z3_get_app_arg`, … Pointers marshal
  as opaque `ArgInt` handles (i64 == ptr width on 64-bit — the basis of
  the whole `fti-z3.md` design). Return handle recovered as `IntResult`.
  **Feasible**; needs `BuildZ3*` sugar (§4).
- **Struct / array / out-pointer (hard).** `Z3_model_eval(…,*out)` and
  `Z3_get_numeral_int(…,*out)` write through an output pointer — LibCall
  has no `&mut out`. Workaround exists: `LibCall("libc","malloc",…)` →
  pass the slot address as `ArgInt` → `__mem` `read_long` it back
  (`libcall.rs:159-189`). Ceremonial but real. `parse_smtlib2_string`'s
  sort/decl arrays are passed as `0, null` in practice (`tick.rs:158`) ⇒
  no array marshaling needed. `solver_check_assumptions`'s `ast[]` needs
  a contiguous handle array (malloc+`write_long` loop). **~4 sites.**
- **char\* return (hard).** `Z3_get_string`, `Z3_ast_to_string`,
  `Z3_get_error_msg` return `const char*`. LibCall returns the pointer
  as i64 but has **no char\*→Evident-String marshaling** (only built-in
  `puts`/`ReadFile` produce strings). `__mem read_long` reads 8 bytes;
  there is no strlen/per-byte read. **~3 sites, no workaround today.**
- **Callback.** None. Z3 supports a `Z3_set_error_handler` fn-pointer
  but the kernel doesn't register one — it polls `Z3_get_error_code`.
  **0 essential callbacks.** (One fewer blocker than feared.)

---

## Section 3 — Reference counting

Z3 is manually refcounted; the kernel mirrors it in Rust Drop
(`tick.rs` pairs every `*_inc_ref` with a `*_dec_ref` on each exit
path: lines 322-331, 387-408, 460-490). Evident has no destructor.

Three models, recommended in order:

1. **Leak-until-process-exit (recommended for v1).** Compiler runs are
   single-shot; `Z3_del_context` at the very end frees the whole subtree
   regardless of intermediate refs. Drop every `inc_ref`/`dec_ref` from
   the Evident port; emit one `BuildZ3DelContext` in the FSM's terminal
   state. Matches `fti-z3.md` "Open question 1" and the
   [[feedback-aot-over-runtime-disk-cache]] "optimize runtime not setup"
   priority. Bounded memory = one model's worth.
2. **Explicit `BuildZ3DecRef` at scope end.** Faithful but the FSM author
   must thread the handle and emit the dec at the right tick — error-prone,
   no compiler help. Defer.
3. **Ref-counted region claim** (lifetime in the FSM structure). Cleanest
   long-term but needs a scope/region language feature that doesn't exist.
   Out of scope.

---

## Section 4 — Sample claim (illustration only — NOT written to stdlib)

The current `Effect` is `LibCall(String, String, Seq(LibArg))` with the
return recovered position-aligned from `last_results` on the **next** tick:

```evident
-- ILLUSTRATION. ctx_h is a Z3_context handle (Int) recovered from a
-- prior tick's last_results. The new solver handle arrives next tick
-- as IntResult at this effect's index in last_results.
claim BuildZ3MkSolver(ctx_h ∈ Int, eff ∈ Effect)
    eff = LibCall("libz3", "Z3_mk_solver", ⟨ArgInt(ctx_h)⟩)

claim BuildZ3MkContext(cfg_h ∈ Int, eff ∈ Effect)
    eff = LibCall("libz3", "Z3_mk_context", ⟨ArgInt(cfg_h)⟩)

claim BuildZ3SolverCheck(ctx_h, solver_h ∈ Int, eff ∈ Effect)
    eff = LibCall("libz3", "Z3_solver_check", ⟨ArgInt(ctx_h), ArgInt(solver_h)⟩)
```

This works for **lifecycle** because each handle is consumed one tick
after it is produced. It does **not** work for model-readback, where a
single decode is "call → branch on the returned kind → call children" —
all within one tick. That needs the `ArgRef`/tick-local scratchpad
extension from `fti-z3.md` §"Sub-problem 1" (Option B), which the current
`libcall.rs` does not have. The legacy `Effect.LibCall` carried `ok_dest`
+ `ArgRef` for exactly this; the live kernel's grammar dropped both.

---

## Section 5 — Verdict + roadmap

**MEDIUM, split.** *Why:* every libz3 function is reachable by name via
the existing dlopen/libffi path, and pointer handles already round-trip
as i64 — so the lifecycle/solve surface (parse → assert → check → sat-int)
ports today with only `BuildZ3*` sugar and a leak-until-exit refcount
model. But the model-**readback** half — which is most of `tick.rs`'s Z3
calls — is an inherently intra-tick "call, branch on the returned handle,
recurse over children" walk, and the live LibCall grammar has neither the
within-tick handle chaining nor the `char*`→String marshaling that walk
requires. So a *Z3-as-library FTI* (programs that build & check their own
auxiliary models — the `fti-z3.md` use case) is HIGH-feasibility now; a
full replacement of the kernel's own Z3 wrapper is BLOCKED.

**Roadmap for the tractable half (Z3-as-library FTI):**
1. **Sugar wave.** Add `BuildZ3MkConfig/MkContext/MkSolver/SolverAssert/`
   `SolverCheck/DelContext` to `stdlib/kernel.ev`, all emitting plain
   `LibCall("libz3", …)`. Add a conformance feature spec that drives a
   2-tick solve of `x = 42` and reads the sat int from `last_results`
   (mirrors `fti-z3.md` step 4). Pure stdlib + tests; no kernel change.
2. **Call-sites wave.** Express the formula-build path (the `Z3_mk_*`
   tree marshal from `fti-z3.md` §3) as an Evident FTI, asserting via
   the §1 step's recovered context handle. Decode the sat int → enum.
3. **Refcount + cleanup wave.** Add `BuildZ3DelContext` in the FTI's
   terminal state (leak-until-exit). No Rust removed yet — this *adds* a
   capability; nothing in `kernel/` is deleted.

**Blockers for the full kernel-Z3 removal (the readback/solve-loop half):**
- **B1 — intra-tick handle chaining.** A model-AST walk calls
  `get_ast_kind`, branches, then calls `get_app_arg` on the *same* tick.
  Resolved by the `ArgRef` + per-tick scratchpad runtime extension
  (`fti-z3.md` §"Sub-problem 1", Option B; ~30-80 LOC, a kernel change
  requiring user approval). Without it, decoding is one-AST-node-per-tick:
  data-dependent depth, hundreds of ticks per model — impractical.
- **B2 — `char*` → Evident String.** `Z3_get_string`/`ast_to_string`/
  `get_error_msg` return C strings; LibCall can't read them back. Needs
  either a LibCall string-return mode or a `__mem` strlen+copy primitive.
- **B3 — bootstrap circularity (the deep one).** The FSM execution model
  *is* solve-per-tick. An Evident "Z3 wrapper" compiles to SMT-LIB that
  itself needs Z3 to run. So the meta-level parse-SMT-LIB + solve that
  drives any tick is irreducibly native; the most Rust can shrink to is a
  trampoline + `Z3_parse_smtlib2_string` + `Z3_solver_check` + the model
  decode (B1/B2). libz3 itself is "kept" per the north star — what's
  removable is the *introspection/marshaling* glue, not the solve call.
  This bounds the ceiling: even with B1+B2 resolved, the kernel's Z3
  surface shrinks, it does not reach zero.

**Recommendation:** do the §-roadmap Z3-as-library FTI now (genuinely
useful, unblocks programs that solve; matches `fti-z3.md`). Treat the
kernel-Z3-removal as gated on B1+B2 and scoped by B3 — a separate,
user-approved kernel-extension wave, not a transcription wave.

---

## Status update (2026-06-09): B1 + B2 LANDED — the readback half is open

- **B2 (char* → String)** had already landed as the `__cstr.copy(ptr) →
  StringResult` pseudo-library in `kernel/src/libcall.rs`.
- **B1 (intra-tick handle chaining)** is now landed as `ArgRef(Int)`:
  a 4th `LibArg` variant (stdlib/kernel.ev) resolving to the result of
  effects[k] dispatched earlier in the SAME tick (same indexing as next
  tick's last_results). The dispatcher resolves refs against its per-tick
  results vec on both the Z3 and functionized paths; a forward/out-of-range
  ref or a ref to a No/Eof/Error result yields an ErrorResult (the same
  observable failure as any failed LibCall) — never a crash. The frozen
  oracle compiles the 4-variant enum from source unchanged.
- **Bonus over the plan:** B1 + `__mem` covers OUT-POINTER signatures
  too, which §5 didn't anticipate —
  `⟨malloc(8), Z3_model_eval(…, ArgRef(slot)), read_long(ArgRef(slot))⟩`
  crosses `Z3_ast*`/`int64_t*` outs without any new kernel shape.
- **Proof:** `tests/kernel/test_argref_z3_readback.ev` — a complete
  model-readback walk in Evident (lifecycle ×4 chained in one tick;
  build+assert `x = 42` ×6 in one tick, exactly the §"Sub-problem 3"
  example; solver_check; get_model → model_eval → get_numeral_int64
  ×8 in one tick) exits with the model's value, 42. Companions:
  `test_argref_chain.ev` (the minimal malloc/write/read chain) and
  `test_argref_error.ev` (forward ref → ErrorResult, observable).
- **Still open:** the compiler2 stage-1 floor (DriverEnum/BuildEff)
  declares a hardcoded 3-variant LibArg, so user programs compiled VIA
  COMPILER2 can't use ArgRef until that floor grows the variant — a
  compiler2/*.ev follow-up. The B3 ceiling stands as written.
