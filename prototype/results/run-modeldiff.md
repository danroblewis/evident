# How the winning tactic reshaped each model

Per encoding (at its largest scale): the baseline model vs the model after its fastest tactic sequence. `Δsym`/`Δnodes` are distinct symbols and DAG nodes; *movers* are the operations whose count changed most (the structural reason for the speedup).

## arith_system  (N=20)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| bitvec | `solve-eqs>simplify[blast_select_store=True]` | +19 | +0 | bvmul 0→19, bvule 0→19, bvult 19→0, not 0→19 |
| int | `elim-term-ite>ctx-simplify` | +0 | +0 | — |
| real | `ctx-simplify>simplify` | +19 | +0 | < 19→0, <= 0→19, not 0→19 |
| real_nl | `simplify>simplify` | +19 | +0 | < 19→0, <= 0→19, not 0→19 |

## coloring  (N=60)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| bitvec | `elim-term-ite>propagate-ineqs` | +0 | +0 | — |
| enum | `propagate-values>simplify[blast_select_store=True]` | +147 | +0 | = 0→147, distinct 147→0, not 0→147 |
| int | `propagate-ineqs>propagate-values` | +0 | +0 | — |
| onehot | `elim-term-ite` | +0 | +0 | — |

## dispatch  (N=200)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| arith | `elim-term-ite` | +0 | +0 | — |
| array | `simplify[blast_select_store=True]>solve-eqs` | -408 | +0 | Int 202→0, store 200→0, const 1→0, < 1→0 |
| func | `elim-term-ite>propagate-values` | +0 | +0 | — |
| ite | `simplify[blast_select_store=True]>solve-eqs` | -603 | +0 | Int 201→0, = 200→0, if 199→0, k 1→0 |
| set | `ctx-simplify>simplify[blast_select_store=True]` | -600 | +0 | P2 201→0, store 200→0, Int 201→4, < 1→0 |
| set_bv | `ctx-simplify>simplify[blast_select_store=True]` | -601 | +0 | PB3 201→0, store 200→0, bv 201→3, bvult 1→0 |

## fp_solve  (N=12)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| fp | `solve-eqs>simplify` | +0 | +0 | fp.gt 13→0, fp.lt 0→13 |

## reachability  (N=60)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| special | `(none)` | — | — | baseline already best |
| unroll_bool | `propagate-values>simplify[blast_select_store=True]` | -8670 | +0 | or 6900→0, = 3660→0, not 0→1892, true 1→0 |
| unroll_set | `simplify>solve-eqs` | -25447 | +0 | store 10921→0, if 10920→0, select 3421→0, = 61→0 |

## seq_build  (N=12)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| seq | `simplify>ctx-simplify` | +0 | +0 | — |

## string_match  (N=12)

| encoding | best sequence | Δnodes | Δsym | top operation movers |
|---|---|--:|--:|---|
| regex | `elim-term-ite>propagate-ineqs` | +0 | +0 | — |
| string | `(none)` | — | — | baseline already best |
