# Findings: runtime/src/commands/sample.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None. The active rulebook (AP-001..AP-008) covers language-core
library leaks, raw FFI in examples, platform paths/C symbols in
examples, skipped/xfail tests in conformance and Rust tests, and
example-file shape (sat_*/unsat_*, FSM-shape, EXPECTATIONS row).
None of these scopes apply to `runtime/src/commands/sample.rs`.

The file also satisfies its per-file invariants from
`runtime-invariants.md` (Group 6, "simple cmd_* files"):

  - Exactly one `pub fn cmd_sample`.
  - Imports limited to `evident_runtime::*` and `super::common::*`
    (plus `std`, `std::process::ExitCode`).
  - 69 lines (under the ~100-line soft cap).
  - No reach into `crate::*` runtime internals.
  - No direct Z3 / Solver use; routes through `rt.sample`.
  - No state across calls.

## Candidate new rules

### Suggested AP-009: cmd-flag-stripping-via-filter
**Pattern observed at runtime/src/commands/sample.rs:14-17 (also
runtime/src/commands/query.rs:17-20):**
> ```rust
> let strict = args.iter().any(|a| a == "--strict");
> let stripped: Vec<String> = args.iter()
>     .filter(|a| a.as_str() != "--strict")
>     .cloned().collect();
> ```

**Why it might be bad:** Two `cmd_*` files now duplicate the same
hand-rolled "detect a flag, then filter it out before re-parsing"
pattern. The flag is recognized in the per-command file, not in
`common::parse_flags`, which means `Flags` doesn't know about
`--strict` — every new command that wants the same flag must
copy this 4-line idiom. It also defeats `parse_flags`' own
"unknown flag" rejection (since `--strict` is silently consumed
upstream rather than registered as a known flag). If a third
command (`effect-run`?) wants `--strict`, the pattern triples.

The Group 6 invariant says: "if helpers are only used by a
single `cmd_*` file, they live in that file, not here. … Reach
into runtime internals — uses only `evident_runtime::*`." This
isn't internals-reach, but it IS the same invariant in the
other direction: a flag shared across multiple commands belongs
in `common::Flags`, not duplicated per-file.

**Suggested fix:** Add `pub strict: bool` to `common::Flags`
with `Default::default()` of `false`, and have
`parse_flags` recognize `--strict`. Each `cmd_*` then reads
`flags.strict` instead of pre-filtering. Removes the duplicated
filter blocks from both `query.rs` and `sample.rs`.

**Detection idea:** grep — `args\.iter\(\)\.any\(\|a\| a ==`
followed within ~5 lines by `\.filter\(\|a\| a\.as_str\(\) !=`
in any `runtime/src/commands/cmd_*.rs` file. Two or more hits =
duplication; one hit = candidate for promotion.

(Review-only candidate as written — the pattern recurs (n=2
today) but isn't yet egregious enough to mechanize. Promote to
an active rule when a third command adopts the same idiom or
when `Flags` is touched for an unrelated reason.)

### Suggested AP-010: hand-rolled-json-emission-in-commands
**Pattern observed at runtime/src/commands/sample.rs:48-58:**
> ```rust
> if flags.json {
>     print!("[");
>     for (i, s) in samples.iter().enumerate() {
>         if i > 0 { print!(", "); }
>         let mut keys: Vec<&String> = s.keys().collect(); keys.sort();
>         let parts: Vec<_> = keys.iter()
>             .map(|k| format!("\"{}\": {}", k, value_as_json(&s[*k])))
>             .collect();
>         print!("{{{}}}", parts.join(", "));
>     }
>     println!("]");
> }
> ```

**Why it might be bad:** `common.rs` already provides
`value_as_json` and `json_str` for value-level serialization, and
`print_query_result` knows how to render a single
`HashMap<String, Value>` as a JSON object (the satisfied-bindings
arm at lines 105-113). `sample.rs` open-codes the array-of-
objects wrapper — sorted keys, comma joins, brace-bracket
weaving — instead of factoring out an `emit_bindings_json`
helper that both `query` and `sample` could call. The two JSON
emitters can drift (e.g., trailing-newline behavior, NaN
handling, key-sort order) because nothing keeps them in sync.

**Suggested fix:** Lift the per-binding emitter
(sorted-keys + `value_as_json` + `{}`-wrap) into
`common::bindings_as_json(map: &HashMap<String, Value>) -> String`.
Then `sample`'s JSON arm becomes `samples.iter().map(bindings_as_json).collect::<Vec<_>>().join(", ")`
inside a `[...]`. `print_query_result`'s satisfied arm calls
the same helper.

**Detection idea:** grep — multiple occurrences of `print!("\\[")`
or `format!("{{{{")` patterns in `runtime/src/commands/cmd_*.rs`,
or any `cmd_*` file that does its own `keys.sort()` + `value_as_json`
loop instead of calling a `common::*_json` helper.

(Review-only candidate. The duplication is minor today; promote
when a third command needs JSON output.)

## Clean-ish

The file is rule-clean against the active rulebook. The two
candidates above are observations about cross-`cmd_*` duplication
patterns, not violations of anything written down today.
