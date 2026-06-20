//! `evident phase-portrait <daemon.ev> --axes a,b` — the generic phase-portrait
//! renderer. A daemon written as an `fsm` IS a dynamical system: its body relates
//! the previous state (`_state`) to the next (`state`). This tool reads NO
//! hardcoded dynamics — it samples a grid of states, QUERIES the daemon's
//! transition at each (pin `_axis`, solve for `axis`) to draw the flow, integrates
//! a few trajectories from seeds, and renders both via SDL. Works for any daemon.
//!
//! See docs/design/phase-portraits.md. This is the "flow" half (Part II.4); the
//! proven-invariant-region half (Spacer) is a later phase.

use std::collections::HashMap;
use std::process::ExitCode;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::ast::{Effect, EffectFfiArg as A, EffectResult};
use evident_runtime::ffi::{DispatchContext, dispatch_all, dispatch_one};

const SDL: &str = "libSDL2-2.0.so.0";
const W: i64 = 640;
const H: i64 = 480;
const MARGIN: f64 = 40.0;

fn lib(sym: &str, sig: &str, args: Vec<A>) -> Effect {
    Effect::LibCall(SDL.into(), sym.into(), sig.into(), args)
}
fn as_int(v: Option<&Value>) -> Option<i64> {
    match v { Some(Value::Int(n)) => Some(*n), _ => None }
}
/// previous-tick name of an axis: prefix the first dotted segment with `_`
/// (`state.pos` -> `_state.pos`, `x` -> `_x`).
fn prev(axis: &str) -> String { format!("_{axis}") }

/// One step of the daemon's transition: pin `_a,_b` to (a,b), solve, read (a,b).
fn step(rt: &EvidentRuntime, claim: &str, ax: &str, bx: &str, a: i64, b: i64) -> Option<(i64, i64)> {
    let mut g: HashMap<String, Value> = HashMap::new();
    g.insert(prev(ax), Value::Int(a));
    g.insert(prev(bx), Value::Int(b));
    g.insert("is_first_tick".into(), Value::Bool(false));
    let r = rt.query_with_pins_and_given(claim, &[], &g).ok()?;
    if !r.satisfied { return None; }
    Some((as_int(r.bindings.get(ax))?, as_int(r.bindings.get(bx))?))
}

struct Args {
    file: String, axis_a: String, axis_b: String,
    seeds: Vec<(i64, i64)>, grid: usize, steps: usize,
    range: Option<(f64, f64, f64, f64)>, text: bool, svg: Option<String>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut file = None; let mut axes = None; let mut seeds = Vec::new();
    let mut grid = 13usize; let mut steps = 220usize; let mut range = None; let mut text = false;
    let mut svg = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--axes" => { axes = Some(args.get(i+1).ok_or("--axes needs a,b")?.clone()); i += 2; }
            "--grid" => { grid = args.get(i+1).and_then(|s| s.parse().ok()).ok_or("--grid N")?; i += 2; }
            "--steps" => { steps = args.get(i+1).and_then(|s| s.parse().ok()).ok_or("--steps N")?; i += 2; }
            "--text" => { text = true; i += 1; }
            "--svg" => { svg = Some(args.get(i+1).ok_or("--svg PATH")?.clone()); i += 2; }
            "--range" => {
                let p: Vec<f64> = args.get(i+1).ok_or("--range alo,ahi,blo,bhi")?
                    .split(',').filter_map(|s| s.parse().ok()).collect();
                if p.len() != 4 { return Err("--range needs alo,ahi,blo,bhi".into()); }
                range = Some((p[0], p[1], p[2], p[3])); i += 2;
            }
            "--seeds" => {
                for pair in args.get(i+1).ok_or("--seeds a,b;a,b")?.split(';') {
                    let c: Vec<i64> = pair.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if c.len() == 2 { seeds.push((c[0], c[1])); }
                }
                i += 2;
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => { file = Some(other.to_string()); i += 1; }
        }
    }
    let axes = axes.ok_or("--axes is required (e.g. --axes state.pos,state.vel, or --axes state)")?;
    // Two axes → numeric vector field. One axis → discrete (enum/bool) state line.
    let (a, b) = match axes.split_once(',') {
        Some((a, b)) => (a, b),
        None => (axes.as_str(), axes.as_str()),
    };
    if seeds.is_empty() { seeds = vec![(200, 0), (0, 150)]; }
    Ok(Args { file: file.ok_or("a daemon .ev file is required")?,
              axis_a: a.into(), axis_b: b.into(), seeds, grid, steps, range, text, svg })
}

