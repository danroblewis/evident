"""SDL2 graphical I/O plugin.

Owns the SDL_Window and SDL_Renderer. Polls events into `input.*` before each
solve; reads the `output.bg` colour and `output.rects` Seq(SDLRect) after
each solve to render the frame.

Window close, SDL_QUIT, or Escape signals halt via after_step → False.
"""

from __future__ import annotations

import ctypes
import time as _time
from typing import Any

from ..plugin import Plugin

try:
    import sdl2
    from sdl2 import SDL_Rect
    HAS_SDL = True
except ImportError:
    HAS_SDL = False


class SDLPlugin(Plugin):
    """SDL2 plugin: window + renderer + event loop."""

    handles_types = {'SDLInput', 'SDLOutput'}

    def __init__(self, width: int = 800, height: int = 600,
                 title: str = 'Evident'):
        super().__init__()
        self.width   = width
        self.height  = height
        self.title   = title
        self.window   = None
        self.renderer = None
        self._running = True
        self._mouse_x = 0
        self._mouse_y = 0
        self._click   = False
        self._quit    = False
        self._last_time_ms = 0

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def start(self) -> None:
        if not HAS_SDL:
            raise RuntimeError(
                "pysdl2 is not installed. Run: pip install pysdl2 pysdl2-dll"
            )
        self._last_time_ms = int(_time.monotonic() * 1000)
        sdl2.SDL_Init(sdl2.SDL_INIT_VIDEO)
        self.window = sdl2.SDL_CreateWindow(
            self.title.encode(),
            sdl2.SDL_WINDOWPOS_CENTERED,
            sdl2.SDL_WINDOWPOS_CENTERED,
            self.width, self.height,
            sdl2.SDL_WINDOW_SHOWN,
        )
        if not self.window:
            raise RuntimeError(f"SDL_CreateWindow failed: {sdl2.SDL_GetError()}")
        self.renderer = sdl2.SDL_CreateRenderer(
            self.window, -1,
            sdl2.SDL_RENDERER_ACCELERATED | sdl2.SDL_RENDERER_PRESENTVSYNC,
        )
        if not self.renderer:
            raise RuntimeError(f"SDL_CreateRenderer failed: {sdl2.SDL_GetError()}")

    def stop(self) -> None:
        if self.renderer:
            sdl2.SDL_DestroyRenderer(self.renderer)
            self.renderer = None
        if self.window:
            sdl2.SDL_DestroyWindow(self.window)
            self.window = None
        if HAS_SDL:
            sdl2.SDL_Quit()

    # ── Per-step ──────────────────────────────────────────────────────────────

    def before_step(self, _state) -> dict[str, Any] | None:
        """Drain SDL events, read keyboard state, return given values."""
        self._click = False

        event = sdl2.SDL_Event()
        while sdl2.SDL_PollEvent(ctypes.byref(event)):
            t = event.type
            if t == sdl2.SDL_QUIT:
                self._quit = True
                self._running = False
            elif t == sdl2.SDL_KEYDOWN:
                if event.key.keysym.sym == sdl2.SDLK_ESCAPE:
                    self._running = False
            elif t == sdl2.SDL_MOUSEMOTION:
                self._mouse_x = event.motion.x
                self._mouse_y = event.motion.y
            elif t == sdl2.SDL_MOUSEBUTTONDOWN:
                self._mouse_x = event.button.x
                self._mouse_y = event.button.y
                self._click = True

        # Continuous keyboard state (allows simultaneous keys → diagonal movement)
        keys = sdl2.SDL_GetKeyboardState(None)

        now_ms  = int(_time.monotonic() * 1000)
        dt      = min(now_ms - self._last_time_ms, 100)   # cap to avoid huge jumps
        self._last_time_ms = now_ms
        unix_ms = int(_time.time() * 1000)

        given: dict[str, Any] = {}
        for var, type_name in self.matched_vars.items():
            if type_name != 'SDLInput':
                continue
            given.update({
                f'{var}.right_held': bool(keys[sdl2.SDL_SCANCODE_RIGHT]),
                f'{var}.left_held':  bool(keys[sdl2.SDL_SCANCODE_LEFT]),
                f'{var}.up_held':    bool(keys[sdl2.SDL_SCANCODE_UP]),
                f'{var}.down_held':  bool(keys[sdl2.SDL_SCANCODE_DOWN]),
                f'{var}.mouse_x':    self._mouse_x,
                f'{var}.mouse_y':    self._mouse_y,
                f'{var}.click':      self._click,
                f'{var}.quit':       self._quit,
                f'{var}.time':       unix_ms,
                f'{var}.dt':         dt,
            })
        return given

    def after_step(self, bindings) -> bool:
        """Render one frame. Return False if window close/Esc was pressed."""
        for var, type_name in self.matched_vars.items():
            if type_name != 'SDLOutput':
                continue
            self._render_output(bindings, var)
        return self._running

    # ── Rendering ─────────────────────────────────────────────────────────────

    def _render_output(self, bindings: dict[str, Any], output_var: str) -> None:
        r = self.renderer
        p = output_var + '.'

        def _int(v, default: int = 0) -> int:
            try:
                return int(v)
            except (TypeError, ValueError):
                return default

        # Clear with background colour
        bg = bindings.get(p + 'bg', {}) or {}
        sdl2.SDL_SetRenderDrawColor(r,
            _int(bg.get('r')), _int(bg.get('g')), _int(bg.get('b')), 255)
        sdl2.SDL_RenderClear(r)

        # Painter's algorithm: list order = z-order
        rects = bindings.get(p + 'rects', []) or []
        for rect in rects:
            if not isinstance(rect, dict):
                continue
            w = _int(rect.get('w'))
            h = _int(rect.get('h'))
            if w == 0 and h == 0:
                continue   # invisible
            x = _int(rect.get('x'))
            y = _int(rect.get('y'))
            color = rect.get('color', {}) or {}
            cr = _int(color.get('r'), 255)
            cg = _int(color.get('g'), 255)
            cb = _int(color.get('b'), 255)
            rect_obj = SDL_Rect(x, y, w, h)
            sdl2.SDL_SetRenderDrawColor(r, cr, cg, cb, 255)
            sdl2.SDL_RenderFillRect(r, rect_obj)

        sdl2.SDL_RenderPresent(r)
