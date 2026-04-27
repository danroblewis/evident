from lark.indenter import Indenter

class EvidentIndenter(Indenter):
    NL_type = "_NEWLINE"
    OPEN_PAREN_types = ["LPAR", "LBRACE", "LSQB"]
    CLOSE_PAREN_types = ["RPAR", "RBRACE", "RSQB"]
    INDENT_type = "_INDENT"
    DEDENT_type = "_DEDENT"
    tab_len = 4
