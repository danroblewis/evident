# Wave 5b — Trampoline + FFI dispatch in Evident (feasibility)

**Type:** diagnostic / design. No code changed.
**Reading:** `kernel/src/{main,tick,libcall}.rs`, `kernel/src/manifest.rs`,
`stdlib/kernel.ev`, `docs/plans/architecture-invariants.md`
§"The `__mem` deref primitive", `legacy-python/docs/fti-z3.md`,
`legacy-python/docs/runtime-architecture.md`. Memory:
[[project-fti-honesty-audit-result]], [[project-functionize-walk-result]],
[[project-constraint-model-compilation]], [[feedback-aot-over-runtime-disk-cache]].

## Section 1 — Today's trampoline + FFI surface

The main loop is `tick::run` (`kernel/src/tick.rs:134`). `main.rs` reads a
`.smt2`, parses the manifest header (`manifest.rs`), and hands off. The loop
(`tick.rs:276`, `for tick in 0..TICK_LIMIT`) does, per tick: (1) build a small
SMT-LIB *pin* string — state-carry `_<name>=val`, `last_results` length +
per-index values, `is_first_tick` — and parse it to ASTs that intern against the
cached simplified body (`tick.rs:360-391`); (2) `Z3_solver_check`; (3) read the
model — each `state_field`, then `effects__len`, then walk the `effects` Seq
element-by-element (`tick.rs:416-448`); (4) `dispatch_effect` per element. Halt
priority: `Exit` → stuck (state unchanged) → UNSAT → error.

**libffi lives in exactly one place.** `dispatch_effect` (`tick.rs:818`) decodes
each effect variant. `Exit`/`ReadLine`/`ReadFile`/`WriteFile` are native
built-ins; `LibCall(lib, fn, Seq(LibArg))` is the only libffi path. It decodes
the three operands (`tick.rs:871`) and calls `libcall::call` (`libcall.rs:50`):
dlopen (cached process-wide in `LIB_CACHE`), dlsym, build a libffi `Cif` from arg
shapes (`Int→i64`, `Str→pointer`, `Real→f64`; **return type always i64**),
marshal args into owned storage that out-lives the call, `cif.call`. The result
i64 becomes `Res::Int`. The `__mem` pseudo-library (`libcall.rs:166`) is a
faithful 8-byte `read_long`/`write_long` deref pair — the one honest-FTI escape
hatch ([[project-fti-honesty-audit-result]]).

**Rust-side bookkeeping the loop owns:** state pinning (model → `Sv` →
SMT-LIB pin text next tick), result marshaling (effects → `Res` → re-asserted
`last_results` array), the dlopen cache, and per-call arg-storage lifetimes.

A note that frames both paths: the trampoline **cannot become pure Evident** —
something native must read the `.smt2`, run the first `Z3_solver_check`, and
invoke the call primitive. Per CLAUDE.md the kernel *stays* minimal Rust; "no
Rust" targets the compiler, not `kernel/`. Both paths below move the **dispatch +
marshaling logic** out of the kernel into Evident sugar, shrinking the kernel's
FFI surface — they do not delete it.

## Section 2 — Path A: keep libffi, move the loop logic to Evident

The dispatch + re-pin half of the loop becomes an Evident FSM over a thin native
floor: a `Z3_solver_check`/model-read primitive (assume wave 5a delivers it) plus
a generic `ffi_call`. The Evident side needs three capabilities, all already in
reach:

- **Read model → pins.** Format `Sv` values into SMT-LIB pin text. Evident has
  the string ops (substr/indexof/replace/`#`, `str_from_int`) landed in
  [[project-string-ops-landed]] — pin formatting is string concatenation.
- **Dispatch `LibCall`** via a sugar `BuildLibCallWith(ctx, …)` over libffi's own
  entry points, called as ordinary `LibCall` effects:
  - `ffi_prep_cif(ffi_cif *cif, ffi_abi abi, uint nargs, ffi_type *rtype, ffi_type **atypes) → ffi_status`
  - `ffi_call(ffi_cif *cif, void (*fn)(void), void *rvalue, void **avalue) → void`

  Both are **pure pointer/int passing** — exactly the shape libffi already
  handles. Pointers travel as `ArgInt(handle)`; the `cif`, `rvalue`, and the
  `avalue`/`atypes` arrays are `libc malloc` blocks populated with `__mem`
  `write_long`. This is the marshaling story wave 5a assumes; it composes.

