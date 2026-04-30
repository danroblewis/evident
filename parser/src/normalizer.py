import re as _re

UNICODE_MAP = {
    '∈': '__IN__',
    '∉': '__NOT_IN__',
    '∋': '__CONTAINS__',
    '∌': '__NOT_CONTAINS__',
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
    '⟨': ' __LSEQ__ ',
    '⟩': ' __RSEQ__ ',
    '⊑': '__PREFIX__',
    '⊒': '__SUFFIX__',
}

# Regex literal: /pattern/ in membership context.
# Converted here (before Lark) to a STRING literal with a magic prefix.
# s ∈ /pat/   →  s ∈ "__REGEX__pat"       (parsed as normal STRING)
# /pat/ ∋ s   →  "__REGEX__pat" ∋ s
# The transformer then inspects StringLiteral values for this prefix.
_REGEX_PREFIX = '__REGEX__'

def _encode_regex_as_string(pattern: str) -> str:
    """Wrap a regex pattern as a STRING literal the parser can handle.

    Lark's STRING token does NOT process escape sequences — it takes the
    raw characters between the quotes.  So we must NOT double backslashes;
    we only need to avoid unescaped " inside the literal.
    """
    escaped = pattern.replace('"', '__DQ__')
    return f'"{_REGEX_PREFIX}{escaped}"'

def decode_regex_string(value: str) -> str | None:
    """If a StringLiteral value is a regex literal, return the pattern. Else None."""
    if value.startswith(_REGEX_PREFIX):
        return value[len(_REGEX_PREFIX):]
    return None

def _replace_regex_lits(source: str) -> str:
    """Replace /pattern/ in membership context with "__REGEX__pattern" strings."""
    # After ∈ ∉ ∋ ∌ (before unicode substitution runs)
    result = _re.sub(
        r'(?<=[∈∉∋∌])\s*/([^/\n]+)/',
        lambda m: ' ' + _encode_regex_as_string(m.group(1)),
        source,
    )
    # /pattern/ before ∋ ∌ (for  /re/ ∋ s  form)
    result = _re.sub(
        r'/([^/\n]+)/(?=\s*[∋∌])',
        lambda m: _encode_regex_as_string(m.group(1)),
        result,
    )
    return result


def normalize(source: str) -> str:
    """Replace Unicode operators and regex literals with ASCII token placeholders."""
    # Strip -- comments first
    result = _re.sub(r'--[^\n]*', '', source)
    # Encode regex literals before Unicode substitution changes the operators
    result = _replace_regex_lits(result)
    # Handle multi-char sequences first (∃! and ¬∃ before ∃ and ¬)
    for unicode_sym, ascii_tok in sorted(UNICODE_MAP.items(), key=lambda x: -len(x[0])):
        result = result.replace(unicode_sym, ascii_tok)
    return result
