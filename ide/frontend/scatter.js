// 2D scatter plot and basic transfer function using Plotly
// Expects Plotly to be loaded from CDN before this script runs.
// Depends on: getGivenBindings() from schema-panel.js

function renderScatterControls(variables) {
    const container = document.getElementById('scatter-controls');
    container.innerHTML = `
        <label>X axis: <select id="scatter-x">${variables.map(v => `<option>${v}</option>`).join('')}</select></label>
        <label>Y axis: <select id="scatter-y">${variables.map((v, i) => `<option ${i === 1 ? 'selected' : ''}>${v}</option>`).join('')}</select></label>
        <button onclick="renderScatterPlot()">Plot</button>
    `;
}

async function renderScatterPlot() {
    const schema = document.getElementById('schema-select').value;
    const source = window.evEditor?.getSource() || '';
    const given = getGivenBindings();
    const xVar = document.getElementById('scatter-x')?.value;
    const yVar = document.getElementById('scatter-y')?.value;

    if (!xVar || !yVar) return;

    // Get samples
    const resp = await fetch('/sample', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source, schema, given, n: 50, strategy: 'random' }),
    });
    const data = await resp.json();
    const samples = data.samples || [];

    Plotly.newPlot('scatter-plot', [{
        x: samples.map(s => s[xVar]),
        y: samples.map(s => s[yVar]),
        mode: 'markers',
        type: 'scatter',
        marker: {
            color: '#89b4fa',
            size: 8,
            opacity: 0.7,
            line: { color: '#cba6f7', width: 1 },
        },
        text: samples.map(s =>
            Object.entries(s).map(([k, v]) => `${k}: ${v}`).join('<br>')
        ),
        hovertemplate: '%{text}<extra></extra>',
    }], {
        paper_bgcolor: '#1e1e2e',
        plot_bgcolor: '#181825',
        font: { color: '#cdd6f4' },
        xaxis: { title: xVar, gridcolor: '#313244' },
        yaxis: { title: yVar, gridcolor: '#313244' },
        title: { text: `${schema}: ${xVar} vs ${yVar}`, font: { color: '#89b4fa' } },
        margin: { t: 40, r: 20, b: 40, l: 50 },
    }, { responsive: true });
}

// Make renderScatterPlot available globally
window.renderScatterPlot = renderScatterPlot;
