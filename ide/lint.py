#!/usr/bin/env python3
"""Code-quality linter for the Evident IDE Python codebase.

Run from the repo root:

    python3 ide/lint.py            # full report, exits non-zero on any violation
    python3 ide/lint.py --quiet    # summary only

Scans `ide/*.py`, `ide/**/*.py`, and `viz/*.py` and enforces four mechanical
guardrails (see the RULE constants below). It REPORTS ONLY — it never edits or
reformats a scanned file. Exit code is non-zero if any violation is found, so it
can gate CI.

`app.js` is JavaScript, not Python, so the AST rules don't apply; it is only
line-count checked (Rule 1) via the JS_EXTRA_FILES list.

Suppression: put `# noqa: lint` on a line to suppress every violation that
reports at that line.
"""

from __future__ import annotations

import argparse
import ast
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Thresholds — every rule is a single editable constant.
# ---------------------------------------------------------------------------

MAX_FILE_LINES = 500          # Rule 1: matches CLAUDE.md's "≤ 500 lines per file".
MAX_DEF_LINES = 70            # Rule 2: a function/method spanning more than this
#                              many source lines (end_lineno - lineno + 1) is a
#                              "do one thing" smell.
MAX_MODULE_FUNCS = 12         # Rule 3: free (module-level, non-class) functions.
#                              Many top-level defs = logic that wants to be a
#                              class or split into modules.

# Rule 4 — module<->module coupling. Two complementary, low-false-positive
# signals, combined into one score:
#   (a) distinct SIBLING-module imports: `from ide.x import …` / `import viz.y`
#       (and the in-package `from .x import …` form). Stdlib / third-party
#       imports are NOT counted — only edges to OUR OWN other modules.
#   (b) reaching into another module's PRIVATE name: `othermod._helper(...)`.
#       Touching a `_`-prefixed attribute on an imported sibling module is a
#       deliberate encapsulation break and weighs heavier.
# A file's coupling score = (# distinct sibling-module imports)
#                         + 2 * (# private cross-module accesses).
# Over MAX_COUPLING the file is flagged. This is intentionally mechanical: it
# measures edges between our own modules, the thing that makes a codebase
# "unwieldy", and ignores library imports which say nothing about internal
# tangle.
MAX_COUPLING = 8

# Top-level package names that count as "our own modules" for Rule 4.
OUR_PACKAGES = {"ide", "viz"}

# Non-Python files that still get the line-count check (Rule 1 only).
JS_EXTRA_FILES = ["ide/web/static/app.js"]

SUPPRESS_MARKER = "noqa: lint"


# ---------------------------------------------------------------------------
# Scanning
# ---------------------------------------------------------------------------

def discover_py_files(repo_root: Path) -> list[Path]:
    """ide/*.py, ide/**/*.py, viz/*.py — excluding caches and this linter."""
    found: list[Path] = []
    found += (repo_root / "ide").rglob("*.py")
    found += (repo_root / "viz").glob("*.py")
    out = []
    for p in sorted(set(found)):
        if "__pycache__" in p.parts or p.name == "lint.py":
            continue
        out.append(p)
    return out


def suppressed_lines(text: str) -> set[int]:
    return {
        i for i, line in enumerate(text.splitlines(), start=1)
        if SUPPRESS_MARKER in line
    }


class Violation:
    __slots__ = ("rule", "path", "line", "message")

    def __init__(self, rule: str, path: str, line: int, message: str):
        self.rule = rule
        self.path = path
        self.line = line
        self.message = message

    def location(self) -> str:
        return f"{self.path}:{self.line}"


# ---------------------------------------------------------------------------
# Rules
# ---------------------------------------------------------------------------

def check_file_lines(rel: str, n_lines: int) -> list[Violation]:
    if n_lines <= MAX_FILE_LINES:
        return []
    return [Violation(
        "file-too-long", rel, 1,
        f"{n_lines} lines (max {MAX_FILE_LINES}); split into focused modules",
    )]


def check_def_lengths(rel: str, tree: ast.AST) -> list[Violation]:
    out = []
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        end = getattr(node, "end_lineno", None) or node.lineno
        span = end - node.lineno + 1
        if span > MAX_DEF_LINES:
            out.append(Violation(
                "function-too-long", rel, node.lineno,
                f"{node.name}() spans {span} lines "
                f"(lines {node.lineno}-{end}, max {MAX_DEF_LINES})",
            ))
    return out


def check_module_functions(rel: str, tree: ast.Module) -> list[Violation]:
    free = [
        n for n in tree.body
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
    ]
    if len(free) <= MAX_MODULE_FUNCS:
        return []
    line = free[MAX_MODULE_FUNCS].lineno  # first def past the budget
    return [Violation(
        "too-many-free-functions", rel, line,
        f"{len(free)} module-level functions (max {MAX_MODULE_FUNCS}); "
        f"group related ones into a class or split the file",
    )]


