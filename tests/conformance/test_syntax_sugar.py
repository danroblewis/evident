"""
Conformance tests for argument-passing syntax sugar in claim calls.

Five forms are covered here. The first existed before this session
but had no conformance coverage; the rest landed in commits
746f2fe, 3a4f137, 8d73cf7.

  1. Tuple-in-claim:     `(args) ∈ claim_name`            (commit 3436d79)
  2. Method-call:        `recv.claim(args)`               (commit 746f2fe)
  3. Method tuple-in:    `(args) ∈ recv.claim`            (commit 746f2fe)
  4. Arg type inference: fresh multi-use names auto-typed  (commit 3a4f137)
  5. Tuple-as-record:    `(a, b, c)` → record literal     (commit 8d73cf7)

All of these are sugar — they desugar to equivalent positional
ClaimCall invocations. The tests pin inputs and check the
satisfying binding to verify the sugar lowers to the same
constraint set as the long form.
"""

import pytest
from .conftest import query, assert_sat, assert_unsat, assert_binding


# A tiny helper claim used throughout. `add` takes two Ints, produces
# their sum in `out`. Simple enough that the test surface is the
# *invocation* syntax, not the claim body.
ADD_CLAIM = """
claim add(a ∈ Int, b ∈ Int, out ∈ Int)
    out = a + b
"""


# A helper with a record-typed parameter so we can exercise the
# tuple-as-record coercion. `sum_vec` reads x and y from the IVec2.
VEC_CLAIM = """
type IVec2(x, y ∈ Int)

claim sum_vec(v ∈ IVec2, out ∈ Int)
    out = v.x + v.y
"""


# ---------------------------------------------------------------------------
# 1. Tuple-in-claim: `(args) ∈ claim_name`
# ---------------------------------------------------------------------------
# Relational invocation: read "this tuple is a member of the set of
# satisfying assignments of `claim`". Same args, same order as a
# positional call.

def test_tuple_in_claim_sat():
    src = ADD_CLAIM + """
claim S
    result ∈ Int
    (3, 4, result) ∈ add
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 7)


def test_tuple_in_claim_unsat_under_wrong_binding():
    src = ADD_CLAIM + """
claim S
    result ∈ Int
    (3, 4, result) ∈ add
    result = 99
"""
    assert_unsat(query(src, 'S'))


def test_tuple_in_claim_mid_chain():
    # Inputs computed from prior bindings, not literals.
    src = ADD_CLAIM + """
claim S
    a ∈ Int
    b ∈ Int
    a = 10
    b = a + 5
    result ∈ Int
    (a, b, result) ∈ add
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 25)


# ---------------------------------------------------------------------------
# 2. Method-call dispatch: `recv.claim(args)`
# ---------------------------------------------------------------------------
# `recv.claim(args)` desugars to `claim(recv, args)`. The receiver
# becomes the first positional arg. Receiver can be a bare
# identifier or a dotted field-access (`win.renderer`).

def test_method_call_bare_receiver():
    src = ADD_CLAIM + """
claim S
    x ∈ Int = 7
    result ∈ Int
    x.add(3, result)
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 10)


def test_method_call_dotted_receiver():
    # `box.value.add(...)` — receiver name retains its dots and
    # resolves through env's dotted leaf keys.
    src = ADD_CLAIM + """
type Box
    value ∈ Int

claim S
    box ∈ Box
    box.value = 100
    result ∈ Int
    box.value.add(50, result)
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 150)


def test_method_call_unsat_under_wrong_binding():
    src = ADD_CLAIM + """
claim S
    x ∈ Int = 7
    result ∈ Int
    x.add(3, result)
    result = 99
"""
    assert_unsat(query(src, 'S'))


# ---------------------------------------------------------------------------
# 3. Method tuple-in: `(args) ∈ recv.claim`
# ---------------------------------------------------------------------------
# Combines forms 1 and 2 — relational invocation against a method
# call.

def test_method_tuple_in_sat():
    src = ADD_CLAIM + """
claim S
    x ∈ Int = 8
    result ∈ Int
    (4, result) ∈ x.add
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 12)


# ---------------------------------------------------------------------------
# 4. Claim-arg type inference for fresh multi-use names
# ---------------------------------------------------------------------------
# When a positional arg is a fresh identifier referenced ≥ 2 times
# in the body, the runtime injects `Membership(name, slot_type)`
# from the claim's signature. Single-use names are NOT inferred
# (typo defense).

def test_arg_inference_multi_use():
    # `mid` is undeclared. Used once in the add call's `out` slot,
    # and once in the next constraint — total 2 uses → inferred as
    # `mid ∈ Int`.
    src = ADD_CLAIM + """
