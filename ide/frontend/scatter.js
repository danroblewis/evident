// 2D plot using D3.js
// Plot type is inferred from the selected axis variable types:
//   numeric × numeric  → scatter plot
//   enum    × numeric  → strip plot (enum columns, numeric Y)
//   numeric × enum     → strip plot (same, axes swapped automatically)
// Optional: color by any variable, size by numeric variable.

(function () {

const PALETTE = ['#89b4fa','#a6e3a1','#fab387','#f38ba8','#cba6f7','#94e2d5','#f9e2af','#74c7ec'];
const NONE = '';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function varType(name, samples) {
    if (!name || !samples.length || samples[0][name] == null) return null;
    return typeof samples[0][name] === 'number' ? 'numeric' : 'enum';
}

function makeSelect(id, options, selectedVal) {
    const sel = document.createElement('select');
    sel.id = id;
    sel.className = 'axis-select';
    options.forEach(({ value, label }) => {
        const opt = document.createElement('option');
        opt.value = value;
        opt.textContent = label;
        if (value === selectedVal) opt.selected = true;
        sel.appendChild(opt);
    });
    return sel;
}

function makeGroup(labelText, selectEl) {
    const wrap = document.createElement('label');
    wrap.style.cssText = 'display:flex;align-items:center;gap:4px;';
    const lbl = document.createElement('span');
    lbl.className = 'scatter-axis-label';
    lbl.textContent = labelText;
    wrap.appendChild(lbl);
    wrap.appendChild(selectEl);
    return wrap;
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let _cachedSamples = [];
let _allVars = [];

// ---------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------

function renderScatterControls(allVars, samples) {
    _allVars = allVars;
    const container = document.getElementById('scatter-controls');
    if (!allVars.length) {
        container.innerHTML = '<span class="samples-empty">Run <strong>Sample</strong> first to populate axes.</span>';
        return;
    }

    const numVars = allVars.filter(v => varType(v, samples) === 'numeric');
    const prev = _readSelects();

    // Build option lists
    const allOpts = allVars.map(v => ({ value: v, label: v }));
    const numOpts = numVars.map(v => ({ value: v, label: v }));
    const noneOpts = [{ value: NONE, label: '—' }, ...allOpts];
    const noneNumOpts = [{ value: NONE, label: '—' }, ...numOpts];

    // Default axis picks
    const defaultX = prev.x && allVars.includes(prev.x) ? prev.x : allVars[0];
    const defaultY = prev.y && allVars.includes(prev.y) ? prev.y
        : allVars.find(v => v !== defaultX) || NONE;

    container.innerHTML = '';
    container.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin-bottom:8px;';

    const typeOpts = [
        { value: 'auto',     label: 'Auto' },
        { value: 'graph',    label: 'Graph' },
        { value: 'relation', label: 'Relation' },
    ];

    const xSel = makeSelect('scatter-x', allOpts, defaultX);
    const ySel = makeSelect('scatter-y', [{ value: NONE, label: '—' }, ...allOpts], defaultY);
    const tSel = makeSelect('scatter-type', typeOpts, prev.type || 'auto');
    const cSel = makeSelect('scatter-color', noneOpts, prev.color && allVars.includes(prev.color) ? prev.color : NONE);
    const sSel = makeSelect('scatter-size', noneNumOpts, prev.size && numVars.includes(prev.size) ? prev.size : NONE);

    container.appendChild(makeGroup('X', xSel));
    container.appendChild(makeGroup('Y', ySel));
    container.appendChild(makeGroup('Type', tSel));
    container.appendChild(makeGroup('Color', cSel));
    container.appendChild(makeGroup('Size', sSel));

    [xSel, ySel, tSel, cSel, sSel].forEach(s => s.addEventListener('change', drawFromCache));
}

function _readSelects() {
    return {
        x:     document.getElementById('scatter-x')?.value     ?? null,
        y:     document.getElementById('scatter-y')?.value     ?? null,
        type:  document.getElementById('scatter-type')?.value  ?? 'auto',
        color: document.getElementById('scatter-color')?.value ?? null,
        size:  document.getElementById('scatter-size')?.value  ?? null,
    };
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Annotated plots — rendered from  -- @plot x=var y=var color=var  comments
// ---------------------------------------------------------------------------

let _annotations = [];           // [{x, y, color, size, title, schema}, …]
let _activeSchema = '';          // currently selected schema name
let _annotationSamples = {};     // { schemaName: [bindings, …] } — per-schema cache

function setPlotAnnotations(annotations, activeSchema) {
    _annotations = annotations || [];
    _activeSchema = activeSchema || '';
    _renderAnnotatedPlots(_cachedSamples);
}

function setSamplesForSchema(schemaName, samples) {
    _annotationSamples[schemaName] = samples || [];
    _renderAnnotatedPlots(_cachedSamples);
}

function clearAnnotationSamples() {
    _annotationSamples = {};
}

function _renderAnnotatedPlots(mainSamples) {
    let container = document.getElementById('annotated-plots');
    if (!container) {
        const body = document.getElementById('scatter-body');
        if (!body) return;
        container = document.createElement('div');
        container.id = 'annotated-plots';
        body.appendChild(container);
    }
    container.innerHTML = '';
    if (!_annotations.length) return;

    _annotations.forEach((cfg) => {
        const xVar     = cfg.x     || null;
        const yVar     = cfg.y     || null;
        const colorVar = cfg.color || null;
        const sizeVar  = cfg.size  || null;
        if (!xVar) return;

        // Use this annotation's schema's samples; fall back to main samples
        // when the annotation belongs to the currently active schema.
        const isActiveSchema = !cfg.schema || cfg.schema === _activeSchema;
        const samples = isActiveSchema
            ? mainSamples
            : (_annotationSamples[cfg.schema] || []);

        const wrapper = document.createElement('div');
        wrapper.style.cssText = 'margin-top:14px;border-top:1px solid var(--border,#45475a);padding-top:12px;';

        const headerText = cfg.title || (cfg.schema ? `${cfg.schema}: ${xVar} × ${yVar || '…'}` : '');
        if (headerText) {
            const lbl = document.createElement('div');
            lbl.style.cssText = 'font-size:11px;color:var(--fg-muted,#6c7086);margin-bottom:6px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;';
            lbl.textContent = headerText;
            wrapper.appendChild(lbl);
        }

        const plotDiv = document.createElement('div');
        plotDiv.style.cssText = 'position:relative;';
        wrapper.appendChild(plotDiv);
        container.appendChild(wrapper);

        if (!samples.length) {
            plotDiv.innerHTML = '<p style="color:var(--fg-muted);font-size:11px;padding:8px 0;">Sampling…</p>';
            return;
        }

        // Explicit type overrides auto-detection
        if (cfg.type === 'graph' && yVar) {
            if (typeof drawGraphPlot === 'function')
                drawGraphPlot(samples, xVar, yVar, colorVar, plotDiv);
            return;
        }
        if (cfg.type === 'relation' && yVar) {
            if (typeof drawRelationPlot === 'function')
                drawRelationPlot(samples, xVar, yVar, colorVar, plotDiv);
            return;
        }

        const xt = varType(xVar, samples);
        const yt = yVar ? varType(yVar, samples) : null;

        if (!xt) return;
        if (xt === 'numeric' && yt === 'numeric') {
            drawScatterPlot(samples, xVar, yVar, colorVar, sizeVar, plotDiv);
        } else if (xt === 'enum' && yt === 'numeric') {
            drawStripPlot(samples, xVar, yVar, colorVar, sizeVar, plotDiv);
        } else if (xt === 'numeric' && yt === 'enum') {
            drawStripPlot(samples, yVar, xVar, colorVar, sizeVar, plotDiv);
        } else if (xt === 'enum') {
            drawCountBars(samples, xVar, colorVar, plotDiv);
        }
    });
}

// ---------------------------------------------------------------------------

function drawScatter(samples) {
    _cachedSamples = samples || [];
    drawFromCache();
    _renderAnnotatedPlots(samples);
}

function drawFromCache() {
    const samples = _cachedSamples;
    const container = document.getElementById('scatter-plot');
    if (!container || !samples.length) return;

    const { x: xVar, y: yVar, type: plotType, color: colorVar, size: sizeVar } = _readSelects();

    const xt = varType(xVar, samples);
    const yt = yVar ? varType(yVar, samples) : null;

    if (!xt) {
        container.innerHTML = '<p style="color:#6c7086;padding:8px;">Select an X variable.</p>';
        return;
    }

    // Explicit type overrides auto-detection
    if (plotType === 'graph' && yVar && yVar !== NONE) {
        if (typeof drawGraphPlot === 'function')
            drawGraphPlot(samples, xVar, yVar, colorVar || null, container);
        return;
    }
    if (plotType === 'relation' && yVar && yVar !== NONE) {
        if (typeof drawRelationPlot === 'function')
            drawRelationPlot(samples, xVar, yVar, colorVar || null, container);
        return;
    }

    // Auto-detect from variable types
    let catVar = null, numVarX = null, numVarY = null;

    if (xt === 'numeric' && yt === 'numeric') {
        numVarX = xVar; numVarY = yVar;
    } else if (xt === 'enum' && yt === 'numeric') {
        catVar = xVar; numVarY = yVar;
    } else if (xt === 'numeric' && yt === 'enum') {
        catVar = yVar; numVarY = xVar;
    } else if (xt === 'enum' && (!yt || yt === 'enum')) {
        catVar = xVar;
    } else {
        numVarX = xVar;
    }

    if (catVar && numVarY) {
        drawStripPlot(samples, catVar, numVarY, colorVar || null, sizeVar || null, container);
    } else if (numVarX && numVarY) {
        drawScatterPlot(samples, numVarX, numVarY, colorVar || null, sizeVar || null, container);
    } else if (catVar) {
        drawCountBars(samples, catVar, colorVar || null, container);
    } else {
        drawStripPlot(samples, null, numVarX, colorVar || null, sizeVar || null, container);
    }
}

// ---------------------------------------------------------------------------
// Shared D3 setup
// ---------------------------------------------------------------------------

function _svgSetup(container, W, H, M) {
    container.innerHTML = '';
    const svg = d3.select(container).append('svg')
        .attr('width', W).attr('height', H).style('background', '#181825');
    const g = svg.append('g').attr('transform', `translate(${M.left},${M.top})`);
    const tooltip = d3.select(container).append('div')
        .style('position','absolute').style('background','#313244')
        .style('color','#cdd6f4').style('padding','6px 10px').style('border-radius','4px')
        .style('font-size','12px').style('pointer-events','none').style('display','none')
        .style('white-space','pre');
    return { svg, g, tooltip };
}

function _colorScale(samples, colorVar) {
    if (!colorVar) return () => '#89b4fa';
    const vals = [...new Set(samples.map(s => s[colorVar]).filter(v => v != null))].sort();
    if (varType(colorVar, samples) === 'numeric') {
        const ext = d3.extent(vals.map(Number));
        const scale = d3.scaleSequential(d3.interpolatePlasma).domain(ext);
        return v => v == null ? '#45475a' : scale(+v);
    }
    return v => v == null ? '#45475a' : PALETTE[vals.indexOf(v) % PALETTE.length];
}

function _legend(g, samples, colorVar, iW) {
    if (!colorVar) return;
    const vals = [...new Set(samples.map(s => s[colorVar]).filter(v => v != null))].sort();
    if (varType(colorVar, samples) === 'numeric') return; // gradient legend skipped for now
    const colorOf = _colorScale(samples, colorVar);
    vals.slice(0, 8).forEach((val, i) => {
        g.append('circle').attr('cx', iW - 75).attr('cy', 8 + i * 16).attr('r', 5).attr('fill', colorOf(val));
        g.append('text').attr('x', iW - 67).attr('y', 12 + i * 16)
            .attr('fill', '#cdd6f4').attr('font-size', 11).text(String(val));
    });
}

// Single invisible overlay + nearest-point search replaces per-element event
// listeners.  With 1000+ circles, individual mouseover/out listeners cause
// the browser to hit-test the full SVG tree on every mouse move — O(n) DOM
// work per frame.  One overlay + O(n) arithmetic is far cheaper.
// hitFn(p, mx, my) → bool — optional custom hit-test per point type.
// Falls back to Euclidean distance < 40px when omitted.
function _addOverlay(g, iW, iH, pts, cx, cy, tooltip, hitFn) {
    const defaultHit = (p, mx, my) =>
        Math.sqrt((cx(p) - mx) ** 2 + (cy(p) - my) ** 2) < 40;
    const test = hitFn || defaultHit;

    g.append('rect')
        .attr('width', iW).attr('height', iH)
        .attr('fill', 'none')
        .style('pointer-events', 'all')
        .on('mousemove', (event) => {
            const [mx, my] = d3.pointer(event);
            let best = null, bestD = Infinity;
            for (const p of pts) {
                if (!test(p, mx, my)) continue;
                const dx = cx(p) - mx, dy = cy(p) - my;
                const d  = dx * dx + dy * dy;
                if (d < bestD) { bestD = d; best = p; }
            }
            if (best) {
                const rect = tooltip.node().parentElement.getBoundingClientRect();
                tooltip.style('display', 'block').text(best.label);
                tooltip.style('left', (event.clientX - rect.left + 14) + 'px')
                       .style('top',  (event.clientY - rect.top  - 12) + 'px');
            } else {
                tooltip.style('display', 'none');
            }
        })
        .on('mouseout', () => tooltip.style('display', 'none'));
}

const jitter = i => (((i * 2654435761) % 1000) / 1000 - 0.5);

// ---------------------------------------------------------------------------
// Strip plot: enum columns on X, numeric values on Y
// ---------------------------------------------------------------------------

function drawStripPlot(samples, catVar, numVar, colorVar, sizeVar, container) {
    const W = container.clientWidth || 320;
    const H = 260;
    const M = { top: 20, right: 80, bottom: 40, left: 48 };
    const iW = W - M.left - M.right, iH = H - M.top - M.bottom;
    const { g, tooltip } = _svgSetup(container, W, H, M);

    const categories = catVar
        ? [...new Set(samples.map(s => s[catVar]).filter(v => v != null))].sort()
        : ['all'];

    const pts = samples.filter(s => s[numVar] != null).map((s, i) => ({
        cat: catVar ? s[catVar] : 'all',
        y: +s[numVar],
        ci: i,
        label: Object.entries(s).map(([k,v]) => `${k}: ${v}`).join('\n'),
        colorVal: colorVar ? s[colorVar] : null,
        sizeVal:  sizeVar  ? +s[sizeVar]  : null,
    }));

    const xScale = d3.scaleBand().domain(categories).range([0, iW]).padding(0.3);
    const yExt = d3.extent(pts, d => d.y);
    const pad = v => v === 0 ? 1 : Math.abs(v) * 0.1;
    const yScale = d3.scaleLinear()
        .domain([yExt[0] - pad(yExt[0]), yExt[1] + pad(yExt[1])]).range([iH, 0]);

    const colorOf = _colorScale(samples, colorVar);
    const sizeVals = sizeVar ? pts.map(d => d.sizeVal).filter(v => v != null && !isNaN(v)) : [];
    const rScale = sizeVals.length > 1 && d3.min(sizeVals) !== d3.max(sizeVals)
        ? d3.scaleSqrt().domain(d3.extent(sizeVals)).range([3, 14])
        : () => 5;

    // Grid
    g.selectAll('.hgrid').data(yScale.ticks(5)).enter().append('line')
        .attr('x1', 0).attr('x2', iW).attr('y1', d => yScale(d)).attr('y2', d => yScale(d))
        .attr('stroke','#313244').attr('stroke-width',1);

    // Axes
    g.append('g').attr('transform',`translate(0,${iH})`).call(d3.axisBottom(xScale)).attr('color','#6c7086');
    g.append('g').call(d3.axisLeft(yScale).ticks(5)).attr('color','#6c7086');

    // Labels
    if (catVar) {
        g.append('text').attr('x', iW/2).attr('y', iH+35)
            .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(catVar);
    }
    g.append('text').attr('transform','rotate(-90)').attr('x',-iH/2).attr('y',-36)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(numVar);

    const bw = xScale.bandwidth();
    g.selectAll('circle').data(pts).enter().append('circle')
        .attr('cx', d => xScale(d.cat) + bw/2 + jitter(d.ci) * bw * 0.5)
        .attr('cy', d => yScale(d.y))
        .attr('r',  d => rScale(d.sizeVal))
        .attr('fill', d => colorOf(d.colorVal))
        .attr('opacity', 0.85)
        .attr('stroke','#1e1e2e').attr('stroke-width',1)
        .style('pointer-events', 'none');

    _addOverlay(g, iW, iH, pts,
        d => xScale(d.cat) + bw/2 + jitter(d.ci) * bw * 0.5,
        d => yScale(d.y), tooltip);
    _legend(g, samples, colorVar, iW);
}

// ---------------------------------------------------------------------------
// Count bars: just an enum variable (no numeric Y)
// ---------------------------------------------------------------------------

function drawCountBars(samples, catVar, colorVar, container) {
    const W = container.clientWidth || 320;
    const H = 200;
    const M = { top: 20, right: 80, bottom: 40, left: 48 };
    const iW = W - M.left - M.right, iH = H - M.top - M.bottom;
    const { g, tooltip } = _svgSetup(container, W, H, M);

    const counts = {};
    samples.forEach(s => { const v = s[catVar]; if (v != null) counts[v] = (counts[v]||0)+1; });
    const cats = Object.keys(counts).sort();
    const colorOf = _colorScale(samples, colorVar);

    const xScale = d3.scaleBand().domain(cats).range([0, iW]).padding(0.2);
    const yScale = d3.scaleLinear().domain([0, d3.max(Object.values(counts))]).range([iH, 0]);

    g.append('g').attr('transform',`translate(0,${iH})`).call(d3.axisBottom(xScale)).attr('color','#6c7086');
    g.append('g').call(d3.axisLeft(yScale).ticks(4)).attr('color','#6c7086');
    g.append('text').attr('x', iW/2).attr('y', iH+35)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(catVar);
    g.append('text').attr('transform','rotate(-90)').attr('x',-iH/2).attr('y',-36)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text('count');

    const barPts = cats.map(c => ({
        label: `${catVar}: ${c}\ncount: ${counts[c]}`,
        cx: xScale(c) + xScale.bandwidth() / 2,
        cy: (yScale(counts[c]) + iH) / 2,   // vertical midpoint for tie-breaking
        x1: xScale(c),
        x2: xScale(c) + xScale.bandwidth(),
        y1: yScale(counts[c]),
    }));

    g.selectAll('rect').data(cats).enter().append('rect')
        .attr('x', d => xScale(d)).attr('y', d => yScale(counts[d]))
        .attr('width', xScale.bandwidth()).attr('height', d => iH - yScale(counts[d]))
        .attr('fill', d => colorOf(d)).attr('opacity', 0.8)
        .style('pointer-events', 'none');

    // Hit-test: cursor must be inside the bar's horizontal band and above the axis.
    _addOverlay(g, iW, iH, barPts, d => d.cx, d => d.cy, tooltip,
        (p, mx, my) => mx >= p.x1 && mx <= p.x2 && my >= p.y1 && my <= iH);
}

// ---------------------------------------------------------------------------
// Scatter plot: two numeric variables
// ---------------------------------------------------------------------------

function drawScatterPlot(samples, xVar, yVar, colorVar, sizeVar, container) {
    const W = container.clientWidth || 320;
    const H = 260;
    const M = { top: 20, right: 80, bottom: 40, left: 48 };
    const iW = W - M.left - M.right, iH = H - M.top - M.bottom;
    const { g, tooltip } = _svgSetup(container, W, H, M);

    const pts = samples.filter(s => s[xVar] != null && s[yVar] != null).map(s => ({
        x: +s[xVar], y: +s[yVar],
        colorVal: colorVar ? s[colorVar] : null,
        sizeVal:  sizeVar  ? +s[sizeVar] : null,
        label: Object.entries(s).map(([k,v]) => `${k}: ${v}`).join('\n'),
    }));

    if (!pts.length) { container.innerHTML = `<p style="color:#6c7086;padding:8px">No data</p>`; return; }

    const pad = v => v === 0 ? 1 : Math.abs(v) * 0.12;
    const xExt = d3.extent(pts, d => d.x);
    const yExt = d3.extent(pts, d => d.y);
    const xScale = d3.scaleLinear().domain([xExt[0]-pad(xExt[0]),xExt[1]+pad(xExt[1])]).range([0,iW]);
    const yScale = d3.scaleLinear().domain([yExt[0]-pad(yExt[0]),yExt[1]+pad(yExt[1])]).range([iH,0]);

    const sizeVals = sizeVar ? pts.map(d => d.sizeVal).filter(v => v != null && !isNaN(v)) : [];
    const rScale = sizeVals.length > 1 && d3.min(sizeVals) !== d3.max(sizeVals)
        ? d3.scaleSqrt().domain(d3.extent(sizeVals)).range([4, 16])
        : () => 6;

    const colorOf = _colorScale(samples, colorVar);

    // Grid
    g.selectAll('.vgrid').data(xScale.ticks(5)).enter().append('line')
        .attr('x1',d=>xScale(d)).attr('x2',d=>xScale(d)).attr('y1',0).attr('y2',iH)
        .attr('stroke','#313244').attr('stroke-width',1);
    g.selectAll('.hgrid').data(yScale.ticks(5)).enter().append('line')
        .attr('x1',0).attr('x2',iW).attr('y1',d=>yScale(d)).attr('y2',d=>yScale(d))
        .attr('stroke','#313244').attr('stroke-width',1);

    g.append('g').attr('transform',`translate(0,${iH})`).call(d3.axisBottom(xScale).ticks(5)).attr('color','#6c7086');
    g.append('g').call(d3.axisLeft(yScale).ticks(5)).attr('color','#6c7086');

    g.append('text').attr('x',iW/2).attr('y',iH+35)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(xVar);
    g.append('text').attr('transform','rotate(-90)').attr('x',-iH/2).attr('y',-36)
        .attr('text-anchor','middle').attr('fill','#cdd6f4').attr('font-size',12).text(yVar);

    g.selectAll('circle').data(pts).enter().append('circle')
        .attr('cx', d => xScale(d.x)).attr('cy', d => yScale(d.y))
        .attr('r',  d => rScale(d.sizeVal))
        .attr('fill', d => colorOf(d.colorVal)).attr('opacity', 0.8)
        .attr('stroke','#1e1e2e').attr('stroke-width',1)
        .style('pointer-events', 'none');

    _addOverlay(g, iW, iH, pts, d => xScale(d.x), d => yScale(d.y), tooltip);
    _legend(g, samples, colorVar, iW);
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

window.renderScatterControls = renderScatterControls;
window.drawScatter = drawScatter;
window.drawFromCache = drawFromCache;
window.setPlotAnnotations = setPlotAnnotations;
window.setSamplesForSchema = setSamplesForSchema;
window.clearAnnotationSamples = clearAnnotationSamples;

})();
