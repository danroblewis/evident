# runtime/src/translate/eval/decompose.rs — Z3-replaceability
**What it does:** `analyze_decomposition` builds a Z3 solver for a schema (same as `evaluate`) but skips `check()`, extracts the assertion set and free-var names, and calls `crate::decompose::decompose` to partition them into independent connected components. `classify_components` adds a per-component functional verdict using a 2-copy uniqueness check (assert a different-from-model assignment; UNSAT means functional/unique).
**Criticality:** peripheral (used only by diagnostic/analysis APIs — `runtime::analysis.rs`, explore examples, probe_mario — never on the per-tick scheduler path)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** The connected-components partition in `crate::decompose` is itself a graph algorithm (could be expressed as a constraint), but `decompose.rs` as a whole is circular: `classify_components` runs Z3 solves (an initial `check()` plus one push/check/pop per component) to determine uniqueness properties. Replacing these Z3 calls with a Z3 solve would be circular. `analyze_decomposition` alone just feeds into `crate::decompose::decompose` — that graph-partition module is the potentially-replaceable part, but it is a separate file.
**Change made:** none
