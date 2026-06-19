fn main() {
    let z3_lib_dir = "/opt/anaconda3/lib/python3.13/site-packages/z3/lib";
    let z3_lib_path = format!("{z3_lib_dir}/libz3.dylib");
    println!("cargo:rustc-link-search=native={z3_lib_dir}");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{z3_lib_dir}");
    let _ = z3_lib_path;
    println!("cargo:rerun-if-changed=build.rs");
}
