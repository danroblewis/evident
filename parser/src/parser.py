from lark import Lark
from pathlib import Path
from .normalizer import normalize
from .indenter import EvidentIndenter
from .transformer import EvidentTransformer
from .ast import Program

_GRAMMAR = (Path(__file__).parent / "grammar.lark").read_text()

_parser = Lark(
    _GRAMMAR,
    parser="earley",
    lexer="basic",
    postlex=EvidentIndenter(),
    ambiguity="resolve",
    start="start",
)


def parse(source: str) -> Program:
    """Parse Evident source text into a Program AST."""
    normalized = normalize(source)
    tree = _parser.parse(normalized)
    return EvidentTransformer().transform(tree)
