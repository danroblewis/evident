# runtime/src/subscriptions.rs — Z3-replaceability
**What it does:** Provides the `AccessSets` type (read/write sets per FSM) and a single `body_references_identifier` AST-walk helper. The actual world-access-set inference was already cut over to `stdlib/passes/subscriptions.ev` in a prior session; this file is the residual Rust shim that holds only the type and the fd-conflict detector.
**Criticality:** peripheral
**Verdict:** replaceable-as-group(subscriptions.rs, stdlib/passes/subscriptions.ev)
**Confidence:** high
**How (if replaceable):** The Evident pass (`subscriptions.ev`) already handles the read/write-set inference via a constraint walk. `body_references_identifier` is a simple recursive AST walk (does `ident` appear anywhere in the body?) — trivially expressible as an Evident claim over the AST reflection API, and semantically analogous to the existing walk. However, this file is currently a ~103-line load-time helper; there is no Z3 *solve* that buys anything here. Replacement would fold into the existing Evident subscriptions pass, not introduce a new solve. The benefit is marginal (the Rust residual is small). As documented in project memory, the full cutover is already done — this file is the intentional thin shim.
**Change made:** none