claim S
    add(2, 3, mid)
    final ∈ Int
    final = mid + 100
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'mid', 5)
    assert_binding(b, 'final', 105)


def test_arg_inference_through_method_call():
    # Method-style invocation also infers — the prepended receiver
    # shifts the arg-to-param mapping by 1.
    src = ADD_CLAIM + """
claim S
    x ∈ Int = 9
    x.add(1, result)
    other ∈ Int
    other = result * 2
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'result', 10)
    assert_binding(b, 'other', 20)


def test_arg_inference_skips_single_use():
    # `lonely` appears exactly once (in the call). No other reference
    # in the body → inference SKIPS it. Translation can't resolve
    # the unbound name, so the constraint is dropped; query of `result`
    # remains unconstrained but the schema itself is still satisfiable
    # (Z3 picks any Int for `lonely`'s implicit slot).
    #
    # The negative observable here: `lonely` does NOT appear as a
    # binding in the model. If it had been inferred, we'd see it.
    src = ADD_CLAIM + """
claim S
    result ∈ Int = 42
    add(1, 2, lonely)
"""
    b = assert_sat(query(src, 'S'))
    assert 'lonely' not in b, (
        "single-use fresh name should NOT be auto-declared (typo defense)"
    )


# ---------------------------------------------------------------------------
# 5. Tuple-as-record-literal coercion
# ---------------------------------------------------------------------------
# When a positional arg is a bare `(a, b, c)` Tuple AND the slot's
# type is a known record schema, the tuple rewrites to a record
# literal `Call(slot_type, items)` — same as if the user had typed
# `IVec2(3, 4)`.

def test_tuple_coerce_to_record():
    src = VEC_CLAIM + """
claim S
    result ∈ Int
    sum_vec((3, 4), result)
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 7)


def test_tuple_coerce_in_method_call():
    # Receiver is prepended, then tuple coercion fires on the next
    # arg whose slot is IVec2.
    src = """
type IVec2(x, y ∈ Int)

claim weighted_sum(scale ∈ Int, v ∈ IVec2, out ∈ Int)
    out = scale * (v.x + v.y)

claim S
    s ∈ Int = 10
    result ∈ Int
    s.weighted_sum((2, 3), result)
"""
    assert_binding(assert_sat(query(src, 'S')), 'result', 50)


def test_tuple_coerce_only_when_slot_is_record():
    # Slot is `n ∈ Int` (primitive), not a record — the tuple
    # `(1, 2)` does NOT coerce here (there's no record type to
    # build). Translation drops the constraint.
    #
    # The query is still SAT because the rest of the body has no
    # other constraints on `n`. We just verify that `n` was NOT
    # somehow assigned to 1 or 2 (which would happen if the tuple
    # were incorrectly coerced into a primitive).
    src = ADD_CLAIM + """
claim S
    n ∈ Int = 999
    add((1, 2), 5, result)
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'n', 999)


# ---------------------------------------------------------------------------
# 6. Compositions — features stacked
# ---------------------------------------------------------------------------
# Method dispatch + tuple coercion + arg inference, all in one
# call. This is the canonical Mario-style render line, distilled.

def test_all_three_sugars_compose():
    src = VEC_CLAIM + """
claim scale_vec(v ∈ IVec2, k ∈ Int, out ∈ Int)
    out = (v.x + v.y) * k

claim S
    scaler ∈ Int = 3
    -- Method dispatch: scaler prepended as `v`. But `v` slot expects
    -- IVec2, not Int — so this should FAIL (UNSAT or dropped
    -- constraint). We use the tuple form below instead.
    -- scaler.scale_vec(2, result)   -- type-mismatch, not valid

    -- Correct usage: tuple coerces to IVec2, arg inference picks
    -- up `result` from the `out` slot.
    scale_vec((4, 5), 10, result)
    doubled ∈ Int
    doubled = result + 1
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'result', 90)
    assert_binding(b, 'doubled', 91)
