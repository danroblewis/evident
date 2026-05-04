"""Built-in I/O plugins for the Evident executor."""

from __future__ import annotations

from ..plugin import Plugin
from .streams import StdinPlugin, StdoutPlugin
from .batch   import BatchInputPlugin, BatchOutputPlugin


def default_plugins(*, sdl_width: int = 800, sdl_height: int = 600,
                    sdl_title: str = 'Evident',
                    http_host: str = '127.0.0.1', http_port: int = 8080,
                    ) -> list[Plugin]:
    """
    Return one fresh instance of every built-in plugin.

    The executor's `run()` calls `initialize()` on each, which keeps only
    those whose `handles_types` match a variable declared in `main`. Plugins
    that don't match any variable are silently dropped.
    """
    from .sockets import HTTPServerPlugin
    plugins: list[Plugin] = [
        StdinPlugin(),
        StdoutPlugin(),
        BatchInputPlugin(),
        BatchOutputPlugin(),
        HTTPServerPlugin(host=http_host, port=http_port),
    ]
    # SDL is optional — only include if pysdl2 is importable
    try:
        from .sdl import SDLPlugin, HAS_SDL
        if HAS_SDL:
            plugins.append(SDLPlugin(width=sdl_width, height=sdl_height, title=sdl_title))
    except ImportError:
        pass
    return plugins


__all__ = [
    'Plugin', 'StdinPlugin', 'StdoutPlugin',
    'BatchInputPlugin', 'BatchOutputPlugin', 'default_plugins',
]
