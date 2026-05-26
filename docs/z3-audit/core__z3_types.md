# runtime/src/core/z3_types.rs — Z3-replaceability

**What it does:** Defines the typed Z3 handle vocabulary: `Var<'ctx>` (IntVar/RealVar/BoolVar/StrVar/SeqVar/DatatypeSeqVar/SetVar/DatatypeSetVar/PinnedInt/EnumVar/EnumValue/EnumCtor), `FieldKind`/`SeqFieldElem` (composite field descriptors for Seq(UserType)), `DatatypeRegistry`/`EnumRegistry` (long-lived sort caches), and `CachedSchema` (pre-asserted solver cache). Accessor methods on `Var` provide typed downcasts. Used pervasively by translate/, runtime/, and functionize/.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This file defines the glue types that bind Evident's type system to Z3's sort system — the typed wrappers around live `z3::ast::*` handles. `DatatypeRegistry` and `EnumRegistry` are long-lived caches of Z3 `DatatypeSort` pointers (leaked to `'static`). `CachedSchema` holds a `Solver<'ctx>` with pre-asserted constraints. These structures ARE the Z3 integration layer; they are not a problem Z3 can solve. Tier 0 kernel: circular.

**Change made:** none
