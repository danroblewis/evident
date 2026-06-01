# CLAUDE.md

Operational rules for future Claude sessions on this repo.

## The Python freeze

The Python code in `src/` is **frozen**. After this initial bootstrap
(trampoline + ffi + parser + transpile + CLI), no new features go in
Python. All new work — language features, FTIs, libraries, data
structures, the eventual self-hosted parser — is written in **Evident**
(`.ev` files in `stdlib/` and `examples/`).

Exceptions:

- Bug fixes to the bootstrap parser/transpiler. Mark the commit as a
  bug fix and describe the bug.
- Adding a new libcall signature character if a new C primitive type
  is genuinely needed (e.g., pointer-to-array for Z3's n-ary builders).
- Memory-safety / correctness fixes.

If you find yourself wanting to add a feature to `src/`, **stop**.
The feature belongs in Evident. The Python is bootstrap; Evident is
the language.

## The architecture in one paragraph

The runtime is two things: a **trampoline** (`src/runtime.py`, ~110
lines) that runs an SMT-LIB FSM body to halt, and **libcall**
(`src/ffi.py`, ~80 lines) that bridges to any C library via ctypes.
Everything else — Z3 access, data structures, multi-FSM patterns,
JIT — is library code that uses libcall. See
[`docs/runtime-architecture.md`](docs/runtime-architecture.md) for
the full design rationale.

## The Evident language

Source files: `.ev`. Top-level declarations are `claim`, `fsm`, or
`type`. Set-membership (`x ∈ S`) is the universal primitive for
declaring + constraining a variable. The grammar lives in
`docs/runtime-architecture.md`; the parser is `src/parser.py`.

Examples that work today:

```
claim sum_is_eight()
    x ∈ {0..10}
    y ∈ {0..10}
    z ∈ {0..20}
    z = x + y
    x = 3
    y = 5
```

```
fsm Counter(count ∈ {0..5})
    count = _count + 1
```

State-pair convention: in an `fsm`, each parameter `name ∈ S` becomes
**two** SMT-LIB constants — `_name` (previous tick) and `name` (this
tick). The body asserts the transition: `name = f(_name)`.

## How to run code

```
python3 src/main.py FILE.ev               # run; prints the final model
python3 src/main.py --emit-smt FILE.ev    # just emit the SMT-LIB
```

## What goes in Evident, not Python

- **All data structures.** Stack, queue, map, mailbox — Z3 datatype
  chains, accessed via libcall to Z3's C API.
- **All effect-mediated types** (Mutex, Channel, etc.) — write as
  FTIs once we have FTI syntax in the parser.
- **Multi-FSM composition.** Either compile-time composition (one
  combined body) or supervisor-pattern (FSM that runs N child FSMs).
  Both are library code.
- **The Z3 bindings.** Wrap Z3's C API as ergonomic claims/FTIs.

## What stays in Python

- The trampoline loop and state-pair handling (`src/runtime.py`).
- libcall + ctypes marshaling (`src/ffi.py`).
- The bootstrap parser (`src/parser.py`).
- The bootstrap transpiler (`src/transpile.py`).
- The CLI entry (`src/main.py`).

That's it. Five files. ~700 lines.

## Failure modes already burned

- **Putting growing data in the FSM body.** Bodies must be bounded
  per tick. Growing data lives outside (Z3 datatype chains).
- **Re-rendering the body per call.** The body is parsed once; inputs
  are pinned via the state-pair convention. No `.j2` templates.
- **Imperative thinking.** Evident is relational. Programs describe
  WHAT a valid answer satisfies; Z3 finds it. No `if/then/else`, no
  `let`, no method-call syntax — those are not Evident.
- **Trying to add features in Python.** Add them in Evident.
