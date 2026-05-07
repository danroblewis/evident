//! SDL2 graphical I/O plugin.
//!
//! Owns the SDL_Window and SDL_Renderer (a `WindowCanvas` in the safe
//! `sdl2` crate). On each step:
//!   - `before_step`: poll SDL events to update mouse position, click,
//!     quit; read continuous keyboard state for arrow keys; compute
//!     dt + unix-time. Contribute these as `input.* / window.*` givens.
//!   - `after_step`: read `output.bg` (a flat Color expanded to
//!     `output.bg.r/g/b`) and `output.rects` (a `Seq(SDLRect)` →
//!     `Value::SeqComposite`) from the bindings, clear with the bg
//!     color, fill each rect with its color, present.
//!
//! Halts (returns `false` from `after_step`) when the window is closed
//! or Escape is pressed.
//!
//! Mirrors the Python `runtime/src/plugins/sdl.py`.

use std::collections::{HashMap, VecDeque};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::pixels::Color as SdlColor;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use sdl2::{EventPump, Sdl};

use crate::executor::Plugin;
use crate::translate::Value;

/// Type names this plugin owns.
pub const SDL_TYPES: &[&str] = &["SDLInput", "SDLOutput", "SDLWindow"];

// ── FPS overlay ──────────────────────────────────────────────────────────────
// Drawn in the top-right corner when EVIDENT_SDL_FPS=1. A line graph of
// recent fps + a numeric readout, all rendered with fill_rect (no font lib).

/// Number of frame samples held for the graph.
const GRAPH_SAMPLES: usize = 240;
/// Width of the graph in window pixels (1 px per sample, padded at right).
const GRAPH_W: i32 = GRAPH_SAMPLES as i32;
/// Height of the graph in window pixels.
const GRAPH_H: i32 = 40;
/// Pixel margin from the top-right corner.
const HUD_MARGIN: i32 = 8;
/// Y-axis ceiling for the graph (any sample above this clamps to top).
/// 250 covers the 240 Hz vsync case with a little headroom.
const FPS_CEIL: f64 = 250.0;
/// Reference line drawn at this fps so 60-fps and 120-fps targets are
/// visible at a glance.
const FPS_REFS: &[f64] = &[60.0, 120.0];

/// 3×5 bitmap font, just digits 0-9 and the letter "FPS" / spaces.
/// Encoded row-major MSB-first: bit n of byte k is set if pixel
/// (k=row, n=col-from-left) should be drawn (col 0 = leftmost).
/// Indexed by ASCII byte — only the entries we use are populated.
fn glyph_3x5(c: u8) -> Option<[u8; 5]> {
    Some(match c {
        b'0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        b'1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        b'2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        b'3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        b'4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        b'5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        b'6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        b'7' => [0b111, 0b001, 0b010, 0b100, 0b100],
        b'8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        b'9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        b' ' => [0b000, 0b000, 0b000, 0b000, 0b000],
        b'F' => [0b111, 0b100, 0b111, 0b100, 0b100],
        b'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        b'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        _    => return None,
    })
}

/// Draw a string at (x, y) using the 3×5 bitmap font, scaled by `scale`
/// (each "pixel" is a `scale × scale` filled rect). Returns the
/// advance-x at the end so callers can chain.
fn draw_text(canvas: &mut WindowCanvas, x: i32, y: i32, scale: i32,
             color: SdlColor, text: &str) -> i32 {
    canvas.set_draw_color(color);
    let mut cx = x;
    for ch in text.bytes() {
        if let Some(rows) = glyph_3x5(ch) {
            for (r, row) in rows.iter().enumerate() {
                for c in 0..3 {
                    if (row >> (2 - c)) & 1 == 1 {
                        let _ = canvas.fill_rect(Rect::new(
                            cx + (c as i32) * scale,
                            y + (r as i32) * scale,
                            scale as u32,
                            scale as u32));
                    }
                }
            }
            cx += 4 * scale; // 3 px + 1 px gap
        }
    }
    cx
}

