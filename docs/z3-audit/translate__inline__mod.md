# runtime/src/translate/inline/mod.rs — Z3-replaceability
**What it does:** Module declaration file for `translate::inline`. Declares the eight submodules and re-exports only the two public entry points (`inline_body_items` and `inline_body_items_tracked`) from `walk.rs` to the rest of `translate`.
**Criticality:** peripheral
**Verdict:** trivial
**Confidence:** high
**How (if replaceable):** Pure glue — 14 lines of `mod` declarations and one `pub(in crate::translate) use`. No logic of its own.
**Change made:** none
