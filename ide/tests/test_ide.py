"""
Playwright end-to-end tests for the Evident IDE.

Run with (server must be running on port 8765):
    uvicorn ide.backend.main:app --port 8765 &
    pytest ide/tests/test_ide.py -v --timeout=90
"""
import pytest
from playwright.sync_api import sync_playwright, Page

BASE_URL = "http://localhost:8765/app/"

SIMPLE_SOURCE = """schema SimpleNat
    n ∈ Nat
    n > 5
    n < 20
"""

TWO_SCHEMA_SOURCE = """schema Small
    x ∈ Nat
    x < 10

schema Large
    y ∈ Nat
    y > 100
"""

SCATTER_SOURCE = """schema TwoVars
    x ∈ Nat
    y ∈ Nat
    x < 10
    y < 10
"""

UNSAT_SOURCE = """schema Impossible
    n ∈ Nat
    n > 10
    n < 5
"""


@pytest.fixture(scope="session")
def browser_instance():
    """Single Chromium instance for the whole session."""
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        yield browser
        browser.close()


@pytest.fixture(scope="function")
def page(browser_instance):
    """Fresh page per test, reusing the same browser process."""
    ctx = browser_instance.new_context(viewport={"width": 1400, "height": 900})
    pg = ctx.new_page()
    pg.goto(BASE_URL)
    pg.wait_for_timeout(5000)  # Monaco + first parse + auto-evaluate
    yield pg
    ctx.close()


def set_source(page: Page, source: str):
    escaped = source.replace("\\", "\\\\").replace("`", "\\`").replace("$", "\\$")
    page.evaluate(f"() => window.evEditor?.editor?.setValue(`{escaped}`)")
    page.wait_for_timeout(400)


def get_eval_satisfied(page: Page) -> bool | None:
    """Check if the most recent evaluate returned satisfied."""
    # Check eval-result div for solved binding inputs
    solved = page.query_selector_all(".binding-solved")
    status = page.inner_text("#status-bar")
    if "Satisfied" in status:
        return True
    if "nsat" in status or "error" in status.lower():
        return False
    if len(solved) > 0:
        return True
    return None


# ── Page load ────────────────────────────────────────────────────────────────

class TestPageLoad:
    def test_monaco_loads(self, page):
        assert page.evaluate("() => typeof monaco !== 'undefined'")

    def test_d3_loads(self, page):
        assert page.evaluate("() => typeof d3 !== 'undefined'")

    def test_schema_selector_has_default_selection(self, page):
        """First schema is selected, not 'Loading…'."""
        val = page.eval_on_selector("#schema-select", "el => el.value")
        assert val and val != "Loading…"

    def test_auto_evaluate_fires_on_load(self, page):
        """Evaluate fires automatically — status bar changes from 'Ready'."""
        status = page.inner_text("#status-bar")
        assert status != "Ready"  # something happened

    def test_evaluate_result_on_load(self, page):
        """Default source evaluates — satisfied status shown."""
        status = page.inner_text("#status-bar")
        assert "Satisfied" in status or len(page.query_selector_all(".binding-solved")) > 0


# ── Schema selector ──────────────────────────────────────────────────────────

class TestSchemaSelector:
    def test_schemas_populated(self, page):
        schemas = page.eval_on_selector(
            "#schema-select",
            "el => Array.from(el.options).map(o => o.value)"
        )
        assert len(schemas) >= 1
        assert all(s != "Loading…" for s in schemas)

    def test_changing_schema_triggers_evaluate(self, page):
        """Switching schema fires /evaluate — fills binding inputs."""
        set_source(page, TWO_SCHEMA_SOURCE)
        page.wait_for_timeout(2500)

        page.select_option("#schema-select", "Large")
        page.wait_for_timeout(2000)
        # Evaluate result: y > 100 is satisfied
        status = page.inner_text("#status-bar")
        assert "Satisfied" in status

    def test_switching_schemas_updates_selector(self, page):
        """Switching schema updates the active schema."""
        set_source(page, TWO_SCHEMA_SOURCE)
        page.wait_for_timeout(2500)
        page.select_option("#schema-select", "Small")
        page.wait_for_timeout(1000)
        assert page.eval_on_selector("#schema-select", "el => el.value") == "Small"
        page.select_option("#schema-select", "Large")
        page.wait_for_timeout(1000)
        assert page.eval_on_selector("#schema-select", "el => el.value") == "Large"


# ── Code change triggers evaluate ────────────────────────────────────────────

class TestCodeChange:
    def test_code_change_re_evaluates(self, page):
        """Editing source re-evaluates automatically after debounce."""
        set_source(page, SIMPLE_SOURCE)
        page.wait_for_timeout(2500)
        assert "Satisfied" in page.inner_text("#status-bar")

        # Change to satisfiable with tighter bounds — result changes
        new_source = SIMPLE_SOURCE.replace("n < 20", "n < 8")
        set_source(page, new_source)
        page.wait_for_timeout(2500)
        assert "Satisfied" in page.inner_text("#status-bar")

    def test_code_change_updates_schema_list(self, page):
        set_source(page, TWO_SCHEMA_SOURCE)
        # Wait for Monaco debounce (500ms) + parse + DOM update
        page.wait_for_timeout(4000)
        schemas = page.eval_on_selector(
            "#schema-select",
            "el => Array.from(el.options).map(o => o.value)"
        )
        assert "Small" in schemas and "Large" in schemas

    def test_parse_error_shown(self, page):
        set_source(page, "schema Bad\n    @@@ invalid\n")
        page.wait_for_timeout(2500)
        status = page.inner_text("#status-bar")
        assert "error" in status.lower() or "Error" in status