pub fn cmd_phase_portrait(args: &[String]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => { eprintln!("phase-portrait: {e}"); usage(); return ExitCode::from(2); }
    };
    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(std::path::Path::new("stdlib/runtime.ev")) {
        eprintln!("phase-portrait: load stdlib: {e}"); return ExitCode::from(2);
    }
    if let Err(e) = rt.load_file(std::path::Path::new(&a.file)) {
        eprintln!("phase-portrait: load {}: {e}", a.file); return ExitCode::from(2);
    }
    let claim = match evident_runtime::trampoline::single_fsm(&rt) {
        Ok(shape) => shape.claim_name,
        Err(e) => { eprintln!("phase-portrait: no single fsm in {}: {e}", a.file); return ExitCode::from(2); }
    };

    // Probe the initial state (is_first_tick) and classify each axis. Enum/bool
    // axes are discrete; their `given` is handled only on the slow Z3 path
    // (session/mod.rs), so force the functionizer off while probing/exploring.
    //   - one discrete axis           → difference-equation state line
    //   - two axes, ≥1 discrete        → mixed forward-explored portrait
    //   - two numeric axes             → numeric vector field (below)
    rt.set_functionize_enabled(false);
    if let Some(init) = probe_init(&rt, &claim) {
        let is_disc = |k: &str| matches!(init.get(k), Some(Value::Enum { .. } | Value::Bool(_)));
        let single = a.axis_a == a.axis_b;
        if single && is_disc(&a.axis_a) {
            return discrete_portrait(&rt, &claim, &a.axis_a,
                init[&a.axis_a].clone(), a.steps, a.text, a.svg.as_deref());
        }
        if !single && (is_disc(&a.axis_a) || is_disc(&a.axis_b)) {
            return mixed_portrait(&rt, &claim, &a.axis_a, &a.axis_b,
                &init, a.steps, a.text, a.svg.as_deref());
        }
    }
    rt.set_functionize_enabled(true);

    // Integrate trajectories by repeatedly querying the transition.
    let mut trajs: Vec<Vec<(i64, i64)>> = Vec::new();
    for &(sa, sb) in &a.seeds {
        let mut pts = vec![(sa, sb)];
        let (mut ca, mut cb) = (sa, sb);
        for _ in 0..a.steps {
            match step(&rt, &claim, &a.axis_a, &a.axis_b, ca, cb) {
                Some((na, nb)) => { ca = na; cb = nb; pts.push((ca, cb)); }
                None => break,
            }
        }
        trajs.push(pts);
    }

    // Auto-range from the trajectories (bbox + padding), unless --range given.
    let (alo, ahi, blo, bhi) = a.range.unwrap_or_else(|| {
        let all: Vec<(i64, i64)> = trajs.iter().flatten().copied().collect();
        let (mut alo, mut ahi, mut blo, mut bhi) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for &(x, y) in &all {
            alo = alo.min(x as f64); ahi = ahi.max(x as f64);
            blo = blo.min(y as f64); bhi = bhi.max(y as f64);
        }
        if alo > ahi { (alo, ahi, blo, bhi) = (-200.0, 200.0, -200.0, 200.0); }
        let (pa, pb) = ((ahi - alo).max(1.0) * 0.12, (bhi - blo).max(1.0) * 0.12);
        (alo - pa, ahi + pa, blo - pb, bhi + pb)
    });
    let to_screen = |x: f64, y: f64| -> (i64, i64) {
        let sx = MARGIN + (x - alo) / (ahi - alo).max(1e-9) * (W as f64 - 2.0 * MARGIN);
        let sy = (H as f64 - MARGIN) - (y - blo) / (bhi - blo).max(1e-9) * (H as f64 - 2.0 * MARGIN);
        (sx.round() as i64, sy.round() as i64)
    };

    // Vector field: query the transition at each grid cell, normalize the
    // displacement to a fixed arrow length.
    let mut arrows: Vec<((i64, i64), (i64, i64))> = Vec::new();
    let g = a.grid.max(2);
    for i in 0..g {
        for j in 0..g {
            let x = alo + (ahi - alo) * (i as f64) / (g as f64 - 1.0);
            let y = blo + (bhi - blo) * (j as f64) / (g as f64 - 1.0);
            if let Some((nx, ny)) = step(&rt, &claim, &a.axis_a, &a.axis_b, x.round() as i64, y.round() as i64) {
                let (dx, dy) = (nx as f64 - x, ny as f64 - y);
                let mag = (dx * dx + dy * dy).sqrt();
                if mag < 1e-6 { continue; }
                // fixed screen-space arrow length
                let l = 16.0;
                let (bx, by) = to_screen(x, y);
                let (ex, ey) = (bx as f64 + dx / mag * l, by as f64 - dy / mag * l);
                arrows.push(((bx, by), (ex.round() as i64, ey.round() as i64)));
            }
        }
    }

    if a.text {
        return render_text(&claim, &a.axis_a, &a.axis_b, (alo, ahi, blo, bhi),
                           to_screen, &arrows, &trajs);
    }
    if let Some(path) = a.svg.as_deref() {
        return render_svg(path, &claim, &a.axis_a, &a.axis_b, (alo, ahi, blo, bhi),
                          to_screen, &arrows, &trajs);
    }
    render(&claim, to_screen, &arrows, &trajs)
}

