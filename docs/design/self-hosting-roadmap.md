# Self-Hosting Roadmap (Post-Stage 7)

Stage-by-stage plan for extending self-hosting beyond the current
state (literal type inference + iteration over single-claim
programs). Updates `docs/design/self-hosting-compiler-passes.md`'s
forward-looking section with concrete next steps.

## Where we are

Stages 0–7 shipped (see `docs/rust-runtime-capabilities.md` for the
runtime side). The user-visible state:

  - `evident infer-types <file>` runs both pattern-matching rules
    (`literal_types.ev`) and existential iteration rules
    (`iter_types.ev`) over the user's program.
  - Inferences are aggregated into a unified `Inferred types:` table
    with rule attribution.
  - Compiler passes are written as Evident claims; the runtime layer
    is just parse → encode → inject → query → render.

## Where it falls short today

  - **Only the first claim's body is iterated.** Programs with
    multiple claims silently lose 80%+ of their inferences. (Stage 8.)
  - **No cross-body-item reasoning.** Rules match a single body item
    or a fixed pair; no rule says "find `x = y` AND `y = literal` and
    propagate." (Stage 9.)
  - **Conflicts aren't surfaced.** The aggregator can render
    `*ambiguous*` output, but no current rule produces conflicting
    inferences. (Stage 10.)
  - **Only type inference exists.** Self-hosting could express any
    pass; there's no demonstration with a different use case. (Stage 11.)
  - **The narrative is scattered.** No single tutorial walks a new
    contributor from "I have an idea for a pass" to "it ships."
    (Stage 12.)

---

## Stage 8 — Multi-claim iteration

**Goal:** every claim in the user's program gets iterated, not just
the first.

**Plumbing change:** runtime injects multiple `body_NN ∈ Seq(BodyItem)`
variables (one per user claim), or alternatively runs the pass once
per claim and aggregates results.

**Recommendation:** the per-claim invocation approach. Cleaner pass
code (passes don't need to know there are multiple bodies); one new
runtime helper that iterates `user.schemas` and calls the existing
`query_with_program_and_body` per-claim with the right body.

**CLI change:** `Inferred types:` table groups by claim:

```
Inferred types:
  in claim `parse_command`:
    verb : Verb     (via has_membership_of_var)
  in claim `dispatch`:
    state : GameState  (via extract_first_membership)
```

**Test surface:** integration tests for 2-claim, 3-claim programs;
ensure iteration finds inferences in claims after the first.

---

## Stage 9 — `=` propagation rule

**Goal:** prove the self-hosting model handles cross-body-item
reasoning by writing a rule that finds two related body items.

**The rule:** "if `x = y` and `y = "literal"` are both in the body,
then `x ∈ String`." Requires a constraint that ranges over two body
indices simultaneously: `∃ i, j ∈ {0..body_len-1} : i ≠ j ∧ body[i]
= BIConstraint(...) ∧ body[j] = BIConstraint(...) ∧ <relate them>`.

**Z3 question:** does the unrolled `∃ i ∃ j` over a 4-element body
fold into 16 disjuncts cleanly? Should — `body_len` is pinned, both
quantifiers unroll. If perf bites, fallback is per-pair claims (one
per `(i, j)` shape).

**Pass file:** `stdlib/passes/propagation.ev` with rules for
String/Int/Bool propagation through `=`. Three rules.

**Demo:**

```
$ cat foo.ev
claim t
    x = y
    y = "hello"
$ evident infer-types foo.ev
propagate_string: x ∈ String  (via y = "hello")
y                : String     (via has_string_assignment)
x                : String     (via propagate_string)
```

**Test surface:** Rust integration tests for the propagation rule.
Conformance .ev tests with hand-built programs.

---

## Stage 10 — Conflict detection

**Goal:** when rules disagree, the aggregator's `*ambiguous*` path
fires and the CLI exits non-zero in `--strict` mode.

**Rule additions:** a rule that *produces* a conflict so the
aggregator's existing scaffolding actually exercises. For example:
"if `x = "a"` AND `x = 5` are both in the body, infer `x` as
`String` (from the first) and `Int` (from the second)." The
aggregator surfaces both and tags them as ambiguous.

**Bigger lift:** real conflict isn't from disagreeing rules, it's
from contradictory user-level facts. A user who writes `x ∈ String`
AND `x = 5` is wrong; a pass that catches this is a *consistency
checker*, not just an inferrer.

**CLI flag:** `evident infer-types --strict <file>` exits 4 (new
code) on any ambiguity. Default behavior unchanged (warns but
exits 0).

**Test surface:** programs that trigger conflicts; assertion that
the table renders ambiguity correctly; exit-code tests for
`--strict`.

---

## Stage 11 — Pure-Evident lint pass: unused variables

**Goal:** prove self-hosting is general by writing a pass that
isn't type inference.

**The lint:** "find variables declared via `BIMembership(name, _, _)`
that are never referenced in any subsequent body item." The pass
walks the body twice (or via two existentials): first to enumerate
declared names, second to check each name appears in some
constraint's expression tree.

**Implementation question:** does Evident's pattern matching reach
into nested `Expr` structures? `EIdentifier(x)` could be at any
depth: bare, in `EBinary`, inside `EForall.body`, etc. This may
require recursive helper claims or a flatten-Expr utility.

**Pass file:** `stdlib/passes/unused.ev` — single rule
`unused_variable_in_first_claim` that returns the offending name
(or UNSAT if every declaration is used).

**CLI subcommand:** `evident lint <file>` runs the lint passes.

**Test surface:** programs with intentional unused vars; programs
where every var is used.

---

## Stage 12 — Documentation + tutorial

**Goal:** a contributor with an idea for a pass can ship it.

**Deliverables:**

  - Update `docs/design/self-hosting-compiler-passes.md` to mark
    Stages 0–11 done; refresh "what's still ahead" section.
  - Refresh `docs/rust-runtime-capabilities.md` with the new CLI
    subcommands, runtime API surface, and stdlib pass files.
  - New `docs/tutorials/writing-a-pass.md` walking through:
    1. The shape of a pass (claim with `program ∈ Program` etc.)
    2. Loading order (`stdlib/ast.ev` first, then the pass, then
       `mark_system_loads_complete`)
    3. The two invocation methods (`query_with_program` for fixed
       patterns; `query_with_program_and_body` for iteration)
    4. The CLI integration story (adding a new pass to the dispatch
       table in `commands/infer_types.rs` or writing a new
       `commands/<name>.rs`)
    5. Worked example: a tiny pass written from scratch.

**Test surface:** none — documentation. The tutorial's worked
example doubles as a working test program if it's checked into
`programs/lang_tests/`.

---

## Sequencing

8 → 9 → 10 → 11 → 12 is the natural order:

  - 8 unblocks 9 (per-claim iteration is needed for cross-claim
    propagation).
  - 9 sets up 10 (conflict detection is most useful when conflicting
    inferences are common, which `=` propagation makes).
  - 11 demonstrates breadth and is a natural test of how reusable
    the runtime infrastructure actually is.
  - 12 codifies what shipped.

Each stage is independently shippable. Stage 8 alone is a real win;
Stages 9–11 are where the model becomes interesting beyond a demo.
