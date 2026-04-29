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

    // Default to the last schema (most recently defined) when there's no
    // prior selection to preserve.
    if (!prev || !_schemas.includes(prev)) {
        sel.value = _schemas[_schemas.length - 1];
    }

    // If schema changed, clear the binding inputs
    if (prev !== sel.value) {
        _clearBindings();
    }
    _activeSchema = sel.value;
}

// ── Eval result ───────────────────────────────────────────────────────────

function renderEvalResult(data) {
    if (data.satisfied) {
        const count = Object.keys(data.bindings || {}).length;
        _showNotification(`Satisfied ✓  (${count} variable${count !== 1 ? 's' : ''})`, 'success');
    } else if (!data.error) {
        _showNotification('Unsatisfiable', 'error');
    }
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
        });
    }
});
