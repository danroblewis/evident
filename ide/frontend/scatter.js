// 2D scatter plot using D3.js
// Renders automatically whenever sample data is available.
// Picks the first two numeric variables with distinct names.

(function () {

function pickAxes(samples) {
    if (!samples || samples.length === 0) return [null, null];
    const vars = Object.keys(samples[0]).filter(k => {
        const v = samples[0][k];
        return typeof v === 'number' && v !== null;
    });
    if (vars.length < 2) return [vars[0] || null, null];
    // Default: first two distinct numeric variables
    return [vars[0], vars[1]];
}

function renderScatterControls(variables) {
    const numericVars = variables; // caller may filter; we accept all
    const container = document.getElementById('scatter-controls');
    container.innerHTML = `
        <label>X <select id="scatter-x">${numericVars.map(v => `<option>${v}</option>`).join('')}</select></label>
        <label>Y <select id="scatter-y">${numericVars.map((v, i) => `<option ${i === 1 ? 'selected' : ''}>${v}</option>`).join('')}</select></label>
    `;
    // Ensure X and Y differ
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

// Cache last samples so redraws (on axis change) don't refetch
let _cachedSamples = [];

function drawScatter(samples) {
    _cachedSamples = samples || [];
    drawFromCache();
}

function drawFromCache() {
    const samples = _cachedSamples;
    const container = document.getElementById('scatter-plot');
    if (!container) return;

    // Pick axes
    const xSel = document.getElementById('scatter-x');
    const ySel = document.getElementById('scatter-y');
    let xVar = xSel ? xSel.value : null;
    let yVar = ySel ? ySel.value : null;

    // Fallback: auto-pick from data
    if (!xVar || !yVar || xVar === yVar) {
        const [ax, ay] = pickAxes(samples);
        xVar = ax; yVar = ay;
        if (xSel && ax) xSel.value = ax;
        if (ySel && ay) ySel.value = ay;
    }

    if (!xVar || !yVar || samples.length === 0) {
        container.innerHTML = '<p style="color:#6c7086;padding:8px;">Waiting for samples…</p>';
        return;
    }

    const pts = samples
        .filter(s => s[xVar] != null && s[yVar] != null)
        .map(s => ({ x: +s[xVar], y: +s[yVar], label: Object.entries(s).map(([k,v])=>`${k}:${v}`).join(' ') }));

    if (pts.length === 0) {
        container.innerHTML = `<p style="color:#6c7086;padding:8px;">No numeric data for ${xVar} / ${yVar}</p>`;
        return;
    }

    // Clear & draw with D3
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

    const xExt = d3.extent(pts, d => d.x);
    const yExt = d3.extent(pts, d => d.y);
    const pad = v => v === 0 ? 1 : Math.abs(v) * 0.15;
    const xScale = d3.scaleLinear().domain([xExt[0]-pad(xExt[0]), xExt[1]+pad(xExt[1])]).range([0, iW]);
    const yScale = d3.scaleLinear().domain([yExt[0]-pad(yExt[0]), yExt[1]+pad(yExt[1])]).range([iH, 0]);

    // Grid
    g.append('g').attr('class', 'grid')
        .selectAll('line.vert')
        .data(xScale.ticks(5)).enter().append('line')
        .attr('x1', d => xScale(d)).attr('x2', d => xScale(d))
        .attr('y1', 0).attr('y2', iH)
        .attr('stroke', '#313244').attr('stroke-width', 1);

    g.append('g').attr('class', 'grid')
        .selectAll('line.horiz')
        .data(yScale.ticks(5)).enter().append('line')
        .attr('x1', 0).attr('x2', iW)
        .attr('y1', d => yScale(d)).attr('y2', d => yScale(d))
        .attr('stroke', '#313244').attr('stroke-width', 1);

    // Axes
    g.append('g').attr('transform', `translate(0,${iH})`)
        .call(d3.axisBottom(xScale).ticks(5))
        .attr('color', '#6c7086');

    g.append('g')
        .call(d3.axisLeft(yScale).ticks(5))
        .attr('color', '#6c7086');

    // Axis labels
    g.append('text').attr('x', iW/2).attr('y', iH+35)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(xVar);
    g.append('text').attr('transform','rotate(-90)')
        .attr('x', -iH/2).attr('y', -36)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(yVar);

    // Points
    const tooltip = d3.select(container).append('div')
        .style('position','absolute').style('background','#313244')
        .style('color','#cdd6f4').style('padding','6px 10px')
        .style('border-radius','4px').style('font-size','12px')
        .style('pointer-events','none').style('display','none');

    g.selectAll('circle').data(pts).enter().append('circle')
        .attr('cx', d => xScale(d.x)).attr('cy', d => yScale(d.y))
        .attr('r', 6).attr('fill', '#89b4fa').attr('opacity', 0.75)
        .attr('stroke', '#cba6f7').attr('stroke-width', 1)
        .on('mouseover', (event, d) => {
            tooltip.style('display','block').html(d.label.replace(/ /g,'<br>'));
        })
        .on('mousemove', event => {
            tooltip.style('left', (event.offsetX+12)+'px').style('top', (event.offsetY-10)+'px');
        })
        .on('mouseout', () => tooltip.style('display','none'));
}

window.renderScatterControls = renderScatterControls;
window.drawScatter = drawScatter;

})();
