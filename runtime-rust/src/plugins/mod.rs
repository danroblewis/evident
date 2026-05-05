//! I/O plugins for the executor.
//!
//! The executor itself ships StdinPlugin and StdoutPlugin inline (they
//! depend only on `Read` / `Write` and need no extra system libraries).
//! Plugins that link against external libraries — currently just
//! `SDLPlugin` for graphical I/O — live in this submodule so the
//! dependency surface is contained.
//!
//! See `executor::Plugin` for the trait every plugin implements.

pub mod sdl;
