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
    let axes = axes.ok_or("--axes a,b is required (e.g. --axes state.pos,state.vel)")?;
    let (a, b) = axes.split_once(',').ok_or("--axes must be a,b")?;
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

pub fn usage() {
    eprintln!("  evident phase-portrait <daemon.ev> --axes a,b [--seeds \"a,b;a,b\"]");
    eprintln!("                         [--range alo,ahi,blo,bhi] [--grid G] [--steps N]");
    eprintln!("                         [--text] [--svg PATH]");
}
