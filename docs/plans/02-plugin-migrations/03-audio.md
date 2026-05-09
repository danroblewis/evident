# Phase 2.3: Audio plugin → stdlib/audio/

## Goal

Replace `runtime-rust/src/plugins/audio.rs` (228 lines) with
`stdlib/audio/` — Evident wrappers around SDL_audio (or PortAudio
later if cleaner).

## Prereqs

- Phase 2.2 (SDL library exists, providing the SDL initialization
  pattern this audio plugin will piggyback on)

## What to build

- `stdlib/audio/queue.ev` — wraps SDL_OpenAudioDevice +
  SDL_QueueAudio.
- Migrate `programs/sdl_demo/synth.ev` and any audio-using demos.

## Files touched

- `runtime-rust/src/plugins/audio.rs` — delete
- `stdlib/audio/*.ev` (new)
- Demo files migrated

## Acceptance

- [ ] synth.ev still produces sound
- [ ] LOC: -228 Rust, +~120 Evident

## Notes

SDL audio uses a callback (audio device pulls samples). Evident's
effect model is request/response, not callback-driven. Workaround:
queue API (push samples) instead of callback API. SDL supports this
via SDL_QueueAudio.

If precise low-latency audio is needed later, the plugin model
might come back for audio specifically. v1: queue is fine for
casual sound.
