# AP-004: no-skip-or-xfail-in-conformance

**Status:** active

**Pattern.** A test under `tests/conformance/` carries a
`pytest.mark.xfail` or `pytest.mark.skip` decorator, or a
`pytest.skip(...)` call inside a test body, or the file's
conftest applies xfail to specific tests via
`pytest_collection_modifyitems`.

**Why.** Conformance specifies what the language MUST do. A
test marked xfail/skip becomes TODO sediment — nobody comes back
to it, the suite grows a permanent list of "known failures," and
new regressions hide among them. The previous state of the
conformance suite had 64 xfails; nobody had looked at them in
weeks. They were resolved by triage: rewrote tests to match
current syntax (where the feature exists), deleted tests for
features that are gone, and filed real runtime gaps as entries
in `examples/COUNTEREXAMPLES.md`.

**Fix.** One of three actions per failing test:

1. **Rewrite** if the test's syntax is stale but the feature
   still works. Update to current syntax.
2. **Delete** if the feature is gone. The test no longer specifies
   anything we want.
3. **File as a runtime gap.** Append an entry to
   `examples/COUNTEREXAMPLES.md` describing the missing/broken
   behavior. Then DELETE the test. The COUNTEREXAMPLE entry is
   the work item; an xfail-marked test would be redundant.

Never just mark and move on.

**Detection.** grep

**Pattern (grep).** In any `tests/conformance/**.py`:
  - `pytest\.mark\.xfail`
  - `pytest\.mark\.skip`
  - `pytest\.skip\(`
  - `add_marker.*xfail` (catches the conftest dynamic-mark form)
  - `KNOWN_FAILING\s*=` (the dict-based dynamic xfail pattern
    that was removed during triage; if it comes back, fail)

**Scope.**
  - Apply to: `tests/conformance/**/*.py`.
  - Do NOT apply elsewhere (lang_tests are Rust integration
    fixtures, not Python).

**Exceptions.** None. If you genuinely can't make a test pass and
can't delete it, file the runtime gap and delete the test.

**Examples.**
  - Conformance triage 2026-05-10: 64 xfailed tests. Outcome:
    17 rewritten, 33 deleted, 10 runtime gaps filed (#11–20 in
    `examples/COUNTEREXAMPLES.md`).
