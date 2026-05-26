# runtime/src/commands/effect_run.rs — Z3-replaceability
**What it does:** Implements `evident effect-run <file>`: parses CLI flags, resolves the functionizer strategy (flag > env > source-marker), constructs the appropriate `EvidentRuntime` variant, loads stdlib + user file, runs `auto_apply_desugar`, and drives the multi-FSM scheduler via `effect_loop::run`. Handles exit codes, profiling summaries, and error reporting.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Pure process orchestration: argument parsing, env-var setting, runtime construction, file loading, and loop invocation. No search problem; the actual constraint solving happens inside `effect_loop::run`. Spec in one line: `effect_run(args) = load(file) → desugar → loop(runtime, opts) → exit_code`.
**Change made:** none
