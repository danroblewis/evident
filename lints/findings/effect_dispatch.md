# Findings: runtime/src/effect_dispatch.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None outside the documented exemptions in `lints/exemptions/AP-001.txt`
(SdlVertexBuf arms at lines 265, 363, 419; libSystem.dylib paths at
lines 693, 712, 797, 827).

Cross-checked against the file's invariants in
`lints/runtime-invariants.md`:

  * No `crate::translate`, `crate::runtime`, `crate::event_sources`,
    or `crate::fti` imports — only `crate::ast` and `crate::ffi`.
    Clean.
  * No `z3::*` use, no `Solver` references. Clean.
  * No global `static mut` / `lazy_static` / `OnceLock`. The only
    cross-call mutable state is `DispatchContext` fields, threaded
    through call args. Clean.
  * `Effect::SpawnFsm` (line 209-213) queues onto
    `DispatchContext::pending_spawns` rather than instantiating an
    FSM, matching the cross-file contract with `effect_loop`. Clean.

## Candidate new rules

### Suggested AP-009: duplicate-dispatch-arm-marshaling (review-only)

**Pattern observed at runtime/src/effect_dispatch.rs:256-283 vs
354-381:**

The `FFICall` and `LibCall` arms each contain the same 28-line
block: 12 lines of `EffectFfiArg → FfiArg` mapping, 5 lines of
`PriorResult` bail-out, and 9 lines of `FfiReturn → EffectResult`
mapping. The two blocks differ in exactly one token (`*fn_id` vs
`sym_handle`). The Replay arms below them (lines 285-311 vs
383-400) are similarly near-duplicates — same cursor-bounds /
symbol-mismatch / sig-mismatch / args-equal pattern.

**Why it might be bad:** Adding any new `EffectFfiArg` variant —
or changing the arg-marshal protocol — requires editing every
duplicated arm; it's easy to update one and miss the other.
The exempted `SdlVertexBuf` arms are already a 3-place edit
(line 265, 363, 419 — also `args_equal`). When the planned
`ArgByteBuf` refactor lands, the multi-place edit hits again.

**Suggested fix:** Extract `marshal_effect_args(args:
&[EffectFfiArg]) -> Result<Vec<FfiArg>, EffectResult>` and
`ffi_return_to_effect_result(r: FfiReturn) -> EffectResult`
helpers; both `FFICall` and `LibCall` call them. Same for the
Replay-mode validate-and-advance block:
`replay_consume(calls, cursor, name, sig, args) -> EffectResult`.

**Detection idea:** Review-only. A textual-similarity check on
adjacent match arms in `effect_dispatch.rs` would have too much
noise to mechanize cheaply. The pattern is narrow (only this
file has multiple effect arms calling into the same FFI surface)
so promoting to a full lint rule isn't worth the carrying cost.

### Suggested AP-010: silent-no-op-on-unsupported-effect-shape (review-only)

**Pattern observed at runtime/src/effect_dispatch.rs:330:**

> `Effect::Seq(_) => EffectResult::NoResult,`

If a caller invokes `dispatch_one` directly on an `Effect::Seq`,
the inner effects never fire and the caller silently gets
`NoResult` — exactly the success-without-side-effect shape. The
inline comment acknowledges the gotcha but ships it. The
`PriorResult → FfiArg::Int(0)` fallback at lines 268 and 366 is
the same family: unsupported input → silent default.

**Why it might be bad:** This is the same shape as the
"constraint silently dropped because the variable is unbound"
footgun documented in the project CLAUDE.md (the `True` vs
`true` bug). Returning `NoResult` for an unhandled structural
case looks identical at the call site to a successful
side-effect-free dispatch. A test asserting "FSM emitted Seq
and the print fired" would pass-then-fail when refactoring
moves the dispatch path.

**Suggested fix:** Either (a) make `dispatch_one` return
`Result<EffectResult, DispatchError>` and return an error for
shape violations callers must handle; (b) `panic!()` /
`debug_assert!` in the unreachable-by-design arms to surface
the bug at test time. Lower-impact: change the return to
`EffectResult::Error("Effect::Seq must be dispatched via
dispatch_all")` so a caller observing the result sees the
violation in stderr.

**Detection idea:** Review-only. Hard to mechanize without false
positives — many genuinely-empty effect arms are correct (e.g.
`Effect::NoEffect`).

### Suggested AP-011: dead-test-helper

**Pattern observed at runtime/src/effect_dispatch.rs:526-534:**

> ```rust
> fn captured_stdout(ctx: DispatchContext) -> String {
>     // The Box<dyn Write> can't be downcast; tests that need stdout
>     // capture should construct their own Vec<u8> and inspect it
>     // via a separate handle pattern. For simplicity these tests
>     // mostly verify the result, not the stdout bytes.
>     // (Returning empty here since we can't unwrap the Box.)
>     let _ = ctx;
>     String::new()
> }
> ```

Helper that takes a context, returns an empty string, and is
documented as non-functional. Called once (line 565) where its
return value is dropped via `let _ =`.

**Why it might be bad:** Dead test infrastructure rots —
future readers see the call and assume stdout-capture works,
and write a new test relying on the (broken) helper. The
honest move is to delete the helper and the call, OR fix the
stdin/stdout struct so capture is possible (e.g. wrap in
`Arc<Mutex<Vec<u8>>>` accessible to both the dispatch context
and the test).

**Suggested fix:** Delete the helper and its caller at line
565. If stdout-byte assertions are wanted later, build the
capture properly then.

**Detection idea:** Review-only. A grep for "Returning empty"
or "can't unwrap" in a function body would catch this specific
shape but is too narrow to be a general rule.
