# runtime/src/event_sources/reflection.rs — Z3-replaceability

**What it does:** One-shot bridge that encodes the loaded Program AST as a `Value` tree (matching `stdlib/ast.ev`) and writes it to a world field typed `Program` on tick 0. The `install_world_plugin` decision: scan world fields for exactly one `Program`-typed field, encode, queue the write.

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** The install decision (count fields with type == "Program", reject >1) could in principle be expressed as a constraint over the world-field map, but it's a 3-line Rust iterator with no combinatorial structure — a solve buys nothing. The payload (encoding the AST as a Value tree) is a pure recursive transformation that runs once at startup; it is not a search problem.

**Change made:** none
