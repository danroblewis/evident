#include "lexer.h"

namespace evc {

namespace {

// Decode a UTF-8 byte string into Unicode codepoints. Evident operators (∈, ∧,
// ⇒, …) are single codepoints, so working over codepoints (like Rust's
// `chars()`) makes the operator matching below a direct mirror of lexer.rs.
std::vector<char32_t> decode_utf8(const std::string &src) {
    std::vector<char32_t> out;
    out.reserve(src.size());
    size_t i = 0, n = src.size();
    while (i < n) {
        unsigned char c = (unsigned char)src[i];
        char32_t cp;
        int len;
        if (c < 0x80) { cp = c; len = 1; }
        else if ((c >> 5) == 0x6) { cp = c & 0x1F; len = 2; }
        else if ((c >> 4) == 0xE) { cp = c & 0x0F; len = 3; }
        else if ((c >> 3) == 0x1E) { cp = c & 0x07; len = 4; }
        else { cp = 0xFFFD; len = 1; }
        for (int k = 1; k < len && i + k < n; k++)
            cp = (cp << 6) | ((unsigned char)src[i + k] & 0x3F);
        out.push_back(cp);
        i += len;
    }
    return out;
}

// Re-encode a single codepoint as UTF-8 bytes appended to `out`.
void encode_utf8(char32_t cp, std::string &out) {
    if (cp < 0x80) {
        out.push_back((char)cp);
    } else if (cp < 0x800) {
        out.push_back((char)(0xC0 | (cp >> 6)));
        out.push_back((char)(0x80 | (cp & 0x3F)));
    } else if (cp < 0x10000) {
        out.push_back((char)(0xE0 | (cp >> 12)));
        out.push_back((char)(0x80 | ((cp >> 6) & 0x3F)));
        out.push_back((char)(0x80 | (cp & 0x3F)));
    } else {
        out.push_back((char)(0xF0 | (cp >> 18)));
        out.push_back((char)(0x80 | ((cp >> 12) & 0x3F)));
        out.push_back((char)(0x80 | ((cp >> 6) & 0x3F)));
        out.push_back((char)(0x80 | (cp & 0x3F)));
    }
}

bool is_ident_start(char32_t c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}
bool is_ident_continue(char32_t c) {
    return is_ident_start(c) || (c >= '0' && c <= '9');
}
bool is_digit(char32_t c) { return c >= '0' && c <= '9'; }

Token keyword_or_ident(const std::string &s) {
    using K = Token::Kind;
    if (s == "schema")   return Token(K::Schema);
    if (s == "claim")    return Token(K::Claim);
    if (s == "type")     return Token(K::Type);
    if (s == "subclaim") return Token(K::Subclaim);
    if (s == "fsm")      return Token(K::Fsm);
    if (s == "external") return Token(K::External);
    if (s == "enum")     return Token(K::Enum);
    if (s == "match")    return Token(K::Match);
    if (s == "matches")  return Token(K::Matches);
    if (s == "import")   return Token(K::Import);
    if (s == "in")       return Token(K::In);
    if (s == "true")     return Token(K::True);
    if (s == "false")    return Token(K::False);
    if (s == "mapsto")   return Token(K::MapsTo);
    Token t(K::Ident);
    t.str = s;
    return t;
}

}  // namespace

std::vector<Token> tokenize(const std::string &src) {
    using K = Token::Kind;
    std::vector<char32_t> cp = decode_utf8(src);
    std::vector<Token> toks;

    size_t i = 0, n = cp.size();
    size_t line = 1, col = 1;
    bool at_line_start = true;
    size_t paren_depth = 0;

    auto peek = [&](size_t off = 0) -> char32_t {
        return (i + off < n) ? cp[i + off] : 0;
    };

    while (i < n) {
        char32_t c = cp[i];

        if (at_line_start) {
            size_t indent = 0;
            while (i < n) {
                if (cp[i] == ' ') { i++; col++; indent++; }
                else if (cp[i] == '\t') { i++; col++; indent += 4; }
                else break;
            }
            if (i >= n) break;  // EOF after indent
            char32_t ch = cp[i];
            if (ch == '\n') { i++; line++; col = 1; at_line_start = true; continue; }
            if (ch == '-' && peek(1) == '-') {
                while (i < n && cp[i] != '\n') { i++; col++; }
                continue;  // comment line: still at_line_start, newline handled next loop
            }
            Token t(K::Indent);
            t.indent = indent;
            toks.push_back(t);
            at_line_start = false;
            continue;
        }

        switch (c) {
            case ' ': case '\t': i++; col++; break;
            case '\n':
                i++; line++; col = 1;
                if (paren_depth == 0) { toks.push_back(Token(K::Newline)); at_line_start = true; }
                break;
            case '-':
                if (peek(1) == '-') {
                    while (i < n && cp[i] != '\n') { i++; col++; }
                } else { i++; col++; toks.push_back(Token(K::Minus)); }
                break;
            case '"': {
                i++; col++;
                std::string s;
                bool closed = false;
                while (i < n) {
                    char32_t d = cp[i];
                    if (d == '"') { i++; col++; closed = true; break; }
                    if (d == '\\') {
                        i++; col++;
                        char32_t e = peek();
                        switch (e) {
                            case '"':  s.push_back('"');  i++; col++; break;
                            case '\\': s.push_back('\\'); i++; col++; break;
                            case 'n':  s.push_back('\n'); i++; col++; break;
                            case 't':  s.push_back('\t'); i++; col++; break;
                            case 0: throw LexError("unterminated string escape", line, col);
                            default: throw LexError("unknown escape", line, col);
                        }
                        continue;
                    }
                    if (d == '\n') throw LexError("unterminated string literal", line, col);
                    encode_utf8(d, s);
                    i++; col++;
                }
                if (!closed) throw LexError("unterminated string at EOF", line, col);
                Token t(K::Str); t.str = s; toks.push_back(t);
                break;
            }
            default:
                if (is_digit(c)) {
                    std::string s;
                    while (i < n && is_digit(cp[i])) { s.push_back((char)cp[i]); i++; col++; }
                    // Real: digits '.' digits — only when a digit follows the dot.
                    if (peek() == '.' && is_digit(peek(1))) {
                        i++; col++; s.push_back('.');
                        while (i < n && is_digit(cp[i])) { s.push_back((char)cp[i]); i++; col++; }
                        Token t(K::Real); t.rval = std::stod(s); toks.push_back(t);
                    } else {
                        Token t(K::Int); t.ival = std::stoll(s); toks.push_back(t);
                    }
                } else if (is_ident_start(c)) {
                    std::string s;
                    while (i < n && is_ident_continue(cp[i])) { s.push_back((char)cp[i]); i++; col++; }
                    toks.push_back(keyword_or_ident(s));
                } else {
                    // single/multi-char operators
                    switch (c) {
                        case '+':
                            i++; col++;
                            if (peek() == '+') { i++; col++; toks.push_back(Token(K::PlusPlus)); }
                            else toks.push_back(Token(K::Plus));
                            break;
                        case '*': i++; col++; toks.push_back(Token(K::Star)); break;
                        case '/': i++; col++; toks.push_back(Token(K::Slash)); break;
                        case '(': i++; col++; toks.push_back(Token(K::LParen)); paren_depth++; break;
                        case ')': i++; col++; toks.push_back(Token(K::RParen)); if (paren_depth) paren_depth--; break;
                        case '{': i++; col++; toks.push_back(Token(K::LBrace)); paren_depth++; break;
                        case '}': i++; col++; toks.push_back(Token(K::RBrace)); if (paren_depth) paren_depth--; break;
                        case '[': i++; col++; toks.push_back(Token(K::LBracket)); paren_depth++; break;
                        case ']': i++; col++; toks.push_back(Token(K::RBracket)); if (paren_depth) paren_depth--; break;
                        case '#': i++; col++; toks.push_back(Token(K::Hash)); break;
                        case ',': i++; col++; toks.push_back(Token(K::Comma)); break;
                        case ':': i++; col++; toks.push_back(Token(K::Colon)); break;
                        case '.':
                            i++; col++;
                            if (peek() == '.') { i++; col++; toks.push_back(Token(K::DotDot)); }
                            else toks.push_back(Token(K::Dot));
                            break;
                        case '=':
                            i++; col++;
                            if (peek() == '>') { i++; col++; toks.push_back(Token(K::Implies)); }
                            else toks.push_back(Token(K::Eq));
                            break;
                        case '<':
                            i++; col++;
                            if (peek() == '=') { i++; col++; toks.push_back(Token(K::Le)); }
                            else toks.push_back(Token(K::Lt));
                            break;
                        case '>':
                            i++; col++;
                            if (peek() == '=') { i++; col++; toks.push_back(Token(K::Ge)); }
                            else toks.push_back(Token(K::Gt));
                            break;
                        case '!':
                            i++; col++;
                            if (peek() == '=') { i++; col++; toks.push_back(Token(K::Neq)); }
                            else throw LexError("unexpected '!'", line, col);
                            break;
                        case '|': i++; col++; toks.push_back(Token(K::Pipe)); break;
                        case '?': i++; col++; toks.push_back(Token(K::Question)); break;
                        // Unicode operators (codepoints).
                        case 0x2208: i++; col++; toks.push_back(Token(K::In)); break;          // ∈
                        case 0x2209: i++; col++; toks.push_back(Token(K::NotIn)); break;        // ∉
                        case 0x220B: i++; col++; toks.push_back(Token(K::ContainsRev)); break;  // ∋
                        case 0x2227: i++; col++; toks.push_back(Token(K::And)); break;          // ∧
                        case 0x2228: i++; col++; toks.push_back(Token(K::Or)); break;           // ∨
                        case 0x00AC: i++; col++; toks.push_back(Token(K::Not)); break;          // ¬
                        case 0x21D2: i++; col++; toks.push_back(Token(K::Implies)); break;      // ⇒
                        case 0x2264: i++; col++; toks.push_back(Token(K::Le)); break;           // ≤
                        case 0x2265: i++; col++; toks.push_back(Token(K::Ge)); break;           // ≥
                        case 0x2260: i++; col++; toks.push_back(Token(K::Neq)); break;          // ≠
                        case 0x2200: i++; col++; toks.push_back(Token(K::ForAll)); break;       // ∀
                        case 0x2203: i++; col++; toks.push_back(Token(K::Exists)); break;       // ∃
                        case 0x21A6: i++; col++; toks.push_back(Token(K::MapsTo)); break;       // ↦
                        case 0x27E8: i++; col++; toks.push_back(Token(K::LSeq)); paren_depth++; break;  // ⟨
                        case 0x27E9: i++; col++; toks.push_back(Token(K::RSeq)); if (paren_depth) paren_depth--; break;  // ⟩
                        default:
                            throw LexError("unexpected character (codepoint " +
                                           std::to_string((uint32_t)c) + ")", line, col);
                    }
                }
        }
    }

    toks.push_back(Token(K::Eof));
    return toks;
}

}  // namespace evc
