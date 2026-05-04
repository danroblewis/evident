"""
SDL2 executor plugin for Evident.

Owns the SDL_Window and SDL_Renderer. Before each solve step, polls events
and injects input.* given values. After each solve, reads output.* bindings
and issues draw calls, then presents the frame.
"""

from __future__ import annotations

import ctypes
import time as _time
from typing import Any

try:
    import sdl2
    from sdl2 import SDL_Rect
    HAS_SDL = True
except ImportError:
    HAS_SDL = False

# Key symbol → SDLKey enum name
_KEY_MAP: dict[int, str] = {}

def _build_key_map():
    if not HAS_SDL:
        return
    _KEY_MAP.update({
        sdl2.SDLK_UP:     'Up',
        sdl2.SDLK_DOWN:   'Down',
        sdl2.SDLK_LEFT:   'Left',
        sdl2.SDLK_RIGHT:  'Right',
        sdl2.SDLK_SPACE:  'Space',
        sdl2.SDLK_RETURN: 'Enter',
        sdl2.SDLK_ESCAPE: 'Escape',
    })


class SDLPlugin:
    """
    Manages one SDL window and renderer for the Evident executor.

    Usage:
        plugin = SDLPlugin(width=800, height=600, title="My Game")
        plugin.init()
        try:
            while plugin.running:
                given = plugin.poll(input_var='input')
                result = rt.query('main', given)
                if result.satisfied:
                    plugin.render(result.bindings, output_var='output')
        finally:
            plugin.cleanup()
    """

    SDL_INPUT_SCHEMA  = 'SDLInput'
    SDL_OUTPUT_SCHEMA = 'SDLOutput'

    def __init__(self, width: int = 800, height: int = 600,
                 title: str = 'Evident'):
        if not HAS_SDL:
            raise RuntimeError(
                "pysdl2 is not installed. Run: pip install pysdl2 pysdl2-dll"
            )
        _build_key_map()
        self.width   = width
        self.height  = height
        self.title   = title
        self.window   = None
        self.renderer = None
        self.running  = True
        self._current_key  = 'NoKey'
        self._mouse_x      = 0
        self._mouse_y      = 0
        self._click        = False
        self._quit         = False
        self._last_time_ms = 0

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def init(self) -> None:
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

    def cleanup(self) -> None:
        if self.renderer:
            sdl2.SDL_DestroyRenderer(self.renderer)
        if self.window:
            sdl2.SDL_DestroyWindow(self.window)
        sdl2.SDL_Quit()

    # ── Input ─────────────────────────────────────────────────────────────────

    def poll(self, input_var: str = 'input') -> dict[str, Any]:
        """Poll SDL events and return given dict for the input variable."""
        self._current_key = 'NoKey'
        self._click = False

        event = sdl2.SDL_Event()
        while sdl2.SDL_PollEvent(ctypes.byref(event)):
            t = event.type
            if t == sdl2.SDL_QUIT:
                self._quit = True
                self.running = False
            elif t == sdl2.SDL_KEYDOWN:
                sym = event.key.keysym.sym
                self._current_key = _KEY_MAP.get(sym, 'NoKey')
                if sym == sdl2.SDLK_ESCAPE:
                    self.running = False
            elif t == sdl2.SDL_MOUSEMOTION:
                self._mouse_x = event.motion.x
                self._mouse_y = event.motion.y
            elif t == sdl2.SDL_MOUSEBUTTONDOWN:
                self._mouse_x = event.button.x
                self._mouse_y = event.button.y
                self._click = True

        now_ms = int(_time.monotonic() * 1000)
        dt = min(now_ms - self._last_time_ms, 100)  # cap at 100 ms
        self._last_time_ms = now_ms
        unix_ms = int(_time.time() * 1000)

        return {
            f'{input_var}.key':     self._current_key,
            f'{input_var}.mouse_x': self._mouse_x,
            f'{input_var}.mouse_y': self._mouse_y,
            f'{input_var}.click':   self._click,
            f'{input_var}.quit':    self._quit,
            f'{input_var}.time':    unix_ms,
            f'{input_var}.dt':      dt,
        }

    # ── Rendering ─────────────────────────────────────────────────────────────

    def render(self, bindings: dict[str, Any], output_var: str = 'output') -> None:
        """Read output.* bindings and render one frame."""
        r = self.renderer
        p = output_var + '.'

        def iget(key: str, default: int = 0) -> int:
            v = bindings.get(p + key, default)
            try:
                return int(v)
            except (TypeError, ValueError):
                return default

        # Clear with background colour
        sdl2.SDL_SetRenderDrawColor(r, iget('bg_r'), iget('bg_g'), iget('bg_b'), 255)
        sdl2.SDL_RenderClear(r)

        # Draw rect slots 0..7 in order (painter's algorithm)
        for i in range(8):
            s = f'rect{i}_'
            w = iget(f'{s}w')
            h = iget(f'{s}h')
            if w == 0 and h == 0:
                break  # first empty slot signals end of draw list
            x  = iget(f'{s}x')
            y  = iget(f'{s}y')
            cr = iget(f'{s}r', 255)
            cg = iget(f'{s}g', 255)
            cb = iget(f'{s}b', 255)
            rect = SDL_Rect(x, y, w, h)
            sdl2.SDL_SetRenderDrawColor(r, cr, cg, cb, 255)
            sdl2.SDL_RenderFillRect(r, rect)

        sdl2.SDL_RenderPresent(r)
