//! Make the produced `evident-runtime` binary run standalone (no need
//! to set `DYLD_LIBRARY_PATH` first). Two-step:
//!
//!   1. At link time: pass `-rpath` + `-search_paths_first` so the
//!      binary records the Z3 dir as a runtime search path.
//!   2. At link time we can't run `install_name_tool` (the binary
//!      doesn't exist yet), so the wrapper script `bin/evident-runtime`
//!      handles the post-link patch — see the README. For `cargo run`
//!      and `cargo test`, `.cargo/config.toml` sets DYLD_LIBRARY_PATH.

fn main() {
    let z3_lib = "/opt/anaconda3/lib/python3.13/site-packages/z3/lib";
    println!("cargo:rustc-link-search=native={}", z3_lib);
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", z3_lib);
    println!("cargo:rerun-if-changed=build.rs");
}
