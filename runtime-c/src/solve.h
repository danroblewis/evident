// SMT-LIB text -> Z3 -> sat/unsat (+ model). Mirrors the solve half of
// runtime/src/translate/smtlib.rs: parse via Z3_solver_from_string, check the
// raw error code (Z3 swallows parser errors), solve, extract the scalar model.
#pragma once

#include <string>
#include <vector>

#include "smtlib.h"
#include "value.h"

namespace evc {

struct SolveResult {
    bool satisfied = false;
    bool unknown = false;
    std::vector<std::pair<std::string, Value>> bindings;  // sorted by name
    std::string smtlib;
};

// Run an emitted claim through Z3. Throws SmtError if Z3 rejects the SMT-LIB or
// returns Unknown on what should be a decidable problem.
SolveResult solve(const EmitResult &emitted);

}  // namespace evc
