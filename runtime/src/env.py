from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any
import z3


@dataclass
class Environment:
    bindings: dict[str, z3.ExprRef] = field(default_factory=dict)
    parent: "Environment | None" = None

    def bind(self, name: str, value: z3.ExprRef) -> "Environment":
        """Return a new Environment with this binding added."""
        new_bindings = dict(self.bindings)
        new_bindings[name] = value
        return Environment(bindings=new_bindings, parent=self.parent)

    def lookup(self, name: str) -> z3.ExprRef | None:
        """Look up a variable, checking parent environments."""
        if name in self.bindings:
            return self.bindings[name]
        if self.parent is not None:
            return self.parent.lookup(name)
        return None

    def is_bound(self, name: str) -> bool:
        """Return True if the name is bound in this or any parent environment."""
        return self.lookup(name) is not None

    def merge(self, other: "Environment") -> "Environment":
        """Merge two environments.

        Variables with the same name are unified: if both environments bind
        the same name to structurally equal Z3 expressions, the result uses
        that expression.  If one environment lacks a binding the other has,
        the result includes it.  If both bind the same name to *different*
        expressions a ValueError is raised — the caller must resolve the
        conflict before merging.
        """
        merged: dict[str, z3.ExprRef] = dict(self.bindings)
        for name, value in other.bindings.items():
            if name in merged:
                existing = merged[name]
                # Accept identical objects or structurally equal Z3 expressions
                if existing is not value and not z3.eq(existing, value):
                    raise ValueError(
                        f"Cannot merge environments: variable '{name}' is bound "
                        f"to incompatible values ({existing} vs {value})"
                    )
                # Keep whichever we already have (they're equivalent)
            else:
                merged[name] = value
        return Environment(bindings=merged)
