/**
 * Evident IDE — Monaco editor setup, live parse, and glyph decorations.
 */

'use strict';

const DEFAULT_SOURCE = `schema Task
    id       ∈ Nat
    duration ∈ Nat
    deadline ∈ Nat
    duration < deadline

schema ValidSchedule
    task   ∈ Task
    slot   ∈ Nat
    budget ∈ Nat
    slot > 0
    slot + task.duration ≤ budget
`;

class EvidentEditor {
    /**
     * @param {string} containerId  ID of the DOM element to mount Monaco in
     */
    constructor(containerId) {
        this.containerId = containerId;
        this.editor = null;

        // Monaco decoration IDs (returned by deltaDecorations)
        this._errorDecorations = [];
        this._glyphDecorations = [];

        // Debounce handle for parse requests
        this._parseTimer = null;

        // Guard against re-entrant substitutions
        this._applyingSubstitution = false;

        // Callbacks — set by the caller after construction
        this.onSchemaListChange = null;   // (schemas: string[]) => void
        this.onParseResult      = null;   // (result: ParseResult) => void
    }

    // ── Initialisation ──────────────────────────────────────────────────

    /**
     * Initialise Monaco and return a promise that resolves when the editor
     * is ready.
     */
    async init() {
        return new Promise((resolve) => {
            require(['vs/editor/editor.main'], () => {
                // Register language + theme (defined in evident-lang.js)
                registerEvidentLanguage(monaco);

                this.editor = monaco.editor.create(
                    document.getElementById(this.containerId),
                    {
                        language:             EVIDENT_LANGUAGE_ID,
                        theme:                'evident-dark',
                        value:                DEFAULT_SOURCE,
                        fontSize:             14,
                        fontFamily:           '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
                        fontLigatures:        true,
                        minimap:              { enabled: false },
                        lineNumbers:          'on',
                        glyphMargin:          true,   // needed for glyph decorations
                        folding:              true,
                        wordWrap:             'off',
                        scrollBeyondLastLine: false,
                        automaticLayout:      true,
                        tabSize:              4,
                        insertSpaces:         true,
                        renderWhitespace:     'selection',
                        cursorBlinking:       'smooth',
                        smoothScrolling:      true,
                        contextmenu:          true,
                        padding:              { top: 8, bottom: 8 },
                    },
                );

                // Symbol substitution — fires before the parse debounce
                this._setupSubstitutions();

                // Live parse on content change (debounced)
                this.editor.onDidChangeModelContent(() => {
                    clearTimeout(this._parseTimer);
                    this._parseTimer = setTimeout(() => this._parseSource(), 500);
                });

                // Update status bar cursor position
                this.editor.onDidChangeCursorPosition((e) => {
                    const pos = e.position;
                    const cursorEl = document.getElementById('cursor-position');
                    if (cursorEl) {
                        cursorEl.textContent = `Ln ${pos.lineNumber}, Col ${pos.column}`;
                    }
                });

                // Initial parse
                this._parseSource();
                resolve(this);
            });
        });
    }

    // ── Source access ───────────────────────────────────────────────────

    getSource() {
        return this.editor ? this.editor.getValue() : '';
    }

    setSource(source) {
        if (this.editor) {
            this.editor.setValue(source);
        }
    }

    // ── Parse ────────────────────────────────────────────────────────────

