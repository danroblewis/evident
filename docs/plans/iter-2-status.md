# Iteration 2 — status

Iter 1 shipped the kernel + emit + SMT-LIB pipeline end-to-end.
Iter 2 makes the kernel run real programs.

## What works after iter 2

### Compile + run an Evident program
```
evident run <file.ev> <claim>          # one-step
evident emit <file.ev> <claim> -o file.smt2
kernel file.smt2                       # two-step
```

### Single-tick programs
A program emitting a `Seq(Effect)` performs each in order then exits
when it sees `Exit(n)`:

```evident
import "stdlib/kernel.ev"

claim main
    effects ∈ Seq(Effect) = ⟨
        LibCall("libc", "puts", ⟨ArgStr("hello")⟩),
        Exit(0)
    ⟩
```

### Multi-tick programs (state + last_results carry)
The kernel ticks the FSM in a loop. Between ticks:
- `state.*` fields are read from the model and re-asserted as
  `_state.*` next tick.
- `last_results` (the prior tick's collected `Result` values) is
  asserted as a Seq literal.
- `is_first_tick` is true on tick 0, false thereafter.

Halt conditions:
1. Effect `Exit(n)` emitted → exit `n`
2. State unchanged tick-to-tick → exit 1 ("stuck")
3. UNSAT → exit 2
4. Internal error → exit 3

Tick limit: 100,000.

### Real I/O via built-in effects
```evident
ReadFile(path)   → StringResult(contents) | ErrorResult(_)
WriteFile(p, c)  → NoResult              | ErrorResult(_)
ReadLine         → StringResult(line)    | EofResult | ErrorResult(_)
```

These read/write actually happen on disk / stdin. Results land
in `last_results` for the next tick to match on.

### Universal libffi escape hatch
`LibCall(lib, fn, args)` performs a real C call:
- `lib`: `"libc"` aliases to `libSystem.dylib` on macOS / `libc.so.6`
  on Linux. Full paths used verbatim.
- `fn`: function symbol name (dlsym).
- `args`: `Seq(LibArg)` with `ArgInt(Int)` → i64, `ArgStr(String)`
  → `*const c_char`, `ArgReal(Real)` → f64.
- Return: always read as `i64` → `IntResult(n)`. Float and pointer
  returns work if the platform ABI lets i64 carry them.

Cached dlopen handles per-library-name for the kernel's lifetime.

### Pattern-match on last tick's results
```evident
file_contents ∈ String = match last_results[0]
    StringResult(s) ⇒ s
    _ ⇒ "<read failed>"
```

The match expression is translated to Z3 ITE. The kernel hands the
prior tick's results to Z3 as Datatype literals on the next solve.

## The "Effect = LibCall + Exit" floor

| Variant | Status |
|---|---|
| `LibCall(lib, fn, args)` | kernel-native — libffi dispatch |
| `Exit(code)` | kernel-native — process exit |
| `ReadFile(path)` | kernel built-in (uses `std::fs::read_to_string`) |
| `WriteFile(path, contents)` | kernel built-in (uses `std::fs::write`) |
| `ReadLine` | kernel built-in (uses stdin readline) |
| ~~`Println(s)`~~ | demoted → `LibCall("libc", "puts", ⟨ArgStr(s)⟩)` |
| ~~`Print(s)`~~ | demoted → `LibCall("libc", "write", ⟨ArgInt(1), ArgStr(s), ArgInt(#s)⟩)` |
| ~~`Time`~~ | demoted → `LibCall("libc", "time", ⟨ArgInt(0)⟩)` |

The three remaining built-ins (`ReadFile`, `WriteFile`, `ReadLine`)
need buffer / fd-handle types in Evident before they can be
demoted to LibCall sequences.

`stdlib/kernel.ev` provides sugar claims:
- `BuildPrintln(s, eff)` → `eff = LibCall("libc","puts",⟨ArgStr(s)⟩)`
- `BuildPrint(s, eff)`   → `eff = LibCall("libc","write",⟨ArgInt(1), ArgStr(s), ArgInt(#s)⟩)`
- `BuildTime(eff)`       → `eff = LibCall("libc","time",⟨ArgInt(0)⟩)`

## Language conveniences

### Auto-injected schema members
`emit` injects these if the user doesn't declare them:
- `last_results ∈ Seq(Result)`
- `is_first_tick ∈ Bool`

The runtime sees them as normal Evident members; the kernel wires
them per the input spec.

### Previous-tick references
Declare `_<name> ∈ <T>` explicitly. The kernel asserts
`_<name> = <prev value>` on every tick after the first.

### Effects via guarded / conditional Seqs
```evident
effects ∈ Seq(Effect) = (is_first_tick
    ? ⟨ReadFile(path)⟩
    : ⟨LibCall("libc", "puts", ⟨ArgStr(contents)⟩), Exit(0)⟩)
```

The single-writer validation accepts:
- One unconditional `effects = <expr>`, OR
- Multiple guarded `cond ⇒ effects = <expr>` constraints (user
  responsible for mutual exclusion).

### State-field discovery
State fields = top-level memberships of primitive type
(`Int`/`Bool`/`Real`/`String`) excluding `effects`, `last_results`,
`is_first_tick`, and `_<name>` carry-overs.

Datatype-typed memberships (e.g., `eff ∈ Effect`) are single-tick
scratch bindings, not carry-state.

## Test surface

| Test | Asserts |
|---|---|
| `test_hello.ev` | basic puts + exit 0 |
| `test_exit_42.ev` | custom exit code propagates |
| `test_multiple_prints.ev` | Seq walked in order |
| `test_concat_composition.ev` | `++` joins per-concern effect Seqs |
| `test_libcall_puts.ev` | `LibCall("libc","puts",ArgStr)` |
| `test_libcall_putchar.ev` | `LibCall` with ArgInt |
| `test_multi_tick.ev` | state + last_results carry across ticks |
| `test_file_io.ev` | 3-tick ReadFile → WriteFile → puts |

All 8 kernel tests green via `./test.sh --kernel` in <1s.

## Code surface

```
kernel/                   ~750 LOC
├── Cargo.toml
└── src/
    ├── main.rs           CLI entry
    ├── manifest.rs       Parse the `;; manifest:` header
    ├── tick.rs           Z3 solve loop, effect dispatch
    └── libcall.rs        libffi marshalling for LibCall

runtime/src/emit.rs       ~400 LOC
                          Translate AST → SMT-LIB. Auto-injects
                          kernel-convention members + Result decl.
                          Single-writer enforcement.

stdlib/kernel.ev          ~75 LOC
                          Effect/Result/LibArg enums + BuildN sugar.
```

## What's not yet done

| Item | Iteration |
|---|---|
| Demote ReadLine/ReadFile/WriteFile to LibCall sequences | 2.8 / 3.0 — needs richer libffi (buffers, fd handles) |
| Unsat-core diagnostics for failed ticks | 3.1 |
| Effect toposort (drop explicit `++` chaining) | 3.2 |
| Self-hosted lexer in Evident | 3.x |
| Self-hosted parser in Evident | 3.x |
| Self-hosted AST → SMT-LIB translator | 3.x |
| Delete `runtime/` once self-hosting works | 3.x |
