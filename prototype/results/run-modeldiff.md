# How the winning tactic reshaped each model

Per encoding (at its largest scale): the baseline model vs the model after its fastest tactic sequence. `Δsym`/`Δnodes` are distinct symbols and DAG nodes; *movers* are the operations whose count changed most (the structural reason for the speedup).

## arith_system  (N=20)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| bitvec | `solve-eqs` | +19 | +0 | bvult 19→0, bvule 0→19, not 0→19, bvmul 0→19 |
| int | `elim-term-ite>elim-term-ite` | +0 | +0 | — |
| real | `simplify[blast_select_store=True]` | +19 | +0 | <= 0→19, < 19→0, not 0→19 |
| real_nl | `ctx-simplify>simplify` | +19 | +0 | <= 0→19, < 19→0, not 0→19 |

## coloring  (N=60)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| bitvec | `simplify[blast_select_store=True]>solve-eqs` | +207 | +0 | not 0→207, = 0→207, distinct 147→0, bvult 60→0 |
| enum | `simplify[blast_select_store=True]>propagate-values` | +147 | +0 | not 0→147, = 0→147, distinct 147→0 |
| int | `propagate-ineqs>propagate-values` | +0 | +0 | — |
| onehot | `propagate-ineqs` | +0 | +0 | — |

## dispatch  (N=200)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| arith | `elim-term-ite>elim-term-ite` | +0 | +0 | — |
| array | `simplify[blast_select_store=True]>solve-eqs` | -408 | +0 | Int 202→0, store 200→0, k 1→0, select 1→0 |
| func | `propagate-values>elim-term-ite` | +0 | +0 | — |
| ite | `simplify>solve-eqs` | -603 | +0 | Int 201→0, = 200→0, if 199→0, k 1→0 |
| set | `ctx-simplify>simplify[blast_select_store=True]` | -600 | +0 | P20 201→0, store 200→0, Int 201→4, not 0→1 |
| set_bv | `ctx-simplify>simplify[blast_select_store=True]` | -601 | +0 | PB21 201→0, store 200→0, bv 201→3, bvult 1→0 |

## fp_solve  (N=12)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| fp | `elim-term-ite>propagate-ineqs` | +0 | +0 | — |

## invariant  (N=200)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| spacer | `(none)` | — | — | baseline already best |
| unroll_k | `ctx-simplify>solve-eqs` | -805 | +0 | = 201→0, >= 201→0, + 200→0, Int 2→0 |

## reachability  (N=60)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| datalog | `(none)` | — | — | baseline already best |
| recfun | `(none)` | — | — | baseline already best |
| special | `propagate-ineqs>elim-term-ite` | +0 | +0 | — |
| unroll_bool | `propagate-values>elim-term-ite` | -8670 | +0 | or 6900→0, = 3660→0, not 0→1892, true 1→0 |
| unroll_set | `simplify>solve-eqs` | -25447 | +0 | store 10921→0, if 10920→0, select 3421→0, = 61→0 |

## recursion  (N=200)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| closed_form | `solve-eqs>simplify` | -3 | +0 | out 1→0, = 1→0, Int 1→0 |
| recfun | `(none)` | — | — | baseline already best |
| unroll | `simplify>solve-eqs` | -205 | +0 | Int 201→0, = 2→0, out 1→0, + 1→0 |

## seq_build  (N=12)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| seq | `simplify[blast_select_store=True]` | +0 | +0 | — |

## string_match  (N=12)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| regex | `propagate-values>solve-eqs` | +0 | +0 | — |
| string | `simplify[blast_select_store=True]>simplify` | +0 | +0 | — |
