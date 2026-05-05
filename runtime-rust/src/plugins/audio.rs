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

/// One voice. Self-contained: holds its own phase + parameters,
/// produces one sample per call. The mixer below runs N of these in
/// parallel and sums the outputs.
#[derive(Clone)]
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

impl Synth {
    fn silent() -> Self {
        Synth { phase: 0.0, phase_inc: 0.0, volume: 0.0, playing: false, waveform: 0 }
    }

    fn next_sample(&mut self) -> f32 {
        if !self.playing || self.phase_inc <= 0.0 || self.volume <= 0.0 {
            return 0.0;
        }
        let s = self.volume * match self.waveform {
            0 => (self.phase * 2.0 * std::f32::consts::PI).sin(),
            _ => if self.phase < 0.5 { 1.0 } else { -1.0 },
        };
        self.phase = (self.phase + self.phase_inc).fract();
        s
    }
}

/// Audio callback that mixes N synths into a single mono stream. SDL
/// invokes `callback` on its own audio thread; the executor mutates
/// `synths[i]` via `device.lock()` between buffer fills.
///
/// Mixing strategy: sum all voices, then divide by sqrt(N) so peak
/// amplitude stays roughly bounded as voices are added (rather than
/// dividing by N which would silence individual voices in dense
/// mixes). With N=4 voices each capped at VOLUME_CAP=0.3, peak is
/// 4 * 0.3 / 2 = 0.6 — still under 1.0 so no clipping.
struct Mixer {
    synths: Vec<Synth>,
}

impl AudioCallback for Mixer {
    type Channel = f32;
    fn callback(&mut self, out: &mut [f32]) {
        let attenuation = 1.0 / (self.synths.len().max(1) as f32).sqrt();
        for sample in out.iter_mut() {
            let mut sum = 0.0_f32;
            for s in self.synths.iter_mut() {
                sum += s.next_sample();
            }
            *sample = sum * attenuation;
        }
    }
}

pub struct SDLAudioPlugin {
    /// All matched SDLAudio variable names, sorted alphabetically for
    /// stable index → var mapping. `synths[i]` (in the audio device's
    /// Mixer) corresponds to `var_names[i]`.
    var_names: Vec<String>,
    /// Kept alive so the audio subsystem stays initialized for the
    /// device's lifetime. Underscore-prefixed because we don't read it.
    _sdl: Option<Sdl>,
    device: Option<AudioDevice<Mixer>>,
}

impl SDLAudioPlugin {
    pub fn new() -> Self {
        SDLAudioPlugin { var_names: Vec::new(), _sdl: None, device: None }
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
        let n_voices = self.var_names.len().max(1);
        let device = audio.open_playback(None, &desired, |_spec| {
            Mixer { synths: vec![Synth::silent(); n_voices] }
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
        // Sort for stable order: the executor's matched_vars comes
        // from a HashMap iteration so the input order is undefined.
        // Sorted order means voice 0 always corresponds to the
        // alphabetically-first SDLAudio var, which matters for
        // diagnostics and tests that read the mixer state.
        let mut sorted = matched_vars;
        sorted.sort();
        self.var_names = sorted;
        if let Err(e) = self.open_device() {
            eprintln!("SDL audio init failed: {e}");
        }
    }

    fn before_step(&mut self) -> Option<HashMap<String, Value>> {
        // Audio is output-only — the program's bindings drive the
        // synths, no input back to the solver.
        Some(HashMap::new())
    }

    fn after_step(&mut self, bindings: &HashMap<String, Value>) -> bool {
        let Some(device) = self.device.as_mut() else { return true };
        let mut cb = device.lock();
        for (i, var) in self.var_names.iter().enumerate() {
            if i >= cb.synths.len() { break; }
            let synth = &mut cb.synths[i];
            let playing   = read_bool(bindings, &format!("{var}.playing")).unwrap_or(false);
            let frequency = read_int(bindings,  &format!("{var}.frequency")).unwrap_or(0);
            let volume    = read_int(bindings,  &format!("{var}.volume")).unwrap_or(0);
            let waveform  = read_int(bindings,  &format!("{var}.waveform")).unwrap_or(0);
            synth.playing  = playing;
            synth.phase_inc = (frequency.max(0) as f32) / (SAMPLE_RATE as f32);
            synth.volume   = (volume.clamp(0, 255) as f32) / 255.0 * VOLUME_CAP;
            synth.waveform = waveform.clamp(0, 255) as u8;
        }
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
