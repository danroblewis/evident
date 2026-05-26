// Tokens -> AST. Hand-rolled recursive-descent parser mirroring
// runtime/src/parser/ (program, schema, body_item, exprs, atoms, types,
// patterns). Implements the subset this seed handles; out-of-subset constructs
// parse into AST nodes that the SMT-LIB emitter then rejects honestly.
#pragma once

#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include "ast.h"
#include "lexer.h"

namespace evc {

struct ParseError : std::runtime_error {
    explicit ParseError(const std::string &msg) : std::runtime_error("parse error: " + msg) {}
};

class Parser {
public:
    explicit Parser(std::vector<Token> toks) : toks_(std::move(toks)), pos_(0) {}
    Program parse_program();

private:
    std::vector<Token> toks_;
    size_t pos_;

    using K = Token::Kind;

    const Token &peek(size_t off = 0) const {
        size_t p = pos_ + off;
        return p < toks_.size() ? toks_[p] : toks_.back();  // back() is Eof
    }
    K kind(size_t off = 0) const { return peek(off).kind; }
    bool is(K k, size_t off = 0) const { return kind(off) == k; }
    Token bump() { Token t = toks_[pos_]; if (pos_ + 1 < toks_.size()) pos_++; return t; }
    void eat(K k);
    void skip_blank_newlines();

    // schema / program
    SchemaDecl parse_schema_decl();
    std::vector<BodyItem> parse_first_line_params();
    std::vector<BodyItem> parse_indented_body();
    BodyItem parse_subclaim();
    EnumDecl parse_enum_decl();
    std::string parse_enum_field_type(const std::string &v_name);

    // body items
    std::vector<BodyItem> parse_body_item();
    std::optional<std::vector<BodyItem>> try_parse_chained_membership();

    // types / pins
    std::optional<std::string> try_parse_generic_args_suffix();
    std::optional<std::pair<std::string, Pins>> try_parse_type_and_pins(const std::string &head);

    // expressions
    ExprPtr parse_expr();
    ExprPtr parse_quantifier();
    ExprPtr parse_implies();
    ExprPtr parse_ternary();
    ExprPtr parse_or();
    ExprPtr parse_and();
    ExprPtr parse_compare();
    ExprPtr parse_addsub();
    ExprPtr parse_muldiv();
    ExprPtr parse_unary();
    ExprPtr parse_postfix();
    ExprPtr parse_atom();

    // patterns / match
    ExprPtr parse_match();
    MatchPattern parse_match_pattern();
};

// Convenience: source string -> Program.
Program parse(const std::string &src);

}  // namespace evc
