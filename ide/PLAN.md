# Evident IDE — Implementation Plan

A web-based IDE for the Evident constraint programming language with a novel
constraint visualizer. The core idea: every schema is a constraint system with
a solution space — the IDE makes that space visible and explorable.

---

## Architecture overview

```
Browser
  Monaco Editor (source)     ←→  FastAPI backend
  Schema selector                   ↓
  Constraint visualizer          parser.src.parser
    Sample vectors                 runtime.src.runtime
    Range sliders                  Z3 (via Optimize + Solver)
    2D/3D scatter (Plotly)         Monte Carlo sampler
    Transfer function plots
```

**Backend**: FastAPI (Python) — wraps our existing runtime.
**Frontend**: Vanilla JS + Monaco Editor + Plotly.js — no framework.
**Transport**: REST JSON + WebSocket for live parse feedback.

---

## Phase 1: Backend API

**Files**: `ide/backend/main.py`, `ide/backend/sampler.py`

### Endpoints

```
POST /parse
  body: { source: str }
  returns: { schemas: [name], errors: [{ line, col, message }] }

POST /evaluate
  body: { source: str, schema: str, given: {name: value} }
  returns: { satisfied: bool, bindings: {name: value}, evidence: {...} }

POST /ranges
  body: { source: str, schema: str, given: {name: value} }
  returns: { ranges: { name: { min, max, type } } }
  Uses Z3 Optimize to find min/max for each numeric variable.

POST /sample
  body: { source: str, schema: str, given: {name: value}, n: int, strategy: str }
  returns: { samples: [{name: value}], count: int }
  Monte Carlo sampling — returns n valid assignments.

POST /transfer
  body: { source: str, schema: str, given: {name: value}, x_var: str, y_var: str,
          x_range: [min, max], steps: int }
  returns: { points: [{x, y, feasible}] }
  Sweeps x_var across a range, solves for y_var at each step.
```

### `ide/backend/sampler.py`

Three sampling strategies:

**1. Blocking clause sampling** — find one solution, add negation of it as a
new constraint, solve again. Produces diverse exact solutions.
```python
def blocking_sample(schema, given, n, runtime):
    samples = []
    solver = build_solver(schema, given, runtime)
    while len(samples) < n and solver.check() == sat:
        model = solver.model()
        sample = extract_bindings(model)
        samples.append(sample)
        # block this assignment
        solver.add(Or([var != val for var, val in sample.items()]))
    return samples
```

**2. Random seed sampling** — add random perturbation constraints to nudge
the solver into different regions of the solution space. Fast, less systematic.
```python
def random_seed_sample(schema, given, n, runtime):
    samples = []
    for _ in range(n * 3):  # try 3x to account for failures
        seed = {var: random.randint(lo, hi) for var, (lo, hi) in ranges.items()}
        result = runtime.query(schema, given={**given, **seed})
        if result.satisfied:
            samples.append(result.bindings)
        if len(samples) >= n:
            break
    return samples
```

**3. Grid sampling** — for 1–2 free variables, sweep a grid. Used for
transfer functions and 2D plots.

### Range analysis via Z3 Optimize

```python
from z3 import Optimize

def compute_ranges(schema, given, runtime):
    ranges = {}
    for var_name in schema_variables(schema):
        if var_name in given:
            ranges[var_name] = {"fixed": given[var_name]}
            continue
        # minimize
        opt = Optimize()
        add_schema_constraints(opt, schema, given, runtime)
        opt.minimize(get_z3_var(var_name))
        if opt.check() == sat:
            lo = opt.model()[get_z3_var(var_name)].as_long()
        else:
            lo = None
        # maximize
        opt = Optimize()
        add_schema_constraints(opt, schema, given, runtime)
        opt.maximize(get_z3_var(var_name))
        if opt.check() == sat:
            hi = opt.model()[get_z3_var(var_name)].as_long()
        else:
            hi = None
        ranges[var_name] = {"min": lo, "max": hi}
    return ranges
```