/// Headless SVG phase portrait. Same `arrows`/`trajs`/`to_screen` data the
/// SDL renderer uses, emitted as `<line>`/`<circle>` elements on a W×H
/// canvas — crisp, resolution-independent, no display, identical on every
/// platform. Trajectory colors match the SDL palette.
fn render_svg<F: Fn(f64, f64) -> (i64, i64)>(
    path: &str, title: &str, ax: &str, bx: &str, range: (f64, f64, f64, f64),
    to_screen: F, arrows: &[((i64, i64), (i64, i64))], trajs: &[Vec<(i64, i64)>],
) -> ExitCode {
    let (alo, ahi, blo, bhi) = range;
    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{W}\" height=\"{H}\" \
         viewBox=\"0 0 {W} {H}\" font-family=\"monospace\" font-size=\"12\">\n"));
    s.push_str(&format!("<rect width=\"{W}\" height=\"{H}\" fill=\"#0a0c14\"/>\n"));

    // Axes (a=0 column, b=0 row) where they fall inside the view.
    s.push_str("<g stroke=\"#323c5a\" stroke-width=\"1\">\n");
    let (zx, zy) = to_screen(0.0, 0.0);
    if (blo..=bhi).contains(&0.0) {
        s.push_str(&format!("<line x1=\"0\" y1=\"{zy}\" x2=\"{W}\" y2=\"{zy}\"/>\n"));
    }
    if (alo..=ahi).contains(&0.0) {
        s.push_str(&format!("<line x1=\"{zx}\" y1=\"0\" x2=\"{zx}\" y2=\"{H}\"/>\n"));
    }
    s.push_str("</g>\n");

    // Vector field: line + a small arrowhead at the tip.
    s.push_str("<g stroke=\"#5aaaff\" stroke-width=\"1\" fill=\"none\">\n");
    for &((bx0, by0), (ex, ey)) in arrows {
        s.push_str(&format!("<line x1=\"{bx0}\" y1=\"{by0}\" x2=\"{ex}\" y2=\"{ey}\"/>\n"));
        let (dx, dy) = ((ex - bx0) as f64, (ey - by0) as f64);
        let mag = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (ux, uy) = (dx / mag, dy / mag);     // unit along the arrow
        let (px, py) = (-uy, ux);                // perpendicular
        let (hl, hw) = (5.0, 2.5);               // head length / half-width
        let (lx, ly) = (ex as f64 - ux * hl + px * hw, ey as f64 - uy * hl + py * hw);
        let (rx, ry) = (ex as f64 - ux * hl - px * hw, ey as f64 - uy * hl - py * hw);
        s.push_str(&format!(
            "<polyline points=\"{lx:.1},{ly:.1} {ex},{ey} {rx:.1},{ry:.1}\"/>\n"));
    }
    s.push_str("</g>\n");

    // Trajectories: a dot per integrated state, one color per seed.
    const PALETTE: [&str; 4] = ["#50e678", "#faaa32", "#5ac8ff", "#f078f0"];
    for (k, t) in trajs.iter().enumerate() {
        let col = PALETTE[k % PALETTE.len()];
        s.push_str(&format!("<g fill=\"{col}\">\n"));
        for &(x, y) in t {
            let (sx, sy) = to_screen(x as f64, y as f64);
            s.push_str(&format!("<circle cx=\"{sx}\" cy=\"{sy}\" r=\"1.6\"/>\n"));
        }
        s.push_str("</g>\n");
    }

    // Caption + axis labels (x along the bottom, y rotated up the left edge).
    s.push_str(&format!(
        "<text x=\"8\" y=\"16\" fill=\"#aab4d0\">{title} — {ax} ∈ [{alo:.0},{ahi:.0}], \
         {bx} ∈ [{blo:.0},{bhi:.0}]</text>\n"));
    let (cx, cy) = (W / 2, H / 2);
    s.push_str(&format!(
        "<text x=\"{cx}\" y=\"{}\" fill=\"#8a94b4\" text-anchor=\"middle\">{ax} →</text>\n",
        H - 6));
    s.push_str(&format!(
        "<text x=\"14\" y=\"{cy}\" fill=\"#8a94b4\" text-anchor=\"middle\" \
         transform=\"rotate(-90 14 {cy})\">{bx} →</text>\n"));
    s.push_str("</svg>\n");

    if let Err(e) = std::fs::write(path, s) {
        eprintln!("phase-portrait: write {path}: {e}");
        return ExitCode::from(1);
    }
    println!("wrote {path}  ({} trajectories, {} field arrows)", trajs.len(), arrows.len());
    ExitCode::SUCCESS
}

