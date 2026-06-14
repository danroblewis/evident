"""Sub-model composition + runtime-owned unrolling (POC).

A *sub-model* is a named, parameterized constraint template (a custom predicate
or function). The runtime composes them and OWNS the recursion via an explicit
bounded unroller — Python lends a for-loop, never its call stack. Two execution
strategies for the same transition relation: one-shot unroll (all states in one
solve) vs incremental (solve one step, reuse the same variable slots = memory
reuse, the tail-recursion runtime). See core.py.
"""