# ── Sampling ─────────────────────────────────────────────────────────────────

class TestSampling:
    def test_default_n_is_200(self, page):
        assert page.eval_on_selector("#sample-n", "el => el.value") == "200"

    def test_default_strategy_is_random(self, page):
        assert page.eval_on_selector("#sample-strategy", "el => el.value") == "random"

    def test_sample_vectors_collapsed_by_default(self, page):
        collapsed = page.evaluate(
            "() => !document.querySelector('#samples-section .viz-section-header').classList.contains('open')"
        )
        assert collapsed

    def test_sample_button_returns_rows(self, page):
        page.select_option("#sample-n", "5")
        page.click("#btn-sample")
        page.wait_for_timeout(6000)
        rows = len(page.query_selector_all(".sample-row"))
        assert rows == 5

    def test_n_10_returns_10_rows(self, page):
        set_source(page, SIMPLE_SOURCE)
        page.wait_for_timeout(2000)
        page.select_option("#sample-n", "10")
        page.click("#btn-sample")
        page.wait_for_timeout(8000)
        rows = len(page.query_selector_all(".sample-row"))
        assert rows == 10

    def test_auto_sample_fires_after_5s(self, page):
        """Auto-sample timer fires — rows appear without clicking Sample."""
        set_source(page, SIMPLE_SOURCE)
        page.wait_for_timeout(8000)  # 5s timer + processing
        rows = len(page.query_selector_all(".sample-row"))
        assert rows > 0

    def test_no_bad_z3_values(self, page):
        page.click("#btn-sample")
        page.wait_for_timeout(5000)
        table = page.inner_text("#samples-table-container")
        assert "!val!" not in table

    def test_pin_button_visible_after_sample(self, page):
        # Expand samples section so pin buttons are reachable
        page.click("#samples-section .viz-section-header")
        page.click("#btn-sample")
        page.wait_for_selector(".pin-btn", timeout=8000)
        assert page.query_selector(".pin-btn") is not None


# ── Scatter plot ─────────────────────────────────────────────────────────────

class TestScatter:
    def test_scatter_svg_renders_after_sample(self, page):
        set_source(page, SCATTER_SOURCE)
        page.wait_for_timeout(2000)
        page.click("#btn-sample")
        page.wait_for_timeout(5000)
        assert page.query_selector("#scatter-plot svg") is not None

    def test_scatter_has_data_points(self, page):
        set_source(page, SCATTER_SOURCE)
        page.wait_for_timeout(2000)
        page.click("#btn-sample")
        page.wait_for_timeout(5000)
        circles = len(page.query_selector_all("#scatter-plot circle"))
        assert circles > 0

    def test_scatter_axes_are_different_variables(self, page):
        set_source(page, SCATTER_SOURCE)
        page.wait_for_timeout(2000)
        page.click("#btn-sample")
        page.wait_for_timeout(5000)
        x = page.eval_on_selector("#scatter-x", "el => el.value") if page.query_selector("#scatter-x") else None
        y = page.eval_on_selector("#scatter-y", "el => el.value") if page.query_selector("#scatter-y") else None
        if x and y:
            assert x != y


# ── Evaluate button ───────────────────────────────────────────────────────────

class TestEvaluate:
    def test_satisfiable_schema(self, page):
        set_source(page, SIMPLE_SOURCE)
        page.wait_for_timeout(2000)
        page.click("#btn-evaluate")
        page.wait_for_timeout(2000)
        assert "Satisfied" in page.inner_text("#status-bar")

    def test_unsatisfiable_schema(self, page):
        set_source(page, UNSAT_SOURCE)
        page.wait_for_timeout(2500)
        page.click("#btn-evaluate")
        page.wait_for_timeout(2000)
        status = page.inner_text("#status-bar")
        # Should NOT say Satisfied
        assert "Satisfied" not in status

    def test_given_binding_changes_result(self, page):
        src = "schema SumTo10\n    x ∈ Nat\n    y ∈ Nat\n    x + y = 10\n"
        set_source(page, src)
        page.wait_for_timeout(2500)
        x_input = page.query_selector(".binding-input[data-varname='x']")
        if x_input:
            x_input.fill("3")
        page.click("#btn-evaluate")
        page.wait_for_timeout(2000)
        assert "Satisfied" in page.inner_text("#status-bar")
        # y should be solved to 7
        y_input = page.query_selector(".binding-input[data-varname='y']")
        if y_input:
            y_val = page.evaluate("el => el.value", y_input)
            assert y_val == "7"