**One small new native primitive is required.** The `ffi_type_sint64` /
`ffi_type_pointer` globals are *data symbols*; `ffi_call` needs their addresses.
Today `libcall::call` only *calls* a resolved symbol — there is no "give me the
address of symbol X without calling it." Path A needs a `dlsym_addr(lib, name) →
i64` primitive (a few lines: dlsym then return the pointer as i64). Minor, but
real — flag it for wave 5a's marshaling scope.

Risk is low: libffi stays a pure C dependency doing what it already does; only the
orchestration migrates. This is precisely the north-star step "move the Rust shim
into Evident, libffi can stay."

## Section 3 — Path B: replace libffi with Evident codegen models

Describe each calling convention as a constraint model and emit a self-contained
call stub. Per architecture:

```
claim SysVAmd64Call(args ∈ Seq(Arg), assigns ∈ Seq(Loc))
    -- int/ptr args → RDI,RSI,RDX,RCX,R8,R9 then 16B-aligned stack;
    -- float args → XMM0..7; return in RAX.
```

An emit pass lowers the assignment to machine bytes (`mov rdi, imm64` = `48 BF
..`, …, `mov rax, fn; call rax; ret`). The byte `Seq` is written into executable
memory and jumped to. ARM64 AAPCS64 (`x0..x7` int, `v0..v7` float, return `x0`)
and RISC-V (`a0..a7`) are each one analogous model.

**This is *not* blocked, but it carries a heavier tail:**

- **Executable memory.** Needs `mmap(RWX)` or `mmap`+`mprotect`, both expressible
  as `LibCall("libc", "mmap"/"mprotect", …)`. On Apple Silicon, W^X forbids RWX;
  you need `MAP_JIT` + `pthread_jit_write_protect_np` + the JIT entitlement. That
  is a genuine platform wrinkle, not a one-liner — the chief medium-risk item.
- **A "call raw address" native primitive.** `libcall::call` resolves by *name*;
  a stub lives at an address. You need `call_addr(addr) → i64` (0-arg — the stub
  sets up its own args). Notably this is *smaller* than the libffi shim it
  replaces: the kernel's FFI floor shrinks to a single bare jump.
- **ABI long tail.** `puts`/`write`/`time` (int + pointer args) are trivial. The
  general case — struct-by-value classification, varargs (`printf`), SSE float
  rules, red zone, stack alignment — is exactly what libffi exists to absorb.
  Reimplementing it faithfully across three arches is the real cost.
- **No assembler needed at runtime** — we emit bytes directly, so the dependency
  is just libc (the spec's "+ as" is avoidable).

**The multiplier that makes B attractive:** ONE codegen model per arch is reused
by *every* tool, and it is the same constraint-model→native substrate the
functionizer wants ([[project-constraint-model-compilation]],
[[project-functionize-walk-result]]). B is FFI *and* JIT in one mechanism. Stub
generation is tiny and AOT/disk-cacheable ([[feedback-aot-over-runtime-disk-cache]]).

## Section 4 — Comparison

| Aspect | A (keep libffi) | B (Evident codegen) |
| ------ | --------------- | ------------------- |
| New code (rough) | ~300–600 ev lines (loop + `BuildLibCallWith` sugar) + a tiny `dlsym_addr` primitive | ~400–800 ev lines **per arch** (ABI model + byte encoder) × 3 + W^X/`MAP_JIT` handling + `call_addr` primitive |
| Cross-platform | yes — libffi handles it | per-arch model (x86-64 / ARM64 / RISC-V) |
| Dependency story | one C lib (libffi) + libc | none beyond libc (no `as` — emit bytes) |
| Bootstrap risk | low | medium (W^X/`MAP_JIT`, ABI struct/vararg tail) |
| Compile-time speed | fast (Cif cheap, libs cached) | stub gen tiny + AOT/disk-cacheable |
| Native floor left in kernel | `Z3_check` + `ffi_call` + `dlsym_addr` | `Z3_check` + bare `call_addr` (smallest) |
| Doubles as JIT substrate | no | **yes** — same model the functionizer needs |

## Section 5 — Verdict + roadmap

- **Path A: `feasibility: HIGH`.** Contingent only on wave 5a's
  `Z3_solver_check`/model-read primitive plus a trivial `dlsym_addr` addition.
  libffi's own entry points are pure pointer-passing, so the existing marshaling
  story covers them; the migration is orchestration, not new ABI work.
- **Path B: `feasibility: MEDIUM`.** Not blocked — every piece is reachable
  (`mmap`/`mprotect` via `LibCall`, bytes via `Seq`, a one-line `call_addr`). The
  two real costs are the **W^X / `MAP_JIT` platform story on Apple Silicon** and
  the **ABI long tail** (structs-by-value, varargs, float classification). Neither
  is a hard blocker; both make B a larger, multi-session effort.

**Ship A first, then B.** A is the natural, low-risk progression: it achieves the
north-star step ("Rust shim → Evident, libffi stays a pure C dep") and, crucially,
its handle-passing-via-`__mem` substrate is *exactly* what B reuses for
`mmap`/`mprotect`/array marshaling. A builds the foundation; B then removes the
last C library and, as a bonus, hands the functionizer its native-codegen
substrate. Do B only when the JIT payoff justifies owning three ABIs.