/// Headless ASCII phase portrait. Plots the vector field (8-way arrow
/// glyphs) and the trajectories (one glyph per seed) onto a character
/// grid, reusing the screen-space `arrows`/`trajs` already computed — no
/// extra solves, no display. Same picture the SDL renderer draws, in the
/// terminal.
fn render_text<F: Fn(f64, f64) -> (i64, i64)>(
    title: &str, ax: &str, bx: &str, range: (f64, f64, f64, f64),
    to_screen: F, arrows: &[((i64, i64), (i64, i64))], trajs: &[Vec<(i64, i64)>],
) -> ExitCode {
    const TCOLS: usize = 78;
    const TROWS: usize = 32;
    let (alo, ahi, blo, bhi) = range;

    // Screen-space (0..W, 0..H) -> character cell. Screen y already runs
    // top=high-b, matching text rows.
    let to_cell = |sx: i64, sy: i64| -> (usize, usize) {
        let c = ((sx as f64) / (W as f64) * (TCOLS as f64 - 1.0)).round();
        let r = ((sy as f64) / (H as f64) * (TROWS as f64 - 1.0)).round();
        (c.clamp(0.0, TCOLS as f64 - 1.0) as usize,
         r.clamp(0.0, TROWS as f64 - 1.0) as usize)
    };

    let mut grid = vec![vec![' '; TCOLS]; TROWS];

    // Axes: the b=0 row and a=0 column, where they fall inside the view.
    let (zx, zy) = to_screen(0.0, 0.0);
    let (zc, zr) = to_cell(zx, zy);
    if (blo..=bhi).contains(&0.0) { for c in 0..TCOLS { grid[zr][c] = '─'; } }
    if (alo..=ahi).contains(&0.0) { for r in 0..TROWS { grid[r][zc] = '│'; } }
    if (blo..=bhi).contains(&0.0) && (alo..=ahi).contains(&0.0) { grid[zr][zc] = '┼'; }

    // Vector field: pick an 8-way arrow from the screen-space displacement
    // (flip y back to math orientation so up = increasing b).
    const ARROWS: [char; 8] = ['→', '↗', '↑', '↖', '←', '↙', '↓', '↘'];
    for &((bx0, by0), (ex, ey)) in arrows {
        let (ddx, ddy) = ((ex - bx0) as f64, -(ey - by0) as f64);
        if ddx.abs() < 1e-9 && ddy.abs() < 1e-9 { continue; }
        let mut oct = (ddy.atan2(ddx) / std::f64::consts::FRAC_PI_4).round() as i64 % 8;
        if oct < 0 { oct += 8; }
        let (c, r) = to_cell(bx0, by0);
        if grid[r][c] == ' ' || grid[r][c] == '─' || grid[r][c] == '│' || grid[r][c] == '┼' {
            grid[r][c] = ARROWS[oct as usize];
        }
    }

    // Trajectories on top, one glyph per seed.
    const GLYPHS: [char; 8] = ['o', '+', '*', 'x', '#', '@', '%', '='];
    for (k, t) in trajs.iter().enumerate() {
        let g = GLYPHS[k % GLYPHS.len()];
        for &(x, y) in t {
            let (sx, sy) = to_screen(x as f64, y as f64);
            let (c, r) = to_cell(sx, sy);
            grid[r][c] = g;
        }
    }

    println!("phase portrait — {title}   x: {ax} ∈ [{alo:.0},{ahi:.0}]   y: {bx} ∈ [{blo:.0},{bhi:.0}]");
    let bar: String = "─".repeat(TCOLS);
    println!("┌{bar}┐");
    for row in &grid {
        let line: String = row.iter().collect();
        println!("│{line}│");
    }
    println!("└{bar}┘");
    for (k, t) in trajs.iter().enumerate() {
        println!("  {} seed {:?} .. end {:?}  ({} pts)",
                 GLYPHS[k % GLYPHS.len()], t.first(), t.last(), t.len());
    }
    println!("  arrows: {}   (→↗↑↖←↙↓↘ = flow direction)", arrows.len());
    ExitCode::SUCCESS
}

fn render<F: Fn(f64, f64) -> (i64, i64)>(
    title: &str, _to: F, arrows: &[((i64, i64), (i64, i64))], trajs: &[Vec<(i64, i64)>],
) -> ExitCode {
    let mut ctx = DispatchContext::new();
    let handle = |r: EffectResult| match r { EffectResult::Handle(h) => h, EffectResult::Int(n) => n as u64, _ => 0 };
    dispatch_one(&mut ctx, &lib("SDL_Init", "i(i)", vec![A::Int(32)]));
    let win = handle(dispatch_one(&mut ctx, &lib("SDL_CreateWindow", "p(siiiii)",
        vec![A::Str(format!("Phase Portrait — {title}")), A::Int(805240832), A::Int(805240832),
             A::Int(W), A::Int(H), A::Int(4)])));
    dispatch_one(&mut ctx, &lib("SDL_ShowWindow", "v(p)", vec![A::Handle(win)]));
    dispatch_one(&mut ctx, &lib("SDL_RaiseWindow", "v(p)", vec![A::Handle(win)]));
    let ren = handle(dispatch_one(&mut ctx, &lib("SDL_CreateRenderer", "p(pii)",
        vec![A::Handle(win), A::Int(-1), A::Int(0)])));
    if ren == 0 { eprintln!("phase-portrait: SDL renderer is null"); return ExitCode::from(1); }

    let color = |r: u64, cr, cg, cb| lib("SDL_SetRenderDrawColor", "i(piiii)",
        vec![A::Handle(r), A::Int(cr), A::Int(cg), A::Int(cb), A::Int(255)]);
    let line = |r: u64, x1, y1, x2, y2| lib("SDL_RenderDrawLine", "i(piiii)",
        vec![A::Handle(r), A::Int(x1), A::Int(y1), A::Int(x2), A::Int(y2)]);
    let dot = |r: u64, x: i64, y: i64| lib("SDL_RenderFillRect", "i(pp)",
        vec![A::Handle(r), A::I32Buf(vec![(x-1) as i32, (y-1) as i32, 3, 3])]);

    // Build one frame (the portrait is static — same frame every tick).
    let mut frame = vec![
        color(ren, 10, 12, 20),
        lib("SDL_RenderClear", "i(p)", vec![A::Handle(ren)]),
        color(ren, 50, 60, 90),
        line(ren, 0, H/2, W, H/2),
        line(ren, W/2, 0, W/2, H),
        color(ren, 90, 170, 255),
    ];
    for &((bx, by), (ex, ey)) in arrows {
        frame.push(line(ren, bx, by, ex, ey));
    }
    let palette = [(80, 230, 120), (250, 170, 50), (90, 200, 255), (240, 120, 240)];
    for (k, t) in trajs.iter().enumerate() {
        let (cr, cg, cb) = palette[k % palette.len()];
        frame.push(color(ren, cr, cg, cb));
        for &(x, y) in t {
            let (sx, sy) = _to(x as f64, y as f64);
            frame.push(dot(ren, sx, sy));
        }
    }
    frame.push(lib("SDL_RenderPresent", "v(p)", vec![A::Handle(ren)]));
    frame.push(lib("SDL_PumpEvents", "v()", vec![]));
    frame.push(lib("SDL_Delay", "v(i)", vec![A::Int(33)]));

    for _ in 0..240 { dispatch_all(&mut ctx, &frame); }
    ExitCode::SUCCESS
}

