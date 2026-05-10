# Writing a Self-Hosted Compiler Pass

Self-hosted passes in Evident are claims that consume an injected
`Program` (and optionally a flat `Seq(BodyItem)`) and produce
constraints over output variables. They run via the standard
solver pipeline; the runtime layer is just *parse → encode → inject
→ query → render*.

This tutorial walks through the shape of a pass, the loading
contract, the two invocation methods, CLI integration, and a
worked example.

## Anatomy of a pass

Every pass is one or more `claim` declarations in an `.ev` file
under `stdlib/passes/`. The minimal shape:

```evident
import "stdlib/ast.ev"

claim my_rule
    program ∈ Program        -- the injected user program
    -- output variables the runtime reads back from the model
    inferred_var ∈ String
    inferred_type ∈ String
    -- structural pattern: SAT iff the program matches and
    -- the outputs are bound consistently
    program = MakeProgram(...)
    inferred_var = ...
    inferred_type = ...
```

The runtime loads the pass file, asserts `program = <encoded user
Program>`, then runs the claim's solver. SAT means "rule fired";
UNSAT means "this rule doesn't apply to this program."

## Two flavors of pass

### Pattern-matching passes

Use `query_with_program(claim_name, "program")`. The pass
pattern-matches a fixed shape:

```evident
claim infer_string_from_single_assignment
    program       ∈ Program
    claim_name    ∈ String
    inferred_var  ∈ String
    inferred_type ∈ String

    string_lit ∈ String
    enums_part ∈ EnumDeclList
    program = MakeProgram(
        SchLCons(
            MakeSchemaDecl(KClaim, claim_name,
                BILCons(BIConstraint(EBinary(OpEq,
                                             EIdentifier(inferred_var),
                                             EStr(string_lit))),
                       BILNil)),
            SchLNil),
        enums_part)
    inferred_type = "String"
```

This rule fires for `claim NAME : var = "literal"`. Z3 binds
`claim_name`, `inferred_var`, `string_lit`, `enums_part` to
whatever values satisfy the equality.

Limitation: the pattern is fixed. A 3-body program won't match.

### Iteration passes

Use `query_with_program_and_body(claim_name, "program", "body")`
or its multi-claim sibling
`query_with_program_and_nth_claim_body(...)`. The pass iterates
over the user's first (or n-th) claim's body via `∀` / `∃`:

```evident
claim has_string_assignment
    program     ∈ Program
    body        ∈ Seq(BodyItem)
    body_len    ∈ Nat
    target_var  ∈ String
    string_lit  ∈ String

    ∃ i ∈ {0..body_len - 1} :
        body[i] = BIConstraint(EBinary(OpEq,
                                       EIdentifier(target_var),
                                       EStr(string_lit)))
```

The runtime asserts `body_len = N` and `body[i] = <encoded
items[i]>` for each i. Quantifiers unroll naturally; `∃` finds an
i where the equality holds and binds `target_var` / `string_lit`
to that body item's contents.

## Loading order

The runtime needs to know which schemas/enums are part of the
"system" (the pass file + stdlib) versus the "user" (the program
to be analyzed). The contract:

```rust
let mut rt = EvidentRuntime::new();
rt.load_file("stdlib/ast.ev")?;          // 1. AST shape
rt.load_file("stdlib/passes/your_pass.ev")?;   // 2. your pass
rt.mark_system_loads_complete();         // 3. snapshot
rt.load_file(user_path)?;                // 4. user program

let r = rt.query_with_program("your_rule", "program")?;
// or:
let r = rt.query_with_program_and_body("your_rule", "program", "body")?;
```

After `mark_system_loads_complete()`, the encoder filters the
program → only user-loaded schemas/enums show up. Forget step 3
and your pass's own claims will appear in the encoded `Program`
value, breaking pattern matches.

## Wiring into the CLI

Two integration points:

### Adding to `evident infer-types`

Edit `runtime/src/commands/infer_types.rs`:

```rust
const PROGRAM_RULES: &[&str] = &[
    // ... existing ...
    "your_new_inference_rule",   // add here for query_with_program
];

const ITER_RULES: &[&str] = &[
    // ... existing ...
    "your_new_iter_rule",        // add here for query_with_program_and_body
];
```

Plus add the `.ev` file to the load chain near the top of
`cmd_infer_types`. That's it — output rendering picks up the new
rule via the existing `render_bindings` + `label_for` dispatch.

### Adding a new subcommand

For passes that don't fit the inference workflow (lints,
optimizations, code transformers), write a new
`runtime/src/commands/<your>.rs` modeled on
`commands/lint.rs`. Wire it into `runtime/src/commands.rs`
and `runtime/src/main.rs`'s dispatch.

Define your own exit-code conventions. Existing precedent:

| Code | Meaning |
|---|---|
| 0 | clean / success |
| 1 | load / encode error |
| 2 | usage error |
| 3 | no rule matched (`infer-types`) |
| 4 | strict-mode failure (`infer-types --strict`) |
| 5 | lint findings (`lint`) |

## Worked example: a "must declare types" rule

Goal: a rule that fires when the user has a `BIConstraint` body
item but no preceding `BIMembership` for any of the variables
it references. (Catches forgotten type declarations.)

The simplified version: assert that the FIRST body item is a
Membership. If it's not, the program "starts with a constraint
without declaring anything" — flag it.

