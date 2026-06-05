//! `.evidentc` side-car cache.
//!
//! Wave 5d minimum: cache the output of `functionize::simplify_assertions`
//! to a file next to the input `.smt2`. On a re-run with the same input,
//! skip the Z3 tactic chain (which is a measurable fraction of the
//! pre-tick setup on big programs like `compiler.smt2`) by reading the
//! cached simplified assertions back via `Z3_parse_smtlib2_string`.
//!
//! Layout (v0):
//!
//!   `;; evidentc cache v0 codegen=<CODEGEN_VERSION> src-hash=<sha256-prefix>`
//!   `;; <decl-preamble — declare-fun / declare-datatypes>`
//!   `(assert <simplified-1>)`
//!   `(assert <simplified-2>)`
//!   …
//!
//! The cache file is invalidated by any one of:
//!   - codegen version mismatch (force-rebuild on kernel upgrade)
//!   - input source hash mismatch (the input .smt2 changed)
//!   - the decl preamble in the cache differs from the current one
//!
//! Per-input cache only — no global `~/.cache/evident/` indexing in v0.
//! `EVIDENT_NO_CACHE=1` disables read+write.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use z3_sys::*;

const CODEGEN_VERSION: &str = "v0-2026-06-05";

/// Compute the side-car cache path for an input `.smt2` path.
pub fn cache_path(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let new_name = match input.file_name().and_then(|s| s.to_str()) {
        Some(s) => format!("{s}.evidentc"),
        None => "out.evidentc".into(),
    };
    p.set_file_name(new_name);
    p
}

/// SHA-256 of the input source, hex-encoded (first 16 chars).
fn hash_src(src: &str) -> String {
    let mut h = Sha256::new();
    h.update(src.as_bytes());
    let bytes = h.finalize();
    let mut hex = String::with_capacity(16);
    for b in bytes.iter().take(8) {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

fn enabled() -> bool {
    std::env::var("EVIDENT_NO_CACHE").ok().as_deref() != Some("1")
}

/// Try to read the cached simplified assertions. Returns `Some(parsed)` only
/// if the file exists, matches the codegen version, matches the source hash,
/// and parses cleanly.
///
/// `decl_preamble` is the textual declarations (`(declare-fun …)` etc.) that
/// must be in scope when re-parsing the cached assertions.
pub unsafe fn try_load(
    input: &Path,
    src: &str,
    ctx: Z3_context,
    decl_preamble: &str,
) -> Option<Vec<Z3_ast>> {
    if !enabled() {
        return None;
    }
    let path = cache_path(input);
    let body = fs::read_to_string(&path).ok()?;
    let first_line = body.lines().next()?;
    let want_header = format!(
        ";; evidentc cache {} codegen={} src-hash={}",
        CACHE_FORMAT_TAG,
        CODEGEN_VERSION,
        hash_src(src),
    );
    if first_line != want_header {
        return None;
    }

    // Body after the header line is `<decl-preamble>` + `<asserts>`. We need
    // BOTH together when calling Z3_parse_smtlib2_string so the asserts'
    // identifiers resolve. Drop our header line; everything else goes to Z3.
    let mut to_parse = String::new();
    for (i, line) in body.lines().enumerate() {
        if i == 0 { continue; }
        to_parse.push_str(line);
        to_parse.push('\n');
    }
    parse_smtlib_to_asts(ctx, &to_parse, decl_preamble)
}

/// Write the simplified assertions to the cache. Best-effort — IO errors
/// are ignored (cache is an optimisation, not a correctness layer).
pub unsafe fn save(
    input: &Path,
    src: &str,
    ctx: Z3_context,
    decl_preamble: &str,
    simplified: &[Z3_ast],
) -> io::Result<()> {
    if !enabled() {
        return Ok(());
    }
    let path = cache_path(input);
    let mut out = String::new();
    out.push_str(&format!(
        ";; evidentc cache {} codegen={} src-hash={}\n",
        CACHE_FORMAT_TAG,
        CODEGEN_VERSION,
        hash_src(src),
    ));
    out.push_str(decl_preamble);
    out.push('\n');
    for &a in simplified {
        let p = Z3_ast_to_string(ctx, a);
        if !p.is_null() {
            let s = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
            out.push_str("(assert ");
            out.push_str(&s);
            out.push_str(")\n");
        }
    }
    fs::write(&path, out)
}

const CACHE_FORMAT_TAG: &str = "v0";

/// Parse a string of SMT-LIB assertions plus a decl preamble into a Vec of
/// Z3_ast handles. Returns None on parse error.
unsafe fn parse_smtlib_to_asts(
    ctx: Z3_context,
    body: &str,
    _decl_preamble: &str,
) -> Option<Vec<Z3_ast>> {
    let c_body = std::ffi::CString::new(body).ok()?;
    let null = std::ptr::null();
    let null_sort: *const Z3_sort = std::ptr::null();
    let null_decl: *const Z3_func_decl = std::ptr::null();
    let vec = Z3_parse_smtlib2_string(
        ctx,
        c_body.as_ptr(),
        0,
        null as *const Z3_symbol,
        null_sort,
        0,
        null as *const Z3_symbol,
        null_decl,
    );
    if vec.is_null() {
        return None;
    }
    Z3_ast_vector_inc_ref(ctx, vec);
    let n = Z3_ast_vector_size(ctx, vec);
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let a = Z3_ast_vector_get(ctx, vec, i);
        Z3_inc_ref(ctx, a);
        out.push(a);
    }
    Z3_ast_vector_dec_ref(ctx, vec);
    Some(out)
}
