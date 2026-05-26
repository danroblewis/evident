# runtime/src/commands.rs — Z3-replaceability
**What it does:** Module root that re-exports the three subcommand modules (`common`, `effect_run`, `sample`, `test`). Contains only `pub mod` declarations — zero logic.
**Criticality:** does-little
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** No computation — purely structural Rust module wiring. Spec in one line: `commands = {common, effect_run, sample, test}`.
**Change made:** none
