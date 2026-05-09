# Phase 3: Language prerequisites for advanced library migrations

The Phase 4 ports (GLSL transpiler, SMT-LIB I/O, test reporters)
need language features Evident doesn't have today:

- **Recursive claims** — a claim that calls itself with smaller
  arguments. Z3 supports this via Datatype induction; we need to
  surface it in the claim system.
- **Unbounded output Seq** — pass results whose length depends on
  the input, not pinned at parse time.
- **Enum-typed pattern bindings** — `Match` arms that bind
  payload fields whose type is an enum (e.g.
  `Cons(head, tail)` where tail is the recursive enum).

Tasks are sequential — recursive claims unlock the others.

## Per-task plans

- `01-recursive-claims.md` — recursive claim invocation
- `02-unbounded-output.md` — variable-length result Seq
- `03-enum-pattern-bindings.md` — match patterns that bind enum-typed payloads

## Why sequential

Recursive claims are the foundational mechanism. Once you can write
a self-recursive walker, the other two become natural extensions
(an unbounded-output recursive walker, an enum-payload-binding
recursive walker).

Each task adds a real language feature to the runtime.
Implementation will require translator changes; expect ~300-500
lines of Rust ADDED in Phase 3, with the payoff being ~2,000+ lines
REMOVED in Phase 4.
