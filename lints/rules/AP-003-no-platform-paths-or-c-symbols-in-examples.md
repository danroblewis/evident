# AP-003: no-platform-paths-or-c-symbols-in-examples

**Status:** active

**Pattern.** An example file contains a hardcoded dynamic-library
path (`.dylib`, `.framework/`, `/opt/homebrew/lib/`,
`/usr/lib/`) or a literal C symbol name string (`"SDL_*"`,
`"gl[A-Z]*"`, `"NS*"`).

**Why.** Same family as AP-002. Even if the demo doesn't directly
call `LibCall`, embedding a dylib path or a C symbol string
locks the demo to a platform and to a specific library symbol
naming convention. Both belong inside stdlib wrappers (or
deeper, in the Rust FTI bridge).

**Fix.** Wrap in stdlib. The wrapper claim takes typed Evident
args and emits the appropriate `LibCall`/`FFICall` internally.
Then the demo references only the wrapper claim.

**Detection.** grep

**Pattern (grep).**
  - dylib paths: `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
    `/usr/lib/lib`, `/usr/lib/x86_64-linux-gnu/`.
  - String-literal C symbols: `"SDL_[A-Z]`, `"gl[A-Z]`,
    `"glsl[A-Z]?`, `"NS[A-Z]` (NSApplicationLoad and friends).
  - Apply only to lines that aren't `--` comments.

**Scope.**
  - Apply to: `examples/*.ev`.
  - Do NOT apply to: `stdlib/*`, `tests/lang_tests/*`,
    `runtime/src/event_sources*` (these layers DO know about
    paths and symbols).

**Exceptions.**
  - String literals that happen to contain "SDL" or "gl" as
    substrings of natural-language messages are exempt
    (heuristic: the string starts with the symbol prefix and
    looks like an identifier — uppercase, no spaces).
  - File-path strings inside `--` doc comments are exempt.

**Examples.**
  - Demo programs hardcoding `"/opt/homebrew/lib/libSDL2.dylib"`
    inside `LibCall` invocations (also AP-002).
  - Demo programs hardcoding `"SDL_PumpEvents"` as the symbol
    arg to `LibCall`.
