// Evident AST Node Types

export type Node =
  | Program
  | Statement
  | SchemaDecl
  | EvidentBlock
  | Constraint
  | Binding
  | Expr
  | Literal
  | PatternArg;

// ── Top level ────────────────────────────────────────────────────────────────

export interface Program {
  type: "Program";
  statements: Statement[];
}

export type Statement =
  | SchemaDecl
  | AssertStmt
  | ForwardRule
  | QueryStmt
  | ConstraintStmt;

export interface SchemaDecl {
  type: "SchemaDecl";
  keyword: "schema" | "type" | "claim";
  name: string;
  params: Param[];
  body: BodyItem[];
}

export interface Param {
  names: string[];          // a, b, c
  set: Expr;                // ∈ Type
}

export type BodyItem = Constraint | EvidentBlock | PassthroughItem;

export interface EvidentBlock {
  type: "EvidentBlock";
  patterns: PatternArg[];
  guard?: Expr;
  body: BodyItem[];
}

export interface PassthroughItem {
  type: "Passthrough";
  name: string;
  mappings: InlineMapping[];
}

export interface AssertStmt {
  type: "AssertStmt";
  name: string;
  value: Expr | null;       // null = unbound (assert x ∈ Type)
  memberOf?: Expr;          // for assert x ∈ Type
  args: Expr[];             // for assert edge 1 2
}

export interface ForwardRule {
  type: "ForwardRule";
  premises: ApplicationConstraint[];
  conclusion: ApplicationConstraint;
}

export interface QueryStmt {
  type: "QueryStmt";
  constraint: Constraint;
}

export interface ConstraintStmt {
  type: "ConstraintStmt";
  constraint: Constraint;
}

// ── Constraints ───────────────────────────────────────────────────────────────

export type Constraint =
  | MembershipConstraint
  | ArithmeticConstraint
  | UniversalConstraint
  | ExistentialConstraint
  | CardinalityConstraint
  | ApplicationConstraint
  | LogicConstraint
  | BindingConstraint
  | SetEqualityConstraint;

export interface MembershipConstraint {
  type: "MembershipConstraint";
  op: "∈" | "∉" | "⊆" | "⊇";
  left: Expr;
  right: Expr;
}

export interface ArithmeticConstraint {
  type: "ArithmeticConstraint";
  op: "=" | "≠" | "<" | ">" | "≤" | "≥";
  left: Expr;
  right: Expr;
}

export interface UniversalConstraint {
  type: "UniversalConstraint";
  bindings: Binding[];
  body: Constraint;
}

export interface ExistentialConstraint {
  type: "ExistentialConstraint";
  quantifier: "∃" | "∃!" | "¬∃";
  bindings: Binding[];
  body: Constraint;
}

export interface CardinalityConstraint {
  type: "CardinalityConstraint";
  op: "exactly" | "at_most" | "at_least";
  count: Expr;
  set: Expr;
}

export interface ApplicationConstraint {
  type: "ApplicationConstraint";
  name: string;
  mappings: InlineMapping[];
  blockMappings?: BlockMapping[];
}

export interface InlineMapping {
  slot: string;
  value: Expr;
}

export interface BlockMapping {
  slot: string;
  value: Expr;
}

export interface LogicConstraint {
  type: "LogicConstraint";
  op: "¬" | "∧" | "∨" | "⇒";
  left?: Constraint;
  right: Constraint;
}

export interface BindingConstraint {
  type: "BindingConstraint";
  name: string;
  value: Expr;
}

export interface SetEqualityConstraint {
  type: "SetEqualityConstraint";
  set: Expr;
  value: Expr;   // typically EmptySet or SetExpr
}

// ── Bindings (for quantifiers) ───────────────────────────────────────────────

export interface Binding {
  names: string[];
  set: Expr;
  guard?: Expr;
  distinct?: boolean;       // ∀ a ≠ b ∈ S
}

// ── Expressions ───────────────────────────────────────────────────────────────

export type Expr =
  | Identifier
  | FieldAccess
  | TupleIndex
  | FilterExpr
  | SetComprehension
  | SetLiteral
  | EmptySet
  | RangeLiteral
  | TupleLiteral
  | BinaryExpr
  | UnaryExpr
  | CardinalityExpr
  | ChainExpr
  | Literal
  | GroupedExpr;

export interface Identifier {
  type: "Identifier";
  name: string;
}

export interface FieldAccess {
  type: "FieldAccess";
  object: Expr;
  field: string;
}

export interface TupleIndex {
  type: "TupleIndex";
  object: Expr;
  index: number;           // .0, .1
}

export interface FilterExpr {
  type: "FilterExpr";
  set: Expr;
  condition: Expr;         // . refers to current element
}

export interface SetComprehension {
  type: "SetComprehension";
  output: Expr;
  generators: ComprehensionGenerator[];
}

export interface ComprehensionGenerator {
  binding?: Binding;
  constraint?: Constraint;
}

export interface SetLiteral {
  type: "SetLiteral";
  elements: Expr[];
}

export interface EmptySet {
  type: "EmptySet";
}

export interface RangeLiteral {
  type: "RangeLiteral";
  from: Expr;
  to: Expr;
}

export interface TupleLiteral {
  type: "TupleLiteral";
  elements: Expr[];
}

export interface BinaryExpr {
  type: "BinaryExpr";
  op: "+" | "-" | "*" | "/" | "∪" | "∩" | "\\" | "×";
  left: Expr;
  right: Expr;
}

export interface UnaryExpr {
  type: "UnaryExpr";
  op: "¬";
  operand: Expr;
}

export interface CardinalityExpr {
  type: "CardinalityExpr";
  set: Expr;
}

export interface ChainExpr {
  type: "ChainExpr";
  op: "·" | "⋈";
  left: Expr;
  right: Expr;
}

export interface GroupedExpr {
  type: "GroupedExpr";
  expr: Expr;
}

// ── Literals ──────────────────────────────────────────────────────────────────

export type Literal = NatLiteral | IntLiteral | RealLiteral | StringLiteral | BoolLiteral;

export interface NatLiteral   { type: "NatLiteral";    value: number; }
export interface IntLiteral   { type: "IntLiteral";    value: number; }
export interface RealLiteral  { type: "RealLiteral";   value: number; }
export interface StringLiteral{ type: "StringLiteral"; value: string; }
export interface BoolLiteral  { type: "BoolLiteral";   value: boolean; }

// ── Pattern args (for evident blocks) ────────────────────────────────────────

export type PatternArg =
  | PatternIdentifier
  | PatternLiteral
  | PatternEmptyList
  | PatternCons
  | PatternRecord
  | PatternWildcard;

export interface PatternIdentifier { type: "PatternIdentifier"; name: string; }
export interface PatternLiteral    { type: "PatternLiteral";    value: Literal; }
export interface PatternEmptyList  { type: "PatternEmptyList"; }
export interface PatternCons       { type: "PatternCons"; head: PatternArg; tail: PatternArg; }
export interface PatternRecord     { type: "PatternRecord"; fields: PatternField[]; }
export interface PatternWildcard   { type: "PatternWildcard"; }

export interface PatternField {
  name: string;
  binding?: string;   // { name = binding } or just { name }
}
