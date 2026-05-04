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
        self.width   = width
        self.height  = height
        self.title   = title
        self.window   = None
        self.renderer = None
        self.running  = True
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
        self._click = False

        # Drain the event queue (handles quit, escape, mouse).
        event = sdl2.SDL_Event()
        while sdl2.SDL_PollEvent(ctypes.byref(event)):
            t = event.type
            if t == sdl2.SDL_QUIT:
                self._quit = True
                self.running = False
            elif t == sdl2.SDL_KEYDOWN:
                sym = event.key.keysym.sym
                if sym == sdl2.SDLK_ESCAPE:
                    self.running = False
            elif t == sdl2.SDL_MOUSEMOTION:
                self._mouse_x = event.motion.x
                self._mouse_y = event.motion.y
            elif t == sdl2.SDL_MOUSEBUTTONDOWN:
                self._mouse_x = event.button.x
                self._mouse_y = event.button.y
                self._click = True

        # Read all four directional keys independently — allows diagonal movement.
        keys = sdl2.SDL_GetKeyboardState(None)
        right = bool(keys[sdl2.SDL_SCANCODE_RIGHT])
        left  = bool(keys[sdl2.SDL_SCANCODE_LEFT])
        down  = bool(keys[sdl2.SDL_SCANCODE_DOWN])
        up    = bool(keys[sdl2.SDL_SCANCODE_UP])

        now_ms = int(_time.monotonic() * 1000)
        dt = min(now_ms - self._last_time_ms, 100)  # cap at 100 ms
        self._last_time_ms = now_ms
        unix_ms = int(_time.time() * 1000)

        return {
            f'{input_var}.right_held': right,
            f'{input_var}.left_held':  left,
            f'{input_var}.up_held':    up,
            f'{input_var}.down_held':  down,
            f'{input_var}.mouse_x':    self._mouse_x,
            f'{input_var}.mouse_y':    self._mouse_y,
            f'{input_var}.click':      self._click,
            f'{input_var}.quit':       self._quit,
            f'{input_var}.time':       unix_ms,
            f'{input_var}.dt':         dt,
        }

    # ── Rendering ─────────────────────────────────────────────────────────────

    def render(self, bindings: dict[str, Any], output_var: str = 'output') -> None:
        """Read output.* bindings and render one frame.

        Expects:
          output.bg.r / .g / .b      — clear colour
          output.rects               — list of dicts:
              {x, y, w, h, color: {r, g, b}}
        """
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

        # Render the rect sequence (painter's algorithm — list order = z-order)
        rects = bindings.get(p + 'rects', []) or []
        for rect in rects:
            if not isinstance(rect, dict):
                continue
            w = _int(rect.get('w'))
            h = _int(rect.get('h'))
            if w == 0 and h == 0:
                continue   # invisible — skip
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
