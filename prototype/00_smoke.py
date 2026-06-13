#!/usr/bin/env python3
"""Smoke test: prove the Z3 capabilities the language is betting on.

Run:  python3 prototype/00_smoke.py
"""
import z3


def under_determined():
    # Partial constraint, the solver fills the rest — a solution SPACE, not a point.
    x, y = z3.Ints("x y")
    s = z3.Solver()
    s.add(0 < x, x < y, y < 10)
    assert s.check() == z3.sat
    print("under-determined:    ", s.model())  # one arbitrary witness


def native_sets():
    # Sets are Array(T, Bool); union/intersect/diff/member/subset are native.
    Int = z3.IntSort()
    A, B = z3.Consts("A B", z3.SetSort(Int))
    e = z3.Int("e")
    s = z3.Solver()
    s.add(A == z3.SetAdd(z3.SetAdd(z3.EmptySet(Int), 1), 2))   # A = {1, 2}
    s.add(B == z3.SetAdd(z3.EmptySet(Int), 2))                 # B = {2}
    s.add(z3.IsMember(e, z3.SetIntersect(A, B)))               # e ∈ A ∩ B
    assert s.check() == z3.sat
    print("set A ∩ B member:    ", "e =", s.model()[e])


def relation_lookup():
    # A relation / dispatch table as a function graph; lookup is a solve.
    sortof = z3.Function("sortof", z3.StringSort(), z3.IntSort())
    s = z3.Solver()
    for k, v in [("Int", 7), ("Bool", 3), ("Real", 4)]:
        s.add(sortof(z3.StringVal(k)) == v)
    key = z3.String("key")
    s.add(key == z3.StringVal("Bool"))
    assert s.check() == z3.sat
    print("relational lookup:   ", "sortof(Bool) =", s.model().eval(sortof(key)))


if __name__ == "__main__":
    print("z3", z3.get_version_string())
    under_determined()
    native_sets()
    relation_lookup()
    print("ok")
