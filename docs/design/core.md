# Core — what Evident is, and the line we defend

This is the yardstick. Every addition and every removal is judged against it.
If this document sprawls, the core isn't crisp yet — keep it one page.

## What Evident is

Evident is a constraint language whose programs are collections of constraints
over sets. The pipeline is the whole thing:

> **parse Evident → lower to Z3 constraints → solve into a model → functionize
> the model for speed → the model's `Effect` values drive FFI calls → the FSM
> ticks.**

The Z3 model *is* the program. There is one `main` FSM model; everything else is
embedded in it. The functionizer produces results from the model itself. Effects
are a Z3 sort, so they survive into the model and are read back and dispatched as
real foreign-function calls. That loop — constraints in, satisfying assignment
out, effects executed — is the core value. Nothing else is.

## The core, by stage (live module map: CLAUDE.md)

1. **Front end** — lexer, parser, AST.
2. **Lower** — desugar / inject (the surface sugar the language promises),
   encode (AST → Z3 sorts + constraints), the Z3-program IR extraction.
3. **Solve & speed** — the Z3 integration, the Cranelift functionizer, and the
   build-once compiled model it reuses each tick. That model — declared once, then
   asserted/checked/popped per frame, the fallback path any translator-gap claim
   hits *every tick* — **is the solve, materialized; it is core, not a cache bolted
   on top.** Performance serves a single end — *sensible, fast-enough-to-iterate
   speed* — not a goal in itself. So what's NOT core is **speculative** optimization
   (alternate solve strategies, tuning knobs, *speculative* caching layered on top
   of the solve) and performance **measurement** — add either back only when a real
   bottleneck demands it. The test: does it *do* the solve, or merely try to make an
   already-working solve faster on a guess?
4. **Execute** — the single-FSM tick loop, effect dispatch, the FFI primitive,
   and the FTI (foreign type interface) bridges. The Effect/Result value codec.
5. **CLI** — exactly `effect-run` (run a program) and `test` (verify claims).

## The cut rule

Ask one question: **is this part of the core pipeline, does it directly protect
it, or is it on the roadmap below — or is it incidental complexity we dragged
along?** If incidental, cut it.

- **"Useful" is not a defense.** Solid, working, helpful code still goes if it
  isn't core. Cutting it loses nothing but defers it — it's easily remade.
- **Add it when we need it,** not before. Speculative infrastructure is a guess
  about the future encoded as present complexity.
- **Half-features are worse than absent features** — they imply a capability the
  core doesn't actually have. Finish it into the core or remove it.
- **When in doubt, cut and defer.** A clean core is worth re-deriving a
  convenience later.
- Incidental-but-real things may be **refactored out** to a library/use-case
  rather than deleted — but they leave the core.

## Not core — cut on sight, add when needed

Performance measurement / stats / timing. Observability / tracing / dumps /
env-gated knobs. Alternate or speculative solve paths (replay, sampling, multiple
functionizer strategies, decompose variants). Half-sketched language features
(generics, `Map`/`Bag`). Self-hosting / reflection machinery. Convenience commands
beyond `effect-run`/`test`. Over-built error handling. Scratch / `*_experiment`
code.

## What "good" means

- **Solid / reliable** — correct and covered; `./test.sh` green is the contract.
- **Fast to iterate** — seconds to build + test; the functionizer keeps runtime fast.
- **Navigable & extensible** — single-concern files (~≤500 lines), Semfora-indexed,
  so a Claude agent can hold the whole subsystem in view. Smallness *is* the
  extensibility mechanism.

## The roadmap the core stays clean for

Better FTI + more FTI implementations · phase-portrait diagrams · fixed-point
reductions · a difference-equation `fsm` syntax · a new IDE.

**Each of these enters as a real subsystem with its own boundary** (its own
crate/use-case, dependency injection, inversion of control) — *never* as a tendril
threaded through the core. A cool idea earns a clean implementation only after it
has proven it belongs; until then it stays out so it can't pollute the core's
purpose.
