"""
I/O plugin protocol for the constraint automaton executor.

A plugin handles the side-effectful side of one or more Evident type names.
The executor inspects the `main` schema, asks every registered plugin which
declared variables it claims, and activates the plugins that have matches.
Plugins that don't match any variable in main never run.

Each step of the loop:
  1. Every active plugin's `before_step()` runs and contributes given values
     for the variables it owns. Returning None signals "no more input — halt".
  2. The runtime adds current state values and solves.
  3. Every active plugin's `after_step()` runs to perform side effects
     (write output, render frame). Returning False signals halt.
  4. State pairs (foo / foo_next) advance for the next step.

Termination is plugin-driven: plugins know when their input is exhausted
or when the user has asked to quit.
"""

from __future__ import annotations

from typing import Any


class Plugin:
    """Base class for I/O plugins.

    Subclasses set `handles_types` to the set of Evident type names they
    claim, and override the lifecycle methods they need.
    """

    handles_types: set[str] = set()

    def __init__(self) -> None:
        # Filled in by initialize(): {var_name: type_name} for variables
        # this plugin claims in the active main schema.
        self.matched_vars: dict[str, str] = {}

    # ── Lifecycle ────────────────────────────────────────────────────────────

    def initialize(self, declared_vars: dict[str, str]) -> bool:
        """
        Called once before the loop begins.

        Inspect `declared_vars` (var_name → type_name from main) and store
        the variables this plugin will handle. Return True if the plugin is
        active for this run, False if it has nothing to do.
        """
        self.matched_vars = {
            v: t for v, t in declared_vars.items() if t in self.handles_types
        }
        return bool(self.matched_vars)

    def start(self) -> None:
        """Called once after `initialize` returned True. Open files, windows, etc."""
        pass

    def before_step(self, current_state: dict[str, dict]) -> dict[str, Any] | None:
        """
        Called before every solve. Return a dict of `given` values
        contributed by this plugin's variables.

        Return None to halt the executor (e.g., stdin EOF, batch one-shot done).
        """
        return {}

    def after_step(self, bindings: dict[str, Any]) -> bool:
        """
        Called after every successful solve. Perform side effects (write to
        a stream, render a frame). Return False to halt the executor
        (e.g., user closed the SDL window).
        """
        return True

    def stop(self) -> None:
        """Called once at shutdown, even if a step raised. Release resources."""
        pass