// ───────────────────────── discrete (enum/bool) state portraits ─────────────────────────
// A daemon whose carried state is an enum/bool is still a difference equation — its
// phase portrait is the reachable states laid on a line with the transition arrows
// between them (docs/design/phase-portraits.md Part IV.1), not a numeric field.

/// Query the claim with `given` pinned and read the axis variable's value.
fn query_axis(rt: &EvidentRuntime, claim: &str, axis: &str, given: &HashMap<String, Value>) -> Option<Value> {
    let r = rt.query_with_pins_and_given(claim, &[], given).ok()?;
    if !r.satisfied { return None; }
    r.bindings.get(axis).cloned()
}

/// A short label for a discrete value: `Start`, `Count(5)`, `true`.
fn val_label(v: &Value) -> String {
    match v {
        Value::Enum { variant, fields, .. } if fields.is_empty() => variant.clone(),
        Value::Enum { variant, fields, .. } =>
            format!("{variant}({})", fields.iter().map(val_label).collect::<Vec<_>>().join(",")),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Str(s) => format!("\"{s}\""),
        other => format!("{other:?}"),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// One step of a discrete transition: pin `_axis` to a value, read the successor.
fn discrete_succ(rt: &EvidentRuntime, claim: &str, axis: &str, s: &Value) -> Option<Value> {
    let mut g: HashMap<String, Value> = HashMap::new();
    g.insert(prev(axis), s.clone());
    g.insert("is_first_tick".into(), Value::Bool(false));
    query_axis(rt, claim, axis, &g)
}

/// Forward-explore the reachable states from the initial state (each has one
/// successor) and render them as a state line with transition arrows.
fn discrete_portrait(
    rt: &EvidentRuntime, claim: &str, axis: &str, init: Value,
    max: usize, text: bool, svg: Option<&str>,
) -> ExitCode {
    let mut order: Vec<Value> = vec![init.clone()];
    let mut seen: HashMap<String, usize> = HashMap::new();
    seen.insert(val_label(&init), 0);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut frontier = vec![0usize];
    let cap = max.max(64);
    while let Some(i) = frontier.pop() {
        if order.len() > cap { break; }
        let s = order[i].clone();
        if let Some(succ) = discrete_succ(rt, claim, axis, &s) {
            let k = val_label(&succ);
            let j = match seen.get(&k) {
                Some(&j) => j,
                None => {
                    let j = order.len();
                    order.push(succ.clone());
                    seen.insert(k, j);
                    frontier.push(j);
                    j
                }
            };
            edges.push((i, j));
        }
    }

    if let Some(path) = svg {
        return render_discrete_svg(path, claim, axis, &order, &edges);
    }
    let _ = text;
    render_discrete_text(claim, axis, &order, &edges)
}

fn render_discrete_text(claim: &str, axis: &str, order: &[Value], edges: &[(usize, usize)]) -> ExitCode {
    let mut succ: HashMap<usize, usize> = HashMap::new();
    for &(i, j) in edges { succ.insert(i, j); }
    println!("phase portrait — {claim}   axis: {axis}   ({} reachable states)", order.len());
    let mut parts: Vec<String> = Vec::new();
    let mut visited = vec![false; order.len()];
    let mut i = 0usize;
    while i < order.len() {
        let lbl = val_label(&order[i]);
        match succ.get(&i) {
            Some(&j) if j == i => { parts.push(format!("{lbl} ⟲")); break; }
            _ if visited[i]    => { parts.push(format!("↺{lbl}")); break; }
            Some(&j)           => { parts.push(lbl); visited[i] = true; i = j; }
            None               => { parts.push(lbl); break; }
        }
    }
    println!("  {}", parts.join(" → "));
    println!("  (⟲ = fixed point / absorbing state)");
    ExitCode::SUCCESS
}

/// SVG state line: reachable states as nodes left-to-right in discovery order,
/// transition arrows as arcs above, the absorbing fixed point highlighted.
fn render_discrete_svg(
    path: &str, claim: &str, axis: &str, order: &[Value], edges: &[(usize, usize)],
) -> ExitCode {
    let n = order.len().max(1);
    let w = (n as i64 * 120).max(660);
    let h = 300i64;
    let y = 175i64;
    let m = 80i64;
    let r = 22i64;
    let span = (w - 2 * m).max(1);
    let xpos = |i: usize| -> i64 { if n == 1 { w / 2 } else { m + (i as i64) * span / (n as i64 - 1) } };
    let mut succ: HashMap<usize, usize> = HashMap::new();
    for &(i, j) in edges { succ.insert(i, j); }

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" font-family=\"monospace\" font-size=\"13\">\n"));
    s.push_str(&format!("<rect width=\"{w}\" height=\"{h}\" fill=\"#0a0c14\"/>\n"));
    s.push_str("<defs><marker id=\"a\" markerWidth=\"9\" markerHeight=\"9\" refX=\"7\" refY=\"3\" \
                orient=\"auto\"><path d=\"M0,0 L8,3 L0,6 Z\" fill=\"#5aaaff\"/></marker></defs>\n");

    s.push_str("<g stroke=\"#5aaaff\" stroke-width=\"1.7\" fill=\"none\">\n");
    for &(i, j) in edges {
        let (xi, xj) = (xpos(i), xpos(j));
        if i == j {
            s.push_str(&format!(
                "<path d=\"M {} {} C {} {}, {} {}, {} {}\" marker-end=\"url(#a)\"/>\n",
                xi - 9, y - r + 4, xi - 36, y - r - 58, xi + 36, y - r - 58, xi + 9, y - r + 4));
        } else {
            let mid = (xi + xj) / 2;
            let lift = (40 + ((j as i64 - i as i64).abs() * 16)).min(120);
            let (sx, ex) = if xj > xi { (xi + r, xj - r) } else { (xi - r, xj + r) };
            s.push_str(&format!(
                "<path d=\"M {sx} {y} Q {mid} {} {ex} {y}\" marker-end=\"url(#a)\"/>\n", y - lift));
        }
    }
    s.push_str("</g>\n");

    for (i, v) in order.iter().enumerate() {
        let x = xpos(i);
        let fp = succ.get(&i) == Some(&i);
        let (fill, stroke) = if fp { ("#16281a", "#50e678") } else { ("#141a2c", "#5a6a90") };
        s.push_str(&format!(
            "<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"2\"/>\n"));
        s.push_str(&format!(
            "<text x=\"{x}\" y=\"{}\" fill=\"#cdd6f0\" text-anchor=\"middle\">{}</text>\n",
            y + r + 20, xml_escape(&val_label(v))));
    }
    s.push_str(&format!(
        "<text x=\"12\" y=\"24\" fill=\"#aab4d0\">{} — {} (difference-equation state line, \
         {n} reachable states; green = fixed point)</text>\n",
        xml_escape(claim), xml_escape(axis)));
    s.push_str("</svg>\n");

    if let Err(e) = std::fs::write(path, s) {
        eprintln!("phase-portrait: write {path}: {e}"); return ExitCode::from(1);
    }
    println!("wrote {path}  ({n} reachable states, {} transitions)", edges.len());
    ExitCode::SUCCESS
}