/// Render the FPS overlay (graph + numeric readout) in the top-right
/// corner of the window. `samples` is a ring buffer of recent fps; the
/// rightmost (newest) sample is drawn at the right edge.
fn draw_fps_overlay(canvas: &mut WindowCanvas, win_w: u32, samples: &VecDeque<f64>) {
    if samples.is_empty() { return; }

    let panel_x = (win_w as i32) - GRAPH_W - HUD_MARGIN - 4;
    let panel_y = HUD_MARGIN;
    let panel_w = GRAPH_W + 4;
    let panel_h = GRAPH_H + 22; // graph + space for readout below

    // Translucent-ish dark panel. SDL's fill_rect doesn't blend by
    // default; just use a flat dark color.
    canvas.set_draw_color(SdlColor::RGB(0, 0, 0));
    let _ = canvas.fill_rect(Rect::new(panel_x, panel_y, panel_w as u32, panel_h as u32));

    // Reference lines (60, 120 fps). Draw before bars so bars overdraw.
    for &fps in FPS_REFS {
        let frac = (fps / FPS_CEIL).clamp(0.0, 1.0);
        let line_y = panel_y + 2 + GRAPH_H - (frac * GRAPH_H as f64) as i32;
        canvas.set_draw_color(SdlColor::RGB(50, 50, 60));
        let _ = canvas.fill_rect(Rect::new(
            panel_x + 2, line_y, GRAPH_W as u32, 1));
    }

    // Bars: one 1px-wide rect per sample. Newest (last) at the right.
    let n = samples.len();
    for (i, &fps) in samples.iter().enumerate() {
        let frac = (fps / FPS_CEIL).clamp(0.0, 1.0);
        let h = (frac * GRAPH_H as f64) as i32;
        let x = panel_x + 2 + (GRAPH_W - n as i32) + (i as i32);
        let y = panel_y + 2 + GRAPH_H - h;
        // Color: red < 30, yellow < 60, green ≥ 60.
        let color = if fps < 30.0      { SdlColor::RGB(220, 70, 70) }
                    else if fps < 60.0 { SdlColor::RGB(220, 180, 60) }
                    else               { SdlColor::RGB(80, 200, 120) };
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(Rect::new(x, y, 1, h.max(1) as u32));
    }

    // Numeric readout: average of the last min(30, n) samples.
    let tail_n = samples.len().min(30);
    let avg: f64 = samples.iter().rev().take(tail_n).sum::<f64>() / tail_n as f64;
    let label = format!("{:>3} FPS", avg.round() as i64);
    draw_text(canvas, panel_x + 4, panel_y + GRAPH_H + 6, 2,
              SdlColor::RGB(220, 220, 220), &label);
}

/// Embedded SDL stdlib: flat type definitions for the SDL primitives so
/// user programs can declare `∈ SDLInput` / `∈ SDLOutput` / `∈ SDLWindow`
/// without an `import` directive. Mirrors the embedded io stdlib in
/// `executor.rs` — same flat shape (no `..` passthrough) because the
/// Rust runtime's sub-schema expansion doesn't recurse through
/// passthroughs.
///
/// Field shapes match exactly what `SDLPlugin::before_step` produces
/// and what `after_step` reads, so every key resolves to a declared var.
pub const STDLIB_SDL_EV: &str = "
type IVec2(x, y ∈ Int)

type Color(r, g, b ∈ Nat)

type SDLRect
    pos   ∈ IVec2
    size  ∈ IVec2
    color ∈ Color

type SDLInput
    right_held ∈ Bool
    left_held  ∈ Bool
    up_held    ∈ Bool
    down_held  ∈ Bool
    mouse      ∈ IVec2
    click      ∈ Bool
    quit       ∈ Bool
    time       ∈ Int
    dt         ∈ Int

type SDLOutput
    bg    ∈ Color
    rects ∈ Seq(SDLRect)

type SDLWindow
    screen ∈ IVec2
    size   ∈ IVec2
    drag   ∈ IVec2

type SDLShaderOutput
    shader_name ∈ String
";

/// SDL plugin. Holds the window + canvas + event pump for the lifetime
/// of the run. `Sdl` and `VideoSubsystem` must outlive the canvas, so we
/// keep them around as fields too.
pub struct SDLPlugin {
    width: u32,
    height: u32,
    title: String,

    // Initialized in `initialize`. We allow them to be Option so the
    // matched-vars pre-check doesn't need to touch SDL at all (saves
    // creating a window for programs that don't actually use it).
    sdl: Option<Sdl>,
    canvas: Option<WindowCanvas>,
    event_pump: Option<EventPump>,

    // Mapping from declared var name to declared type name (one of the
    // SDL_TYPES). Used by before_step / after_step to dispatch.
    var_types: HashMap<String, String>,