    /**
     * POST the current source to /parse and update decorations + UI.
     */
    async _parseSource() {
        const source = this.getSource();
        const statusEl = document.getElementById('status-text') ||
                         document.getElementById('status-bar');
        try {
            const resp = await fetch('/parse', {
                method:  'POST',
                headers: { 'Content-Type': 'application/json' },
                body:    JSON.stringify({ source }),
            });

            if (!resp.ok) {
                throw new Error(`HTTP ${resp.status}`);
            }

            const data = await resp.json();
            const errors  = data.errors  || [];
            const schemas = data.schemas || [];

            // Update gutter decorations for errors
            this._applyErrorDecorations(errors);

            // Notify schema list listener
            if (this.onSchemaListChange) {
                this.onSchemaListChange(schemas);
            }

            // Notify raw result listener
            if (this.onParseResult) {
                this.onParseResult(data);
            }

            // Update status bar
            if (statusEl) {
                if (errors.length === 0) {
                    statusEl.textContent = `✓ ${schemas.length} schema${schemas.length !== 1 ? 's' : ''}`;
                    statusEl.parentElement?.classList.remove('status-error');
                    statusEl.parentElement?.classList.add('status-ok');
                    const sb = document.getElementById('status-bar');
                    if (sb) { sb.style.cursor = ''; sb.onclick = null; }
                    document.getElementById('error-modal')?.remove();
                } else {
                    const first = errors[0].message || '';
                    const summary = first.length > 80 ? first.slice(0, 80) + '…' : first;
                    statusEl.textContent = `✗ ${errors.length} error${errors.length !== 1 ? 's' : ''}: ${summary}`;
                    statusEl.parentElement?.classList.remove('status-ok');
                    statusEl.parentElement?.classList.add('status-error');
                    _setStatusErrors(errors);
                }
            }
        } catch (e) {
            // Backend unreachable (e.g. during dev with no server)
            if (statusEl) {
                statusEl.textContent = 'No connection to backend';
                statusEl.parentElement?.classList.remove('status-ok', 'status-error');
                statusEl.parentElement?.classList.add('status-idle');
            }
        }
    }

    // ── Error modal ──────────────────────────────────────────────────────

    _setStatusErrors(errors) {
        const statusBar = document.getElementById('status-bar');
        if (!statusBar) return;
        statusBar.style.cursor = 'pointer';
        statusBar.onclick = () => this._openErrorModal(errors);
    }

    _openErrorModal(errors) {
        // Remove any existing modal
        document.getElementById('error-modal')?.remove();

        const overlay = document.createElement('div');
        overlay.id = 'error-modal';
        overlay.style.cssText = `
            position: fixed; inset: 0; z-index: 9999;
            background: rgba(0,0,0,0.6);
            display: flex; align-items: center; justify-content: center;
        `;

        const box = document.createElement('div');
        box.style.cssText = `
            background: var(--bg-surface, #1e1e2e);
            border: 1px solid var(--border, #45475a);
            border-radius: 8px;
            max-width: min(700px, 90vw);
            max-height: 80vh;
            width: 100%;
            display: flex; flex-direction: column;
            box-shadow: 0 8px 32px rgba(0,0,0,0.5);
            font-family: var(--font-mono, monospace);
        `;

        // Header
        const header = document.createElement('div');
        header.style.cssText = `
            display: flex; align-items: center; justify-content: space-between;
            padding: 12px 16px; border-bottom: 1px solid var(--border, #45475a);
            font-size: 13px; font-weight: 600; color: var(--red, #f38ba8);
        `;
        header.innerHTML = `<span>✗ ${errors.length} error${errors.length !== 1 ? 's' : ''}</span>`;

        const closeBtn = document.createElement('button');
        closeBtn.textContent = '✕';
        closeBtn.style.cssText = `
            background: none; border: none; color: var(--fg-muted, #6c7086);
            cursor: pointer; font-size: 14px; padding: 0 4px; line-height: 1;
        `;
        closeBtn.onclick = () => overlay.remove();
        header.appendChild(closeBtn);
        box.appendChild(header);

        // Error list
        const body = document.createElement('div');
        body.style.cssText = `overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 16px;`;

        errors.forEach((err, i) => {
            const block = document.createElement('div');

            // Location badge
            if (err.line != null) {
                const loc = document.createElement('div');
                loc.style.cssText = `font-size: 11px; color: var(--fg-muted, #6c7086); margin-bottom: 4px;`;
                loc.textContent = `Line ${err.line}${err.col != null ? ', Col ' + err.col : ''}`;
                block.appendChild(loc);
            }

            // Message
            const msg = document.createElement('pre');
            msg.style.cssText = `
                margin: 0; white-space: pre-wrap; word-break: break-word;
                font-size: 12px; color: var(--fg, #cdd6f4);
                line-height: 1.5;
            `;
            msg.textContent = err.message || String(err);
            block.appendChild(msg);

            if (i < errors.length - 1) {
                block.style.borderBottom = '1px solid var(--border, #45475a)';
                block.style.paddingBottom = '16px';
            }

            body.appendChild(block);
        });

        box.appendChild(body);
        overlay.appendChild(box);
        document.body.appendChild(overlay);

        // Dismiss on backdrop click or Escape
        overlay.addEventListener('click', e => { if (e.target === overlay) overlay.remove(); });
        document.addEventListener('keydown', function esc(e) {
            if (e.key === 'Escape') { overlay.remove(); document.removeEventListener('keydown', esc); }
        });
    }

