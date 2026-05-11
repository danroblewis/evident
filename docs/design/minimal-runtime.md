# The Minimal Evident Runtime

## What "minimal" means here

Evident is its own language. The Rust runtime exists only because some
things genuinely cannot be expressed in Evident itself: lexing source
bytes, calling Z3, calling the operating system, dispatching the
state-machine step loop.

Anything else — graphics, audio, codegen, SMT-LIB I/O, inference passes,
test reporters, wire-format encoders, network protocols — is **library
code written in Evident**, not Rust. The runtime exposes a small,
generic FFI primitive; everything that needs to talk to the outside
world goes through it.

This document defines what stays in Rust, what moves to Evident, and
what the resulting runtime looks like.

## What the runtime IS

A program that:

1. Loads `.ev` source files into an AST.
2. Translates AST into Z3 constraint expressions.
3. Drives a state-machine loop: each step solves
   `(next_state, effects) given (current_state, last_effect_results)`,
   then performs the effects to gather inputs for the next step.
4. Exposes a generic FFI primitive so Evident libraries can wrap
   any compiled C library or POSIX syscall.

That is the whole job. Everything we do beyond this — formatting test
output, drawing graphics, parsing alternate file formats — is doable
in Evident with FFI access to the underlying primitives.

## The execution model: state machines + effects

Every Evident program is a finite state machine. A claim of the form
`type main(state, state_next ∈ S, last_results ∈ ResultList,
effects ∈ EffectList)` says: given the current state and the results
of effects performed in the previous step, what is the next state and
what effects should the runtime perform now?

The step loop:

```
state₀ = initial
last_results = []
loop:
    (state', effects) = solve(claim_main, state, last_results)
    last_results = perform_each(effects)
    state = state'
    if state.halt: break
```

`perform_each(effects)` walks the effect list and either:

- Runs a built-in effect directly (Print, Read, Time, Exit).
- Resolves an FFI effect via `dlopen` + `dlsym` + libffi call.

Each effect produces a `Result` value the next step's solve can use.

This shape replaces today's plugin architecture. There is no
`StdinPlugin` / `SDLPlugin` / `AudioPlugin` — there is one effect
dispatcher. Plugin-equivalent code lives in Evident libraries that
declare effect shapes for their domain.

## What stays in Rust (the "core")

| Component | Lines (est.) | Why it must stay |
|---|---|---|
| Lexer | ~400 | Reads source bytes; bootstrap problem if self-hosted. |
| Parser | ~1,900 | Same bootstrap problem; see "Parser bootstrapping" below. |
| AST | ~350 | Shared definitions consumed by parser + translator. |
| Z3 translator (`translate/`) | ~5,200 | Calls the Z3 API. Self-hosting it requires self-hosting Z3, which is circular. |
| Runtime API | ~600 (slimmed from 1,189) | `load_*`, `query`, `sample`. |
| Step engine | ~300 (slimmed from 1,118) | The pure FSM dispatch loop with effect performance. |
| Built-in effects | ~200 (new) | Print, Read, Time, Exit — the few that pre-date FFI. |
| FFI primitive | ~700 (new) | `dlopen` / `dlsym` / `libffi` call + type marshalling. |
| CLI shell | ~400 (slimmed from ~2,700) | `evident query/sample/test/run`, basic flag parsing. |
| Total | **~10,050** | |

That is a **~7,000-line cut** from today's ~17,100. The reductions
mostly come from cutting feature areas and shrinking the
plugin/executor code, not from heroic self-hosting.

## What moves OUT (becomes Evident libraries)

| Today's Rust | Lines today | New home |
|---|---|---|
| `plugins/sdl.rs` | 556 | `packages/sdl/` — Evident library wrapping libSDL2 via FFI |
| `plugins/audio.rs` | 228 | `packages/audio/` — wraps SDL_audio or PulseAudio |
| `plugins/shader.rs` | 443 | `packages/gl/` — wraps libshaderc / glslang / OpenGL via FFI |
| `glsl.rs` | 1,007 | `stdlib/glsl/` — pure Evident AST → string transpiler (needs recursive claims; see "Prerequisites") |
| `smtlib.rs` (import + export) | 957 | `stdlib/smtlib/` — same shape as GLSL transpiler |
| Inference passes (`commands/infer_types.rs`, etc.) | ~700 | `stdlib/passes/` — already self-hosted in spirit; the Rust glue can shrink |
| `executor.rs` plugin lifecycle code | ~400 | Removed entirely — no plugins to manage |
| `commands/test.rs` formatters (TAP/JUnit/JSON) | ~400 | `stdlib/testing/reporters/` — Evident formats via String operations |
| Plugin abstraction (`plugin.py`-style trait) | ~50 | Removed |
| Total | **~4,750** | |

