// Runtime value extracted from a Z3 model — the C-runtime analogue of
// runtime/src/core/value.rs's `Value`, restricted to what the emitter produces.
#pragma once

#include <cstdint>
#include <sstream>
#include <string>

namespace evc {

struct Value {
    enum class Tag { Int, Real, Bool, Str, Enum, Seq } tag = Tag::Int;
    int64_t i = 0;
    double r = 0.0;
    bool b = false;
    std::string s;  // Str value, Enum variant name, or preformatted Seq `[…]`

    static Value Int(int64_t v)  { Value x; x.tag = Tag::Int;  x.i = v; return x; }
    static Value Real(double v)  { Value x; x.tag = Tag::Real; x.r = v; return x; }
    static Value Bool(bool v)    { Value x; x.tag = Tag::Bool; x.b = v; return x; }
    static Value Str(std::string v)  { Value x; x.tag = Tag::Str;  x.s = std::move(v); return x; }
    static Value Enum(std::string v) { Value x; x.tag = Tag::Enum; x.s = std::move(v); return x; }
    static Value Seq(std::string formatted) { Value x; x.tag = Tag::Seq; x.s = std::move(formatted); return x; }

    // Matches the Rust CLI's `format_value`: strings quoted, others bare; Seq holds
    // a preformatted `[e0, e1, …]` (elements already formatted) printed bare.
    std::string format() const {
        switch (tag) {
            case Tag::Int:  return std::to_string(i);
            case Tag::Real: {
                std::ostringstream os; os << r; return os.str();
            }
            case Tag::Bool: return b ? "true" : "false";
            case Tag::Str:  return "\"" + s + "\"";
            case Tag::Enum: return s;
            case Tag::Seq:  return s;
        }
        return "?";
    }
};

}  // namespace evc
