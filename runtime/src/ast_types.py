"""
Re-export of the parser's AST types needed by the runtime.

Ensure the project root is on sys.path so `parser.src.ast` is importable
as a regular package, giving a single shared module instance. This is
critical: isinstance() checks in load_program() only work when the runtime
and the parser share the same class objects.
"""
import sys
import os

# Add project root so `parser` is importable as a package.
_project_root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from parser.src.ast import (  # noqa: E402
    Program,
    SchemaDecl,
    EnumDecl,
    Param,
    BodyItem,
    Constraint,
    MembershipConstraint,
    ArithmeticConstraint,
    UniversalConstraint,
    ExistentialConstraint,
    CardinalityConstraint,
    ApplicationConstraint,
    InlineMapping,
    BlockMapping,
    LogicConstraint,
    BindingConstraint,
    SetEqualityConstraint,
    Binding,
    Identifier,
    FieldAccess,
    TupleIndex,
    FilterExpr,
    SetComprehension,
    ComprehensionGenerator,
    SetLiteral,
    EmptySet,
    RangeLiteral,
    TupleLiteral,
    BinaryExpr,
    UnaryExpr,
    CardinalityExpr,
    ChainExpr,
    NatLiteral,
    IntLiteral,
    RealLiteral,
    StringLiteral,
    BoolLiteral,
    EvidentBlock,
    PassthroughItem,
    AssertStmt,
    ForwardRule,
    QueryStmt,
    ConstraintStmt,
    PatternIdentifier,
    PatternLiteral,
    PatternEmptyList,
    PatternCons,
    PatternRecord,
    PatternWildcard,
    PatternField,
)

__all__ = [
    "Program", "SchemaDecl", "EnumDecl", "Param", "BodyItem", "Constraint",
    "MembershipConstraint", "ArithmeticConstraint", "UniversalConstraint",
    "ExistentialConstraint", "CardinalityConstraint", "ApplicationConstraint",
    "InlineMapping", "BlockMapping", "LogicConstraint", "BindingConstraint",
    "SetEqualityConstraint", "Binding", "Identifier", "FieldAccess",
    "TupleIndex", "FilterExpr", "SetComprehension", "ComprehensionGenerator",
    "SetLiteral", "EmptySet", "RangeLiteral", "TupleLiteral", "BinaryExpr",
    "UnaryExpr", "CardinalityExpr", "ChainExpr", "NatLiteral", "IntLiteral",
    "RealLiteral", "StringLiteral", "BoolLiteral", "EvidentBlock",
    "PassthroughItem", "AssertStmt", "ForwardRule", "QueryStmt",
    "ConstraintStmt", "PatternIdentifier", "PatternLiteral", "PatternEmptyList",
    "PatternCons", "PatternRecord", "PatternWildcard", "PatternField",
]
