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

use std::collections::HashMap;
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
type Color
    r ∈ Nat
    g ∈ Nat
    b ∈ Nat

type SDLRect
    x     ∈ Int
    y     ∈ Int
    w     ∈ Nat
    h     ∈ Nat
    color ∈ Color

type SDLInput
    right_held ∈ Bool
    left_held  ∈ Bool
    up_held    ∈ Bool
    down_held  ∈ Bool
    mouse_x    ∈ Int
    mouse_y    ∈ Int
    click      ∈ Bool
    quit       ∈ Bool
    time       ∈ Int
    dt         ∈ Int

type SDLOutput
    bg    ∈ Color
    rects ∈ Seq(SDLRect)

type SDLWindow
    screen_x ∈ Int
    screen_y ∈ Int
    width    ∈ Int
    height   ∈ Int
    dx       ∈ Int
    dy       ∈ Int
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
        }
    }

    /// Stash the per-var type. The executor's matcher only knows
    /// "this plugin claims these vars" — we recover the per-var type
    /// from a side-channel in `initialize_with_types`.
    fn record_types(&mut self, types: HashMap<String, String>) {
        self.var_types = types;
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
        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|e| e.to_string())?;
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

    fn initialize(&mut self, matched_vars: Vec<String>) {
        // The executor only hands us var names — it doesn't tell us
        // what type each one was declared as. We require the caller to
        // populate `var_types` before activation via
        // `initialize_with_types` (see `create_sdl_plugin`). If they
        // didn't, fall back to assuming any unmatched var is SDLInput
        // (a no-op for safety; the dispatch in before_step will skip
        // it if the type doesn't match anything we know).
        for v in &matched_vars {
            self.var_types.entry(v.clone()).or_insert_with(|| "SDLInput".to_string());
        }
        // Open the window now that we know we're active.
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
                    out.insert(format!("{var}.mouse_x"), Value::Int(self.mouse_x as i64));
                    out.insert(format!("{var}.mouse_y"), Value::Int(self.mouse_y as i64));
                    out.insert(format!("{var}.click"), Value::Bool(self.click));
                    out.insert(format!("{var}.quit"), Value::Bool(self.quit));
                    out.insert(format!("{var}.time"), Value::Int(unix_ms));
                    out.insert(format!("{var}.dt"), Value::Int(dt_ms));
                }
                "SDLWindow" => {
                    out.insert(format!("{var}.screen_x"), Value::Int(screen_x as i64));
                    out.insert(format!("{var}.screen_y"), Value::Int(screen_y as i64));
                    out.insert(format!("{var}.width"), Value::Int(self.width as i64));
                    out.insert(format!("{var}.height"), Value::Int(self.height as i64));
                    out.insert(format!("{var}.dx"), Value::Int(wdx as i64));
                    out.insert(format!("{var}.dy"), Value::Int(wdy as i64));
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

            // Rects — Seq(SDLRect) → SeqComposite.
            if let Some(Value::SeqComposite(rects)) = bindings.get(&format!("{var}.rects")) {
                for rect in rects {
                    let w = composite_int(rect, "w").unwrap_or(0);
                    let h = composite_int(rect, "h").unwrap_or(0);
                    if w == 0 && h == 0 {
                        continue;
                    }
                    let x = composite_int(rect, "x").unwrap_or(0);
                    let y = composite_int(rect, "y").unwrap_or(0);

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

            canvas.present();
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

/// Read an Int field from a Composite map (used for rect.x/y/w/h).
fn composite_int(map: &HashMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key)? {
        Value::Int(n) => Some(*n),
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
    var_types: HashMap<String, String>,
) -> Box<dyn Plugin> {
    let mut plugin = SDLPlugin::new(width, height, title);
    plugin.record_types(var_types);
    Box::new(plugin)
}
