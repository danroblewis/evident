# runtime/src/commands/common.rs — Z3-replaceability
**What it does:** Shared CLI helpers for all subcommands: flag parsing (`parse_flags`, `split_files_and_flags`), runtime loading (`load_runtime`, `load_runtime_with_passes`), the passthrough-desugar pipeline (`collect_passthrough_rewrites`, `auto_apply_desugar`), and value formatting (`format_value`). The desugar pipeline itself calls `rt.query_with_nth_claim_body_only_given` — i.e., it already IS a Z3 solve — to detect bare-identifier passthrough rewrites, then applies AST mutations to the caller's runtime.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Flag parsing, argument splitting, string-to-Value coercion, and value formatting are pure string/IO transforms — no search problem, no constraint system. The `auto_apply_desugar` orchestration already wraps a Z3 query (the desugar rule lives in `stdlib/passes/desugar_passthrough.ev`); the Rust here is the glue that wires query results back into AST mutations, which is not itself a CSP. Spec in one line: `desugar_glue(files) = apply_rewrites(collect_passthrough_rewrites(files))`.
**Change made:** none
