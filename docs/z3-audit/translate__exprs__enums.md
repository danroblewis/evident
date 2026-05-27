# runtime/src/translate/exprs/enums.rs — Z3-replaceability
**What it does:** Translates Evident enum-typed expressions (EnumVar/EnumValue identifiers, constructor calls, Seq(enum) indexing, Ternary, Match, and SeqLit Cons-chains) into Z3 `Datatype<'ctx>` AST nodes. Also provides `build_cons_chain` which materializes `Cons/Nil` Z3 datatype values from Evident `⟨a,b,c⟩` sequence literals.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file constructs Z3 datatype AST nodes from Evident enum expressions; it is part of the compile pipeline that produces the Z3 input. Replacing the enum-lowering pass with a Z3 solve would presuppose the lowering has already happened — a direct bootstrap cycle. There is no separable property being decided here.
**Change made:** none
