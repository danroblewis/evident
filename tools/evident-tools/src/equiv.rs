//! Semantic-verification plumbing for `evt`: flatten → oracle emit → Z3.
//!
//! Evident's superpower: a program compiles to an SMT formula. So we can ask
//! Z3 questions about it directly — is this claim satisfiable? do two versions
//! of the compiler agree on a fixture? These commands wrap the same
//! flatten/oracle path the rest of the toolchain uses.
//!
//! - `evt sat <file.ev> <claim>` — emit + `(check-sat)`. Catches silent
//!   over-constraining (a new type invariant that turns the claim UNSAT).
//! - `evt diff <fixture.ev>`      — compile the fixture through the CURRENT and
//!   an `--old` compiler and diff stdout+exit. Auto-derives "expected" from
//!   the old version; no hand-written assertions.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Tools {
    pub root: PathBuf,
    pub flatten: PathBuf,
    pub oracle: String,
    pub z3: String,
    pub kernel: PathBuf,
}

impl Tools {
    pub fn discover(root: &Path) -> Tools {
        let oracle = std::env::var("EVIDENT_ORACLE")
            .unwrap_or_else(|_| "/usr/local/bin/evident-oracle".to_string());
        let z3 = std::env::var("EVIDENT_Z3").unwrap_or_else(|_| "z3".to_string());
        Tools {
            root: root.to_path_buf(),
            flatten: root.join("scripts/flatten-evident.sh"),
            oracle,
            z3,
            kernel: root.join("kernel/target/release/kernel"),
        }
    }
}