def _is_our_module(dotted: str) -> bool:
    return dotted.split(".", 1)[0] in OUR_PACKAGES


def check_coupling(rel: str, tree: ast.Module) -> list[Violation]:
    sibling_imports: set[str] = set()
    module_aliases: dict[str, str] = {}  # local name -> dotted sibling module

    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if _is_our_module(alias.name):
                    sibling_imports.add(alias.name)
                    local = alias.asname or alias.name.split(".")[0]
                    module_aliases[local] = alias.name
        elif isinstance(node, ast.ImportFrom):
            if node.level:  # relative `from .x import …`
                sibling_imports.add("." * node.level + (node.module or ""))
            elif node.module and _is_our_module(node.module):
                sibling_imports.add(node.module)
            else:
                continue
            for alias in node.names:
                module_aliases.setdefault(alias.asname or alias.name, node.module or "")

    # Private cross-module access: `mod._name` where `mod` is a sibling alias.
    private_hits: list[tuple[int, str]] = []
    for node in ast.walk(tree):
        if (isinstance(node, ast.Attribute)
                and node.attr.startswith("_")
                and not node.attr.startswith("__")
                and isinstance(node.value, ast.Name)
                and node.value.id in module_aliases):
            private_hits.append((node.lineno, f"{node.value.id}.{node.attr}"))

    score = len(sibling_imports) + 2 * len(private_hits)
    if score <= MAX_COUPLING:
        return []
    detail = f"{len(sibling_imports)} sibling-module imports"
    if private_hits:
        detail += f", {len(private_hits)} private cross-module access(es)"
    line = private_hits[0][0] if private_hits else 1
    return [Violation(
        "high-coupling", rel, line,
        f"coupling score {score} (max {MAX_COUPLING}): {detail}",
    )]


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------

def lint_repo(repo_root: Path) -> list[Violation]:
    violations: list[Violation] = []

    for path in discover_py_files(repo_root):
        rel = str(path.relative_to(repo_root))
        text = path.read_text(encoding="utf-8", errors="replace")
        suppress = suppressed_lines(text)
        file_vs = check_file_lines(rel, len(text.splitlines()))
        try:
            tree = ast.parse(text, filename=rel)
        except SyntaxError as exc:
            file_vs.append(Violation(
                "syntax-error", rel, exc.lineno or 1,
                f"could not parse: {exc.msg}",
            ))
        else:
            file_vs += check_def_lengths(rel, tree)
            file_vs += check_module_functions(rel, tree)
            file_vs += check_coupling(rel, tree)
        violations += [v for v in file_vs if v.line not in suppress]

    for js_rel in JS_EXTRA_FILES:
        jp = repo_root / js_rel
        if not jp.exists():
            continue
        text = jp.read_text(encoding="utf-8", errors="replace")
        suppress = suppressed_lines(text)
        violations += [
            v for v in check_file_lines(js_rel, len(text.splitlines()))
            if v.line not in suppress
        ]

    return violations


RULE_TITLES = {
    "file-too-long": f"Rule 1 — files over {MAX_FILE_LINES} lines",
    "function-too-long": f"Rule 2 — functions over {MAX_DEF_LINES} lines",
    "too-many-free-functions": f"Rule 3 — over {MAX_MODULE_FUNCS} module-level functions",
    "high-coupling": f"Rule 4 — coupling score over {MAX_COUPLING}",
    "syntax-error": "Parse errors",
}
RULE_ORDER = list(RULE_TITLES)


def report(violations: list[Violation], quiet: bool) -> None:
    by_rule: dict[str, list[Violation]] = {}
    for v in violations:
        by_rule.setdefault(v.rule, []).append(v)

    if not quiet:
        for rule in RULE_ORDER:
            vs = by_rule.get(rule)
            if not vs:
                continue
            print(f"\n{RULE_TITLES[rule]}  ({len(vs)})")
            for v in sorted(vs, key=lambda x: (x.path, x.line)):
                print(f"  {v.location()}: {v.message}")
        if not violations:
            print("No violations.")

    print("\n=== Summary ===")
    if not violations:
        print("clean — 0 violations")
    else:
        for rule in RULE_ORDER:
            n = len(by_rule.get(rule, []))
            if n:
                print(f"  {RULE_TITLES[rule]}: {n}")
        print(f"  TOTAL: {len(violations)}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Lint the Evident IDE Python codebase.")
    parser.add_argument("--quiet", action="store_true", help="print only the summary")
    args = parser.parse_args(argv)

    repo_root = Path(__file__).resolve().parent.parent
    violations = lint_repo(repo_root)
    report(violations, args.quiet)
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
