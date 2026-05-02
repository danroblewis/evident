// Relation and graph visualisations for Evident IDE.
//
// drawGraphPlot(samples, xVar, yVar, colorVar, container)
//   Homogeneous directed graph — both variables from the same node set.
//   D3 force simulation. Pan + zoom. Each unique (x,y) pair is a directed
//   edge; edge thickness scales with how many samples share that pair.
//
// drawRelationPlot(samples, xVar, yVar, colorVar, container)
//   Bipartite / relation diagram — two node columns, arrows between them.
//   Left = unique xVar values, right = unique yVar values.
//   Shows function vs many-to-many, surjectivity, injectivity at a glance.

(function () {

const NODE_R    = 14;
const BG        = '#181825';
const NODE_FILL = '#313244';
const MUTED     = '#6c7086';
const FG        = '#cdd6f4';
const BLUE      = '#89b4fa';
const GREEN     = '#a6e3a1';

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

function _edgesFromSamples(samples, xVar, yVar) {
    const nodeSet = new Set();
    const edgeMap = new Map();   // "src\0dst" → count
    for (const s of samples) {
        const src = s[xVar], dst = s[yVar];
        if (src == null || dst == null) continue;
        const sk = String(src), dk = String(dst);
        nodeSet.add(sk);
        nodeSet.add(dk);
        const key = `${sk}\x00${dk}`;
        edgeMap.set(key, (edgeMap.get(key) || 0) + 1);
    }
    return { nodeSet, edgeMap };
}

function _svgBase(container, W, H) {
    container.innerHTML = '';
    return d3.select(container).append('svg')
        .attr('width', W).attr('height', H)
        .style('background', BG).style('border-radius', '4px');
}

function _arrowMarker(defs, id, color) {
    defs.append('marker')
        .attr('id', id)
        .attr('viewBox', '0 -5 10 10')
        .attr('refX', 8).attr('refY', 0)
        .attr('orient', 'auto')
        .attr('markerWidth', 6).attr('markerHeight', 6)
        .append('path').attr('d', 'M0,-5L10,0L0,5').attr('fill', color);
}

function _tooltip(container) {
    return d3.select(container).append('div')
        .style('position', 'absolute').style('background', '#313244')
        .style('color', FG).style('padding', '5px 9px').style('border-radius', '4px')
        .style('font-size', '12px').style('pointer-events', 'none')
        .style('display', 'none').style('white-space', 'pre');
}

function _showTip(tip, event, text) {
    const rect = tip.node().parentElement.getBoundingClientRect();
    tip.style('display', 'block').text(text)
       .style('left', (event.clientX - rect.left + 14) + 'px')
       .style('top',  (event.clientY - rect.top  - 12) + 'px');
}

function _addZoom(svg, g) {
    const zoom = d3.zoom()
        .scaleExtent([0.15, 6])
        .on('zoom', event => g.attr('transform', event.transform));
    svg.call(zoom).on('dblclick.zoom', null);

    // Hint text
    svg.append('text')
        .attr('x', 6).attr('y', svg.attr('height') - 6)
        .attr('fill', MUTED).attr('font-size', 9)
        .style('pointer-events', 'none')
        .text('scroll to zoom · drag to pan');
}

// ---------------------------------------------------------------------------
// Force-directed graph (homogeneous — one node set)
// ---------------------------------------------------------------------------

function drawGraphPlot(samples, xVar, yVar, colorVar, container) {
    const W = container.clientWidth || 420;
    const H = 300;

    const { nodeSet, edgeMap } = _edgesFromSamples(samples, xVar, yVar);
    if (!nodeSet.size) return;

    // Nodes — D3 force mutates these objects with .x, .y
    const nodes = [...nodeSet].map(id => ({ id }));
    const nodeById = Object.fromEntries(nodes.map(n => [n.id, n]));

    // Links — use string IDs; force link with .id(d=>d.id) resolves them to objects
    const links = [];
    for (const [key, count] of edgeMap) {
        const [src, dst] = key.split('\x00');
        links.push({ source: src, target: dst, count });
    }

    const svg  = _svgBase(container, W, H);
    const tip  = _tooltip(container);
    const defs = svg.append('defs');
    _arrowMarker(defs, 'g-arr',    MUTED);
    _arrowMarker(defs, 'g-arr-hi', BLUE);

    // Pan/zoom container
    const g = svg.append('g');
    _addZoom(svg, g);

    const edgeG  = g.append('g');
    const nodeG  = g.append('g');
    const labelG = g.append('g');

    // Self-loop path (cubic Bézier that leaves and returns to the same point)
    function selfLoop(x, y) {
        const r = NODE_R + 6;
        return `M ${x},${y - NODE_R} C ${x + r * 2.2},${y - r * 2.8} ${x - r * 2.2},${y - r * 2.8} ${x},${y - NODE_R}`;
    }

    // Straight edge path — stops at circle boundary
    function straightEdge(s, t) {
        const dx = t.x - s.x, dy = t.y - s.y;
        const len = Math.sqrt(dx * dx + dy * dy) || 1;
        return `M ${s.x},${s.y} L ${t.x - dx / len * (NODE_R + 3)},${t.y - dy / len * (NODE_R + 3)}`;
    }

    function edgePath(d) {
        const s = d.source, t = d.target;
        if (typeof s !== 'object' || typeof t !== 'object') return '';
        return s.id === t.id ? selfLoop(s.x, s.y) : straightEdge(s, t);
    }

    const edgePaths = edgeG.selectAll('path').data(links).enter().append('path')
        .attr('fill', 'none')
        .attr('stroke', MUTED)
        .attr('stroke-width', d => Math.min(1 + d.count * 0.5, 5))
        .attr('marker-end', 'url(#g-arr)')
        .style('cursor', 'default')
        .on('mouseover', (event, d) => {
            d3.select(event.currentTarget).attr('stroke', BLUE).attr('marker-end', 'url(#g-arr-hi)');
            const src = typeof d.source === 'object' ? d.source.id : d.source;
            const dst = typeof d.target === 'object' ? d.target.id : d.target;
            _showTip(tip, event, `${src} → ${dst}  (${d.count}×)`);
        })
        .on('mousemove', (event, d) => {
            const src = typeof d.source === 'object' ? d.source.id : d.source;
            const dst = typeof d.target === 'object' ? d.target.id : d.target;
            _showTip(tip, event, `${src} → ${dst}  (${d.count}×)`);
        })
        .on('mouseout', event => {
            d3.select(event.currentTarget).attr('stroke', MUTED).attr('marker-end', 'url(#g-arr)');
            tip.style('display', 'none');
        });

    const circles = nodeG.selectAll('circle').data(nodes).enter().append('circle')
        .attr('r', NODE_R)
        .attr('fill', NODE_FILL).attr('stroke', BLUE).attr('stroke-width', 1.5)
        .style('cursor', 'grab')
        .call(d3.drag()
            .on('start', (event, d) => { if (!event.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
            .on('drag',  (event, d) => { d.fx = event.x; d.fy = event.y; })
            .on('end',   (event, d) => { if (!event.active) sim.alphaTarget(0); d.fx = null; d.fy = null; })
        );

    const labels = labelG.selectAll('text').data(nodes).enter().append('text')
        .attr('text-anchor', 'middle').attr('dy', '0.35em')
        .attr('fill', FG).attr('font-size', 11).style('pointer-events', 'none')
        .text(d => d.id);

    // Column header
    svg.append('text').attr('x', 6).attr('y', 14)
        .attr('fill', MUTED).attr('font-size', 10).attr('font-weight', '600')
        .text(`${xVar} → ${yVar}`);

    const sim = d3.forceSimulation(nodes)
        .force('link',    d3.forceLink(links).id(d => d.id).distance(90).strength(0.7))
        .force('charge',  d3.forceManyBody().strength(-250))
        .force('center',  d3.forceCenter(W / 2, H / 2))
        .force('collide', d3.forceCollide(NODE_R + 8))
        .on('tick', () => {
            edgePaths.attr('d', edgePath);
            circles.attr('cx', d => d.x).attr('cy', d => d.y);
            labels.attr('x', d => d.x).attr('y', d => d.y);
        });
}

// ---------------------------------------------------------------------------
// Bipartite / relation diagram (two node sets)
// ---------------------------------------------------------------------------

function drawRelationPlot(samples, xVar, yVar, colorVar, container) {
    const W = container.clientWidth || 420;

    const leftVals  = new Set(), rightVals = new Set();
    const edgeMap   = new Map();

    for (const s of samples) {
        const l = s[xVar], r = s[yVar];
        if (l == null || r == null) continue;
        const lk = String(l), rk = String(r);
        leftVals.add(lk);
        rightVals.add(rk);
        const key = `${lk}\x00${rk}`;
        edgeMap.set(key, (edgeMap.get(key) || 0) + 1);
    }
    if (!leftVals.size) return;

    const numSort = (a, b) => isNaN(+a) ? a.localeCompare(b) : +a - +b;
    const lefts   = [...leftVals].sort(numSort);
    const rights  = [...rightVals].sort(numSort);

    const ROW_H  = 38;
    const PAD_T  = 38;
    const PAD_B  = 20;
    const H      = PAD_T + Math.max(lefts.length, rights.length) * ROW_H + PAD_B;
    const leftX  = 70;
    const rightX = W - 70;

    function nodeY(arr, i) {
        return PAD_T + i * ROW_H + ROW_H / 2;
    }

    const leftPos  = Object.fromEntries(lefts.map((id, i)  => [id, { x: leftX,  y: nodeY(lefts, i)  }]));
    const rightPos = Object.fromEntries(rights.map((id, i) => [id, { x: rightX, y: nodeY(rights, i) }]));

    const svg  = _svgBase(container, W, H);
    const tip  = _tooltip(container);
    const defs = svg.append('defs');
    _arrowMarker(defs, 'r-arr',    MUTED);
    _arrowMarker(defs, 'r-arr-hi', BLUE);

    const g = svg.append('g');
    _addZoom(svg, g);

    // Column labels
    g.append('text').attr('x', leftX).attr('y', 20)
        .attr('text-anchor', 'middle').attr('fill', BLUE)
        .attr('font-size', 10).attr('font-weight', '600').attr('letter-spacing', '.06em')
        .text(xVar.toUpperCase());
    g.append('text').attr('x', rightX).attr('y', 20)
        .attr('text-anchor', 'middle').attr('fill', GREEN)
        .attr('font-size', 10).attr('font-weight', '600').attr('letter-spacing', '.06em')
        .text(yVar.toUpperCase());

    // Degree maps (for ×N annotations)
    const leftDeg = {}, rightDeg = {};
    for (const key of edgeMap.keys()) {
        const [lId, rId] = key.split('\x00');
        leftDeg[lId]  = (leftDeg[lId]  || 0) + 1;
        rightDeg[rId] = (rightDeg[rId] || 0) + 1;
    }

    // Edges
    for (const [key, count] of edgeMap) {
        const [lId, rId] = key.split('\x00');
        const l = leftPos[lId], r = rightPos[rId];
        if (!l || !r) continue;

        const dx = r.x - l.x, dy = r.y - l.y;
        const len = Math.sqrt(dx * dx + dy * dy) || 1;
        // Stop the path just before the right node's edge
        const ex = r.x - dx / len * (NODE_R + 3);
        const ey = r.y - dy / len * (NODE_R + 3);
        // Slight quadratic curve
        const mx = (l.x + r.x) / 2, my = (l.y + r.y) / 2 - 8;

        const label = `${lId} → ${rId}  (${count}×)`;
        const path = g.append('path')
            .attr('d', `M ${l.x + NODE_R},${l.y} Q ${mx},${my} ${ex},${ey}`)
            .attr('fill', 'none').attr('stroke', MUTED)
            .attr('stroke-width', Math.min(1 + count * 0.5, 5))
            .attr('marker-end', 'url(#r-arr)').style('cursor', 'default')
            .on('mouseover', event => {
                path.attr('stroke', BLUE).attr('marker-end', 'url(#r-arr-hi)');
                _showTip(tip, event, label);
            })
            .on('mousemove', event => _showTip(tip, event, label))
            .on('mouseout', () => {
                path.attr('stroke', MUTED).attr('marker-end', 'url(#r-arr)');
                tip.style('display', 'none');
            });
    }

    // Left nodes
    lefts.forEach(id => {
        const { x, y } = leftPos[id];
        g.append('circle').attr('cx', x).attr('cy', y).attr('r', NODE_R)
            .attr('fill', NODE_FILL).attr('stroke', BLUE).attr('stroke-width', 1.5);
        g.append('text').attr('x', x).attr('y', y)
            .attr('text-anchor', 'middle').attr('dy', '0.35em')
            .attr('fill', FG).attr('font-size', 11).style('pointer-events', 'none').text(id);
        if (leftDeg[id] > 1) {
            g.append('text').attr('x', x - NODE_R - 4).attr('y', y)
                .attr('text-anchor', 'end').attr('dy', '0.35em')
                .attr('fill', MUTED).attr('font-size', 9).text(`×${leftDeg[id]}`);
        }
    });

    // Right nodes
    rights.forEach(id => {
        const { x, y } = rightPos[id];
        g.append('circle').attr('cx', x).attr('cy', y).attr('r', NODE_R)
            .attr('fill', NODE_FILL).attr('stroke', GREEN).attr('stroke-width', 1.5);
        g.append('text').attr('x', x).attr('y', y)
            .attr('text-anchor', 'middle').attr('dy', '0.35em')
            .attr('fill', FG).attr('font-size', 11).style('pointer-events', 'none').text(id);
        if (rightDeg[id] > 1) {
            g.append('text').attr('x', x + NODE_R + 4).attr('y', y)
                .attr('text-anchor', 'start').attr('dy', '0.35em')
                .attr('fill', MUTED).attr('font-size', 9).text(`×${rightDeg[id]}`);
        }
    });
}

window.drawGraphPlot    = drawGraphPlot;
window.drawRelationPlot = drawRelationPlot;

})();