```evident
-- stdlib/passes/lint_undeclared.ev
import "stdlib/ast.ev"

claim first_body_is_constraint_without_decl
    program     ∈ Program
    body        ∈ Seq(BodyItem)
    body_len    ∈ Nat
    bad_expr    ∈ Expr

    body_len > 0
    body[0] = BIConstraint(bad_expr)
```

This SAT means "the first body item is a Constraint, not a
Membership." For programs that follow the convention of declaring
types first, this is UNSAT. For programs like:

```evident
claim t
    msg = "hello"   -- no `msg ∈ String` declaration first
```

it's SAT, and `bad_expr` binds to the offending Constraint's
expression.

Add a CLI runner:

```rust
// runtime/src/commands/lint_undeclared.rs (sketch)
const STDLIB_AST: &str = "stdlib/ast.ev";
const PASS:       &str = "stdlib/passes/lint_undeclared.ev";

pub fn cmd_lint_undeclared(args: &[String]) -> ExitCode {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_file(Path::new(PASS)).unwrap();
    rt.mark_system_loads_complete();
    rt.load_file(Path::new(&args[0])).unwrap();

    for claim_idx in 0..rt.user_claim_count() {
        let r = rt.query_with_program_and_nth_claim_body(
            "first_body_is_constraint_without_decl",
            "program", "body", claim_idx,
        ).unwrap();
        if let Some(r) = r {
            if r.satisfied {
                println!("claim {} starts with a constraint, no decl",
                         rt.user_claim_name(claim_idx).unwrap_or_default());
            }
        }
    }
    ExitCode::SUCCESS
}
```

Total work to ship a new pass: one `.ev` file (~15 lines), one
Rust file (~25 lines), three lines of dispatch wiring. The pass
itself is the hard part — and it's a constraint problem, which
is what Evident is for.

## Testing your pass

Two test surfaces — use both:

### Hand-built `.ev` conformance

Under `tests/lang_tests/test_pass_<name>.ev`. Construct
sample `Program` / `body` / `body_len` values via the
`MakeProgram(...)` constructors, invoke the rule via names-match,
assert the bound output values:

```evident
import "stdlib/ast.ev"
import "stdlib/passes/your_pass.ev"

claim sat_your_rule_finds_x
    program     ∈ Program
    body        ∈ Seq(BodyItem)
    body_len    ∈ Nat
    -- ... pin to a specific shape ...
    your_rule
    -- assert outputs match
    inferred_var = "x"
```

These tests run via `evident test` and don't depend on the Rust
encoder. A regression in the pass file fails here independently.

### Rust integration

Under `runtime/tests/<pass>_pass.rs`. Load the user source
as a real program string, call the runtime API, check bindings:

```rust
let mut rt = EvidentRuntime::new();
rt.load_file(Path::new("../stdlib/ast.ev")).unwrap();
rt.load_file(Path::new("../stdlib/passes/your_pass.ev")).unwrap();
rt.mark_system_loads_complete();
rt.load_source("claim t\n    x = \"hi\"\n").unwrap();

let r = rt.query_with_program("your_rule", "program").unwrap();
assert!(r.satisfied);
assert_eq!(r.bindings.get("inferred_var"),
           Some(&Value::Str("x".to_string())));
```

These tests verify the encode + inject + query round trip.

## Patterns and pitfalls

**Always declare `body_len ∈ Nat`** when iterating. The runtime
auto-injects the body's length under that exact name (a `given`
Int) so `n = #body` resolves and `∀ i ∈ {0..body_len - 1}`
unrolls cleanly. Without it, the quantifier is symbolic and Z3
silently drops the constraint.

**Use a `_part` suffix for "don't care" structural variables**
like `enums_part ∈ EnumDeclList` or `pins_part ∈ Pins`. They're
free vars Z3 binds to whatever the actual shape is; you ignore
them but they need to be present so the equality has a valid LHS
shape.

**Per-name instances of structural vars.** If you reuse a name
across different bodies of the same claim's body (e.g., two
different `EnumDeclList` slots), Z3 may incorrectly require both
to be the same. Use `enums_a` / `enums_b` style fresh names.

**Distinguish "rule didn't match" from "wrong shape".** UNSAT
means the rule's pattern doesn't apply. If you wanted a stronger
guarantee — "this program is consistent" — flip the polarity:
write a rule that's SAT *when the bug exists* (consistency.ev's
pattern), then treat SAT as a finding.

**Prefer existential over universal**. `∃` is cheap (one binding
per match). `∀` over body items unrolls into a conjunction —
useful for "every item must satisfy P" but more expensive and
harder to make selective.

## Reference index

| File | Purpose |
|---|---|
| `stdlib/ast.ev` | Canonical AST: enums for every node type |
| `stdlib/passes/literal_types.ev` | Pattern-matching inference rules (Stage 3+4) |
| `stdlib/passes/iter_types.ev` | Iteration-based inference (Stage 5.5) |
| `stdlib/passes/propagation.ev` | Cross-body-item `=` propagation (Stage 9) |
| `stdlib/passes/consistency.ev` | Type-mismatch detection (Stage 10) |
| `stdlib/passes/lint_duplicate_decls.ev` | Duplicate-declaration lint (Stage 11) |
| `runtime/src/translate/encode_ast.rs` | Rust → Z3 datatype encoder |
| `runtime/src/runtime.rs` | `EvidentRuntime` API |
| `runtime/src/commands/infer_types.rs` | `evident infer-types` CLI |
| `runtime/src/commands/lint.rs` | `evident lint` CLI |

For deeper background, see
[`docs/design/self-hosting-compiler-passes.md`](../design/self-hosting-compiler-passes.md).