    // Per-step input state.
    mouse_x: i32,
    mouse_y: i32,
    click: bool,
    quit: bool,
    running: bool,

    // dt tracking. `Instant::now()` for monotonic dt; `SystemTime` for
    // unix-ms `time` field.
    last_instant: Instant,

    // Window position tracking — first frame has dx=dy=0 by convention.
    last_screen_xy: Option<(i32, i32)>,

    // FPS instrumentation. Counts after_step invocations; prints to
    // stderr when EVIDENT_SDL_FPS=1 once per second.
    fps_enabled: bool,
    fps_frames: u64,
    fps_last_report: Instant,

    // Per-frame fps samples for the in-window graph overlay. One entry
    // per rendered frame, capped at GRAPH_SAMPLES (oldest popped first).
    // Value is instantaneous fps (1.0 / dt). Drawn in after_step when
    // `fps_enabled` is true.
    fps_samples: VecDeque<f64>,
    fps_last_frame: Instant,
}

impl SDLPlugin {
    pub fn new(width: u32, height: u32, title: impl Into<String>) -> Self {
        SDLPlugin {
            width,
            height,
            title: title.into(),
            sdl: None,
            canvas: None,
            event_pump: None,
            var_types: HashMap::new(),
            mouse_x: 0,
            mouse_y: 0,
            click: false,
            quit: false,
            running: true,
            last_instant: Instant::now(),
            last_screen_xy: None,
            fps_enabled: std::env::var("EVIDENT_SDL_FPS").as_deref() == Ok("1"),
            fps_frames: 0,
            fps_last_report: Instant::now(),
            fps_samples: VecDeque::with_capacity(GRAPH_SAMPLES),
            fps_last_frame: Instant::now(),
        }
    }

    /// Initialize SDL itself and open a window + renderer. Idempotent
    /// (no-op if already opened).
    fn open_window(&mut self) -> Result<(), String> {
        if self.canvas.is_some() {
            return Ok(());
        }
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let window = video
            .window(&self.title, self.width, self.height)
            .position_centered()
            .build()
            .map_err(|e| e.to_string())?;
        // EVIDENT_SDL_NO_VSYNC=1 disables vsync. Useful when the display
        // refresh rate is throttling the loop below the solver's
        // achievable rate (e.g. headless / VNC / when you want to know
        // the maximum frame rate the executor can produce).
        let no_vsync = std::env::var("EVIDENT_SDL_NO_VSYNC").as_deref() == Ok("1");
        let mut cb = window.into_canvas().accelerated();
        if !no_vsync {
            cb = cb.present_vsync();
        }
        let canvas = cb.build().map_err(|e| e.to_string())?;
        if self.fps_enabled {
            eprintln!("[SDL] renderer={} vsync={}",
                      canvas.info().name, !no_vsync);
        }
        let event_pump = sdl.event_pump()?;
        self.sdl = Some(sdl);
        self.canvas = Some(canvas);
        self.event_pump = Some(event_pump);
        self.last_instant = Instant::now();
        Ok(())
    }
}

