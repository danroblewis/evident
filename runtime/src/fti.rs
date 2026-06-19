const SHIMMED_STDLIB_PATHS: &[&str] = &[
    "packages/sdl.ev",
    "stdlib/io.ev",
];

pub fn is_shimmed_stdlib(import_path: &str) -> bool {
    SHIMMED_STDLIB_PATHS.contains(&import_path)
}
