// M0 — Z3 link proof.
//
// The smallest possible program that proves the native↔Z3 seam works: build a
// hardcoded SMT-LIB string, hand it to Z3's own parser (Z3_solver_from_string,
// exactly as the Rust prototype in runtime/src/translate/smtlib.rs does), solve,
// and print the sat verdict plus an extracted model value.
//
// This links nothing but libz3 — no lexer, no parser. It exists to verify the
// build + link + Z3 C API call path before any Evident code is involved.

#include <z3.h>
#include <cstdio>
#include <cstdint>

int main() {
    // The constraint system: (n : Int), n >= 0, n > 5.  Should be SAT with n>5.
    const char *smtlib =
        "(declare-const n Int)\n"
        "(assert (>= n 0))\n"
        "(assert (> n 5))\n";

    Z3_config cfg = Z3_mk_config();
    Z3_set_param_value(cfg, "model", "true");
    Z3_context ctx = Z3_mk_context(cfg);
    Z3_del_config(cfg);

    Z3_solver solver = Z3_mk_solver(ctx);
    Z3_solver_inc_ref(ctx, solver);

    Z3_solver_from_string(ctx, solver, smtlib);

    // Z3 swallows parser errors into its context error state; check it.
    Z3_error_code ec = Z3_get_error_code(ctx);
    if (ec != Z3_OK) {
        std::fprintf(stderr, "Z3 rejected SMT-LIB: %s\n", Z3_get_error_msg(ctx, ec));
        return 2;
    }

    printf("=== M0: Z3 link proof ===\n");
    printf("SMT-LIB handed to Z3:\n%s\n", smtlib);

    Z3_lbool r = Z3_solver_check(ctx, solver);
    switch (r) {
        case Z3_L_TRUE: {
            printf("result: SAT\n");
            Z3_model m = Z3_solver_get_model(ctx, solver);
            Z3_model_inc_ref(ctx, m);
            // Reconstruct the const handle by name+sort, then evaluate it.
            Z3_ast n = Z3_mk_const(ctx, Z3_mk_string_symbol(ctx, "n"),
                                   Z3_mk_int_sort(ctx));
            Z3_ast val = nullptr;
            if (Z3_model_eval(ctx, m, n, true, &val)) {
                int64_t iv = 0;
                if (Z3_get_numeral_int64(ctx, val, &iv)) {
                    printf("model: n = %lld\n", (long long)iv);
                } else {
                    printf("model: n = %s\n", Z3_ast_to_string(ctx, val));
                }
            }
            Z3_model_dec_ref(ctx, m);
            break;
        }
        case Z3_L_FALSE:
            printf("result: UNSAT\n");
            break;
        case Z3_L_UNDEF:
            printf("result: UNKNOWN\n");
            break;
    }

    Z3_solver_dec_ref(ctx, solver);
    Z3_del_context(ctx);
    return 0;
}
