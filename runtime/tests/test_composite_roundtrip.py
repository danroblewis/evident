"""
Tests that composite Datatype values round-trip cleanly through the
runtime: model extraction produces dict / list-of-dicts, and feeding
that result back as a `given` value works (composite types referenced
by the schema get pre-registered, dicts get reconstructed via the
matching Z3 Datatype constructor).

This is what the executor's state-forwarding loop relies on when
`state ∈ Seq(SomeType)` carries composite values across steps.
"""
from runtime.src.runtime import EvidentRuntime


def test_dict_roundtrip_seq_composite():
    rt = EvidentRuntime()
    rt.load_source("""
type DotState
    pos_x     ∈ Int
    pos_y     ∈ Int
    collected ∈ Bool

claim step
    state      ∈ Seq(DotState)
    state_next ∈ Seq(DotState)
    #state = 2
    #state_next = 2
    state_next = state
""")
    given = {
        'state': [
            {'pos_x': 80,  'pos_y': 80, 'collected': False},
            {'pos_x': 660, 'pos_y': 80, 'collected': True},
        ],
    }
    r1 = rt.query('step', given=given)
    assert r1.satisfied
    nxt = r1.bindings['state_next']
    assert nxt == given['state']

    # Round-trip: feed the extracted dicts back as the input.
    r2 = rt.query('step', given={'state': nxt})
    assert r2.satisfied
    assert r2.bindings['state_next'] == given['state']


def test_dict_roundtrip_nested_composite():
    rt = EvidentRuntime()
    rt.load_source("""
type Color
    r ∈ Int
    g ∈ Int
    b ∈ Int

type Rect
    x     ∈ Int
    y     ∈ Int
    color ∈ Color

claim step
    rects      ∈ Seq(Rect)
    rects_next ∈ Seq(Rect)
    #rects = 2
    #rects_next = 2
    rects_next = rects
""")
    given = {
        'rects': [
            {'x': 10, 'y': 20, 'color': {'r': 255, 'g': 0,   'b': 0}},
            {'x': 30, 'y': 40, 'color': {'r': 0,   'g': 255, 'b': 0}},
        ],
    }
    r1 = rt.query('step', given=given)
    assert r1.satisfied
    nxt = r1.bindings['rects_next']
    assert nxt == given['rects']

    r2 = rt.query('step', given={'rects': nxt})
    assert r2.satisfied
    assert r2.bindings['rects_next'] == given['rects']


def test_dict_unknown_fields_rejected():
    """A dict with no matching composite type raises a clear error."""
    rt = EvidentRuntime()
    rt.load_source("""
type Point
    x ∈ Int
    y ∈ Int

claim identity
    pts ∈ Seq(Point)
""")
    import pytest
    with pytest.raises(ValueError, match="no composite type with fields"):
        rt.query('identity', given={'pts': [{'a': 1, 'b': 2}]})
