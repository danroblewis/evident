/**
 * Evident IDE — Schema selector and variable binding panel.
 *
 * Manages:
 *  - The <select id="schema-select"> dropdown
 *  - The "Variable Bindings" section in the visualizer
 *  - Tracking which variables are "pinned" (given fixed values)
 */

'use strict';

// ── Module state ─────────────────────────────────────────────────────────

let _schemas      = [];    // string[]
let _activeSchema = null;  // string | null

// ── Schema dropdown ──────────────────────────────────────────────────────

/**
 * Called by editor.js (via evEditor.onSchemaListChange) whenever /parse
 * returns a new list of schema names.
 * @param {string[]} schemas
 */
function updateSchemaList(schemas) {
    _schemas = schemas || [];

    const sel = document.getElementById('schema-select');
    if (!sel) return;

    const prev = sel.value;

    sel.innerHTML = '';

    if (_schemas.length === 0) {
        const opt = document.createElement('option');
        opt.value       = '';
        opt.textContent = '— no schemas —';
        opt.disabled    = true;
        sel.appendChild(opt);
        _activeSchema = null;
        _clearBindings();
        return;
    }

    _schemas.forEach((name) => {
        const opt = document.createElement('option');
        opt.value       = name;
        opt.textContent = name;
        if (name === prev) opt.selected = true;
        sel.appendChild(opt);
    });

    // If schema changed, clear the binding inputs
    if (prev !== sel.value) {
        _clearBindings();
    }
    _activeSchema = sel.value;
}

// ── Given (pinned) bindings ──────────────────────────────────────────────

/**
 * Return the current "given" bindings — reads all non-empty binding inputs.
 * Called by index.html when building POST bodies.
 * @returns {{ [name: string]: number | string }}
 */
function getGivenBindings() {
    const bindings = {};
    document.querySelectorAll('.binding-input').forEach((input) => {
        const val = input.value.trim();
        if (val !== '') {
            const name = input.dataset.varname;
            if (!name) return;
            const num = Number(val);
            bindings[name] = isNaN(num) ? val : num;
        }
    });
    return bindings;
}

function _clearBindings() {
    const el = document.getElementById('bindings-inputs');
    if (el) el.innerHTML = '';
}

// ── Load schema variables from /ranges ───────────────────────────────────

/**
 * Fetch the variable info for a schema by calling /ranges, then render inputs.
 * @param {string} source   Current Evident source text
 * @param {string} schemaName
 */
async function loadSchemaVariables(source, schemaName) {
    if (!schemaName) {
        _clearBindings();
        return;
    }
    try {
        const resp = await fetch('/ranges', {
            method:  'POST',
            headers: { 'Content-Type': 'application/json' },
            body:    JSON.stringify({ source, schema: schemaName, given: {} }),
        });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        renderBindingInputs(data.ranges || {});
    } catch (e) {
        console.warn('Could not load schema variables:', e.message);
    }
}

// ── Binding inputs ────────────────────────────────────────────────────────

/**
 * Render one input row per variable in #bindings-inputs.
 * @param {{ [name: string]: { min?: number|null, max?: number|null, type?: string, fixed?: any } }} ranges
 */
function renderBindingInputs(ranges) {
    const container = document.getElementById('bindings-inputs');
    if (!container) return;

    container.innerHTML = '';

    const entries = Object.entries(ranges);
    if (entries.length === 0) {
        container.innerHTML =
            '<p style="color:var(--fg-muted);font-size:12px;font-style:italic">No variables found.</p>';
        return;
    }

    entries.forEach(([name, info]) => {
        const row = document.createElement('div');
        row.className = 'binding-row';

        // Label
        const label = document.createElement('span');
        label.className   = 'binding-label';
        label.textContent = name;

        // Type badge
        if (info.type) {
            const typeSpan = document.createElement('span');
            typeSpan.className   = 'binding-type';
            typeSpan.textContent = info.type;
            label.appendChild(typeSpan);
        }

        // Input
        const input = document.createElement('input');
        input.type      = 'text';
        input.className = 'binding-input';
        input.dataset.varname = name;

        if (info.fixed !== undefined) {
            input.value    = String(info.fixed);
            input.disabled = true;
            input.className += ' free';
            input.title    = 'Fixed by "given" bindings';
        } else {
            const lo = info.min != null ? info.min : '?';
            const hi = info.max != null ? info.max : '?';
            input.placeholder = `[${lo}, ${hi}]`;
        }

        // Range hint
        const hint = document.createElement('span');
        hint.className = 'binding-range-hint';
        if (info.fixed === undefined && info.min != null && info.max != null) {
            hint.textContent = `${info.min}…${info.max}`;
        }

        row.appendChild(label);
        row.appendChild(input);
        row.appendChild(hint);
        container.appendChild(row);
    });
}

// ── Eval result rendering ────────────────────────────────────────────────

/**
 * Show the /evaluate result in the bindings panel.
 * @param {{ satisfied: boolean, bindings?: object, error?: string }} data
 */
function renderEvalResult(data) {
    if (data.error) {
        _showNotification(`Error: ${data.error}`, 'error');
        return;
    }

    if (!data.satisfied) {
        _showNotification('Unsatisfiable — no valid assignment exists', 'error');
        return;
    }

    // Fill in solved values in the binding inputs
    for (const [name, val] of Object.entries(data.bindings || {})) {
        const input = document.querySelector(`.binding-input[data-varname="${name}"]`);
        if (input && !input.disabled) {
            input.value = String(val);
            input.classList.add('binding-solved');
        }
    }
    const count = Object.keys(data.bindings || {}).length;
    _showNotification(`Satisfied ✓  (${count} variable${count !== 1 ? 's' : ''})`, 'success');
}

// ── Notification helper ───────────────────────────────────────────────────

/**
 * Show a brief status message in the status bar (auto-clears after 3s).
 * @param {string} message
 * @param {'info'|'success'|'error'} type
 */
function _showNotification(message, type = 'info') {
    const bar = document.getElementById('status-bar');
    if (!bar) return;

    const textEl = bar.querySelector('#status-text') || bar;

    // Save and restore original classes
    const origClass = bar.className;
    const origText  = textEl.textContent;

    bar.className   = `status-${type}`;
    textEl.textContent = message;

    clearTimeout(bar._notifTimeout);
    bar._notifTimeout = setTimeout(() => {
        bar.className      = origClass;
        textEl.textContent = origText;
    }, 3000);
}

// Expose for other modules (samples.js uses it)
window.showNotification = _showNotification;

// ── Init ─────────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    const schemaSelect = document.getElementById('schema-select');
    if (schemaSelect) {
        schemaSelect.addEventListener('change', () => {
            _activeSchema = schemaSelect.value;
            const source  = window.evEditor?.getSource() || '';
            loadSchemaVariables(source, _activeSchema);
        });
    }
});
