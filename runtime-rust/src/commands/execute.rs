//! `evident execute <file> [--width N] [--height N] [--title S]
//! [--host H] [--port P] [--quiet | --explain]` — run `schema main`
//! as a constraint automaton. SDL plugin auto-activates when `main`
//! references SDLInput / SDLOutput / SDLWindow; otherwise headless
//! stdin/stdout.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{executor, EvidentRuntime, Value};
use evident_runtime::executor::Plugin;
use evident_runtime::plugins::audio as audio_plugin;
use evident_runtime::plugins::sdl as sdl_plugin;
use evident_runtime::ast::BodyItem;

use super::initial_state::parse_initial_state;

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
    /// `--initial-state path/to/file.json`: load a JSON object whose
    /// top-level keys become first-frame `given` entries. Useful for
    /// seeding `world.*` state at startup without hard-coding it in
    /// the program. JSON shape: top-level object of int / bool /
    /// string / homogeneous array values.
    initial_state: Option<std::path::PathBuf>,
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
            initial_state: None,
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
            "--initial-state" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--initial-state needs a path".to_string())?;
                out.initial_state = Some(std::path::PathBuf::from(v));
                i += 1;
            }
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
    // Loader: builds a fresh runtime with all stdlibs + the user file.
    // Used both for the initial program and for any program loaded mid-
    // run via `next_main = "..."` swap.
    //
    // Embedded MainCoordinator stdlib is inlined here so users can
    // declare `..MainCoordinator` without an `import` and so program-
    // swap works even when stdlib/ isn't on disk.
    const STDLIB_MAIN_COORDINATOR_EV: &str =
        "claim MainCoordinator\n    next_main ∈ String\n";
    let load_program = |path: &Path| -> Result<EvidentRuntime, String> {
        let mut rt = EvidentRuntime::new();
        executor::load_io_stdlib(&mut rt)?;
        rt.load_source(sdl_plugin::STDLIB_SDL_EV).map_err(|e| e.to_string())?;
        rt.load_source(audio_plugin::STDLIB_SDL_AUDIO_EV).map_err(|e| e.to_string())?;
        rt.load_source(STDLIB_MAIN_COORDINATOR_EV).map_err(|e| e.to_string())?;
        rt.load_file(path).map_err(|e| e.to_string())?;
        Ok(rt)
    };

    // Pre-load the initial program so we can decide whether to
    // activate the SDL plugin (which needs window dimensions baked in
    // at construction time).
    let initial_path = std::path::PathBuf::from(path);
    let initial_rt = match load_program(&initial_path) {
        Ok(rt) => rt,
        Err(e) => { eprintln!("execute: {path}: {e}"); return ExitCode::from(1); }
    };
    let sdl_vars = collect_sdl_vars(&initial_rt);

    let exec_opts = executor::ExecOptions { quiet: opts.quiet, explain: opts.explain };
    // Always include stdio + audio plugins. The executor's matcher
    // filters out plugins whose handles_types() doesn't match any
    // declared var in main, so unused ones are zero-cost (the audio
    // device only opens if the program declares `∈ SDLAudio`).
    let stdin  = executor::StdinPlugin::new(std::io::stdin());
    let stdout = executor::StdoutPlugin::new(std::io::stdout());
    let mut plugins: Vec<Box<dyn Plugin>> = vec![
        Box::new(stdin),
        Box::new(stdout),
        audio_plugin::create_audio_plugin(),
    ];
    // Decide whether to bring the SDL window plugin along based on
    // whether the initial program declares any SDL vars. Programs
    // loaded later via `next_main` swap can also use SDL only if the
    // initial program did — the executor reuses the same plugin list.
    if !sdl_vars.is_empty() {
        // SDL window plugin needs --width/--height/--title for window
        // construction (defaults: 800×600 "Evident" — same as evident.py).
        // Per-var type info is now handed to the plugin via the
        // executor's plugin matcher; no need to pre-populate.
        let _ = sdl_vars; // keep the activation check; var_types comes from initialize()
        plugins.push(sdl_plugin::create_sdl_plugin(
            opts.width, opts.height, opts.title.clone()));
    }

    // Wrap loader for the multi-program executor: the executor calls
    // it for every NEW program, so we need to re-build runtimes from
    // scratch each time. We've already loaded the initial one — pass
    // it through on the first call to avoid double-loading.
    let mut initial_consumed = false;
    let mut initial_holder = Some(initial_rt);
    let loader_for_executor = move |p: &Path| -> Result<EvidentRuntime, String> {
        if !initial_consumed && p == initial_path {
            initial_consumed = true;
            return Ok(initial_holder.take().unwrap());
        }
        load_program(p)
    };

    // Parse --initial-state JSON if provided, into a HashMap that
    // seeds the first frame's `given`.
    let initial_given: HashMap<String, Value> = match &opts.initial_state {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(src) => match parse_initial_state(&src) {
                Ok(map) => map,
                Err(e) => { eprintln!("execute: --initial-state {}: {e}", p.display()); return ExitCode::from(1); }
            },
            Err(e) => { eprintln!("execute: --initial-state {}: {e}", p.display()); return ExitCode::from(1); }
        },
        None => HashMap::new(),
    };

    match executor::run_with_main_coordinator(
        std::path::PathBuf::from(path),
        loader_for_executor,
        &mut plugins,
        &exec_opts,
        initial_given,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("execute: {e}"); ExitCode::from(1) }
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
            BodyItem::Membership { name, type_name, .. } => {
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
