# runtime/src/core/ast.rs — Z3-replaceability

**What it does:** Defines the complete Evident AST: `SchemaDecl`, `BodyItem`, `Expr`, `BinOp`, `MatchPattern`, `Program`, `EnumDecl`, `Effect`, `EffectResult`, `EffectFfiArg`, `PackedField`, `Keyword` — every node type produced by the parser and consumed by the translator.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This file is pure Tier 0 kernel: the grammar's data model. It IS the AST that programs run against. Every other module in the runtime (parser, translator, functionizer, effect_loop, portable passes) imports from here. There is no algorithm here — only type definitions. Z3 solves constraint satisfaction problems; it cannot replace a recursive enum type definition.

**Change made:** none (file is OFF-LIMITS per audit instructions)