impl Plugin for SDLPlugin {
    fn handles_types(&self) -> &'static [&'static str] {
        SDL_TYPES
    }

    fn initialize(&mut self, matched_vars: Vec<(String, String)>) {
        // Replace var_types entirely (rather than merge) so re-init
        // after a program swap reflects the NEW program's declared
        // SDL vars — old entries from a prior program are dropped.
        // open_window() is idempotent so the window survives swaps.
        self.var_types.clear();
        for (name, ty) in matched_vars {
            self.var_types.insert(name, ty);
        }
        if let Err(e) = self.open_window() {
            eprintln!("SDL init failed: {e}");
            self.running = false;
        }
    }

    fn before_step(&mut self) -> Option<HashMap<String, Value>> {
        if !self.running {
            return None;
        }

        // Reset per-frame click flag (mouse_x/y persist between frames).
        self.click = false;

        // Drain events. If init failed, event_pump is None — bail.
        let pump = self.event_pump.as_mut()?;
        for event in pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    self.quit = true;
                    self.running = false;
                }
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    self.running = false;
                }
                Event::MouseMotion { x, y, .. } => {
                    self.mouse_x = x;
                    self.mouse_y = y;
                }
                Event::MouseButtonDown { x, y, .. } => {
                    self.mouse_x = x;
                    self.mouse_y = y;
                    self.click = true;
                }
                _ => {}
            }
        }

        // Continuous keyboard state — supports diagonal movement.
        let kb = pump.keyboard_state();
        let right = kb.is_scancode_pressed(Scancode::Right);
        let left = kb.is_scancode_pressed(Scancode::Left);
        let up = kb.is_scancode_pressed(Scancode::Up);
        let down = kb.is_scancode_pressed(Scancode::Down);

        // dt in ms (capped at 100 to avoid huge jumps after pause/resume).
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_instant).as_millis() as i64;
        let dt_ms = dt_ms.min(100);
        self.last_instant = now;

        // Unix time in ms.
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // Window position + delta.
        let canvas = self.canvas.as_ref()?;
        let (screen_x, screen_y) = canvas.window().position();
        let (wdx, wdy) = match self.last_screen_xy {
            Some((px, py)) => (screen_x - px, screen_y - py),
            None => (0, 0),
        };
        self.last_screen_xy = Some((screen_x, screen_y));

        // Build givens for every matched var.
        let mut out: HashMap<String, Value> = HashMap::new();
        for (var, t) in &self.var_types {
            match t.as_str() {
                "SDLInput" => {
                    out.insert(format!("{var}.right_held"), Value::Bool(right));
                    out.insert(format!("{var}.left_held"), Value::Bool(left));
                    out.insert(format!("{var}.up_held"), Value::Bool(up));
                    out.insert(format!("{var}.down_held"), Value::Bool(down));
                    // mouse is now an IVec2 sub-field; the runtime
                    // expands `input.mouse ∈ IVec2` to flat env keys
                    // `input.mouse.x` / `input.mouse.y`, so plugin
                    // bindings target those dotted keys directly.
                    out.insert(format!("{var}.mouse.x"), Value::Int(self.mouse_x as i64));
                    out.insert(format!("{var}.mouse.y"), Value::Int(self.mouse_y as i64));
                    out.insert(format!("{var}.click"), Value::Bool(self.click));
                    out.insert(format!("{var}.quit"), Value::Bool(self.quit));
                    out.insert(format!("{var}.time"), Value::Int(unix_ms));
                    out.insert(format!("{var}.dt"), Value::Int(dt_ms));
                }
                "SDLWindow" => {
                    // SDLWindow's three IVec2 sub-fields: window.screen
                    // (top-left in screen coords), window.size (the
                    // window's pixel dimensions), window.drag (this
                    // step's screen-pos delta — non-zero on a drag).
                    out.insert(format!("{var}.screen.x"), Value::Int(screen_x as i64));
                    out.insert(format!("{var}.screen.y"), Value::Int(screen_y as i64));
                    out.insert(format!("{var}.size.x"), Value::Int(self.width as i64));
                    out.insert(format!("{var}.size.y"), Value::Int(self.height as i64));
                    out.insert(format!("{var}.drag.x"), Value::Int(wdx as i64));
                    out.insert(format!("{var}.drag.y"), Value::Int(wdy as i64));
                }
                _ => {} // SDLOutput is read-only on the input side
            }
        }
        Some(out)
    }

    fn after_step(&mut self, bindings: &HashMap<String, Value>) -> bool {
        // Render every SDLOutput var.
        let Some(canvas) = self.canvas.as_mut() else {
            return self.running;
        };

        // For each SDLOutput var, clear with bg + draw rects.
        let outputs: Vec<String> = self
            .var_types
            .iter()
            .filter(|(_, t)| t.as_str() == "SDLOutput")
            .map(|(v, _)| v.clone())
            .collect();

        for var in &outputs {
            // Background — flat sub-schema expansion: output.bg.r/g/b.
            let r = read_int_field(bindings, &format!("{var}.bg.r")).unwrap_or(0);
            let g = read_int_field(bindings, &format!("{var}.bg.g")).unwrap_or(0);
            let b = read_int_field(bindings, &format!("{var}.bg.b")).unwrap_or(0);
            canvas.set_draw_color(SdlColor::RGB(r as u8, g as u8, b as u8));
            canvas.clear();

            // Rects — Seq(SDLRect) → SeqComposite. SDLRect's `pos` and
            // `size` are now `IVec2` sub-records, surfacing as
            // `Value::Composite({x, y})` inside each rect map. Drill
            // through `composite_nested_int` to read x/y leaves.
            if let Some(Value::SeqComposite(rects)) = bindings.get(&format!("{var}.rects")) {
                for rect in rects {
                    let w = composite_nested_int(rect, "size", "x").unwrap_or(0);
                    let h = composite_nested_int(rect, "size", "y").unwrap_or(0);
                    if w == 0 && h == 0 {
                        continue;
                    }
                    let x = composite_nested_int(rect, "pos", "x").unwrap_or(0);
                    let y = composite_nested_int(rect, "pos", "y").unwrap_or(0);

                    // Per-rect color. The current Rust runtime's
                    // `extract_seq_composite` only handles flat
                    // Int/Nat/Pos/Bool/String fields — if a parallel
                    // agent's nested-composite work isn't merged, the
                    // `color` field will be absent and we fall back
                    // to white. Defensive default also helps when the
                    // user simply didn't constrain color (Z3 may pick
                    // values out of range).
                    let (cr, cg, cb) = match rect.get("color") {
                        Some(Value::Composite(cmap)) => (
                            composite_int(cmap, "r").unwrap_or(255).clamp(0, 255) as u8,
                            composite_int(cmap, "g").unwrap_or(255).clamp(0, 255) as u8,
                            composite_int(cmap, "b").unwrap_or(255).clamp(0, 255) as u8,
                        ),
                        _ => (255, 255, 255),
                    };

                    canvas.set_draw_color(SdlColor::RGB(cr, cg, cb));
                    let r_obj = Rect::new(x as i32, y as i32, w.max(0) as u32, h.max(0) as u32);
                    let _ = canvas.fill_rect(r_obj);
                }
            }

            // FPS overlay — sample one fps value from the inter-frame
            // delta, then draw the graph + readout on top of everything
            // else. Drawn before present() so it shows up this frame.
            if self.fps_enabled {
                let now = Instant::now();
                let dt = now.duration_since(self.fps_last_frame).as_secs_f64();
                self.fps_last_frame = now;
                if dt > 0.0 && dt < 1.0 {
                    let inst_fps = 1.0 / dt;
                    if self.fps_samples.len() == GRAPH_SAMPLES {
                        self.fps_samples.pop_front();
                    }
                    self.fps_samples.push_back(inst_fps);
                }
                draw_fps_overlay(canvas, self.width, &self.fps_samples);
            }

            canvas.present();
        }

        if self.fps_enabled {
            self.fps_frames += 1;
            let dt = self.fps_last_report.elapsed();
            if dt.as_secs() >= 1 {
                let fps = self.fps_frames as f64 / dt.as_secs_f64();
                eprintln!("fps: {:.1} ({} frames in {:.2}s)",
                          fps, self.fps_frames, dt.as_secs_f64());
                self.fps_frames = 0;
                self.fps_last_report = Instant::now();
            }
        }

        self.running
    }
}

