# runtime/src/translate/inline/membership.rs — Z3-replaceability
**What it does:** Handles `Membership` body items: declares the variable as a Z3 constant, resolves and fires type-use pins (named or positional), then inherits the type's body `Constraint` items onto the instance by prefixing field references. Also handles `Seq(SomeType)` membership by unrolling type constraints per element index.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file expands membership declarations into Z3 variable declarations and constraint assertions — it constructs the Z3 problem. The prefix-rewriting and per-element unrolling are compile-time AST transformations that must complete before Z3 runs. Replacing this with a Z3 solve would be circular (Z3 needs this output to have anything to solve).
**Change made:** none