    // ── Symbol substitutions ─────────────────────────────────────────────

    /**
     * Real-time keyword → symbol replacement (LaTeX-style).
     *
     * Operator pairs (<=, >=, !=, =>) are replaced immediately on the second
     * character.  Word keywords ( in , not in , subset , superset , mapsto )
     * are replaced when the user types the trailing space that completes them.
     *
     * Longer patterns are checked first so "not in" wins over "in".
     * A re-entrancy guard prevents the programmatic edit from triggering
     * another substitution pass.
     */
    _setupSubstitutions() {
        // [match string, replacement string]
        // Order: longest first to avoid partial matches.
        // Word keywords: trigger on trailing space; must be at a word boundary
        // (preceded by a non-alphanumeric char or start of line) so that
        // partial matches inside identifiers (e.g. "island" ↛ "isl∧") are safe.
        // Longest keywords first to avoid 'in' matching before 'not in'.
        const WORD_SUBS = [
            ['not in',      '∉ '],
            ['superset',    '⊇ '],
            ['subset',      '⊆ '],
            ['intersection','∩ '],
            ['union',       '∪ '],
            ['mapsto',      '↦ '],
            ['and',         '∧ '],
            ['or',          '∨ '],
            ['in',          '∈ '],
        ];

        // Operator pairs: trigger immediately on the second character.
        const OP_SUBS = [
            ['!=', '≠'],
            ['<=', '≤'],
            ['>=', '≥'],
            ['=>', '⇒'],
        ];

        this.editor.onDidChangeModelContent((e) => {
            if (this._applyingSubstitution) return;

            // Only react to single typed characters (ignore paste, undo, etc.)
            if (e.changes.length !== 1) return;
            const change = e.changes[0];
            if (change.text.length !== 1) return;

            const pos    = this.editor.getPosition();
            const model  = this.editor.getModel();
            const line   = model.getLineContent(pos.lineNumber);
            const before = line.slice(0, pos.column - 1);

            // ── Word keywords (trigger on space) ─────────────────────────
            if (change.text === ' ') {
                for (const [kw, sym] of WORD_SUBS) {
                    const full = kw + ' ';
                    if (!before.endsWith(full)) continue;
                    // Word boundary: char before the keyword must be non-alphanumeric
                    // or there must be no char (start of line / only whitespace before).
                    const prevIdx  = before.length - full.length - 1;
                    const prevChar = prevIdx >= 0 ? before[prevIdx] : null;
                    if (prevChar !== null && /[a-zA-Z0-9_]/.test(prevChar)) continue;

                    const startCol = pos.column - full.length;
                    this._applyingSubstitution = true;
                    this.editor.executeEdits('symbol-sub', [{
                        range: new monaco.Range(pos.lineNumber, startCol,
                                               pos.lineNumber, pos.column),
                        text: sym,
                    }]);
                    this.editor.setPosition({ lineNumber: pos.lineNumber,
                                             column: startCol + sym.length });
                    this._applyingSubstitution = false;
                    return;
                }
            }

            // ── Operator pairs (trigger on 2nd character) ─────────────────
            for (const [op, sym] of OP_SUBS) {
                if (!before.endsWith(op)) continue;
                const startCol = pos.column - op.length;
                this._applyingSubstitution = true;
                this.editor.executeEdits('symbol-sub', [{
                    range: new monaco.Range(pos.lineNumber, startCol,
                                           pos.lineNumber, pos.column),
                    text: sym,
                }]);
                this.editor.setPosition({ lineNumber: pos.lineNumber,
                                         column: startCol + sym.length });
                this._applyingSubstitution = false;
                return;
            }
        });
    }

