//! SDL2 audio plugin.
//!
//! Owns an SDL audio device + a tiny synthesis callback running on
//! SDL's audio thread. Each step:
//!   - `before_step`: contributes nothing (audio is output-only).
//!   - `after_step`: read `audio.{playing, frequency, volume,
//!     waveform}` from the bindings, lock the device, update the
//!     callback's parameters. The audio thread picks them up on its
//!     next buffer fill.
//!
//! The synthesis is intentionally minimal — square or sine wave at the
//! requested frequency, scaled by volume, gated by playing. No
//! envelope, no mixing, no samples. Enough to demonstrate event-driven
//! audio while keeping the plugin under 150 lines.
//!
//! Thread safety: the SDL `AudioDevice::lock()` returns a guard that
//! briefly pauses the audio thread while the main thread mutates the
//! callback's state. That's what keeps `playing`/`frequency`/etc.
//! updates atomic from the synth's perspective.

use std::collections::HashMap;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::Sdl;

use crate::executor::Plugin;
use crate::translate::Value;

pub const SDL_AUDIO_TYPES: &[&str] = &["SDLAudio"];

/// Embedded stdlib snippet — the `SDLAudio` type. Loaded by
/// `cmd_execute` alongside `STDLIB_SDL_EV` so user programs can
/// declare `∈ SDLAudio` without an `import`.
///
/// Field semantics:
///   - `playing`   : on/off. False = silence (the callback emits zeros).
///   - `frequency` : pitch in Hz. 0 also silences.
///   - `volume`    : 0..255. Internally mapped to 0.0..0.3 on the
///                   sample side (capped low to avoid blowing out
///                   ears or speakers — easy to bump if you need it
///                   louder).
///   - `waveform`  : 0 = sine, anything else = square. Trivial to
///                   add sawtooth / triangle later.
pub const STDLIB_SDL_AUDIO_EV: &str = "
type SDLAudio
    playing   ∈ Bool
    frequency ∈ Nat
    volume    ∈ Nat
    waveform  ∈ Nat
";

const SAMPLE_RATE: i32 = 44100;
const VOLUME_CAP: f32 = 0.3;

/// The synthesis callback. SDL invokes `callback` on its own audio
/// thread; the executor mutates this struct's fields via
/// `device.lock()`.
struct Synth {
    /// 0.0..1.0 cycle position. Advances by `phase_inc` per sample.
    phase: f32,
    /// frequency / sample_rate. 0 = silent.
    phase_inc: f32,
    /// 0.0..1.0 amplitude.
    volume: f32,
    /// Master gate.
    playing: bool,
    /// 0 = sine, anything else = square.
    waveform: u8,
}

impl AudioCallback for Synth {
    type Channel = f32;
    fn callback(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            if !self.playing || self.phase_inc <= 0.0 || self.volume <= 0.0 {
                *sample = 0.0;
                continue;
            }
            *sample = self.volume * match self.waveform {
                0 => (self.phase * 2.0 * std::f32::consts::PI).sin(),
                _ => if self.phase < 0.5 { 1.0 } else { -1.0 },
            };
            self.phase = (self.phase + self.phase_inc).fract();
        }
    }
}

pub struct SDLAudioPlugin {
    /// The matched variable name (e.g. "audio") so we know which
    /// bindings to read in `after_step`. Audio plugins assume one
    /// SDLAudio var per program; if multiple are declared we use the
    /// first matched one.
    var_name: Option<String>,
    /// Kept alive so the audio subsystem stays initialized for the
    /// device's lifetime. Underscore-prefixed because we don't read it.
    _sdl: Option<Sdl>,
    device: Option<AudioDevice<Synth>>,
}

impl SDLAudioPlugin {
    pub fn new() -> Self {
        SDLAudioPlugin { var_name: None, _sdl: None, device: None }
    }

    fn open_device(&mut self) -> Result<(), String> {
        if self.device.is_some() {
            return Ok(());
        }
        // Multiple sdl2::init() calls are reference-counted by the
        // sdl2 crate, so this is safe even when the SDL graphical
        // plugin is also active. Each plugin holds its own handle.
        let sdl = sdl2::init()?;
        let audio = sdl.audio()?;
        let desired = AudioSpecDesired {
            freq: Some(SAMPLE_RATE),
            channels: Some(1), // mono — stereo would need (l, r) per sample
            samples: Some(512),
        };
        let device = audio.open_playback(None, &desired, |_spec| {
            Synth {
                phase: 0.0,
                phase_inc: 0.0,
                volume: 0.0,
                playing: false,
                waveform: 0,
            }
        }).map_err(|e| e.to_string())?;
        device.resume(); // start the callback firing
        self._sdl = Some(sdl);
        self.device = Some(device);
        Ok(())
    }
}

impl Default for SDLAudioPlugin {
    fn default() -> Self { Self::new() }
}

impl Plugin for SDLAudioPlugin {
    fn handles_types(&self) -> &'static [&'static str] {
        SDL_AUDIO_TYPES
    }

    fn initialize(&mut self, matched_vars: Vec<String>) {
        self.var_name = matched_vars.into_iter().next();
        if let Err(e) = self.open_device() {
            eprintln!("SDL audio init failed: {e}");
        }
    }

    fn before_step(&mut self) -> Option<HashMap<String, Value>> {
        // Audio is output-only — the program's bindings drive the
        // synth, no input back to the solver.
        Some(HashMap::new())
    }

    fn after_step(&mut self, bindings: &HashMap<String, Value>) -> bool {
        let Some(var) = self.var_name.clone() else { return true };
        let Some(device) = self.device.as_mut() else { return true };
        let var = var.as_str();
        let playing   = read_bool(bindings, &format!("{var}.playing")).unwrap_or(false);
        let frequency = read_int(bindings,  &format!("{var}.frequency")).unwrap_or(0);
        let volume    = read_int(bindings,  &format!("{var}.volume")).unwrap_or(0);
        let waveform  = read_int(bindings,  &format!("{var}.waveform")).unwrap_or(0);
        let mut cb = device.lock();
        cb.playing  = playing;
        cb.phase_inc = (frequency.max(0) as f32) / (SAMPLE_RATE as f32);
        cb.volume   = (volume.clamp(0, 255) as f32) / 255.0 * VOLUME_CAP;
        cb.waveform = waveform.clamp(0, 255) as u8;
        // `cb` drops here, releasing the lock — audio thread resumes.
        true
    }
}

pub fn create_audio_plugin() -> Box<dyn Plugin> {
    Box::new(SDLAudioPlugin::new())
}

fn read_bool(bindings: &HashMap<String, Value>, key: &str) -> Option<bool> {
    match bindings.get(key)? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn read_int(bindings: &HashMap<String, Value>, key: &str) -> Option<i64> {
    match bindings.get(key)? {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}