/// Read an Int field (key) from a flat bindings map, returning the i64.
fn read_int_field(bindings: &HashMap<String, Value>, key: &str) -> Option<i64> {
    match bindings.get(key)? {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

/// Read an Int field from a Composite map (used for rect.color.r/g/b).
fn composite_int(map: &HashMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key)? {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

/// Read `outer.inner` where `outer` is a Composite sub-field of `map`
/// and `inner` is an Int leaf inside it. Used for SDLRect's nested
/// `pos.x` / `pos.y` / `size.x` / `size.y` after the IVec2 refactor.
fn composite_nested_int(map: &HashMap<String, Value>, outer: &str, inner: &str) -> Option<i64> {
    match map.get(outer)? {
        Value::Composite(sub) => composite_int(sub, inner),
        _ => None,
    }
}

/// Build an SDL plugin with the per-variable type mapping pre-populated.
/// The executor's matcher only knows variable names; this constructor
/// is the caller-side hook that lets `cmd_execute` thread per-var type
/// info into the plugin before activation.
///
/// `var_types` maps declared variable name → declared type name (one of
/// `"SDLInput"`, `"SDLOutput"`, `"SDLWindow"`). Variables not in this
/// map will default to SDLInput in `initialize` — generally harmless,
/// but pre-populating gives the right dispatch.
pub fn create_sdl_plugin(
    width: u32,
    height: u32,
    title: impl Into<String>,
) -> Box<dyn Plugin> {
    Box::new(SDLPlugin::new(width, height, title))
}
