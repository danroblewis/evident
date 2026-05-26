// AST -> SMT-LIB text. Mirrors runtime/src/translate/smtlib.rs (the Rust
// prototype) and grows it: scalar sorts + arithmetic + logic + membership +
// ternary (M2), enums as Z3 datatypes + match/recognizers + finite-range
// quantifier unrolling (M4).
//
// The boundary is enforced positively: emit throws SmtError the instant it sees
// something out of subset, so a partial transpile can never silently drop a
// constraint (Evident's "missing constraint is a silent bug" failure mode).
#pragma once

#include <memory>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <vector>

#include "ast.h"

namespace evc {

struct SmtError : std::runtime_error {
    explicit SmtError(const std::string &msg) : std::runtime_error("smtlib: " + msg) {}
};

// SMT sort the seed handles. `Enum` names a declared Z3 datatype; `Seq` carries
// its element sort (Z3 sequence theory, lowered as SMT-LIB text).
struct Sort {
    enum class Tag { Int, Bool, Real, Str, Enum, Seq } tag = Tag::Int;
    std::string enum_name;        // when tag == Enum
    std::shared_ptr<Sort> elem;   // when tag == Seq

    Sort() = default;
    Sort(Tag t) : tag(t) {}
    Sort(Tag t, std::string en) : tag(t), enum_name(std::move(en)) {}
    static Sort seq(Sort element) {
        Sort s;
        s.tag = Tag::Seq;
        s.elem = std::make_shared<Sort>(std::move(element));
        return s;
    }

    bool operator==(const Sort &o) const {
        if (tag != o.tag) return false;
        if (tag == Tag::Enum) return enum_name == o.enum_name;
        if (tag == Tag::Seq) return elem && o.elem && *elem == *o.elem;
        return true;
    }
    bool operator!=(const Sort &o) const { return !(*this == o); }
    std::string smt() const;  // SMT-LIB sort name
};

struct EmitResult {
    std::string text;                                   // declare/assert lines, no check-sat
    std::vector<std::pair<std::string, Sort>> declared; // for model extraction
};

// Emit SMT-LIB for a claim's free-query semantics: every declared scalar/enum is
// a fresh const, every constraint is asserted. `prog` supplies enum decls for the
// datatype preamble. Throws SmtError for anything out of subset.
EmitResult emit_schema(const SchemaDecl &schema, const Program &prog);

// Convenience: just the text (tests / debugging).
std::string schema_to_smtlib(const SchemaDecl &schema, const Program &prog);

}  // namespace evc
