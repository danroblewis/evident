# Blocked: `translate_bool` recursive `(and (or x y) z)` target

## Status

Partial. The recursive WorkList walker (`BoolTranslateStep` in
`compiler/translate_bool.ev`) is implemented and verified; the
`tests/kernel/test_translate_bool_recursive.ev` fixture proves
arbitrary-depth recursion on the Boolean operators the AST can carry.
The *specific* target output from the task spec —

    EBinOp(OpAnd, EBinOp(OpOr, EIdent("x"), EIdent("y")), EIdent("z"))
        →  (and (or x y) z)

— is **not reachable** without editing a frozen file. The fixture
instead exercises

    EBinOp(OpImpl, EBinOp(OpEq, EIdent("x"), EIdent("y")), EIdent("z"))
        →  (=> (= x y) z)

which drives the identical recursive machinery.

## The blocker

`EBinOp(OpAnd, …)` / `EBinOp(OpOr, …)` cannot be constructed. The
parser's binary-operator enum (`compiler/parser.ev`):

    enum Op =
        OpPlus  OpMul  OpEq  OpSub  OpDiv  OpImpl  OpConcat

has **no `OpAnd` / `OpOr` / `OpNot` variant**. Those names exist only as
*lexer* `Token` variants in `compiler/lexer.ev` (the tokens emitted for
`∧` / `∨` / `¬`); they are a different enum from the parser's `Op`, and
Evident enum-variant names are globally unique + enums are closed, so an
`Op` value spelled `OpAnd` does not exist and a new file cannot reopen
`Op` to add one.

`compiler/parser.ev` is frozen under this task's rules ("Forbidden:
editing … other `compiler/*.ev`"). So the fix is out of scope here.

## What unblocks it

Add `OpAnd`, `OpOr` (and likely `OpNot`, which is unary and needs an
`EUnOp` Expr variant or a dedicated node) to the `Op` enum in
`compiler/parser.ev`, and a `Token → Op` mapping for `∧`/`∨` in the
parser (the lexer already produces the tokens). Then `BoolTranslateStep`
needs two more `op_str` arms (`OpAnd → "and"`, `OpOr → "or"`) — a
two-line change — and the fixture's input/expected can become the
literal `(and (or x y) z)` form. The walker shape is already correct; it
is purely an AST-vocabulary gap in a frozen file.

## Why this is not a workaround-in-`translate_bool` situation

A parallel local Boolean-expr enum (e.g. `BExpr`/`BOp` defined inside
`translate_bool.ev`) would render `(and (or x y) z)` but would *not* walk
the parser's `Expr` AST — it would be a private datatype disconnected
from the rest of the compiler, contradicting the point of the pass
(translate the AST the parser actually produces). The honest boundary is
"the AST can't represent `∧`/`∨` yet," recorded here.
