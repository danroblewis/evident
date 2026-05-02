"""
Tests for graph and relation diagram data.

The JS renderers (drawGraphPlot, drawRelationPlot) receive a list of sample
dicts and build a node set + edge map from two chosen variables. These tests
verify that the schemas we use as graph/relation examples produce the correct
nodes and edges when sampled — i.e. that what the renderer receives is right.

Schemas used:

  FunctionRel  — a total function  x ↦ x*2  for x in {1,2,3}
                 Each left node maps to exactly one right node.

  MultiRel     — a non-function relation  x < y  for x,y in {1,2,3,4}
                 Multiple right values per left node.

  HomogeneousGraph — closed relation on one set  (edges from a fixed set)
                     Both src and dst come from the same node pool.

  SymmetricRel — symmetric: if (a,b) then (b,a). Expected to show
                 bidirectional arrows in the graph renderer.
"""

import pytest
from runtime.src.runtime import EvidentRuntime


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _collect(source, schema, n=30):
    """Return up to n unique bindings from sampling schema."""
    import sys, pathlib
    sys.path.insert(0, str(pathlib.Path(__file__).parent.parent.parent / 'ide' / 'backend'))
    from sampler import random_seed_sample
    results = random_seed_sample(source, schema, {}, n)
    return [r.bindings for r in results]


def _edges(samples, x_var, y_var):
    """Build {(x_val, y_val): count} from samples, mirroring _edgesFromSamples in graph.js."""
    edge_map = {}
    for s in samples:
        x, y = s.get(x_var), s.get(y_var)
        if x is None or y is None:
            continue
        key = (str(x), str(y))
        edge_map[key] = edge_map.get(key, 0) + 1
    return edge_map


def _nodes(edge_map):
    """All unique node IDs that appear in the edge map."""
    nodes = set()
    for (src, dst) in edge_map:
        nodes.add(src)
        nodes.add(dst)
    return nodes


# ---------------------------------------------------------------------------
# Test schemas (self-contained source strings)
# ---------------------------------------------------------------------------

FUNCTION_REL = """
schema FunctionRel
    x ∈ Nat
    y ∈ Nat
    x ∈ {1, 2, 3}
    y = x * 2
"""

MULTI_REL = """
schema MultiRel
    x, y ∈ Nat
    1 ≤ x ≤ 4
    1 ≤ y ≤ 4
    x < y
"""

HOMOGENEOUS_GRAPH = """
type Stage = Design | Code | Test | Review | Deploy

assert pipeline = {
    (Design, Code),
    (Code,   Test),
    (Code,   Review),
    (Test,   Deploy),
    (Review, Deploy)
}

schema PipelineEdge
    from_stage ∈ Stage
    to_stage   ∈ Stage
    (from_stage, to_stage) ∈ pipeline
"""

SYMMETRIC_REL = """
assert sym_edges = {
    (1, 2), (2, 1),
    (2, 3), (3, 2),
    (1, 3), (3, 1)
}

schema SymEdge
    a ∈ Nat
    b ∈ Nat
    (a, b) ∈ sym_edges
"""


# ---------------------------------------------------------------------------
# FunctionRel — total function x ↦ 2x
# ---------------------------------------------------------------------------

class TestFunctionRel:
    def setup_method(self):
        self.samples = _collect(FUNCTION_REL, 'FunctionRel', n=20)
        self.edge_map = _edges(self.samples, 'x', 'y')

    def test_produces_samples(self):
        assert len(self.samples) >= 3, "Should sample at least 3 distinct assignments"

    def test_correct_left_nodes(self):
        left = {src for (src, _) in self.edge_map}
        assert left == {'1', '2', '3'}

    def test_correct_right_nodes(self):
        right = {dst for (_, dst) in self.edge_map}
        assert right == {'2', '4', '6'}

    def test_correct_edges(self):
        assert ('1', '2') in self.edge_map
        assert ('2', '4') in self.edge_map
        assert ('3', '6') in self.edge_map

    def test_no_spurious_edges(self):
        expected = {('1', '2'), ('2', '4'), ('3', '6')}
        assert set(self.edge_map.keys()) == expected

    def test_function_property(self):
        """Each left node maps to exactly one right node (it's a function)."""
        from collections import defaultdict
        out_degree = defaultdict(set)
        for (src, dst) in self.edge_map:
            out_degree[src].add(dst)
        for src, dsts in out_degree.items():
            assert len(dsts) == 1, f"Node {src} maps to {dsts}, not a function"