// ───────────────────────── mixed (numeric × discrete) state portraits ─────────────────────────
// A utility program's state is a record of mixed fields (enum mode, Int balance,
// Bool flag). Its portrait projects the forward-explored trajectory onto two
// chosen fields — each mapped to a coordinate (Int→value, enum→ordinal, bool→0/1)
// — and draws the transition arrows. Numeric axes keep their values; discrete
// axes get labelled tick marks.

/// Full binding map of the initial state (is_first_tick = true).
fn probe_init(rt: &EvidentRuntime, claim: &str) -> Option<HashMap<String, Value>> {
    let mut g: HashMap<String, Value> = HashMap::new();
    g.insert("is_first_tick".into(), Value::Bool(true));
    let r = rt.query_with_pins_and_given(claim, &[], &g).ok()?;
    if r.satisfied { Some(r.bindings) } else { None }
}

/// One transition step carrying the WHOLE state: pin every `_field`, read every
/// `field` back. (Pinning only the plotted axes would leave the rest free.)
fn step_full(rt: &EvidentRuntime, claim: &str, cur: &HashMap<String, Value>, fields: &[String])
    -> Option<HashMap<String, Value>> {
    let mut g: HashMap<String, Value> = HashMap::new();
    for (k, v) in cur { g.insert(format!("_{k}"), v.clone()); }
    g.insert("is_first_tick".into(), Value::Bool(false));
    let r = rt.query_with_pins_and_given(claim, &[], &g).ok()?;
    if !r.satisfied { return None; }
    let mut next = HashMap::new();
    for k in fields { next.insert(k.clone(), r.bindings.get(k)?.clone()); }
    Some(next)
}

fn state_key(s: &HashMap<String, Value>, fields: &[String]) -> String {
    fields.iter().map(|k| format!("{k}={}", s.get(k).map(val_label).unwrap_or_default()))
        .collect::<Vec<_>>().join(";")
}

/// Per-state coordinate for one axis, plus tick labels (empty if the axis is
/// purely numeric). Discrete values get ordinals by first appearance.
fn axis_coords(states: &[HashMap<String, Value>], axis: &str) -> (Vec<f64>, Vec<(f64, String)>, bool) {
    let discrete = states.iter()
        .any(|s| matches!(s.get(axis), Some(Value::Enum { .. } | Value::Bool(_))));
    if !discrete {
        let coords = states.iter()
            .map(|s| match s.get(axis) { Some(Value::Int(n)) => *n as f64, _ => 0.0 }).collect();
        return (coords, vec![], true);
    }
    let mut ord: HashMap<String, f64> = HashMap::new();
    let mut ticks: Vec<(f64, String)> = Vec::new();
    let mut coords = Vec::new();
    for s in states {
        let lbl = s.get(axis).map(val_label).unwrap_or_default();
        let c = *ord.entry(lbl.clone()).or_insert_with(|| {
            let o = ticks.len() as f64; ticks.push((o, lbl.clone())); o
        });
        coords.push(c);
    }
    (coords, ticks, false)
}

