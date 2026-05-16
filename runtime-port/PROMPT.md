# Task: build an Evident runtime, from scratch, in a new language

You are implementing the **Evident** runtime from scratch in a host
language of your choice (C, Go, Python, OCaml, Common Lisp, Zig,
Swift, Java — anything that can link against libz3 and `dlopen` +
libffi). The goal is a working `effect-run` for the four programs in
`examples/` and a passing run of the `conformance/` test suite.

This file is your standing brief. Read it first, then `SPEC.md`. Then
start.

## What "done" means

A single binary, call it `your-evident`, that satisfies both of the
following from this directory:

```
# Executor side — four programs in examples/, exact-byte stdout match
for n in 01_hello 02_counter 03_exit_code 04_two_fsms; do
  diff <(./your-evident effect-run examples/${n}.ev) expected/${n}.txt
  test "$(./your-evident effect-run examples/${n}.ev > /dev/null; echo $?)" \
       = "$(cat expected/${n}.exitcode)"
done

# Language side — 119 black-box tests
EVIDENT_CMD=./your-evident pytest conformance/ -q
```

Both must produce empty diffs / `119 passed`. No `xfail`, no
`skip`, no "good enough." If your runtime can't pass a test, the
test either reveals a real bug or a gap in `SPEC.md` — fix one of
them.

## Pick a language

The first decision is what language you're targeting. Things to
weigh:

* **Z3 binding** — every mainstream language has one; pick one with
  a binding you trust (rust-z3, z3-sys, z3-c-api, microsoft/z3 from
  Python, etc.).
* **FFI** — for the FFI primitive layer (after the four examples),
  you'll need `dlopen` + `dlsym` + `libffi` (or equivalent). C is
  trivial; Go/Rust have crates; Python has `ctypes`/`cffi`; OCaml
  has `ctypes`. Higher-level managed runtimes (JVM, .NET) are
  workable but need more glue.
* **Unicode** — Evident source uses `∈ ∀ ∃ ⇒ ⟸ ↦ ⟨ ⟩ ≤ ≥ ≠` and a
  few other glyphs. Your lexer must handle UTF-8; pick a language
  whose string handling makes that pleasant.
* **Recursion depth** — the AST translator recurses through nested
  expressions and bound bodies. Make sure your language won't blow
  the stack on programs of the size in `examples/`.

State your choice in your first response and the reasoning behind
it. Don't change languages partway through.

## Read order

1. `SPEC.md` — the language-agnostic spec. Source of truth. Read
   end-to-end before writing any code.
2. `README.md` — what's in this directory and how it fits.
3. `stdlib/runtime.ev` — the smallest Evident file that defines the
   types `effect-run` needs (`Effect`, `Result`, `FFIArg`,
   `EffectPair`). Your runtime must be able to load this.
4. `examples/01_hello.ev` — the smallest end-to-end program. Trace
   through it by hand against `SPEC.md` to make sure you understand
   every construct before you start coding.

The Rust reference implementation lives in the parent repo's
`runtime/src/` (paths called out in `README.md`). Use it to
disambiguate when `SPEC.md` is silent — but treat the spec as the
contract, not the Rust code. Don't port line-by-line.

## Staging — do not skip ahead

Build the runtime in this order. Each stage is testable on its own.

1. **Lexer.** Produce a stream of tokens from UTF-8 source. Smoke
   test: tokenise `examples/01_hello.ev` and print the stream; eyeball
   that operators, identifiers, and string literals come out right.
2. **Parser → AST.** Recursive-descent, no shift-reduce. Smoke
   test: parse all four `.ev` files and `stdlib/runtime.ev` without
   error; print-back should round-trip.
3. **Z3 link + translator.** Translate a trivial schema (e.g.
   `schema S\n  x ∈ Nat\n  x = 5`) to Z3 sorts + constraints,
   call `check-sat`, extract the model. At this point a meaningful
   chunk of `conformance/test_language.py` should pass.
4. **`query` / `check` CLI shape.** Match the JSON shapes in
   `SPEC.md` and `conformance/test_cli.py`. Aim for all of
   `conformance/` green here, or as close as you can get without
   the multi-FSM scheduler.
5. **Effect dispatch.** `Println`, `Exit`, `IntToStr`, basic
   `Result` plumbing. Now `01_hello`, `02_counter`, and `03_exit_code`
   should pass.
6. **Multi-FSM scheduler.** Subscription-driven by default
   (`SPEC.md` covers the rules). Now `04_two_fsms` passes.
7. **FFI.** `LibCall` over `dlopen` + `libffi`. The four examples
   don't exercise this, but you should add a smoke test that opens
   `libc` and calls `getpid` once the scheduler works.

After each stage runs the relevant subset of the tests, you should
be in green-bar territory for that stage. Do not move to the next
stage on red.

## Working style

* **Small commits, green between each.** A stage half-done is worse
  than the previous stage fully done; the test target tells you which
  you're in.
* **Test in the host language too.** The `conformance/` suite is
  black-box and slow. For internal correctness, write unit tests in
  your host language for the lexer, parser, and translator. The
  port goes wrong in those layers first; black-box tests find the
  symptom, host-language tests find the cause.
* **Honest about what works.** When you report progress, list which
  tests pass and which fail, by name. Don't say "mostly working" —
  say `42 passed, 77 failed` and which the next failure is.
* **Don't reach for the reference too early.** The spec is meant to
  be implementable on its own. If you find a question the spec
  doesn't answer, write down the question in your notes before you
  open the Rust code, then check whether the reference's answer is
  load-bearing or incidental. Update the spec (in your fork) if
  it's load-bearing.
* **Implement the bootstrap subset first.** `SPEC.md` calls out
  which language features are required to load `stdlib/runtime.ev`
  and run the four examples. You do not need the full language to
  reach a green `effect-run`. Build the subset, lock it in with
  tests, then expand.

## Out of scope (for now)

* Visual / SDL demos (graphics FFI). They're in the parent repo's
  `examples/` but not here.
* Self-hosted compiler passes. The parent repo has an experiment
  where desugar / inference passes are themselves Evident programs
  loaded from `stdlib/passes/`. Those files are present here
  because the reference runtime auto-loads them, but your runtime
  does not need to honor them — the spec doesn't require it.
* The REPL.
* The `evident test` discovery subcommand.

Get the four examples green, get conformance green, then we'll talk
about the next layer.

## When you're stuck

In rough order:

1. Re-read the relevant section of `SPEC.md`.
2. Look at the reference module in the parent repo (`runtime/src/…`)
   for the same concern.
3. Construct a minimal `.ev` file that reproduces the question and
   compare what the reference binary does versus what yours does.
4. Ask, with the reproducer attached.

Don't make up semantics. If the spec is silent and the reference is
silent, ask.

## Start

Tell me the language you've chosen and your reasoning, then begin
at Stage 1 (lexer). Report status after each stage with the test
counts as evidence.
