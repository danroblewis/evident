# Findings: runtime/src/ffi.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

Rule applicability:
- **AP-001** (no-library-specific-in-language-core): ffi.rs is in
  scope. Ran the rule's grep
  (`SDL_|Sdl[A-Z]|\bGl[A-Z]|Glsl|Audio[A-Z]|\.dylib|\.framework/|/opt/homebrew/lib/|/usr/lib/lib`).
  Hits at lines 61, 64, 65, 325, 393 are comment-only (doc /
  `//` lines) — exempt by the rule's own carve-out. Hits at
  lines 66, 326, 365, 394, 435 (the `SdlVertexBuf` cluster) and
  line 503 (`libSystem.dylib`) are listed in
  `lints/exemptions/AP-001.txt`. The Linux fallback `"libc.so.6"`
  on line 504 doesn't match the rule's regex (`.so` is not
  enumerated) and is the same libc-fallback intent as the
  exempted line above it. No NEW library names appear (no
  `gl[A-Z]`, no `Audio[A-Z]`, no second hardcoded dylib path
  beyond the libc one).
- **AP-002, AP-003, AP-006, AP-007, AP-008**: scoped to
  `examples/*.ev` — not applicable.
- **AP-004**: scoped to `tests/conformance/**.py` — not
  applicable.
- **AP-005**: scoped to `runtime/tests/**.rs` — not applicable
  (the in-file tests at lines 496–641 are a `#[cfg(test)] mod`
  inside the source file, not a `runtime/tests/` integration
  file; AP-005's `#[ignore]` ban applies regardless of location
  and grep finds no `#[ignore]` attribute here).

## Invariant compliance (per `lints/runtime-invariants.md`)

The brief for `ffi.rs` (Group 5) says:

> Pure libffi marshaling. Knows about C ABIs but never about
> specific libraries. Must NOT contain library-specific
> knowledge beyond what's already exempted. Must NOT hardcode
> dylib paths beyond callers' supplied args. Must NOT build Z3
> expressions or run the Solver. Must NOT schedule FSMs. Must
> NOT know about Effects (callers translate Effect args into
> FfiArgs). Zero crate-internal imports — pure leaf.

Verified, point by point:

- **No new library-specific knowledge.** The five `SdlVertexBuf`
  arms (66, 326, 365, 394, 435) are the documented exemption
  cluster; no other library identifier appears in
  non-comment code. No `SDL_*` function names, no `gl*`, no
  `Audio*`, no GL/Vulkan/CoreFoundation references.
- **No additional hardcoded dylib paths.** The only dylib /
  shared-object paths in the file are `libSystem.dylib` /
  `libc.so.6` on lines 503–504, both inside `#[cfg(test)]
  fn libc_path()` and used solely to exercise the FFI primitive
  itself against the host's libc. Production code paths
  (`ffi_open`, `ffi_lookup`, `ffi_call`) take the path/symbol
  from caller arguments — no defaults, no fallbacks.
- **No Z3, no Solver.** Grep for `z3` / `Solver` / `Sort` /
  `Datatype` / `Context` returns nothing. The file uses
  `libffi::middle::Type` (a libffi type code, not a Z3 sort)
  and that's the only `Type` it knows.
- **No FSM scheduling.** No reference to `FSM`, `tick`,
  `scheduler`, `EventSource`, `WriteQueue`, `LoopResult`,
  `subscriptions`, `effect_loop`, or any scheduler-side
  concept.
- **No Effect knowledge.** No `use crate::ast::Effect`. No
  reference to `Effect`, `EffectResult`, `EffectList`,
  `ResultList`, `dispatch`, or `DispatchContext`. The arg
  enum is `FfiArg` (the file's own type), not `EffectFfiArg` —
  callers in `effect_dispatch.rs` translate `EffectFfiArg →
  FfiArg` before calling in.
- **Crate-internal imports.** No `use crate::*` statements at
  the top of the file. The only crate-internal references are
  three inline-qualified `crate::ast::SdlVertex` paths at
  lines 66, 326, 394 — all in the documented `SdlVertexBuf`
  exemption cluster. The invariant text reads "Zero
  crate-internal imports"; an inline `crate::ast::SdlVertex` is
  technically not a `use` statement, but it IS a crate-internal
  reference to a sibling module's type. Reading the invariant in
  spirit (the file should be a pure leaf), these references are
  the same exempted intrusion that the `use` form would be —
  they belong on the AP-001 exemption list (which they are on),
  and they will go away with the same refactor that removes
  `SdlVertexBuf`. No fresh leak beyond that.

## Candidate new rules

One observation worth recording, but it does NOT clear the bar
for promotion to a rule.

**Observation (review-only): per-variant marshaling pass
duplication.** `ffi_call` (lines 282–494) has two parallel
match-on-variant blocks: pass 1 (lines 331–372) populates the
backing-storage Vecs, pass 2 (lines 424–439) builds the libffi
`Arg` references using cursor indices. Adding a new `FfiArg`
variant requires four coordinated edits: (a) the enum at line
44, (b) a backing-storage Vec near line 311, (c) a pass-1 arm
near line 332, (d) a pass-2 arm near line 425, plus matching
`TypeCode` / parser support if a new type code is needed. This
is the "shotgun" shape that historically motivated the
`SdlVertexBuf` accretion (each new buffer kind → another full
fan-out across these blocks). The observed pattern is that
generic infrastructure with per-variant fan-out invites
specialized variants because the cost of one more arm is
locally cheap.

A formal rule capping the number of `FfiArg` variants would
be arbitrary; the right long-term answer (per the AP-001
exemption header in `lints/exemptions/AP-001.txt` itself) is to
collapse the buffer-shape variants behind a single
`ArgByteBuf(Vec<u8>)` primitive and push struct packing into
stdlib. That's a refactor task, not a lint rule. Not promoting.

## Clean

The file is clean against all 8 active rules (with the
documented exemptions) and against its `runtime-invariants.md`
brief. No new findings to fix.