fn mixed_portrait(
    rt: &EvidentRuntime, claim: &str, axis_a: &str, axis_b: &str,
    init: &HashMap<String, Value>, max: usize, text: bool, svg: Option<&str>,
) -> ExitCode {
    // The carried state is every binding sharing the axis record prefix (`state.*`),
    // or the bare scalar if there's no dot.
    let prefix = axis_a.split('.').next().unwrap_or(axis_a);
    let dotted = format!("{prefix}.");
    let mut fields: Vec<String> = init.keys()
        .filter(|k| k.as_str() == prefix || k.starts_with(&dotted))
        .cloned().collect();
    fields.sort();
    if !fields.iter().any(|f| f == axis_b) && init.contains_key(axis_b) {
        fields.push(axis_b.to_string());
    }
    let cur0: HashMap<String, Value> = fields.iter()
        .filter_map(|k| init.get(k).map(|v| (k.clone(), v.clone()))).collect();

    let mut states = vec![cur0.clone()];
    let mut seen: HashMap<String, usize> = HashMap::new();
    seen.insert(state_key(&cur0, &fields), 0);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut cur = cur0;
    let cap = max.max(64);
    while states.len() <= cap {
        let Some(next) = step_full(rt, claim, &cur, &fields) else { break };
        let k = state_key(&next, &fields);
        if let Some(&j) = seen.get(&k) { edges.push((states.len() - 1, j)); break; }
        let j = states.len();
        edges.push((states.len() - 1, j));
        states.push(next.clone());
        seen.insert(k, j);
        cur = next;
    }

    let (xc, xticks, xnum) = axis_coords(&states, axis_a);
    let (yc, yticks, ynum) = axis_coords(&states, axis_b);
    let pts: Vec<(f64, f64)> = xc.iter().zip(yc.iter()).map(|(&x, &y)| (x, y)).collect();

    if let Some(p) = svg {
        return render_mixed_svg(p, claim, axis_a, axis_b, &pts, &edges, &xticks, &yticks, xnum, ynum);
    }
    let _ = text;
    render_mixed_text(claim, axis_a, axis_b, &states, &edges)
}

fn render_mixed_text(
    claim: &str, ax: &str, bx: &str, states: &[HashMap<String, Value>], edges: &[(usize, usize)],
) -> ExitCode {
    let mut succ: HashMap<usize, usize> = HashMap::new();
    for &(i, j) in edges { succ.insert(i, j); }
    println!("phase portrait — {claim}   axes: ({ax}, {bx})   ({} reachable states)", states.len());
    let label = |i: usize| {
        let la = states[i].get(ax).map(val_label).unwrap_or_default();
        let lb = states[i].get(bx).map(val_label).unwrap_or_default();
        format!("({la}, {lb})")
    };
    let mut parts: Vec<String> = Vec::new();
    let mut visited = vec![false; states.len()];
    let mut i = 0usize;
    loop {
        parts.push(label(i));
        visited[i] = true;
        match succ.get(&i) {
            Some(&j) if visited[j] => { parts.push(format!("↺ back to {}", label(j))); break; }
            Some(&j) => { i = j; }
            None => break,
        }
        if parts.len() > states.len() + 2 { break; }
    }
    println!("  {}", parts.join(" → "));
    ExitCode::SUCCESS
}

