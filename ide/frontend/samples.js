// Sample vectors table

// Accumulated sample set — grows across runs, cleared when source/schema changes.
let _allSamples = [];
const _seenKeys  = new Set();

function clearAccumulatedSamples() {
    _allSamples = [];
    _seenKeys.clear();
    const countEl = document.getElementById('sample-count');
    if (countEl) countEl.textContent = '';
    const container = document.getElementById('samples-table-container');
    if (container) container.innerHTML =
        '<p class="samples-empty">Click <strong>Sample</strong> to generate valid assignments.</p>';
    if (typeof drawScatter === 'function') drawScatter([]);
}

function _mergeSamples(incoming) {
    const added = [];
    for (const s of incoming) {
        const key = JSON.stringify(Object.entries(s).sort());
        if (!_seenKeys.has(key)) {
            _seenKeys.add(key);
            _allSamples.push(s);
            added.push(s);
        }
    }
    return added;
}

async function renderSamples(source, schemaName, given, n = 5, strategy = 'random') {
    const container = document.getElementById('samples-table-container');
    container.innerHTML = '<div class="loading">Sampling...</div>';

    try {
        const resp = await fetch('/sample', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ source, schema: schemaName, given, n, strategy }),
        });
        const data = await resp.json();
        const incoming = data.samples || [];

        // Merge new samples into the accumulated set
        const newlyAdded = _mergeSamples(incoming);
        const samples = incoming;   // table shows latest batch

        const total = _allSamples.length;
        document.getElementById('sample-count').textContent =
            newlyAdded.length < total ? `(${total} total)` : `(${total})`;

        if (samples.length === 0) {
            container.innerHTML = '<div class="empty">No valid assignments found</div>';
            return;
        }

        // Build table
        const vars = Object.keys(samples[0]);
        const table = document.createElement('table');
        table.className = 'samples-table';

        // Header
        const thead = document.createElement('thead');
        thead.innerHTML = `<tr>
            <th>#</th>
            ${vars.map(v => `<th class="col-header" data-col="${v}">${v}</th>`).join('')}
            <th>Pin</th>
        </tr>`;
        table.appendChild(thead);

        // Sort state
        let sortCol = null;
        let sortAsc = true;
        let currentSamples = samples.slice();

        function rebuildRows(data) {
            tbody.innerHTML = '';
            data.forEach((sample, i) => {
                const tr = document.createElement('tr');
                tr.className = 'sample-row';
                tr.innerHTML = `
                    <td class="sample-index">${i + 1}</td>
                    ${vars.map(v => `<td class="sample-val">${formatVal(sample[v])}</td>`).join('')}
                    <td>
                        <button class="pin-btn" title="Pin these values">📌</button>
                    </td>
                `;

                // Pin button: fill binding inputs with this sample
                tr.querySelector('.pin-btn').addEventListener('click', () => {
                    pinSample(sample);
                    // Mark the row pinned
                    tbody.querySelectorAll('tr.pinned').forEach(r => r.classList.remove('pinned'));
                    tr.classList.add('pinned');
                });

                // Hover: highlight row
                tr.addEventListener('mouseenter', () => tr.classList.add('hovered'));
                tr.addEventListener('mouseleave', () => tr.classList.remove('hovered'));

                tbody.appendChild(tr);
            });
        }

        // Column sort handlers
        thead.querySelectorAll('.col-header').forEach(th => {
            th.style.cursor = 'pointer';
            th.addEventListener('click', () => {
                const col = th.dataset.col;
                if (sortCol === col) {
                    sortAsc = !sortAsc;
                } else {
                    sortCol = col;
                    sortAsc = true;
                }
                // Update header indicators
                thead.querySelectorAll('.col-header').forEach(h => {
                    h.classList.remove('sort-asc', 'sort-desc');
                });
                th.classList.add(sortAsc ? 'sort-asc' : 'sort-desc');

                currentSamples = currentSamples.slice().sort((a, b) => {
                    const av = a[col], bv = b[col];
                    if (av === null || av === undefined) return 1;
                    if (bv === null || bv === undefined) return -1;
                    if (typeof av === 'number' && typeof bv === 'number') {
                        return sortAsc ? av - bv : bv - av;
                    }
                    return sortAsc
                        ? String(av).localeCompare(String(bv))
                        : String(bv).localeCompare(String(av));
                });
                rebuildRows(currentSamples);
            });
        });

        // Rows
        const tbody = document.createElement('tbody');
        table.appendChild(tbody);
        rebuildRows(currentSamples);

        container.innerHTML = '';
        container.appendChild(table);

        // Export CSV button
        const exportBtn = document.getElementById('export-csv-btn');
        if (exportBtn) {
            exportBtn.onclick = () => exportCSV(vars, currentSamples, schemaName);
        }

        // Pass all vars so scatter can offer enum axes too.
        // Draw scatter from the full accumulated set so the plot grows over time.
        if (typeof renderScatterControls === 'function') {
            renderScatterControls(vars, _allSamples);
        }
        if (typeof drawScatter === 'function') {
            drawScatter(_allSamples);
        }

        return samples;

    } catch (e) {
        container.innerHTML = `<div class="error">Sampling failed: ${e.message}</div>`;
        return [];
    }
}

function formatVal(v) {
    if (v === null || v === undefined) return '—';
    if (typeof v === 'number') return v.toLocaleString();
    return String(v);
}

function pinSample(sample) {
    // Fill all binding inputs with this sample's values
    for (const [name, val] of Object.entries(sample)) {
        const input = document.querySelector(`.binding-input[data-varname="${name}"]`);
        if (input && !input.disabled) {
            input.value = String(val);
        }
    }
    // Prefer the status-bar notification from schema-panel.js if available
    const notify = window.showNotification || showNotification;
    notify('Values pinned to bindings ✓', 'success');
}

function exportCSV(vars, samples, schemaName) {
    const header = ['#', ...vars].join(',');
    const rows = samples.map((s, i) =>
        [i + 1, ...vars.map(v => {
            const val = s[v];
            if (val === null || val === undefined) return '';
            const str = String(val);
            // Quote values containing commas or quotes
            if (str.includes(',') || str.includes('"') || str.includes('\n')) {
                return '"' + str.replace(/"/g, '""') + '"';
            }
            return str;
        })].join(',')
    );
    const csv = [header, ...rows].join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${schemaName || 'samples'}.csv`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

// Notification helper (used by pinSample; may also be defined elsewhere)
function showNotification(message, type = 'info') {
    // If a global showNotification is already defined by another module, skip.
    if (window._notificationShowing) return;
    window._notificationShowing = true;

    let el = document.getElementById('notification-toast');
    if (!el) {
        el = document.createElement('div');
        el.id = 'notification-toast';
        el.style.cssText = `
            position: fixed; bottom: 1.5rem; right: 1.5rem;
            padding: 0.6rem 1.2rem; border-radius: 6px;
            font-size: 0.9rem; z-index: 9999;
            transition: opacity 0.3s ease;
        `;
        document.body.appendChild(el);
    }

    el.textContent = message;
    el.style.opacity = '1';
    el.style.background = type === 'success' ? '#2d6a4f' : '#1a5276';
    el.style.color = '#fff';

    clearTimeout(el._timeout);
    el._timeout = setTimeout(() => {
        el.style.opacity = '0';
        window._notificationShowing = false;
    }, 2500);
}
