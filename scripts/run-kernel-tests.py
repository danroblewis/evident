#!/usr/bin/env python3
"""Drive every tests/kernel/test_*.ev through `evident emit` + `kernel`.

Each fixture has a header comment block describing expected stdout +
exit code:

    -- expect: stdout = "hello world"
    -- expect: exit = 0

Multiple `expect: stdout` lines stack into a multi-line expected output.
Missing `expect:` lines default to "stdout = '', exit = 0".

Conventions:
- Each .ev file declares a top-level `claim` named after the file
  (`test_hello.ev` → claim name `hello`, drop the `test_` prefix).
- Or the file may use `main` as the claim name; both are tried.
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EVIDENT = ROOT / "runtime/target/release/evident"
KERNEL  = ROOT / "kernel/target/release/kernel"
TESTS   = ROOT / "tests/kernel"


def parse_expectations(src: str):
    """Return (expected_stdout, expected_exit)."""
    stdout_lines = []
    exit_code = 0
    for line in src.splitlines():
        m = re.match(r"--\s*expect:\s*stdout\s*=\s*(.*)$", line)
        if m:
            val = m.group(1).strip()
            if val.startswith('"') and val.endswith('"'):
                val = val[1:-1]
            stdout_lines.append(val)
            continue
        m = re.match(r"--\s*expect:\s*exit\s*=\s*(-?\d+)", line)
        if m:
            exit_code = int(m.group(1))
    return "\n".join(stdout_lines), exit_code


def guess_claim_name(src: str, file_stem: str):
    """Pick the top-level claim to emit. Try the file's natural name
    first (`test_hello.ev` → `hello`), fall back to `main`."""
    natural = file_stem.removeprefix("test_")
    candidates = [natural, "main", "hello", "app"]
    for c in candidates:
        if re.search(rf"^\s*(claim|fsm|type|schema)\s+{re.escape(c)}\b", src, re.M):
            return c
    return natural


def run_one(path: Path) -> tuple[bool, str]:
    """Returns (passed, message)."""
    src = path.read_text()
    expected_stdout, expected_exit = parse_expectations(src)
    claim = guess_claim_name(src, path.stem)

    # File-I/O fixture sets up its own input file.
    if path.name == "test_file_io.ev":
        Path("/tmp/evident_kernel_io_input.txt").write_text("file roundtrip\n")
        Path("/tmp/evident_kernel_io_output.txt").unlink(missing_ok=True)

    # Stdin fixture sets stdin input.
    stdin_text = None
    if path.name == "test_echo_lines.ev":
        stdin_text = "alpha\nbeta\ngamma\n"

    # File-driven lexer fixture writes its input file.
    if path.name == "test_file_lexer.ev":
        Path("/tmp/evident_lex_input.txt").write_text("(7+3)\n")
    if path.name == "test_multichar_ident.ev":
        Path("/tmp/evident_multichar_input.txt").write_text("abc def\n")
    if path.name == "test_multichar_int.ev":
        Path("/tmp/evident_digits_input.txt").write_text("12+345\n")
    if path.name == "test_keyword_lexer.ev":
        Path("/tmp/evident_kw_input.txt").write_text("claim hello type fsm\n")
    if path.name == "test_comment_lexer.ev":
        Path("/tmp/evident_comment_input.txt").write_text("x = 5 -- this is a comment\ny = 7\n")

    with tempfile.NamedTemporaryFile(suffix=".smt2", mode="w", delete=False) as f:
        smt_path = f.name

    try:
        # 1. emit
        r = subprocess.run(
            [str(EVIDENT), "emit", str(path), claim, "-o", smt_path],
            capture_output=True, text=True, timeout=30,
        )
        if r.returncode != 0:
            return False, f"emit failed: {r.stderr.strip() or r.stdout.strip()}"

        # 2. kernel
        r = subprocess.run(
            [str(KERNEL), smt_path],
            input=stdin_text,
            capture_output=True, text=True, timeout=30,
        )
        actual_stdout = r.stdout.rstrip("\n")
        actual_exit   = r.returncode

        if expected_stdout and actual_stdout != expected_stdout:
            return False, (
                f"stdout mismatch:\n"
                f"  expected: {expected_stdout!r}\n"
                f"  got:      {actual_stdout!r}\n"
                f"  stderr:   {r.stderr.strip()!r}"
            )
        if actual_exit != expected_exit:
            return False, (
                f"exit mismatch: expected {expected_exit}, got {actual_exit}; "
                f"stderr: {r.stderr.strip()!r}"
            )
        return True, ""
    finally:
        Path(smt_path).unlink(missing_ok=True)


def main() -> int:
    if not EVIDENT.exists():
        print(f"evident binary missing at {EVIDENT}", file=sys.stderr)
        return 1
    if not KERNEL.exists():
        print(f"kernel binary missing at {KERNEL}", file=sys.stderr)
        return 1
    files = sorted(TESTS.glob("test_*.ev"))
    failed = []
    for f in files:
        ok, msg = run_one(f)
        if ok:
            print(f"  ✓ {f.name}")
        else:
            print(f"  ✗ {f.name}: {msg}")
            failed.append(f.name)
    print(f"{len(files)} kernel tests, {len(failed)} failed")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
