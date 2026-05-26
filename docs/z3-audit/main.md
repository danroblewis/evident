# runtime/src/main.rs — Z3-replaceability
**What it does:** Process entry point: reads `argv`, dispatches `sample` / `test` / `effect-run` to the matching `cmd_*` function, prints usage on unknown subcommand, exits with the returned `ExitCode`.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Four-arm match on a string followed by a function call. No computation, no data, no search problem. Spec in one line: `main(args) = dispatch(args[0], args[1..])`.
**Change made:** none