Most cuts collapse cleanly into Evident libraries that call FFI for the
parts that need OS access. Some (GLSL transpiler) require language
infrastructure we don't have yet — flagged below.

## What we don't try to remove

- **Z3 translation code** (`translate/exprs.rs` etc.) — translating
  Evident expressions to Z3's API is the runtime's reason to exist.
  Self-hosting it requires self-hosting Z3, which is a different
  project.
- **Parser/lexer** — the bootstrap problem. Possible but a major
  rewrite; see below.

## Parser bootstrapping (the unfinished question)

A truly minimal runtime would parse Evident with Evident. Two paths:

1. **Stay with Rust parser** — accept the floor at ~10K Rust lines,
   keep the parser as the largest single file, ship.
2. **Bootstrap an Evident parser** — write a minimal Rust parser that
   handles only enough syntax to load `stdlib/parser.ev`, which then
   parses the full language. Lua, Smalltalk, OCaml, and others have
   done this. It would shrink the Rust core to ~6K lines but is its
   own project.

Recommendation: defer. Land the FFI work, port plugins to libraries,
re-evaluate. The parser is the next obvious target only after the
easier wins are in.

## Prerequisites for the moves above

Some library ports need language features we don't yet support:

| Migration | Needs |
|---|---|
| GLSL transpiler | Recursive claims (walk an Expr tree); unbounded output Seq(String); enum-typed pattern bindings. |
| SMT-LIB export | Same as GLSL — recursive AST walk producing strings. |
| SMT-LIB import | A self-hosted parser for SMT-LIB syntax. Smaller surface than Evident's parser; achievable once we have a string-tokenizer primitive. |
| Inference passes | Already work in spirit; extending to passes that produce **rewrites** (not just inferences) needs a way for Evident to express "transform this AST into that AST." Today the rewrite happens in Rust glue (`commands/desugar.rs`). |
| SDL/audio/shader libraries | FFI primitive (the next milestone). |

The minimum-Rust target is reachable in stages. The first stage —
SDL/audio/shader → FFI libraries — only needs FFI. Later stages need
the language extensions above.

## The path forward

1. **Now**: design and implement the FFI primitive. Validate with one
   end-to-end call (libc `getpid` from Evident, returning Int).
2. **Next**: design Effect type and dispatch loop in executor. Migrate
   the simplest plugin (Stdin/Stdout) to effects, drop ~400 lines.
3. **Then**: port SDL plugin to `packages/sdl/` library. Drop ~556 lines.
4. **After that**: port audio + shader, drop another ~670.
5. **Later** (needs language work): GLSL transpiler, SMT-LIB,
   recursive desugar passes.
6. **Eventually**: bootstrap an Evident parser, drop ~1,900.

Each stage stands on its own — none requires the next.

## Estimated end state

After stages 1-4 (the ones that only need FFI):

| Component | Lines |
|---|---|
| Lexer | ~400 |
| Parser | ~1,900 |
| AST | ~350 |
| Z3 translator | ~5,200 |
| Runtime API | ~600 |
| Step engine + effect dispatch | ~500 |
| Built-in effects | ~200 |
| FFI primitive | ~700 |
| CLI shell | ~400 |
| **Total** | **~10,250** |

After stage 5 (recursive language features land, GLSL/SMT-LIB ported):
**~8,250**.

After stage 6 (parser bootstrapped):
**~6,350**.

The achievable minimum is somewhere between 6K and 10K Rust lines,
depending on how far we push. The current 17K is the cost of having
shipped feature-by-feature without an architectural target.

## What the user-facing experience looks like

Today:
```bash
evident execute programs/sdl_demo/bouncing_dots.ev
```

After this work:
```bash
evident execute programs/sdl_demo/bouncing_dots.ev
```

— same command, same behavior. The difference is invisible: `bouncing_dots.ev`
now `import`s `packages/sdl/window.ev`, which is Evident code calling FFI. The
runtime doesn't know about SDL; it knows about effects.

This is the architectural goal: the runtime becomes language infrastructure.
Features become libraries. Evident grows by writing Evident, not by writing
Rust.
