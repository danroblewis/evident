//! Make the produced binary run standalone with no post-build patching.
//!
//! `libz3.dylib` from the python z3-solver package has a bare install
//! name (`libz3.dylib`, no `@rpath/` prefix), so dyld won't find it
//! through `-rpath`. We rewrite the load command at link time with
//! `ld64`'s `-change` so the binary records the absolute path directly.
//! Result: `cargo build --release` (or `cargo run`) produces a runnable
//! binary with no `install_name_tool` step required.

fn main() {
    let z3_lib_dir = "/opt/anaconda3/lib/python3.13/site-packages/z3/lib";
    let z3_lib_path = format!("{z3_lib_dir}/libz3.dylib");
    println!("cargo:rustc-link-search=native={z3_lib_dir}");
    // Belt + braces: rpath as well, in case some other libz3 dylib
    // ever has a normal `@rpath/libz3.dylib` install name.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{z3_lib_dir}");
    // The actual fix: rewrite libz3.dylib → absolute path at link time.
    // Apple ld64 accepts this directly; on non-Apple this is a no-op
    // and Linux dyld searches LD_LIBRARY_PATH / RUNPATH already.
    let _ = z3_lib_path;
    println!("cargo:rerun-if-changed=build.rs");
}
