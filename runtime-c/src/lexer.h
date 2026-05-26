// Tokenize Evident source. Mirrors runtime/src/lexer.rs: Unicode operators are
// recognized directly (UTF-8 decoded), indentation is significant (Newline +
// Indent(n) tokens), and newlines inside (), {}, [], ⟨⟩ are consumed silently.
#pragma once

#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

namespace evc {

struct Token {
    enum class Kind {
        Ident, Int, Real, Str, True, False,
        Schema, Claim, Type, Subclaim, Fsm, External, Enum, Match, Matches,
        Import, In, NotIn, ContainsRev,
        Eq, Neq, Lt, Le, Gt, Ge, Plus, PlusPlus, Minus, Star, Slash,
        And, Or, Not, Implies,
        LParen, RParen, LBrace, RBrace, LBracket, RBracket, LSeq, RSeq,
        Hash, Comma, Pipe, Question, DotDot, Dot, Colon, ForAll, Exists, MapsTo,
        Newline, Indent, Eof,
    } kind;

    std::string str;     // Ident, Str
    int64_t ival = 0;    // Int
    double rval = 0.0;   // Real
    size_t indent = 0;   // Indent column count

    explicit Token(Kind k) : kind(k) {}
};

struct LexError : std::runtime_error {
    size_t line, col;
    LexError(const std::string &msg, size_t line, size_t col)
        : std::runtime_error("lex error at line " + std::to_string(line) +
                             ", col " + std::to_string(col) + ": " + msg),
          line(line), col(col) {}
};

std::vector<Token> tokenize(const std::string &src);

}  // namespace evc
