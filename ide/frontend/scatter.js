// 2D scatter plot / strip plot using D3.js
// - 2+ numeric vars: scatter plot
// - 1 numeric var: strip/dot plot (jittered), colored by enum var if present

(function () {

const PALETTE = ['#89b4fa','#a6e3a1','#fab387','#f38ba8','#cba6f7','#94e2d5','#f9e2af','#74c7ec'];

function pickAxes(samples) {
    if (!samples || samples.length === 0) return [null, null];
    const vars = Object.keys(samples[0]).filter(k => {
        const v = samples[0][k];
        return typeof v === 'number' && v !== null;
    });
    if (vars.length < 2) return [vars[0] || null, null];
    return [vars[0], vars[1]];
}

function pickColorVar(samples) {
    if (!samples || samples.length === 0) return null;
    // First string-valued variable — likely an enum
    return Object.keys(samples[0]).find(k => typeof samples[0][k] === 'string') || null;
}

function renderScatterControls(variables) {
    const numericVars = variables;
    const container = document.getElementById('scatter-controls');
    if (numericVars.length === 0) {
        container.innerHTML = `<span class="samples-empty">No numeric variables to plot.</span>`;
        return;
    }
    if (numericVars.length === 1) {
        // Strip plot mode — only an X selector
        container.innerHTML = `
            <label>X <select id="scatter-x"><option>${numericVars[0]}</option></select></label>
            <span style="color:var(--fg-muted);font-size:11px;margin-left:8px;">strip plot</span>
        `;
        document.getElementById('scatter-x').addEventListener('change', drawFromCache);
        return;
    }
    container.innerHTML = `
        <label>X <select id="scatter-x">${numericVars.map(v => `<option>${v}</option>`).join('')}</select></label>
        <label>Y <select id="scatter-y">${numericVars.map((v, i) => `<option ${i === 1 ? 'selected' : ''}>${v}</option>`).join('')}</select></label>
    `;
    const sx = document.getElementById('scatter-x');
    const sy = document.getElementById('scatter-y');
    function syncSelects() {
        if (sx.value === sy.value) {
            const other = numericVars.find(v => v !== sx.value);
            if (other) sy.value = other;
        }
    }
    sx.addEventListener('change', () => { syncSelects(); drawFromCache(); });
    sy.addEventListener('change', () => { drawFromCache(); });
    syncSelects();
}

// Cache last samples so redraws on axis change don't refetch
let _cachedSamples = [];

function drawScatter(samples) {
    _cachedSamples = samples || [];
    drawFromCache();
}

function drawFromCache() {
    const samples = _cachedSamples;
    const container = document.getElementById('scatter-plot');
    if (!container) return;

    const xSel = document.getElementById('scatter-x');
    const ySel = document.getElementById('scatter-y');
    let xVar = xSel ? xSel.value : null;
    let yVar = ySel ? ySel.value : null;

    const numericKeys = samples.length > 0
        ? Object.keys(samples[0]).filter(k => typeof samples[0][k] === 'number' && samples[0][k] !== null)
        : [];

    // Validate selects against current sample keys
    if (!numericKeys.includes(xVar)) xVar = null;
    if (!numericKeys.includes(yVar)) yVar = null;

    if (!xVar) {
        const [ax, ay] = pickAxes(samples);
        xVar = ax; yVar = ay;
        if (xSel && ax) xSel.value = ax;
        if (ySel && ay) ySel.value = ay;
    }

    if (!xVar || samples.length === 0) {
        container.innerHTML = '<p style="color:#6c7086;padding:8px;">Waiting for samples…</p>';
        return;
    }

    if (!yVar) {
        drawStripPlot(samples, xVar, container);
    } else {
        drawScatterPlot(samples, xVar, yVar, container);
    }
}

// ---------------------------------------------------------------------------
// Strip plot: enum variable as columns (X), numeric variable as values (Y)
// ---------------------------------------------------------------------------

function drawStripPlot(samples, numVar, container) {
    const catVar = pickColorVar(samples);  // first string-valued variable

    container.innerHTML = '';
    const W = container.clientWidth || 320;
    const H = 260;
    const M = { top: 20, right: 20, bottom: 40, left: 48 };
    const iW = W - M.left - M.right;
    const iH = H - M.top - M.bottom;

    const svg = d3.select(container).append('svg')
        .attr('width', W).attr('height', H)
        .style('background', '#181825');
    const g = svg.append('g').attr('transform', `translate(${M.left},${M.top})`);

    const tooltip = d3.select(container).append('div')
        .style('position', 'absolute').style('background', '#313244')
        .style('color', '#cdd6f4').style('padding', '6px 10px')
        .style('border-radius', '4px').style('font-size', '12px')
        .style('pointer-events', 'none').style('display', 'none')
        .style('white-space', 'pre');

    if (catVar) {
        // Categorical X, numeric Y — proper strip plot
        const categories = [...new Set(samples.map(s => s[catVar]).filter(v => v != null))].sort();
        const pts = samples
            .filter(s => s[numVar] != null && s[catVar] != null)
            .map(s => ({ cat: s[catVar], y: +s[numVar],
                         label: Object.entries(s).map(([k,v]) => `${k}: ${v}`).join('\n') }));

        const xScale = d3.scaleBand().domain(categories).range([0, iW]).padding(0.3);
        const yExt = d3.extent(pts, d => d.y);
        const pad = v => v === 0 ? 1 : Math.abs(v) * 0.1;
        const yScale = d3.scaleLinear()
            .domain([yExt[0] - pad(yExt[0]), yExt[1] + pad(yExt[1])])
            .range([iH, 0]);

        // Horizontal grid
        g.selectAll('line.grid')
            .data(yScale.ticks(5)).enter().append('line')
            .attr('x1', 0).attr('x2', iW)
            .attr('y1', d => yScale(d)).attr('y2', d => yScale(d))
            .attr('stroke', '#313244').attr('stroke-width', 1);

        // Axes
        g.append('g').attr('transform', `translate(0,${iH})`)
            .call(d3.axisBottom(xScale)).attr('color', '#6c7086');
        g.append('g')
            .call(d3.axisLeft(yScale).ticks(5)).attr('color', '#6c7086');

        // Y axis label
        g.append('text').attr('transform', 'rotate(-90)')
            .attr('x', -iH / 2).attr('y', -36)
            .attr('text-anchor', 'middle').attr('fill', '#cdd6f4').attr('font-size', 12)
            .text(numVar);

        // Dots — small deterministic jitter on X within the band so they don't stack
        const bw = xScale.bandwidth();
        const jitter = (i) => (((i * 2654435761) % 1000) / 1000 - 0.5) * bw * 0.5;

        g.selectAll('circle').data(pts).enter().append('circle')
            .attr('cx', (d, i) => xScale(d.cat) + bw / 2 + jitter(i))
            .attr('cy', d => yScale(d.y))
            .attr('r', 5)
            .attr('fill', (d, i) => PALETTE[categories.indexOf(d.cat) % PALETTE.length])
            .attr('opacity', 0.85)
            .attr('stroke', '#1e1e2e').attr('stroke-width', 1)
            .on('mouseover', (event, d) => tooltip.style('display', 'block').text(d.label))
            .on('mousemove', event => tooltip.style('left', (event.offsetX+12)+'px').style('top', (event.offsetY-10)+'px'))
            .on('mouseout', () => tooltip.style('display', 'none'));

    } else {
        // No categorical variable — single column, numeric values on Y
        const pts = samples.filter(s => s[numVar] != null)
            .map(s => ({ y: +s[numVar], label: Object.entries(s).map(([k,v]) => `${k}: ${v}`).join('\n') }));

        const yExt = d3.extent(pts, d => d.y);
        const pad = v => v === 0 ? 1 : Math.abs(v) * 0.1;
        const yScale = d3.scaleLinear()
            .domain([yExt[0] - pad(yExt[0]), yExt[1] + pad(yExt[1])])
            .range([iH, 0]);

        g.selectAll('line.grid')
            .data(yScale.ticks(5)).enter().append('line')
            .attr('x1', 0).attr('x2', iW)
            .attr('y1', d => yScale(d)).attr('y2', d => yScale(d))
            .attr('stroke', '#313244').attr('stroke-width', 1);

        g.append('g').call(d3.axisLeft(yScale).ticks(5)).attr('color', '#6c7086');
        g.append('text').attr('transform', 'rotate(-90)')
            .attr('x', -iH / 2).attr('y', -36)
            .attr('text-anchor', 'middle').attr('fill', '#cdd6f4').attr('font-size', 12)
            .text(numVar);

        const jitter = (i) => (((i * 2654435761) % 1000) / 1000 - 0.5) * iW * 0.4;

        g.selectAll('circle').data(pts).enter().append('circle')
            .attr('cx', (d, i) => iW / 2 + jitter(i))
            .attr('cy', d => yScale(d.y))
            .attr('r', 5).attr('fill', '#89b4fa').attr('opacity', 0.85)
            .attr('stroke', '#1e1e2e').attr('stroke-width', 1)
            .on('mouseover', (event, d) => tooltip.style('display', 'block').text(d.label))
            .on('mousemove', event => tooltip.style('left', (event.offsetX+12)+'px').style('top', (event.offsetY-10)+'px'))
            .on('mouseout', () => tooltip.style('display', 'none'));
    }
}

// ---------------------------------------------------------------------------
// Scatter plot: two numeric variables
// ---------------------------------------------------------------------------

function drawScatterPlot(samples, xVar, yVar, container) {
    const colorVar = pickColorVar(samples);
    const pts = samples
        .filter(s => s[xVar] != null && s[yVar] != null)
        .map(s => ({ x: +s[xVar], y: +s[yVar], color: colorVar ? s[colorVar] : null,
                     label: Object.entries(s).map(([k,v]) => `${k}: ${v}`).join('\n') }));

    if (pts.length === 0) {
        container.innerHTML = `<p style="color:#6c7086;padding:8px;">No data for ${xVar} / ${yVar}</p>`;
        return;
    }

    container.innerHTML = '';

    const W = container.clientWidth || 320;
    const H = 260;
    const M = { top: 20, right: 20, bottom: 40, left: 48 };
    const iW = W - M.left - M.right;
    const iH = H - M.top - M.bottom;

    const categories = colorVar ? [...new Set(pts.map(d => d.color))] : [];
    const colorOf = d => {
        if (!d.color) return '#89b4fa';
        return PALETTE[categories.indexOf(d.color) % PALETTE.length];
    };

    const xExt = d3.extent(pts, d => d.x);
    const yExt = d3.extent(pts, d => d.y);
    const pad = v => v === 0 ? 1 : Math.abs(v) * 0.15;
    const xScale = d3.scaleLinear().domain([xExt[0]-pad(xExt[0]), xExt[1]+pad(xExt[1])]).range([0, iW]);
    const yScale = d3.scaleLinear().domain([yExt[0]-pad(yExt[0]), yExt[1]+pad(yExt[1])]).range([iH, 0]);

    const svg = d3.select(container).append('svg')
        .attr('width', W).attr('height', H)
        .style('background', '#181825');

    const g = svg.append('g').attr('transform', `translate(${M.left},${M.top})`);

    g.append('g').selectAll('line.vert')
        .data(xScale.ticks(5)).enter().append('line')
        .attr('x1', d => xScale(d)).attr('x2', d => xScale(d))
        .attr('y1', 0).attr('y2', iH)
        .attr('stroke', '#313244').attr('stroke-width', 1);

    g.append('g').selectAll('line.horiz')
        .data(yScale.ticks(5)).enter().append('line')
        .attr('x1', 0).attr('x2', iW)
        .attr('y1', d => yScale(d)).attr('y2', d => yScale(d))
        .attr('stroke', '#313244').attr('stroke-width', 1);

    g.append('g').attr('transform', `translate(0,${iH})`)
        .call(d3.axisBottom(xScale).ticks(5)).attr('color', '#6c7086');
    g.append('g')
        .call(d3.axisLeft(yScale).ticks(5)).attr('color', '#6c7086');

    g.append('text').attr('x', iW/2).attr('y', iH+35)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(xVar);
    g.append('text').attr('transform','rotate(-90)')
        .attr('x', -iH/2).attr('y', -36)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(yVar);

    const tooltip = d3.select(container).append('div')
        .style('position','absolute').style('background','#313244')
        .style('color','#cdd6f4').style('padding','6px 10px')
        .style('border-radius','4px').style('font-size','12px')
        .style('pointer-events','none').style('display','none')
        .style('white-space', 'pre');

    g.selectAll('circle').data(pts).enter().append('circle')
        .attr('cx', d => xScale(d.x)).attr('cy', d => yScale(d.y))
        .attr('r', 6).attr('fill', d => colorOf(d)).attr('opacity', 0.8)
        .attr('stroke', '#1e1e2e').attr('stroke-width', 1)
        .on('mouseover', (event, d) => tooltip.style('display','block').text(d.label))
        .on('mousemove', event => tooltip.style('left', (event.offsetX+12)+'px').style('top', (event.offsetY-10)+'px'))
        .on('mouseout', () => tooltip.style('display','none'));

    // Legend
    if (categories.length > 0) {
        const lx = iW - 4;
        categories.forEach((cat, i) => {
            g.append('circle').attr('cx', lx - 80).attr('cy', 6 + i * 16)
                .attr('r', 5).attr('fill', PALETTE[i % PALETTE.length]);
            g.append('text').attr('x', lx - 70).attr('y', 10 + i * 16)
                .attr('fill', '#cdd6f4').attr('font-size', 11).text(cat);
        });
    }
}

window.renderScatterControls = renderScatterControls;
window.drawScatter = drawScatter;

})();
