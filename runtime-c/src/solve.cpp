#include "solve.h"

#include <z3.h>

#include <algorithm>
#include <cstdint>

namespace evc {

namespace {

// Parse a Z3 numeral string ("3", "-2", "3/2") into a double.
double parse_rational(const std::string &s) {
    auto slash = s.find('/');
    if (slash == std::string::npos) return std::stod(s);
    double num = std::stod(s.substr(0, slash));
    double den = std::stod(s.substr(slash + 1));
    return den != 0 ? num / den : 0.0;
}

// Recursively read a Z3 model value AST into a Value, dispatching on its sort
// kind. Datatype values become Value::Enum with a formatted `Ctor(args)` string,
// matching the Rust CLI's format_value for enums.
Value read_ast_value(Z3_context ctx, Z3_ast ast) {
    Z3_sort sort = Z3_get_sort(ctx, ast);
    Z3_sort_kind sk = Z3_get_sort_kind(ctx, sort);
    if (Z3_is_string_sort(ctx, sort)) {
        const char *sv = Z3_get_string(ctx, ast);
        return Value::Str(sv ? sv : "");
    }
    switch (sk) {
        case Z3_INT_SORT: {
            int64_t iv = 0;
            Z3_get_numeral_int64(ctx, ast, &iv);
            return Value::Int(iv);
        }
        case Z3_BOOL_SORT: {
            Z3_lbool bv = Z3_get_bool_value(ctx, ast);
            return Value::Bool(bv == Z3_L_TRUE);
        }
        case Z3_REAL_SORT: {
            const char *ns = Z3_get_numeral_string(ctx, ast);
            return Value::Real(ns ? parse_rational(ns) : 0.0);
        }
        case Z3_DATATYPE_SORT: {
            Z3_app app = Z3_to_app(ctx, ast);
            Z3_func_decl decl = Z3_get_app_decl(ctx, app);
            std::string name = Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl));
            unsigned n = Z3_get_app_num_args(ctx, app);
            if (n == 0) return Value::Enum(name);
            std::string s = name + "(";
            for (unsigned i = 0; i < n; i++) {
                if (i) s += ", ";
                s += read_ast_value(ctx, Z3_get_app_arg(ctx, app, i)).format();
            }
            s += ")";
            return Value::Enum(s);
        }
        default:
            return Value::Str(Z3_ast_to_string(ctx, ast));
    }
}

}  // namespace

SolveResult solve(const EmitResult &emitted) {
    Z3_config cfg = Z3_mk_config();
    Z3_set_param_value(cfg, "model", "true");
    Z3_context ctx = Z3_mk_context(cfg);
    Z3_del_config(cfg);

    Z3_solver solver = Z3_mk_solver(ctx);
    Z3_solver_inc_ref(ctx, solver);

    Z3_solver_from_string(ctx, solver, emitted.text.c_str());

    Z3_error_code ec = Z3_get_error_code(ctx);
    if (ec != Z3_OK) {
        std::string msg = Z3_get_error_msg(ctx, ec);
        Z3_solver_dec_ref(ctx, solver);
        Z3_del_context(ctx);
        throw SmtError("Z3 rejected generated SMT-LIB: " + msg + "\n" + emitted.text);
    }

    SolveResult res;
    res.smtlib = emitted.text;

    Z3_lbool chk = Z3_solver_check(ctx, solver);
    if (chk == Z3_L_UNDEF) {
        res.unknown = true;
        Z3_solver_dec_ref(ctx, solver);
        Z3_del_context(ctx);
        return res;
    }
    res.satisfied = (chk == Z3_L_TRUE);

    if (res.satisfied) {
        Z3_model m = Z3_solver_get_model(ctx, solver);
        Z3_model_inc_ref(ctx, m);

        // Map each model-assigned const name to its value AST — used for enum
        // (datatype) consts, whose sort can't be cheaply reconstructed by name.
        std::unordered_map<std::string, Z3_ast> by_name;
        unsigned nc = Z3_model_get_num_consts(ctx, m);
        for (unsigned i = 0; i < nc; i++) {
            Z3_func_decl d = Z3_model_get_const_decl(ctx, m, i);
            std::string n = Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, d));
            Z3_ast v = Z3_model_get_const_interp(ctx, m, d);
            if (v) by_name[n] = v;
        }

        for (const auto &[name, sort] : emitted.declared) {
            if (sort.tag == Sort::Tag::Enum) {
                auto it = by_name.find(name);
                if (it != by_name.end())
                    res.bindings.push_back({name, read_ast_value(ctx, it->second)});
                continue;  // free (unconstrained) enum vars: any value, not reported
            }
            Z3_sort z3sort;
            switch (sort.tag) {
                case Sort::Tag::Int:  z3sort = Z3_mk_int_sort(ctx); break;
                case Sort::Tag::Bool: z3sort = Z3_mk_bool_sort(ctx); break;
                case Sort::Tag::Real: z3sort = Z3_mk_real_sort(ctx); break;
                case Sort::Tag::Str:  z3sort = Z3_mk_string_sort(ctx); break;
                case Sort::Tag::Enum: continue;  // handled above
            }
            Z3_ast c = Z3_mk_const(ctx, Z3_mk_string_symbol(ctx, name.c_str()), z3sort);
            Z3_ast val = nullptr;
            if (!Z3_model_eval(ctx, m, c, true, &val) || val == nullptr) continue;

            switch (sort.tag) {
                case Sort::Tag::Int: {
                    int64_t iv = 0;
                    if (Z3_get_numeral_int64(ctx, val, &iv))
                        res.bindings.push_back({name, Value::Int(iv)});
                    break;
                }
                case Sort::Tag::Bool: {
                    Z3_lbool bv = Z3_get_bool_value(ctx, val);
                    if (bv != Z3_L_UNDEF)
                        res.bindings.push_back({name, Value::Bool(bv == Z3_L_TRUE)});
                    break;
                }
                case Sort::Tag::Real: {
                    const char *ns = Z3_get_numeral_string(ctx, val);
                    if (ns) res.bindings.push_back({name, Value::Real(parse_rational(ns))});
                    break;
                }
                case Sort::Tag::Str: {
                    const char *sv = Z3_get_string(ctx, val);
                    if (sv) res.bindings.push_back({name, Value::Str(sv)});
                    break;
                }
                case Sort::Tag::Enum: break;
            }
        }
        Z3_model_dec_ref(ctx, m);
    }

    std::sort(res.bindings.begin(), res.bindings.end(),
              [](const auto &a, const auto &b) { return a.first < b.first; });

    Z3_solver_dec_ref(ctx, solver);
    Z3_del_context(ctx);
    return res;
}

}  // namespace evc
