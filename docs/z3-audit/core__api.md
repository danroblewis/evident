# runtime/src/core/api.rs — Z3-replaceability

**What it does:** Defines `QueryResult` (satisfied bool + bindings map) and `RuntimeError` (Parse/UnknownSchema/Io variants) — the public-facing query result and error types returned to all callers of the runtime API.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This file is a pure data-definition module — structs and enums with no logic, no computation, no algorithm. There is no constraint to solve and no problem to optimize. Replacing it with a Z3 solve would be meaningless: Z3 produces satisfying assignments, not type definitions.

**Change made:** none
