UNICODE_MAP = {
    '∈': '__IN__',
    '∉': '__NOT_IN__',
    '⊆': '__SUBSET__',
    '⊇': '__SUPERSET__',
    '∀': '__FORALL__',
    '∃!': '__UNIQUE__',
    '¬∃': '__NONE__',
    '∃': '__EXISTS__',
    '¬': '__NOT__',
    '∧': '__AND__',
    '∨': '__OR__',
    '⇒': '__IMPLIES__',
    '↦': '__MAPSTO__',
    '·': '__CHAIN__',
    '⋈': '__JOIN__',
    '≤': '<=',
    '≥': '>=',
    '≠': '!=',
    '×': '__CROSS__',
    '∩': '__INTERSECT__',
    '∪': '__UNION__',
    'Σ': '__SUM__',
}

import re

def normalize(source: str) -> str:
    """Replace Unicode operators with ASCII token placeholders.

    Also strips -- line comments (SQL/Haskell style) before Lark sees the
    source; the Earley basic lexer doesn't handle extra %ignore patterns
    reliably.  // and # comments are already handled by the grammar.
    """
    # Strip -- comments first (before Unicode substitution alters the text)
    result = re.sub(r'--[^\n]*', '', source)
    # Handle multi-char sequences first (∃! and ¬∃ before ∃ and ¬)
    for unicode_sym, ascii_tok in sorted(UNICODE_MAP.items(), key=lambda x: -len(x[0])):
        result = result.replace(unicode_sym, ascii_tok)
    return result