/// SVG: the forward-explored trajectory as nodes + transition arrows in
/// (axis_a × axis_b) coordinate space, with labelled ticks on discrete axes.
fn render_mixed_svg(
    path: &str, claim: &str, ax: &str, bx: &str, pts: &[(f64, f64)], edges: &[(usize, usize)],
    xticks: &[(f64, String)], yticks: &[(f64, String)], xnum: bool, ynum: bool,
) -> ExitCode {
    let (wf, hf) = (760.0f64, 460.0f64);
    let (lm, rm, tm, bm) = (120.0f64, 40.0f64, 44.0f64, 56.0f64);
    let pad = |lo: f64, hi: f64, disc_n: usize| -> (f64, f64) {
        if disc_n > 0 { (lo - 0.6, hi + 0.6) }
        else { let p = (hi - lo).max(1.0) * 0.12; (lo - p, hi + p) }
    };
    let (mut xlo, mut xhi, mut ylo, mut yhi) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(x, y) in pts { xlo = xlo.min(x); xhi = xhi.max(x); ylo = ylo.min(y); yhi = yhi.max(y); }
    if xlo > xhi { (xlo, xhi, ylo, yhi) = (0.0, 1.0, 0.0, 1.0); }
    let (xlo, xhi) = pad(xlo, xhi, xticks.len());
    let (ylo, yhi) = pad(ylo, yhi, yticks.len());
    let sx = |x: f64| lm + (x - xlo) / (xhi - xlo).max(1e-9) * (wf - lm - rm);
    let sy = |y: f64| (hf - bm) - (y - ylo) / (yhi - ylo).max(1e-9) * (hf - tm - bm);

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{wf:.0}\" height=\"{hf:.0}\" \
         viewBox=\"0 0 {wf:.0} {hf:.0}\" font-family=\"monospace\" font-size=\"12\">\n"));
    s.push_str(&format!("<rect width=\"{wf:.0}\" height=\"{hf:.0}\" fill=\"#0a0c14\"/>\n"));
    s.push_str("<defs><marker id=\"m\" markerWidth=\"9\" markerHeight=\"9\" refX=\"7\" refY=\"3\" \
                orient=\"auto\"><path d=\"M0,0 L8,3 L0,6 Z\" fill=\"#5aaaff\"/></marker></defs>\n");

    // Gridlines + tick labels for discrete axes; light numeric ticks otherwise.
    s.push_str("<g stroke=\"#1e2740\" stroke-width=\"1\">\n");
    for &(o, _) in yticks { let y = sy(o); s.push_str(&format!("<line x1=\"{lm:.0}\" y1=\"{y:.1}\" x2=\"{:.0}\" y2=\"{y:.1}\"/>\n", wf - rm)); }
    for &(o, _) in xticks { let x = sx(o); s.push_str(&format!("<line x1=\"{x:.1}\" y1=\"{tm:.0}\" x2=\"{x:.1}\" y2=\"{:.0}\"/>\n", hf - bm)); }
    s.push_str("</g>\n");
    s.push_str("<g fill=\"#8a94b4\">\n");
    for &(o, ref l) in yticks { let y = sy(o); s.push_str(&format!("<text x=\"{:.0}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>\n", lm - 10.0, y + 4.0, xml_escape(l))); }
    for &(o, ref l) in xticks { let x = sx(o); s.push_str(&format!("<text x=\"{x:.1}\" y=\"{:.0}\" text-anchor=\"middle\">{}</text>\n", hf - bm + 18.0, xml_escape(l))); }
    if xnum { for k in 0..=4 { let v = xlo + (xhi - xlo) * k as f64 / 4.0; let x = sx(v); s.push_str(&format!("<text x=\"{x:.1}\" y=\"{:.0}\" text-anchor=\"middle\">{v:.0}</text>\n", hf - bm + 18.0)); } }
    if ynum { for k in 0..=4 { let v = ylo + (yhi - ylo) * k as f64 / 4.0; let y = sy(v); s.push_str(&format!("<text x=\"{:.0}\" y=\"{:.1}\" text-anchor=\"end\">{v:.0}</text>\n", lm - 10.0, y + 4.0)); } }
    s.push_str("</g>\n");

    // Transition arrows: a slightly curved arc per edge, so A→B and B→A separate.
    let r = 9.0f64;
    s.push_str("<g stroke=\"#5aaaff\" stroke-width=\"1.7\" fill=\"none\">\n");
    for &(i, j) in edges {
        let (x1, y1) = (sx(pts[i].0), sy(pts[i].1));
        let (x2, y2) = (sx(pts[j].0), sy(pts[j].1));
        let (dx, dy) = (x2 - x1, y2 - y1);
        let mag = (dx * dx + dy * dy).sqrt();
        if mag < 1.0 { continue; }
        let (ux, uy) = (dx / mag, dy / mag);
        let (px, py) = (-uy, ux);
        let off = 16.0;
        let (mx, my) = ((x1 + x2) / 2.0 + px * off, (y1 + y2) / 2.0 + py * off);
        let (sx0, sy0) = (x1 + ux * r, y1 + uy * r);
        let (ex0, ey0) = (x2 - ux * r, y2 - uy * r);
        s.push_str(&format!("<path d=\"M {sx0:.1} {sy0:.1} Q {mx:.1} {my:.1} {ex0:.1} {ey0:.1}\" marker-end=\"url(#m)\"/>\n"));
    }
    s.push_str("</g>\n");

    // Nodes (distinct projected points).
    let mut drawn: Vec<(i64, i64)> = Vec::new();
    for &(x, y) in pts {
        let (cx, cy) = (sx(x).round() as i64, sy(y).round() as i64);
        if drawn.contains(&(cx, cy)) { continue; }
        drawn.push((cx, cy));
        s.push_str(&format!("<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r:.0}\" fill=\"#141a2c\" stroke=\"#7a86b0\" stroke-width=\"2\"/>\n"));
    }

    s.push_str(&format!(
        "<text x=\"12\" y=\"22\" fill=\"#aab4d0\">{} — ({}, {}) difference-equation portrait, \
         {} states</text>\n", xml_escape(claim), xml_escape(ax), xml_escape(bx), pts.len()));
    s.push_str(&format!("<text x=\"{:.0}\" y=\"{:.0}\" fill=\"#8a94b4\" text-anchor=\"middle\">{} →</text>\n", wf / 2.0, hf - 8.0, xml_escape(ax)));
    s.push_str("</svg>\n");

    if let Err(e) = std::fs::write(path, s) {
        eprintln!("phase-portrait: write {path}: {e}"); return ExitCode::from(1);
    }
    println!("wrote {path}  ({} states, {} transitions)", pts.len(), edges.len());
    ExitCode::SUCCESS
}

pub fn usage() {
    eprintln!("  evident phase-portrait <daemon.ev> --axes a,b   # numeric: vector field + trajectories");
    eprintln!("  evident phase-portrait <prog.ev>   --axes state # discrete: enum/bool state line");
    eprintln!("  evident phase-portrait <prog.ev>   --axes balance,mode  # mixed: numeric × discrete");
    eprintln!("                         [--seeds \"a,b;a,b\"] [--range alo,ahi,blo,bhi] [--grid G]");
    eprintln!("                         [--steps N] [--text] [--svg PATH]");
}