# ---------------------------------------------------------------------------
# MultiRel — many-to-many: x < y for x,y in {1..4}
# ---------------------------------------------------------------------------

class TestMultiRel:
    def setup_method(self):
        self.samples = _collect(MULTI_REL, 'MultiRel', n=50)
        self.edge_map = _edges(self.samples, 'x', 'y')

    def test_produces_samples(self):
        assert len(self.samples) >= 5

    def test_expected_edges_present(self):
        # x < y for x,y in {1,2,3,4} gives: (1,2),(1,3),(1,4),(2,3),(2,4),(3,4)
        expected = {('1','2'),('1','3'),('1','4'),('2','3'),('2','4'),('3','4')}
        # With enough sampling all should appear
        assert self.edge_map.keys() <= expected, "Unexpected edges found"

    def test_no_reverse_edges(self):
        """x < y means no (y, x) edge."""
        for (src, dst) in self.edge_map:
            assert int(src) < int(dst), f"Edge ({src},{dst}) violates x < y"

    def test_not_a_function(self):
        """Node 1 should map to multiple destinations."""
        from collections import defaultdict
        out = defaultdict(set)
        for (src, dst) in self.edge_map:
            out[src].add(dst)
        assert len(out.get('1', set())) > 1, "Node 1 should have multiple successors"


# ---------------------------------------------------------------------------
# HomogeneousGraph — closed on one node set
# ---------------------------------------------------------------------------

class TestHomogeneousGraph:
    def setup_method(self):
        self.samples = _collect(HOMOGENEOUS_GRAPH, 'PipelineEdge', n=30)
        self.edge_map = _edges(self.samples, 'from_stage', 'to_stage')

    def test_produces_samples(self):
        assert len(self.samples) >= 5

    def test_all_expected_edges(self):
        expected = {
            ('Design','Code'), ('Code','Test'), ('Code','Review'),
            ('Test','Deploy'), ('Review','Deploy'),
        }
        assert set(self.edge_map.keys()) == expected

    def test_node_pool(self):
        """All nodes should come from the Stage enum."""
        stages = {'Design', 'Code', 'Test', 'Review', 'Deploy'}
        assert _nodes(self.edge_map) <= stages

    def test_homogeneous(self):
        """from_stage and to_stage values come from the same Stage enum."""
        srcs = {src for (src, _) in self.edge_map}
        dsts = {dst for (_, dst) in self.edge_map}
        stages = {'Design', 'Code', 'Test', 'Review', 'Deploy'}
        assert srcs <= stages and dsts <= stages

    def test_no_self_loops(self):
        for (src, dst) in self.edge_map:
            assert src != dst, f"Unexpected self-loop ({src}→{src})"

    def test_dag_structure(self):
        """No cycles — topological order: Design→Code→{Test,Review}→Deploy."""
        order = ['Design', 'Code', 'Test', 'Review', 'Deploy']
        idx = {s: i for i, s in enumerate(order)}
        for (src, dst) in self.edge_map:
            assert idx[src] < idx[dst], f"Edge {src}→{dst} goes backward in the pipeline"


# ---------------------------------------------------------------------------
# SymmetricRel — bidirectional edges
# ---------------------------------------------------------------------------

class TestSymmetricRel:
    def setup_method(self):
        self.samples = _collect(SYMMETRIC_REL, 'SymEdge', n=30)
        self.edge_map = _edges(self.samples, 'a', 'b')

    def test_produces_samples(self):
        assert len(self.samples) >= 6

    def test_symmetric_property(self):
        """For every edge (a,b) the reverse (b,a) should also appear."""
        for (a, b) in list(self.edge_map.keys()):
            assert (b, a) in self.edge_map, f"Edge ({a},{b}) exists but ({b},{a}) does not"

    def test_correct_node_count(self):
        nodes = _nodes(self.edge_map)
        assert nodes == {'1', '2', '3'}

    def test_edge_count(self):
        # 6 directed edges total (each pair in both directions)
        assert len(self.edge_map) == 6
