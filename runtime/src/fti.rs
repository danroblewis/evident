// FTI — Foreign Type Interface.
//
// After the single-FSM teardown, the only FTI mechanism is the
// declarative `install ∈ Seq(InstallStep)` path, driven directly by
// the effect loop (`effect_loop/install.rs`) — no Rust-side bridge
// registry, no async timer bridges. This module retains only the
// shimmed-stdlib-path policy that the loader consults.

/// Stdlib import paths that the loader treats as optional: if the file
/// is missing at the expected location, the import silently no-ops
/// rather than erroring. (Historically these named files whose types
/// were provided by Rust FTI bridges; the policy is retained so
/// existing programs that `import` them don't break when the file
/// isn't present.)
const SHIMMED_STDLIB_PATHS: &[&str] = &[
    "packages/sdl.ev",
    "stdlib/io.ev",
];

/// True if `import_path` is a stdlib file whose absence should silently
/// no-op rather than error.
pub fn is_shimmed_stdlib(import_path: &str) -> bool {
    SHIMMED_STDLIB_PATHS.contains(&import_path)
}