---

## Phase 2: Editor with syntax highlighting

**Files**: `ide/frontend/index.html`, `ide/frontend/editor.js`,
`ide/frontend/evident-lang.js`

### Monaco Editor setup

```javascript
require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor/min/vs' }});
require(['vs/editor/editor.main'], function() {
    // Register the Evident language
    monaco.languages.register({ id: 'evident' });
    monaco.languages.setMonarchTokensProvider('evident', EVIDENT_TOKENS);
    monaco.languages.setLanguageConfiguration('evident', EVIDENT_CONFIG);

    const editor = monaco.editor.create(document.getElementById('editor'), {
        language: 'evident',
        theme: 'evident-dark',
        value: DEFAULT_SOURCE,
    });

    // Live parse on change (debounced 500ms)
    editor.onDidChangeModelContent(debounce(onSourceChange, 500));
});
```

### Monarch token rules (`evident-lang.js`)

Adapt the existing highlight.js grammar from `~/md/static/index.html` to
Monaco's Monarch format. Key token classes:
- `keyword` — `schema`, `claim`, `type`, `assert`, `evident`
- `type.identifier` — `Nat`, `Int`, `Bool`, `String`, capitalized names
- `operator` — `∈`, `∉`, `⊆`, `∀`, `∃`, `⇒`, `·`, `≤`, `≥`, `≠`
- `string` — `"..."`
- `number` — integers and reals
- `comment` — `-- ...`
- `variable` — `_internal` names (dim color)
- `function` — claim names at declaration sites

### Gutter decorations (the "debugger circle" feature)

Monaco supports custom decorations. Use these to show constraint status:
- **Green circle** — this schema line is satisfiable given current bindings
- **Red circle** — this constraint is currently violated
- **Orange circle** — this constraint is satisfiable but tight (near boundary)
- **Grey circle** — unchecked / unknown

```javascript
function updateDecorations(parseResult, evalResult) {
    const decorations = [];
    for (const constraint of evalResult.constraint_status) {
        decorations.push({
            range: new monaco.Range(constraint.line, 1, constraint.line, 1),
            options: {
                glyphMarginClassName: `glyph-${constraint.status}`,  // css class
                glyphMarginHoverMessage: { value: constraint.message }
            }
        });
    }
    editor.deltaDecorations(currentDecorations, decorations);
}
```

---

## Phase 3: Schema selector and variable panel

**Files**: `ide/frontend/schema-panel.js`

A sidebar showing all schemas in the current program. Click one to open
its visualizer panel.

```
┌─────────────────────┐
│ Schemas             │
│ ─────────────────── │
│ ◉ Task              │
│   ValidAssignment   │
│   sorted            │
│   valid_conference  │
└─────────────────────┘
```

For the selected schema, show all its variables with:
- **Bound** (fixed value) — input box with current value
- **Free** (unbound) — shows computed range `[lo, hi]` or `?`
- A checkbox to "pin" a variable to a specific value (add it to `given`)

---

## Phase 4: Sample vectors table

**Files**: `ide/frontend/samples.js`

Calls `POST /sample` and renders results as a table:

```
┌──────────────────────────────────────────────┐
│ Sample vectors (N=10)          [Resample] [+]│
├──────┬──────────┬──────────┬──────────────── │
│  #   │   id     │ duration │ deadline        │
├──────┼──────────┼──────────┼──────────────── │
│  1   │    3     │   45     │    300          │
│  2   │    7     │   90     │    480          │
│  3   │   12     │   60     │    200          │
│  ... │  ...     │  ...     │    ...          │
└──────┴──────────┴──────────┴──────────────── │
```

Features:
- Click a row to pin its values into the `given` bindings
- Hover a row to highlight which constraints it satisfies (glyph colors)
- Sort columns
- Export as CSV

---

