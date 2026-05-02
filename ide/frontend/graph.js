// Relation and graph visualisations for Evident IDE.
//
// drawGraphPlot(samples, xVar, yVar, colorVar, container)
//   Homogeneous directed graph — both variables from the same node set.
//   Uses D3 force simulation. Each unique (x,y) pair is a directed edge.
//   Edge thickness scales with how many samples share that pair.
//
// drawRelationPlot(samples, xVar, yVar, colorVar, container)
//   Bipartite / relation diagram — two node columns, arrows between them.
//   Left column = unique xVar values, right column = unique yVar values.
//   Immediately shows function vs many-to-many, surjectivity, injectivity.

(function () {

const NODE_R   = 14;
const PALETTE  = ['#89b4fa','#a6e3a1','#fab387','#f38ba8','#cba6f7','#94e2d5'];
const BG       = '#181825';
const NODE_FILL = '#313244';
const MUTED    = '#6c7086';
const FG       = '#cdd6f4';

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

function _edgesFromSamples(samples, xVar, yVar) {
    const edgeMap = new Map();
    const nodeSet = new Set();
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
    const svg = d3.select(container).append('svg')
        .attr('width', W).attr('height', H)
        .style('background', BG).style('border-radius', '4px');
    return svg;
}

function _arrowMarker(svg, id, color) {
    svg.append('defs').append('marker')
        .attr('id', id)
        .attr('viewBox', '0 -5 10 10')
        .attr('refX', 8).attr('refY', 0)
        .attr('orient', 'auto')
        .attr('markerWidth', 7).attr('markerHeight', 7)
        .append('path')
        .attr('d', 'M0,-5L10,0L0,5')
        .attr('fill', color || MUTED);
}

function _tooltip(container) {
    return d3.select(container).append('div')
        .style('position', 'absolute')
        .style('background', '#313244').style('color', FG)
        .style('padding', '5px 9px').style('border-radius', '4px')
        .style('font-size', '12px').style('pointer-events', 'none')
        .style('display', 'none').style('white-space', 'pre');
}

function _showTip(tip, container, event, text) {
    const rect = tip.node().parentElement.getBoundingClientRect();
    tip.style('display', 'block').text(text)
       .style('left', (event.clientX - rect.left + 14) + 'px')
       .style('top',  (event.clientY - rect.top  - 12) + 'px');
}

// ---------------------------------------------------------------------------
// Force-directed graph (homogeneous — one node set)
// ---------------------------------------------------------------------------

function drawGraphPlot(samples, xVar, yVar, colorVar, container) {
    const W = container.clientWidth || 400;
    const H = 300;

    const { nodeSet, edgeMap } = _edgesFromSamples(samples, xVar, yVar);
    if (!nodeSet.size) return;

    const nodes = [...nodeSet].map(id => ({ id }));
    const nodeIndex = Object.fromEntries(nodes.map((n, i) => [n.id, i]));

    const links = [];
    for (const [key, count] of edgeMap) {
        const [src, dst] = key.split('\x00');
        links.push({ source: nodeIndex[src], target: nodeIndex[dst], src, dst, count });
    }

    const svg   = _svgBase(container, W, H);
    const tip   = _tooltip(container);

    // Arrowhead markers — one default, one highlighted
    _arrowMarker(svg, 'g-arrow',  MUTED);
    _arrowMarker(svg, 'g-arrow-hi', '#89b4fa');

    // Self-loop path helper
    function selfLoopPath(x, y) {
        const r = NODE_R + 4;
        return `M ${x},${y - NODE_R} C ${x + r * 2.5},${y - r * 3} ${x - r * 2.5},${y - r * 3} ${x},${y - NODE_R}`;
    }

    // Straight-line path helper (stops at circle edge)
    function edgePath(s, t) {
        if (s.id === t.id) return selfLoopPath(s.x, s.y);
        const dx = t.x - s.x, dy = t.y - s.y;
        const len = Math.sqrt(dx * dx + dy * dy) || 1;
        const ex = t.x - dx / len * (NODE_R + 2);
        const ey = t.y - dy / len * (NODE_R + 2);
        return `M ${s.x},${s.y} L ${ex},${ey}`;
    }

    const edgeG = svg.append('g');
    const nodeG = svg.append('g');
    const labelG = svg.append('g');

    // Draw edges
    const edgePaths = edgeG.selectAll('path').data(links).enter().append('path')
        .attr('fill', 'none')
        .attr('stroke', MUTED)
        .attr('stroke-width', d => Math.min(1 + d.count * 0.4, 4))
        .attr('marker-end', 'url(#g-arrow)')
        .style('cursor', 'default')
        .on('mouseover', (event, d) => {
            d3.select(event.currentTarget).attr('stroke', '#89b4fa').attr('marker-end', 'url(#g-arrow-hi)');
            _showTip(tip, container, event, `${d.src} → ${d.dst}\n${d.count} sample${d.count > 1 ? 's' : ''}`);
        })
        .on('mousemove', (event, d) => _showTip(tip, container, event, `${d.src} → ${d.dst}\n${d.count} sample${d.count > 1 ? 's' : ''}`))
        .on('mouseout', (event) => {
            d3.select(event.currentTarget).attr('stroke', MUTED).attr('marker-end', 'url(#g-arrow)');
            tip.style('display', 'none');
        });

    // Draw nodes
    const circles = nodeG.selectAll('circle').data(nodes).enter().append('circle')
        .attr('r', NODE_R)
        .attr('fill', NODE_FILL)
        .attr('stroke', '#89b4fa').attr('stroke-width', 1.5)
        .style('cursor', 'grab')
        .call(d3.drag()
            .on('start', (event, d) => { if (!event.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
            .on('drag',  (event, d) => { d.fx = event.x; d.fy = event.y; })
            .on('end',   (event, d) => { if (!event.active) sim.alphaTarget(0); d.fx = null; d.fy = null; })
        );

    const labels = labelG.selectAll('text').data(nodes).enter().append('text')
        .attr('text-anchor', 'middle').attr('dy', '0.35em')
        .attr('fill', FG).attr('font-size', 11)
        .style('pointer-events', 'none')
        .text(d => d.id);

    // Column headers
    svg.append('text').attr('x', 8).attr('y', 14)
        .attr('fill', MUTED).attr('font-size', 10).attr('font-weight', '600')
        .text(`${xVar} → ${yVar}`);

    // Force simulation
    const sim = d3.forceSimulation(nodes)
        .force('link',   d3.forceLink(links).id(d => d.id).distance(80).strength(0.8))
        .force('charge', d3.forceManyBody().strength(-220))
        .force('center', d3.forceCenter(W / 2, H / 2))
        .force('collide', d3.forceCollide(NODE_R + 6))
        .on('tick', () => {
            // Clamp to bounds
            nodes.forEach(d => {
                d.x = Math.max(NODE_R + 4, Math.min(W - NODE_R - 4, d.x));
                d.y = Math.max(NODE_R + 4, Math.min(H - NODE_R - 4, d.y));
            });
            edgePaths.attr('d', d => {
                const s = typeof d.source === 'object' ? d.source : nodes[d.source];
                const t = typeof d.target === 'object' ? d.target : nodes[d.target];
                return edgePath(s, t);
            });
            circles.attr('cx', d => d.x).attr('cy', d => d.y);
            labels.attr('x', d => d.x).attr('y', d => d.y);
        });
}

// ---------------------------------------------------------------------------
// Bipartite / relation diagram (two node sets)
// ---------------------------------------------------------------------------

function drawRelationPlot(samples, xVar, yVar, colorVar, container) {
    const W = container.clientWidth || 400;

    // Collect unique left and right values
    const leftSet = new Set(), rightSet = new Set();
    const edgeMap = new Map();
    for (const s of samples) {
        const l = s[xVar], r = s[yVar];
        if (l == null || r == null) continue;
        const lk = String(l), rk = String(r);
        leftSet.add(lk);
        rightSet.add(rk);
        const key = `${lk}\x00${rk}`;
        edgeMap.set(key, (edgeMap.get(key) || 0) + 1);
    }

    if (!leftSet.size) return;

    const lefts  = [...leftSet].sort((a, b) => isNaN(a) ? a.localeCompare(b) : +a - +b);
    const rights = [...rightSet].sort((a, b) => isNaN(a) ? a.localeCompare(b) : +a - +b);

    const ROW_H  = 36;
    const PAD_T  = 36;  // top padding for column labels
    const PAD_B  = 16;
    const H      = PAD_T + Math.max(lefts.length, rights.length) * ROW_H + PAD_B;
    const leftX  = 70;
    const rightX = W - 70;

    function nodeY(arr, i) {
        return PAD_T + i * ROW_H + ROW_H / 2;
    }

    const leftPos  = Object.fromEntries(lefts.map((id, i)  => [id, { x: leftX,  y: nodeY(lefts, i)  }]));
    const rightPos = Object.fromEntries(rights.map((id, i) => [id, { x: rightX, y: nodeY(rights, i) }]));

    const svg = _svgBase(container, W, H);
    const tip = _tooltip(container);

    _arrowMarker(svg, 'r-arrow',    MUTED);
    _arrowMarker(svg, 'r-arrow-hi', '#89b4fa');

    // Column labels
    svg.append('text').attr('x', leftX).attr('y', 18)
        .attr('text-anchor', 'middle').attr('fill', '#89b4fa')
        .attr('font-size', 10).attr('font-weight', '600').attr('letter-spacing', '.06em')
        .text(xVar.toUpperCase());
    svg.append('text').attr('x', rightX).attr('y', 18)
        .attr('text-anchor', 'middle').attr('fill', '#a6e3a1')
        .attr('font-size', 10).attr('font-weight', '600').attr('letter-spacing', '.06em')
        .text(yVar.toUpperCase());

    // Edges
    const edgeG = svg.append('g');
    for (const [key, count] of edgeMap) {
        const [lId, rId] = key.split('\x00');
        const l = leftPos[lId], r = rightPos[rId];
        if (!l || !r) continue;

        // Slight quadratic curve for readability
        const mx = (l.x + r.x) / 2;
        const my = (l.y + r.y) / 2;
        // End point pulled back from right node center
        const dx = r.x - l.x, dy = r.y - l.y;
        const len = Math.sqrt(dx * dx + dy * dy) || 1;
        const ex = r.x - dx / len * (NODE_R + 2);
        const ey = r.y - dy / len * (NODE_R + 2);

        const path = edgeG.append('path')
            .attr('d', `M ${l.x + NODE_R},${l.y} Q ${mx},${my} ${ex},${ey}`)
            .attr('fill', 'none')
            .attr('stroke', MUTED)
            .attr('stroke-width', Math.min(1 + count * 0.4, 4))
            .attr('marker-end', 'url(#r-arrow)')
            .style('cursor', 'default');

        const label = `${lId} → ${rId}\n${count} sample${count > 1 ? 's' : ''}`;
        path.on('mouseover', (event) => {
                path.attr('stroke', '#89b4fa').attr('marker-end', 'url(#r-arrow-hi)');
                _showTip(tip, container, event, label);
            })
            .on('mousemove', (event) => _showTip(tip, container, event, label))
            .on('mouseout', () => {
                path.attr('stroke', MUTED).attr('marker-end', 'url(#r-arrow)');
                tip.style('display', 'none');
            });
    }

    // Left nodes
    lefts.forEach((id, i) => {
        const { x, y } = leftPos[id];
        svg.append('circle').attr('cx', x).attr('cy', y).attr('r', NODE_R)
            .attr('fill', NODE_FILL).attr('stroke', '#89b4fa').attr('stroke-width', 1.5);
        svg.append('text').attr('x', x).attr('y', y)
            .attr('text-anchor', 'middle').attr('dy', '0.35em')
            .attr('fill', FG).attr('font-size', 11).text(id);
    });

    // Right nodes
    rights.forEach((id, i) => {
        const { x, y } = rightPos[id];
        svg.append('circle').attr('cx', x).attr('cy', y).attr('r', NODE_R)
            .attr('fill', NODE_FILL).attr('stroke', '#a6e3a1').attr('stroke-width', 1.5);
        svg.append('text').attr('x', x).attr('y', y)
            .attr('text-anchor', 'middle').attr('dy', '0.35em')
            .attr('fill', FG).attr('font-size', 11).text(id);
    });

    // Degree annotations (fan-out count beside left nodes, fan-in beside right)
    const leftDegree  = {};
    const rightDegree = {};
    for (const key of edgeMap.keys()) {
        const [lId, rId] = key.split('\x00');
        leftDegree[lId]  = (leftDegree[lId]  || 0) + 1;
        rightDegree[rId] = (rightDegree[rId] || 0) + 1;
    }
    lefts.forEach(id => {
        const { x, y } = leftPos[id];
        if (leftDegree[id] > 1) {
            svg.append('text').attr('x', x - NODE_R - 4).attr('y', y)
                .attr('text-anchor', 'end').attr('dy', '0.35em')
                .attr('fill', MUTED).attr('font-size', 9)
                .text(`×${leftDegree[id]}`);
        }
    });
    rights.forEach(id => {
        const { x, y } = rightPos[id];
        if (rightDegree[id] > 1) {
            svg.append('text').attr('x', x + NODE_R + 4).attr('y', y)
                .attr('text-anchor', 'start').attr('dy', '0.35em')
                .attr('fill', MUTED).attr('font-size', 9)
                .text(`×${rightDegree[id]}`);
        }
    });
}

// Expose
window.drawGraphPlot    = drawGraphPlot;
window.drawRelationPlot = drawRelationPlot;

})();