/// Flatten a `.ev` file (resolve imports + run the pre-oracle transforms).
pub fn flatten(t: &Tools, src: &Path) -> Result<Vec<u8>, String> {
    if !t.flatten.exists() {
        return Err(format!("flatten script not found: {}", t.flatten.display()));
    }
    let out = Command::new("bash")
        .arg(&t.flatten)
        .arg(src)
        .output()
        .map_err(|e| format!("flatten exec: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "flatten failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

/// Emit a flattened source to SMT-LIB for `claim`, returning the .smt2 text.
pub fn emit(t: &Tools, flat: &[u8], claim: &str) -> Result<String, String> {
    if !Path::new(&t.oracle).exists() {
        return Err(format!("oracle not found: {} (set EVIDENT_ORACLE)", t.oracle));
    }
    let tmp_in = unique_tmp("evt_sat_in", "ev");
    let tmp_out = unique_tmp("evt_sat_out", "smt2");
    std::fs::write(&tmp_in, flat).map_err(|e| format!("write temp: {e}"))?;
    let out = Command::new(&t.oracle)
        .arg("emit")
        .arg(&tmp_in)
        .arg(claim)
        .arg("-o")
        .arg(&tmp_out)
        .output()
        .map_err(|e| format!("oracle exec: {e}"))?;
    let _ = std::fs::remove_file(&tmp_in);
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp_out);
        return Err(format!(
            "oracle emit failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let smt = std::fs::read_to_string(&tmp_out).map_err(|e| format!("read emit: {e}"))?;
    let _ = std::fs::remove_file(&tmp_out);
    Ok(smt)
}

pub struct SatResult {
    pub status: String, // "sat" | "unsat" | "unknown"
    pub model: Option<String>,
}

/// Run `(check-sat)` (and optionally `(get-model)`) on an emitted body.
/// The manifest `;;` lines are comments; Z3 ignores them.
pub fn check_sat(t: &Tools, smt_body: &str, want_model: bool) -> Result<SatResult, String> {
    let mut script = String::with_capacity(smt_body.len() + 64);
    if want_model {
        script.push_str("(set-option :produce-models true)\n");
    }
    script.push_str(smt_body);
    if !smt_body.ends_with('\n') {
        script.push('\n');
    }
    script.push_str("(check-sat)\n");
    if want_model {
        script.push_str("(get-model)\n");
    }
    let tmp = unique_tmp("evt_checksat", "smt2");
    std::fs::write(&tmp, &script).map_err(|e| format!("write script: {e}"))?;
    let out = Command::new(&t.z3)
        .arg("-smt2")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("z3 exec ({}): {e}", t.z3))?;
    let _ = std::fs::remove_file(&tmp);
    let text = String::from_utf8_lossy(&out.stdout);
    let mut status = String::new();
    let mut model_lines: Vec<&str> = Vec::new();
    let mut in_model = false;
    for line in text.lines() {
        let l = line.trim();
        if status.is_empty() && (l == "sat" || l == "unsat" || l == "unknown") {
            status = l.to_string();
            in_model = want_model && l == "sat";
            continue;
        }
        if in_model {
            model_lines.push(line);
        }
        if l.starts_with("(error") {
            return Err(format!("z3 error: {}", text.trim()));
        }
    }
    if status.is_empty() {
        return Err(format!("z3 produced no sat/unsat: {}", text.trim()));
    }
    Ok(SatResult {
        status,
        model: if model_lines.is_empty() {
            None
        } else {
            Some(model_lines.join("\n"))
        },
    })
}

pub struct RunResult {
    pub exit: i32,
    pub stdout: String,
}

/// Compile `fixture` through a stage1 compiler (`comp` = stage1.smt2) by running
/// it under the kernel with the wave-4o wire protocol (stdin line 1 = flattened
/// fixture path, line 2 = claim). Returns the emitted unit's text (the
/// compiler's stdout, functionizer line stripped).
pub fn c2_compile(t: &Tools, comp: &Path, fixture: &Path, claim: &str) -> Result<String, String> {
    let flat = flatten(t, fixture)?;
    let flat_path = unique_tmp("evt_diff_flat", "ev");
    std::fs::write(&flat_path, &flat).map_err(|e| format!("write flat: {e}"))?;
    let stdin = format!("{}\n{}\n", flat_path.display(), claim);
    let mut child = Command::new(&t.kernel)
        .arg(comp)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("kernel spawn: {e}"))?;
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .map_err(|e| format!("kernel stdin: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("kernel wait: {e}"))?;
    let _ = std::fs::remove_file(&flat_path);
    let text = String::from_utf8_lossy(&out.stdout);
    let stripped: String = text
        .lines()
        .filter(|l| !l.starts_with("[functionizer]"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(stripped)
}

/// Build a stage1 compiler (`compiler2/driver.ev` → driver_main) from a given
/// source root (an arbitrary checkout / worktree). Returns the stage1 .smt2.
pub fn build_stage1(t: &Tools, src_root: &Path, out: &Path) -> Result<(), String> {
    let driver = src_root.join("compiler2/driver.ev");
    if !driver.exists() {
        return Err(format!("no compiler2/driver.ev under {}", src_root.display()));
    }
    // Use the source root's OWN flatten script (its transforms may differ).
    let its_flatten = src_root.join("scripts/flatten-evident.sh");
    let flatten = if its_flatten.exists() { its_flatten } else { t.flatten.clone() };
    // The OLD checkout may lack a built kernel (its `kernel/target` symlink is
    // absent in a throwaway worktree), but its flatten/autocarry transforms
    // need ONE. Point them at THIS worktree's kernel via EVIDENT_KERNEL — the
    // kernel is the runner, identical across adjacent commits.
    let fl = Command::new("bash")
        .arg(&flatten)
        .arg(&driver)
        .env("EVIDENT_KERNEL", &t.kernel)
        .output()
        .map_err(|e| format!("flatten exec: {e}"))?;
    if !fl.status.success() {
        return Err(format!("flatten failed: {}", String::from_utf8_lossy(&fl.stderr)));
    }
    let tmp_in = unique_tmp("evt_stage1_in", "ev");
    std::fs::write(&tmp_in, &fl.stdout).map_err(|e| format!("write flat: {e}"))?;
    let em = Command::new(&t.oracle)
        .arg("emit")
        .arg(&tmp_in)
        .arg("driver_main")
        .arg("-o")
        .arg(out)
        .output()
        .map_err(|e| format!("oracle exec: {e}"))?;
    let _ = std::fs::remove_file(&tmp_in);
    if !em.status.success() {
        return Err(format!("oracle emit failed: {}", String::from_utf8_lossy(&em.stderr)));
    }
    Ok(())
}

fn unique_tmp(stem: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let seq = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{stem}.{pid}.{seq}.{ext}"))
}
