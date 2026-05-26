# runtime/src/chc.rs — Z3-replaceability
**What it does:** Provides a thin, refcount-safe Rust wrapper over the raw `z3-sys` Fixedpoint/Spacer API (CHC / Horn-clause engine). Exposes `Relation`, `Fixedpoint`, and `ChcResult` so callers can register Horn clauses and pose safety queries against Spacer — the unbounded, IC3/PDR-modulo-theories prover. A worked countdown example lives in the tests.
**Criticality:** peripheral (additive — on no existing runtime path; not yet selector-wired into compose.rs)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS a Z3 binding layer — it calls `Z3_mk_fixedpoint`, `Z3_fixedpoint_add_rule`, and `Z3_fixedpoint_query` via raw `z3-sys`. Spacer itself is a Z3 engine (IC3/PDR); replacing this module with "a Z3 constraint solve" would mean calling Spacer to implement Spacer's own interface, which is circular. The correct framing is that this file is the Rust glue that lets Evident programs eventually drive CHC queries; it is part of the Z3-interface layer, not a domain algorithm that could itself be expressed as a constraint.
**Change made:** none
