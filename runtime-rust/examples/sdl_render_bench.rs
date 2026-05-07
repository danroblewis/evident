//! Micro-bench: open a window and run the same draw calls anchor_collect
//! does each frame (clear + ~6 fillrects + present) for 5 seconds.
//! Reports frames-per-second to stdout.
//!
//! Run with:
//!   cargo run --release --example sdl_render_bench
//!   VSYNC=0 cargo run --release --example sdl_render_bench
//!   SDL_VIDEODRIVER=x11 cargo run --release --example sdl_render_bench
//!
//! Vary VSYNC=0/1 and the renderer driver to identify which is responsible.

use sdl2::pixels::Color;
use sdl2::rect::Rect;
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let vsync = std::env::var("VSYNC").map(|v| v != "0").unwrap_or(true);
    let bench_secs: u64 = std::env::var("BENCH_SECS").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(5);

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    println!("video driver: {}", video.current_video_driver());
    let window = video.window("bench", 800, 600).position_centered()
        .build().map_err(|e| e.to_string())?;
    let mut canvas_builder = window.into_canvas().accelerated();
    if vsync { canvas_builder = canvas_builder.present_vsync(); }
    let mut canvas = canvas_builder.build().map_err(|e| e.to_string())?;
    let info = canvas.info();
    println!("renderer: {} (vsync={})", info.name, vsync);

    let mut event_pump = sdl.event_pump()?;
    let start = Instant::now();
    let deadline = start + Duration::from_secs(bench_secs);
    let mut frames: u64 = 0;
    let mut clear_t = Duration::ZERO;
    let mut draw_t = Duration::ZERO;
    let mut present_t = Duration::ZERO;

    while Instant::now() < deadline {
        for _ in event_pump.poll_iter() {}

        let t0 = Instant::now();
        canvas.set_draw_color(Color::RGB(20, 20, 60));
        canvas.clear();
        let t1 = Instant::now();

        // Player + 4 dots + a few extras = ~6 rects (matches anchor_collect)
        for i in 0..6u8 {
            canvas.set_draw_color(Color::RGB(80 + i * 20, 200, 180));
            let x = 50 + (i as i32) * 100;
            canvas.fill_rect(Rect::new(x, 200, 25, 25))?;
        }
        let t2 = Instant::now();

        canvas.present();
        let t3 = Instant::now();

        clear_t += t1 - t0;
        draw_t += t2 - t1;
        present_t += t3 - t2;
        frames += 1;
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!("frames={} elapsed={:.3}s fps={:.1}", frames, elapsed, frames as f64 / elapsed);
    println!("avg per-frame: clear={:.2}ms draw={:.2}ms present={:.2}ms",
             clear_t.as_secs_f64() * 1000.0 / frames as f64,
             draw_t.as_secs_f64()  * 1000.0 / frames as f64,
             present_t.as_secs_f64() * 1000.0 / frames as f64);
    Ok(())
}