## Phase 5: Range visualization

**Files**: `ide/frontend/ranges.js`

For each free variable with a computed range, show a range bar:

```
n    ████████░░░░░░░░░░░░   [6 ... 99]   currently: 42
     min=6              max=99
```

Sliders let users set the `given` value for that variable and watch
other variables' ranges update reactively.

---

## Phase 6: 2D scatter plot

**Files**: `ide/frontend/scatter.js`

Pick any two free variables as X and Y axes. Plot the sample vectors as
points. Each point represents a valid assignment.

```javascript
Plotly.newPlot('scatter', [{
    x: samples.map(s => s[xVar]),
    y: samples.map(s => s[yVar]),
    mode: 'markers',
    type: 'scatter',
    marker: { color: 'green', size: 8, opacity: 0.7 }
}], {
    title: `${schema}: ${xVar} vs ${yVar}`,
    xaxis: { title: xVar },
    yaxis: { title: yVar },
});
```

Add a "density" mode: darken regions where samples cluster — shows which
parts of the valid space are "typical."

---

## Phase 7: Transfer function plot

**Files**: `ide/frontend/transfer.js`

Fix all variables except two. Sweep X from min to max in N steps.
At each X value, solve for Y. Plot Y(X).

```javascript
// calls POST /transfer
const points = await fetch('/transfer', {
    method: 'POST',
    body: JSON.stringify({ schema, given, x_var, y_var, x_range, steps: 50 })
});
// render as line chart
Plotly.newPlot('transfer', [{
    x: points.map(p => p.x),
    y: points.map(p => p.y),
    mode: 'lines+markers',
    // infeasible points shown as gaps
}]);
```

Shows how one variable responds to another across its valid range.
Infeasible regions (where no valid Y exists) appear as gaps in the line.

---

## Phase 8: 3D scatter and surface

**Files**: `ide/frontend/scatter3d.js`

Pick three free variables. Plot samples as a 3D scatter:

```javascript
Plotly.newPlot('scatter3d', [{
    x: samples.map(s => s[xVar]),
    y: samples.map(s => s[yVar]),
    z: samples.map(s => s[zVar]),
    mode: 'markers',
    type: 'scatter3d',
}]);
```

For a denser view: fix Z at several values ("slices") and show 2D
cross-sections at each Z level.

---

## Directory structure

```
ide/
  backend/
    main.py          FastAPI app, all endpoints
    sampler.py       Blocking clause / random / grid sampling
    ranges.py        Z3 Optimize-based range analysis
    transfer.py      Transfer function sweep
  frontend/
    index.html       Single-page app shell
    evident-lang.js  Monaco Monarch token rules
    editor.js        Monaco setup, live parse, glyph decorations
    schema-panel.js  Schema selector sidebar
    samples.js       Sample vectors table
    ranges.js        Range bars and sliders
    scatter.js       2D scatter plot (Plotly)
    transfer.js      Transfer function plot
    scatter3d.js     3D scatter / slicing
    style.css
  requirements.txt   fastapi, uvicorn, z3-solver
```

---

## Parallelization

| Agent | Owns |
|---|---|
| A | FastAPI backend + sampler + ranges + transfer endpoints |
| B | Monaco editor setup + Evident syntax highlighting + glyph decorations |
| C | Schema selector panel + variable binding UI |
| D | Sample vectors table + range bars |
| E (after A+D) | 2D scatter + transfer function plots |
| F (after A+D) | 3D scatter + slicing |

---

## Definition of done

- User can write Evident source in the editor and see syntax highlighting
- Parse errors appear as red squiggles with hover messages
- Selecting a schema opens its visualizer
- Sample vectors table shows ≥ 10 valid assignments
- Range bars show computed min/max for numeric variables
- 2D scatter shows valid solution cloud for any two variables
- Transfer function shows how Y responds to X across its valid range
- All backend endpoints respond in < 2 seconds for simple schemas
