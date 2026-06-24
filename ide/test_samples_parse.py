#!/usr/bin/env python3
"""Parse-gate for the IDE's bundled sample programs.

The samples in ide/web/static/app-data.js ship to every newcomer as the first
Evident they ever see — and they were NOT covered by any test, so a broken edit
to one would ship silently (it would only fail when a user clicked it). This
extracts every template-literal sample from app-data.js and runs each through
`evident export`, asserting the runtime loads it. Run standalone (`python3
ide/test_samples_parse.py`) or as a phase in ./test.sh.

It is a LOAD gate, not a behaviour gate: it asserts each sample parses + encodes
(export succeeds), which is exactly the silent-breakage a sample edit risks.
"""
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
APP_DATA = os.path.join(HERE, "web", "static", "app-data.js")
EVIDENT = os.path.join(ROOT, "runtime", "target", "release", "evident")


def extract_samples(js_source):
    """Pull every backtick template-literal value out of app-data.js.

    Returns a list of (label, program) pairs. `label` is the SAMPLES key when the
    literal is a value of the SAMPLES/DEFAULT_PROGRAM object, else a line marker.
    Handles `\\\\\\`` (escaped backtick) and `\\\\\\\\` (escaped backslash); the samples
    never use `${...}` interpolation, so we don't evaluate it — a literal `${` in a
    future sample would surface here as text, which export would then reject.
    """
    out = []
    lines = js_source.splitlines()
    i, n = 0, len(js_source)
    # Walk char by char, tracking line-comment state so a `//`-commented backtick
    # (none today, but cheap insurance) is never mistaken for a literal opener.
    in_line_comment = False
    label = None
    while i < n:
        c = js_source[i]
        if in_line_comment:
            if c == "\n":
                in_line_comment = False
            i += 1
            continue
        if c == "/" and i + 1 < n and js_source[i + 1] == "/":
            in_line_comment = True
            i += 2
            continue
        if c == "`":
            # Capture the nearest preceding quoted key on this/earlier lines as the label.
            label = _label_before(js_source, i)
            j = i + 1
            buf = []
            while j < n:
                cj = js_source[j]
                if cj == "\\" and j + 1 < n:
                    nxt = js_source[j + 1]
                    buf.append(nxt if nxt in "`\\$" else "\\" + nxt)
                    j += 2
                    continue
                if cj == "`":
                    break
                buf.append(cj)
                j += 1
            out.append((label or f"@offset {i}", "".join(buf)))
            i = j + 1
            continue
        i += 1
    return out


def _label_before(src, backtick_idx):
    """The label naming a backtick value: a `const NAME =` or a `"key":`, whichever
    sits immediately before the backtick (top-level consts like DEFAULT_PROGRAM use
    the former; SAMPLES entries the latter)."""
    head = src[:backtick_idx].rstrip()
    # `const NAME =` / `NAME: DEFAULT_PROGRAM,` — a bare `const X =\n`backtick wins.
    import re
    m = re.search(r"const\s+([A-Za-z_]\w*)\s*=\s*$", head)
    if m:
        return m.group(1)
    # The pattern is `"key":` then optional whitespace then the backtick. Find the
    # last double-quoted string before the colon that precedes this backtick.
    if not head.endswith(":"):
        return None
    seg = head[:-1]
    end = seg.rfind('"')
    if end == -1:
        return None
    start = seg.rfind('"', 0, end)
    if start == -1:
        return None
    return seg[start + 1:end]


def export_loads(program):
    """Run `evident export` on one program. Returns (ok, message)."""
    with tempfile.TemporaryDirectory() as work:
        ev = os.path.join(work, "prog.ev")
        with open(ev, "w") as f:
            f.write(program)
        prefix = os.path.join(work, "prog")
        r = subprocess.run([EVIDENT, "export", ev, "--out", prefix],
                           capture_output=True, text=True, timeout=60, cwd=ROOT)
        msg = ((r.stderr or "") + (r.stdout or "")).strip()
        ok = r.returncode == 0 and os.path.exists(prefix + ".smt2")
        return ok, msg[-600:]


def main():
    if not os.path.exists(EVIDENT):
        print(f"SKIP: evident binary not built at {EVIDENT} "
              "(run ./test.sh, or `cargo build --release` in runtime/)")
        return 0
    with open(APP_DATA) as f:
        samples = extract_samples(f.read())
    if not samples:
        print("FAIL: no samples extracted from app-data.js")
        return 1
    failures = []
    for label, program in samples:
        ok, msg = export_loads(program)
        mark = "ok" if ok else "FAIL"
        print(f"  [{mark}] {label}")
        if not ok:
            failures.append((label, msg))
    print(f"\n{len(samples) - len(failures)}/{len(samples)} samples load.")
    if failures:
        print("\nFAILURES:")
        for label, msg in failures:
            print(f"\n--- {label} ---\n{msg}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
