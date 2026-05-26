cmake_minimum_required(VERSION 3.16)
project(evidentc CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

# ---------------------------------------------------------------------------
# Locate Z3 (the one unavoidable dependency — see docs/design/c-runtime.md).
# Honors -DZ3_ROOT=<prefix>; otherwise probes Homebrew's default location.
# ---------------------------------------------------------------------------
if(NOT Z3_ROOT)
  if(EXISTS "/opt/homebrew/include/z3.h")
    set(Z3_ROOT "/opt/homebrew")
  elseif(EXISTS "/usr/local/include/z3.h")
    set(Z3_ROOT "/usr/local")
  endif()
endif()

find_path(Z3_INCLUDE_DIR z3.h HINTS "${Z3_ROOT}/include")
find_library(Z3_LIBRARY NAMES z3 libz3 HINTS "${Z3_ROOT}/lib")

if(NOT Z3_INCLUDE_DIR OR NOT Z3_LIBRARY)
  message(FATAL_ERROR
    "Could not find Z3. Install it (brew install z3) or pass -DZ3_ROOT=<prefix>. "
    "Looked under Z3_ROOT='${Z3_ROOT}'.")
endif()

message(STATUS "Z3 header:  ${Z3_INCLUDE_DIR}/z3.h")
message(STATUS "Z3 library: ${Z3_LIBRARY}")

# ---------------------------------------------------------------------------
# The seed library: lexer + parser + SMT-LIB emit + Z3 binding.
# Kept as a library so tests can link it without re-listing sources.
# ---------------------------------------------------------------------------
add_library(evidentc_seed STATIC
  src/lexer.cpp
  src/parser.cpp
  src/smtlib.cpp
  src/solve.cpp
)
target_include_directories(evidentc_seed PUBLIC src "${Z3_INCLUDE_DIR}")
target_link_libraries(evidentc_seed PUBLIC "${Z3_LIBRARY}")
target_compile_options(evidentc_seed PRIVATE -Wall -Wextra)

# The CLI: evidentc <file.ev> <claim>
add_executable(evidentc src/main.cpp)
target_link_libraries(evidentc PRIVATE evidentc_seed)

# M0 — Z3 link proof: hands a hardcoded SMT-LIB string to Z3, solves, prints.
add_executable(z3_link_proof src/z3_link_proof.cpp)
target_include_directories(z3_link_proof PRIVATE "${Z3_INCLUDE_DIR}")
target_link_libraries(z3_link_proof PRIVATE "${Z3_LIBRARY}")

# Unit/integration tests for the seed (lexer, parser, emit, solve).
enable_testing()
add_executable(seed_tests tests/seed_tests.cpp)
target_link_libraries(seed_tests PRIVATE evidentc_seed)
add_test(NAME seed_tests COMMAND seed_tests)
