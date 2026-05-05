//! `evident execute <file> [--width N] [--height N] [--title S]
//! [--host H] [--port P] [--quiet | --explain]` — run `schema main`
//! as a constraint automaton. SDL plugin auto-activates when `main`
//! references SDLInput / SDLOutput / SDLWindow; otherwise headless
//! stdin/stdout.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{executor, EvidentRuntime};
use evident_runtime::executor::Plugin;
use evident_runtime::plugins::sdl as sdl_plugin;
use evident_runtime::ast::BodyItem;

use super::common::usage;

/// Flags accepted by the `execute` subcommand. Mirrors the argparse
/// declarations in `evident.py` (`ex.add_argument('--width', …)` etc.).
///
/// `width` / `height` / `title` are consumed by the SDL plugin; `host`
/// / `port` are reserved for a future TCP-socket plugin. Parsing them
/// today (even though the plugins they target may not be wired in yet)
/// keeps the CLI surface stable so adding the plugin doesn't have to
/// retouch arg-parsing.
#[allow(dead_code)]
struct ExecuteOpts {
    width:  u32,
    height: u32,
    title:  String,
    host:   String,
    port:   u16,
    /// `--quiet`: suppress per-step UNSAT warnings. Default behavior is
    /// loud — every UNSAT step prints a one-line warning so the user
    /// can't miss that the program is silently dropping frames.
    quiet:  bool,
    /// `--explain`: when a step is UNSAT, dump the per-step `given`
    /// values + the schema body items pretty-printed, so the user has
    /// enough context to start narrowing the conflict without re-running
    /// `evident query` separately.
    explain: bool,
}

impl Default for ExecuteOpts {
    fn default() -> Self {
        ExecuteOpts {
            width:  800,
            height: 600,
            title:  "Evident".to_string(),
            host:   "127.0.0.1".to_string(),
            port:   8080,
            quiet:  false,
            explain: false,
        }
    }
}

fn parse_execute_flags(flags: &[String]) -> Result<ExecuteOpts, String> {
    let mut out = ExecuteOpts::default();
    let mut i = 0;
    while i < flags.len() {
        match flags[i].as_str() {
            "--width" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--width needs a value".to_string())?;
                out.width = v.parse::<u32>().map_err(|e| format!("bad --width {v:?}: {e}"))?;
                i += 1;
            }
            "--height" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--height needs a value".to_string())?;
                out.height = v.parse::<u32>().map_err(|e| format!("bad --height {v:?}: {e}"))?;
                i += 1;
            }
            "--title" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--title needs a value".to_string())?;
                out.title = v.clone();
                i += 1;
            }
            "--host" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--host needs a value".to_string())?;
                out.host = v.clone();
                i += 1;
            }
            "--port" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--port needs a value".to_string())?;
                out.port = v.parse::<u16>().map_err(|e| format!("bad --port {v:?}: {e}"))?;
                i += 1;
            }
            "--quiet"   => { out.quiet   = true; i += 1; }
            "--explain" => { out.explain = true; i += 1; }
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown execute flag: {other}")),
        }
    }
    Ok(out)
}

pub fn cmd_execute(args: &[String]) -> ExitCode {
    // `--help` first, before file-positional check, so `execute --help`
    // works without needing a file argument.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return ExitCode::SUCCESS;
    }
    if args.is_empty() {
        eprintln!("execute: need <file.ev>");
        return ExitCode::from(2);
    }
    // First positional is the file; everything after is flags.
    let path = &args[0];
    let opts = match parse_execute_flags(&args[1..]) {
        Ok(o) => o,
        Err(e) => { eprintln!("execute: {e}"); return ExitCode::from(2); }
    };
    let mut rt = EvidentRuntime::new();
    // Load embedded stdlibs first so user programs can declare
    // ∈ Stdin / ∈ Stdout / ∈ SDLInput etc. without `import`. Both are
    // flat shims (no `..` passthrough chains) since the Rust runtime
    // doesn't yet recurse into `..` during sub-schema field expansion.
    if let Err(e) = executor::load_io_stdlib(&mut rt) {
        eprintln!("execute: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_source(sdl_plugin::STDLIB_SDL_EV) {
        eprintln!("execute: sdl stdlib: {e}");
        return ExitCode::from(1);
    }
    // Use load_file so `import "..."` statements in the user program
    // resolve relative to the file's own directory.
    if let Err(e) = rt.load_file(Path::new(path)) {
        eprintln!("execute: {path}: {e}");
        return ExitCode::from(1);
    }

    // Inspect main's body to find SDL var declarations. If any are
    // present, instantiate the SDL plugin and add it to the plugin
    // list. Otherwise, fall back to the headless stdin/stdout path.
    let sdl_vars = collect_sdl_vars(&rt);

    let exec_opts = executor::ExecOptions { quiet: opts.quiet, explain: opts.explain };
    if sdl_vars.is_empty() {
        // Pure headless: stdin/stdout only.
        let stdin  = executor::StdinPlugin::new(std::io::stdin());
        let stdout = executor::StdoutPlugin::new(std::io::stdout());
        let mut plugins: Vec<Box<dyn Plugin>> = vec![Box::new(stdin), Box::new(stdout)];
        match executor::run_with_plugins_opts(&rt, &mut plugins, &exec_opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("execute: {e}"); ExitCode::from(1) }
        }
    } else {
        // SDL active: defaults from --width/--height/--title (else
        // 800×600 "Evident" — same defaults as evident.py).
        let sdl = sdl_plugin::create_sdl_plugin(
            opts.width, opts.height, opts.title.clone(), sdl_vars);
        let stdin = executor::StdinPlugin::new(std::io::stdin());
        let stdout = executor::StdoutPlugin::new(std::io::stdout());
        let mut plugins: Vec<Box<dyn Plugin>> = vec![Box::new(stdin), Box::new(stdout), sdl];
        match executor::run_with_plugins_opts(&rt, &mut plugins, &exec_opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("execute: {e}"); ExitCode::from(1) }
        }
    }
}

/// Walk `main`'s body (including `..` passthroughs) collecting variables
/// whose declared type is one of the SDL types. Returns the same
/// `var → type_name` map shape that `SDLPlugin` needs in `var_types`.
fn collect_sdl_vars(rt: &EvidentRuntime) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(main) = rt.get_schema("main") else { return out };
    let mut visited: Vec<String> = Vec::new();
    walk_body_for_sdl(rt, &main.name, &mut visited, &mut out);
    out
}

fn walk_body_for_sdl(
    rt: &EvidentRuntime,
    schema_name: &str,
    visited: &mut Vec<String>,
    out: &mut HashMap<String, String>,
) {
    if visited.iter().any(|n| n == schema_name) {
        return;
    }
    visited.push(schema_name.to_string());
    let Some(schema) = rt.get_schema(schema_name) else { return };
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                if sdl_plugin::SDL_TYPES.iter().any(|t| *t == type_name.as_str()) {
                    out.entry(name.clone()).or_insert_with(|| type_name.clone());
                }
            }
            BodyItem::Passthrough(claim) => {
                walk_body_for_sdl(rt, claim, visited, out);
            }
            _ => {}
        }
    }
}
