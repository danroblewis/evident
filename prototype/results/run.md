# Z3 theory × encoding × tactic — benchmark report

2744 cases from `results/run.csv`. `baseline` = no-tactic solve; `best` = fastest tactic sequence (apply+solve).

**Theories exercised (17):** `array`, `bitvec`, `bool`, `datatype`, `fixedpoint`, `fp`, `horn`, `int`, `real`, `recfun`, `regex`, `relations`, `seq`, `set`, `string`, `tuple`, `uf`

## arith_system  (N=20)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| real | real | sat | 0.8 | 0.8 | `simplify[blast_select_store=True]` |
| real_nl | real | sat | 1.9 | 1.8 | `ctx-simplify>simplify` |
| int | int | sat | 2.0 | 1.8 | `elim-term-ite>elim-term-ite` |
| bitvec | bitvec | sat | 40.6 | 18.6 | `solve-eqs` |

## coloring  (N=60)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| enum | datatype | sat | 1.9 | 1.7 | `simplify[blast_select_store=True]>propagate-values` |
| onehot | bool | sat | 2.3 | 2.2 | `propagate-ineqs` |
| bitvec | bitvec | sat | 2.9 | 2.3 | `simplify[blast_select_store=True]>solve-eqs` |
| int | int | sat | 14.2 | 7.5 | `propagate-ineqs>propagate-values` |

## dispatch  (N=200)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| arith | int | sat | 0.3 | 0.2 | `elim-term-ite>elim-term-ite` |
| ite | bool+int | sat | 0.6 | 0.4 | `simplify>solve-eqs` |
| set_bv | set+tuple+bitvec | sat | 2.8 | 0.2 | `ctx-simplify>simplify[blast_select_store=True]` |
| func | uf+int | sat | 2.9 | 2.7 | `propagate-values>elim-term-ite` |
| set | set+tuple | sat | 22.8 | 0.5 | `ctx-simplify>simplify[blast_select_store=True]` |
| array | array+int | sat | 166.3 | 0.5 | `simplify[blast_select_store=True]>solve-eqs` |

## fp_solve  (N=12)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| fp | fp | sat | 1254.3 | 1053.1 | `elim-term-ite>propagate-ineqs` |

## invariant  (N=200)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| unroll_k | int | sat | 2.1 | 0.9 | `ctx-simplify>solve-eqs` |
| spacer | fixedpoint+horn | unsat | 8.9 | 8.9 | `(none)` |

## reachability  (N=60)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| datalog | fixedpoint+bitvec | sat | 2.7 | 2.7 | `(none)` |
| unroll_bool | bool | sat | 5.0 | 3.0 | `propagate-values>elim-term-ite` |
| special | relations | unsat | 13.1 | 12.8 | `propagate-ineqs>elim-term-ite` |
| unroll_set | set | sat | 64.7 | 41.8 | `simplify>solve-eqs` |
| recfun | recfun | sat | 73.1 | 73.1 | `(none)` |

## recursion  (N=200)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| closed_form | int | sat | 0.5 | 0.2 | `solve-eqs>simplify` |
| unroll | int | sat | 0.5 | 0.2 | `simplify>solve-eqs` |
| recfun | recfun | sat | 1.1 | 1.1 | `(none)` |

## seq_build  (N=12)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| seq | seq | sat | 10.5 | 9.8 | `simplify[blast_select_store=True]` |

## string_match  (N=12)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| string | string | sat | 14.6 | 12.4 | `simplify[blast_select_store=True]>simplify` |
| regex | string+regex | sat | 48.0 | 41.4 | `propagate-values>solve-eqs` |
