// Variable range visualization

async function renderRanges(source, schemaName, given) {
    const container = document.getElementById('ranges-display');
    container.innerHTML = '<div class="loading">Computing ranges...</div>';

    try {
        const resp = await fetch('/ranges', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ source, schema: schemaName, given }),
        });
        const data = await resp.json();
        const ranges = data.ranges || {};

        container.innerHTML = '';

        for (const [name, info] of Object.entries(ranges)) {
            const row = document.createElement('div');
            row.className = 'range-row';

            if (info.fixed !== undefined) {
                // Variable is pinned in `given`
                row.innerHTML = `
                    <div class="range-name">${name}</div>
                    <div class="range-fixed">= ${info.fixed}</div>
                    <div class="range-type">${info.type || ''}</div>
                `;
            } else if (info.min !== null && info.max !== null &&
                       info.min !== undefined && info.max !== undefined) {
                // Bounded numeric range — show a proportional fill bar
                const span = info.max - info.min;
                // The bar always fills the full container but shows the label span.
                // A non-zero span fills 100%; a degenerate single-point range is shown
                // at 100% width with equal min/max labels.
                const fillPct = span > 0 ? 100 : 50;

                row.innerHTML = `
                    <div class="range-name">${name}</div>
                    <div class="range-bar-container">
                        <span class="range-lo">${info.min}</span>
                        <div class="range-bar">
                            <div class="range-fill" style="width: ${fillPct}%"></div>
                        </div>
                        <span class="range-hi">${info.max}</span>
                    </div>
                    <div class="range-type">${info.type || ''}</div>
                `;

                // Add an interactive slider for integer types
                if (info.type === 'Nat' || info.type === 'Int') {
                    const sliderWrap = document.createElement('div');
                    sliderWrap.className = 'range-slider-wrap';

                    const slider = document.createElement('input');
                    slider.type = 'range';
                    slider.min = info.min;
                    slider.max = info.max;
                    slider.step = 1;
                    slider.value = Math.floor((info.min + info.max) / 2);
                    slider.className = 'range-slider';
                    slider.title = `${name}: ${slider.value}`;

                    const valueLabel = document.createElement('span');
                    valueLabel.className = 'range-slider-value';
                    valueLabel.textContent = slider.value;

                    slider.addEventListener('input', () => {
                        valueLabel.textContent = slider.value;
                        slider.title = `${name}: ${slider.value}`;
                        // Sync to the binding input for this variable, if present
                        const input = document.querySelector(`.binding-input[data-varname="${name}"]`);
                        if (input) {
                            input.value = slider.value;
                            // Fire a change event so the rest of the UI can react
                            input.dispatchEvent(new Event('change', { bubbles: true }));
                        }
                    });

                    sliderWrap.appendChild(slider);
                    sliderWrap.appendChild(valueLabel);
                    row.appendChild(sliderWrap);
                }
            } else {
                // Unbounded or unsolvable range
                row.innerHTML = `
                    <div class="range-name">${name}</div>
                    <div class="range-unbounded">unbounded</div>
                    <div class="range-type">${info.type || ''}</div>
                `;
            }

            container.appendChild(row);
        }

        if (Object.keys(ranges).length === 0) {
            container.innerHTML = '<div class="empty">No numeric variables found</div>';
        }

    } catch (e) {
        container.innerHTML = `<div class="error">Range computation failed: ${e.message}</div>`;
    }
}

// Refresh ranges and re-render whenever a binding value changes.
// Call this from your binding-change handler:
//   onBindingChange(() => renderRanges(currentSource, currentSchema, currentGiven));
function onBindingChange(callback) {
    document.addEventListener('change', (e) => {
        if (e.target && e.target.classList.contains('binding-input')) {
            callback();
        }
    });
}
