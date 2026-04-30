from __future__ import annotations
from dataclasses import dataclass, field
from typing import Literal, Optional, Union

# ── Top level ────────────────────────────────────────────────────────────────

@dataclass
class Program:
    statements: list[Statement]

Statement = Union[
    "SchemaDecl", "EnumDecl", "ImportStmt",
    "AssertStmt", "ForwardRule", "QueryStmt", "ConstraintStmt"
]

@dataclass
class ImportStmt:
    path: str   # raw path string, e.g. "beavers.ev"

@dataclass
class MultiMembershipDecl:
    """x, y, z ∈ Type — shorthand for multiple same-type body declarations."""
    names: list[str]
    set: "Expr"

@dataclass
class EnumDecl:
    name: str
    variants: list[str]

@dataclass
class InlineEnumExpr:
    """Anonymous enum set written inline: x ∈ Red | Green | Blue."""
    variants: list[str]

@dataclass
class SchemaDecl:
    keyword: Literal["schema", "type", "claim"]
    name: str
    params: list[Param]
    body: list[BodyItem]

@dataclass
class Param:
    names: list[str]
    set: Expr

BodyItem = Union["Constraint", "EvidentBlock", "PassthroughItem", "MultiMembershipDecl"]

@dataclass
class EvidentBlock:
    patterns: list[PatternArg]
    guard: Optional[Expr]
    body: list[BodyItem]

@dataclass
class PassthroughItem:
    name: str
    mappings: list[InlineMapping]

@dataclass
class AssertStmt:
    name: str
    value: Optional[Expr]      # None = unbound
    member_of: Optional[Expr]  # assert x ∈ Type
    args: list[Expr]           # assert edge 1 2

@dataclass
class ForwardRule:
    premises: list[ApplicationConstraint]
    conclusion: ApplicationConstraint

@dataclass
class QueryStmt:
    constraint: Constraint

@dataclass
class ConstraintStmt:
    constraint: Constraint

# ── Constraints ───────────────────────────────────────────────────────────────

Constraint = Union[
    "MembershipConstraint",
    "ArithmeticConstraint",
    "UniversalConstraint",
    "ExistentialConstraint",
    "CardinalityConstraint",
    "ApplicationConstraint",
    "LogicConstraint",
    "BindingConstraint",
    "SetEqualityConstraint",
]

@dataclass
class MembershipConstraint:
    op: Literal["∈", "∉", "⊆", "⊇"]
    left: Expr
    right: Expr

@dataclass
class ArithmeticConstraint:
    op: Literal["=", "≠", "<", ">", "≤", "≥", "starts_with", "ends_with", "contains", "matches"]
    left: Expr
    right: Expr

@dataclass
class UniversalConstraint:
    bindings: list[Binding]
    body: Constraint

@dataclass
class ExistentialConstraint:
    quantifier: Literal["∃", "∃!", "¬∃"]
    bindings: list[Binding]
    body: Constraint

@dataclass
class CardinalityConstraint:
    op: Literal["exactly", "at_most", "at_least"]
    count: Expr
    set: Expr

@dataclass
class ApplicationConstraint:
    name: str
    args: list[Expr] = field(default_factory=list)
    mappings: list[InlineMapping] = field(default_factory=list)
    block_mappings: list[BlockMapping] = field(default_factory=list)

@dataclass
class InlineMapping:
    slot: str
    value: Expr

@dataclass
class BlockMapping:
    slot: str
    value: Expr

@dataclass
class LogicConstraint:
    op: Literal["¬", "∧", "∨", "⇒"]
    right: Constraint
    left: Optional[Constraint] = None

@dataclass
class BindingConstraint:
    name: str
    value: Expr

@dataclass
class SetEqualityConstraint:
    set: Expr
    value: Expr

# ── Bindings ──────────────────────────────────────────────────────────────────

@dataclass
class Binding:
    names: list[str]
    set: Expr
    guard: Optional[Expr] = None
    distinct: bool = False     # ∀ a ≠ b ∈ S

# ── Expressions ───────────────────────────────────────────────────────────────

Expr = Union[
    "Identifier", "FieldAccess", "TupleIndex", "FilterExpr",
    "SetComprehension", "SetLiteral", "EmptySet", "RangeLiteral",
    "TupleLiteral", "BinaryExpr", "UnaryExpr", "CardinalityExpr",
    "ChainExpr", "NatLiteral", "IntLiteral", "RealLiteral",
    "StringLiteral", "BoolLiteral", "InlineEnumExpr",
]

@dataclass
class Identifier:
    name: str

@dataclass
class FieldAccess:
    obj: Expr
    field: str

@dataclass
class TupleIndex:
    obj: Expr
    index: int

@dataclass
class FilterExpr:
    set: Expr
    condition: Expr

@dataclass
class SetComprehension:
    output: Expr
    generators: list[ComprehensionGenerator]

@dataclass
class ComprehensionGenerator:
    binding: Optional[Binding] = None
    constraint: Optional[Constraint] = None

@dataclass
class SetLiteral:
    elements: list[Expr]

@dataclass
class EmptySet:
    pass

@dataclass
class RangeLiteral:
    from_: Expr
    to: Expr

@dataclass
class TupleLiteral:
    elements: list[Expr]

@dataclass
class BinaryExpr:
    op: Literal["+", "-", "*", "/", "++", "∪", "∩", "\\", "×"]
    left: Expr
    right: Expr

@dataclass
class UnaryExpr:
    op: Literal["¬", "-"]
    operand: Expr

@dataclass
class CardinalityExpr:
    set: Expr

@dataclass
class ChainExpr:
    op: Literal["·", "⋈"]
    left: Expr
    right: Expr

# ── Literals ──────────────────────────────────────────────────────────────────

@dataclass
class NatLiteral:    value: int
@dataclass
class IntLiteral:    value: int
@dataclass
class RealLiteral:   value: float
@dataclass
class StringLiteral: value: str
@dataclass
class BoolLiteral:   value: bool

# ── Pattern args (for evident blocks) ────────────────────────────────────────

PatternArg = Union[
    "PatternIdentifier", "PatternLiteral", "PatternEmptyList",
    "PatternCons", "PatternRecord", "PatternWildcard",
]

@dataclass
class PatternIdentifier: name: str
@dataclass
class PatternLiteral:    value: Expr
@dataclass
class PatternEmptyList:  pass
@dataclass
class PatternCons:       head: PatternArg; tail: PatternArg
@dataclass
class PatternRecord:     fields: list[PatternField]
@dataclass
class PatternWildcard:   pass

@dataclass
class PatternField:
    name: str
    binding: Optional[str] = None
