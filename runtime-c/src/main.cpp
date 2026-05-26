// evidentc — the seed runtime CLI.
//
//   evidentc <file.ev> <claim>            solve one claim: sat/unsat + model
//   evidentc <file.ev> --all              sat-check every schema (cross-check aid)
//   evidentc <file.ev> <claim> --smtlib   also dump the generated SMT-LIB to stderr
//
// Pipeline: read -> parse -> emit SMT-LIB -> Z3 solve -> extract -> print.

#include <cstdio>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "parser.h"
#include "smtlib.h"
#include "solve.h"

namespace {

std::string read_file(const std::string &path) {
    std::ifstream in(path, std::ios::binary);
    if (!in) throw std::runtime_error("cannot open file: " + path);
    std::ostringstream ss;
    ss << in.rdbuf();
    return ss.str();
}

const evc::SchemaDecl *find_schema(const evc::Program &prog, const std::string &name) {
    for (const auto &s : prog.schemas)
        if (s.name == name) return &s;
    return nullptr;
}

int solve_one(const evc::Program &prog, const std::string &claim, bool dump_smtlib) {
    const evc::SchemaDecl *schema = find_schema(prog, claim);
    if (!schema) {
        std::cerr << "no schema named `" << claim << "`\n";
        return 1;
    }
    try {
        evc::EmitResult emitted = evc::emit_schema(*schema, prog);
        if (dump_smtlib) std::cerr << "--- SMT-LIB ---\n" << emitted.text << "---------------\n";
        evc::SolveResult r = evc::solve(emitted);
        if (r.unknown) { std::cout << "unknown\n"; return 0; }
        std::cout << (r.satisfied ? "sat" : "unsat") << "\n";
        if (r.satisfied) {
            for (const auto &[name, val] : r.bindings)
                std::cout << name << " = " << val.format() << "\n";
        }
        return 0;
    } catch (const evc::SmtError &e) {
        std::cerr << e.what() << "\n";
        return 3;  // out of subset / Z3 rejection
    }
}

bool is_generic_template(const evc::SchemaDecl &s) { return !s.type_params.empty(); }
bool has_generic_seq_param(const evc::SchemaDecl &s) {
    for (const auto &b : s.body)
        if (b.kind == evc::BodyItem::Kind::Membership && b.type_name == "Seq") return true;
    return false;
}

int solve_all(const evc::Program &prog) {
    for (const auto &s : prog.schemas) {
        if (is_generic_template(s)) { std::cout << "SKIP   " << s.name << "  (generic template)\n"; continue; }
        if (has_generic_seq_param(s)) { std::cout << "SKIP   " << s.name << "  (generic Seq param)\n"; continue; }
        try {
            evc::EmitResult emitted = evc::emit_schema(s, prog);
            evc::SolveResult r = evc::solve(emitted);
            if (r.unknown) std::cout << "UNKWN  " << s.name << "\n";
            else std::cout << (r.satisfied ? "SAT   " : "UNSAT ") << " " << s.name << "\n";
        } catch (const evc::SmtError &e) {
            std::cout << "ERR    " << s.name << "  (" << e.what() << ")\n";
        }
    }
    return 0;
}

}  // namespace

int main(int argc, char **argv) {
    std::vector<std::string> args(argv + 1, argv + argc);
    if (args.empty()) {
        std::cerr << "usage:\n"
                  << "  evidentc <file.ev> <claim> [--smtlib]\n"
                  << "  evidentc <file.ev> --all\n";
        return 2;
    }

    std::string file = args[0];
    bool all = false, dump_smtlib = false;
    std::string claim;
    for (size_t i = 1; i < args.size(); i++) {
        if (args[i] == "--all") all = true;
        else if (args[i] == "--smtlib") dump_smtlib = true;
        else if (claim.empty()) claim = args[i];
    }

    evc::Program prog;
    try {
        std::string src = read_file(file);
        prog = evc::parse(src);
    } catch (const evc::ParseError &e) {
        std::cerr << e.what() << "\n";
        return 1;
    } catch (const evc::LexError &e) {
        std::cerr << e.what() << "\n";
        return 1;
    } catch (const std::exception &e) {
        std::cerr << e.what() << "\n";
        return 1;
    }

    if (!prog.imports.empty())
        std::cerr << "note: " << prog.imports.size()
                  << " import(s) ignored (seed runtime resolves no imports yet)\n";

    if (all) return solve_all(prog);
    if (claim.empty()) {
        std::cerr << "need a <claim> name (or --all)\n";
        return 2;
    }
    return solve_one(prog, claim, dump_smtlib);
}