    // ── Error decorations ────────────────────────────────────────────────

    /**
     * Show red squiggles and glyph circles for parse errors.
     *
     * @param {Array<{line: number, col?: number, message: string}>} errors
     */
    _applyErrorDecorations(errors) {
        const newDecorations = errors.map((err) => {
            const line = err.line || 1;
            const col  = err.col  || 1;
            return {
                range: new monaco.Range(line, col, line, 1000),
                options: {
                    isWholeLine:           false,
                    className:             'error-line',
                    glyphMarginClassName:  'glyph-error',
                    glyphMarginHoverMessage: {
                        value:     `**Parse error**: ${err.message}`,
                        isTrusted: true,
                    },
                    hoverMessage: {
                        value:     `**Parse error**: ${err.message}`,
                        isTrusted: true,
                    },
                    overviewRuler: {
                        color:    'rgba(243, 139, 168, 0.8)',   // --red
                        position: monaco.editor.OverviewRulerLane.Right,
                    },
                },
            };
        });

        this._errorDecorations = this.editor.deltaDecorations(
            this._errorDecorations,
            newDecorations,
        );
    }

    // ── Constraint glyph decorations ─────────────────────────────────────

    /**
     * Apply per-constraint glyph decorations after evaluation.
     *
     * @param {Array<{line: number, status: 'sat'|'unsat'|'tight'|'unknown', message?: string}>} statuses
     */
    setConstraintGlyphs(statuses) {
        const newDecorations = statuses.map((cs) => ({
            range: new monaco.Range(cs.line, 1, cs.line, 1),
            options: {
                glyphMarginClassName:  `glyph-${cs.status}`,
                glyphMarginHoverMessage: {
                    value:     cs.message ? `**${cs.status}**: ${cs.message}` : `**${cs.status}**`,
                    isTrusted: true,
                },
            },
        }));

        this._glyphDecorations = this.editor.deltaDecorations(
            this._glyphDecorations,
            newDecorations,
        );
    }

    /**
     * Clear all glyph decorations (e.g. after the schema changes).
     */
    clearGlyphs() {
        this._glyphDecorations = this.editor.deltaDecorations(
            this._glyphDecorations,
            [],
        );
    }

    // ── Diagnostics (Monaco markers) ────────────────────────────────────

    /**
     * Set Monaco model markers (yellow/red underlines in editor and Problems
     * panel).  Complementary to glyph decorations.
     *
     * @param {Array<{line: number, col?: number, endLine?: number, endCol?: number,
     *                message: string, severity?: 'error'|'warning'|'info'}>} diagnostics
     */
    setMarkers(diagnostics) {
        const model = this.editor.getModel();
        if (!model) return;

        const markers = diagnostics.map((d) => ({
            startLineNumber: d.line    || 1,
            startColumn:     d.col     || 1,
            endLineNumber:   d.endLine || d.line || 1,
            endColumn:       d.endCol  || 1000,
            message:         d.message,
            severity: {
                error:   monaco.MarkerSeverity.Error,
                warning: monaco.MarkerSeverity.Warning,
                info:    monaco.MarkerSeverity.Info,
            }[d.severity || 'error'] ?? monaco.MarkerSeverity.Error,
        }));

        monaco.editor.setModelMarkers(model, 'evident', markers);
    }

    clearMarkers() {
        const model = this.editor.getModel();
        if (model) {
            monaco.editor.setModelMarkers(model, 'evident', []);
        }
    }

    // ── Utility ──────────────────────────────────────────────────────────

    /** Focus the editor */
    focus() {
        this.editor?.focus();
    }

    /** Force a re-layout (useful after the panel is resized) */
    layout() {
        this.editor?.layout();
    }
}
