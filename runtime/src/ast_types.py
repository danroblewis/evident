"""
Re-export of the parser's AST types needed by the runtime.

We import directly from the parser package.  If the parser is not on
sys.path the import will fail with a clear message.

The parser module is named ``ast.py``, which shadows Python's built-in
``ast`` module.  We use ``importlib`` to load it under an unambiguous name.
"""
import sys
import os
import importlib.util

# Absolute path to the parser's ast.py
_parser_ast_path = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "..", "parser", "src", "ast.py")
)

# Load the module under a unique name to avoid shadowing the stdlib `ast`.
_spec = importlib.util.spec_from_file_location("parser_ast", _parser_ast_path)
_parser_ast = importlib.util.module_from_spec(_spec)
# Register before exec so that @dataclass can resolve the module's __dict__
sys.modules["parser_ast"] = _parser_ast
_spec.loader.exec_module(_parser_ast)

# Re-export everything that downstream code needs.
Program = _parser_ast.Program
SchemaDecl = _parser_ast.SchemaDecl
Param = _parser_ast.Param
BodyItem = _parser_ast.BodyItem
Constraint = _parser_ast.Constraint
MembershipConstraint = _parser_ast.MembershipConstraint
ArithmeticConstraint = _parser_ast.ArithmeticConstraint
UniversalConstraint = _parser_ast.UniversalConstraint
ExistentialConstraint = _parser_ast.ExistentialConstraint
CardinalityConstraint = _parser_ast.CardinalityConstraint
ApplicationConstraint = _parser_ast.ApplicationConstraint
InlineMapping = _parser_ast.InlineMapping
BlockMapping = _parser_ast.BlockMapping
LogicConstraint = _parser_ast.LogicConstraint
BindingConstraint = _parser_ast.BindingConstraint
SetEqualityConstraint = _parser_ast.SetEqualityConstraint
Binding = _parser_ast.Binding
Identifier = _parser_ast.Identifier
FieldAccess = _parser_ast.FieldAccess
TupleIndex = _parser_ast.TupleIndex
FilterExpr = _parser_ast.FilterExpr
SetComprehension = _parser_ast.SetComprehension
ComprehensionGenerator = _parser_ast.ComprehensionGenerator
SetLiteral = _parser_ast.SetLiteral
EmptySet = _parser_ast.EmptySet
RangeLiteral = _parser_ast.RangeLiteral
TupleLiteral = _parser_ast.TupleLiteral
BinaryExpr = _parser_ast.BinaryExpr
UnaryExpr = _parser_ast.UnaryExpr
CardinalityExpr = _parser_ast.CardinalityExpr
ChainExpr = _parser_ast.ChainExpr
NatLiteral = _parser_ast.NatLiteral
IntLiteral = _parser_ast.IntLiteral
RealLiteral = _parser_ast.RealLiteral
StringLiteral = _parser_ast.StringLiteral
BoolLiteral = _parser_ast.BoolLiteral
EvidentBlock = _parser_ast.EvidentBlock
PassthroughItem = _parser_ast.PassthroughItem
AssertStmt = _parser_ast.AssertStmt
ForwardRule = _parser_ast.ForwardRule
QueryStmt = _parser_ast.QueryStmt
ConstraintStmt = _parser_ast.ConstraintStmt
PatternIdentifier = _parser_ast.PatternIdentifier
PatternLiteral = _parser_ast.PatternLiteral
PatternEmptyList = _parser_ast.PatternEmptyList
PatternCons = _parser_ast.PatternCons
PatternRecord = _parser_ast.PatternRecord
PatternWildcard = _parser_ast.PatternWildcard
PatternField = _parser_ast.PatternField

__all__ = [
    "Program", "SchemaDecl", "Param", "BodyItem", "Constraint",
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
