# Z3 theory × encoding × tactic — benchmark report

2394 cases from `results/run.csv`. `baseline` = no-tactic solve; `best` = fastest tactic sequence (apply+solve).

**Theories exercised (14):** `array`, `bitvec`, `bool`, `datatype`, `fp`, `int`, `real`, `regex`, `relations`, `seq`, `set`, `string`, `tuple`, `uf`

## arith_system  (N=20)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| real | real | sat | 0.6 | 0.5 | `ctx-simplify>simplify` |
| int | int | sat | 1.4 | 1.2 | `elim-term-ite>ctx-simplify` |
| real_nl | real | sat | 1.4 | 1.4 | `simplify>simplify` |
| bitvec | bitvec | sat | 36.4 | 13.8 | `solve-eqs>simplify[blast_select_store=True]` |

## coloring  (N=60)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| enum | datatype | sat | 2.3 | 1.6 | `propagate-values>simplify[blast_select_store=True]` |
| onehot | bool | sat | 2.3 | 2.2 | `elim-term-ite` |
| bitvec | bitvec | sat | 2.8 | 2.3 | `elim-term-ite>propagate-ineqs` |
| int | int | sat | 13.4 | 7.3 | `propagate-ineqs>propagate-values` |

## dispatch  (N=200)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| arith | int | sat | 0.3 | 0.2 | `elim-term-ite` |
| ite | bool+int | sat | 0.7 | 0.3 | `simplify[blast_select_store=True]>solve-eqs` |
| set_bv | set+tuple+bitvec | sat | 2.7 | 0.2 | `ctx-simplify>simplify[blast_select_store=True]` |
| func | uf+int | sat | 2.8 | 2.6 | `elim-term-ite>propagate-values` |
| set | set+tuple | sat | 22.5 | 0.5 | `ctx-simplify>simplify[blast_select_store=True]` |
| array | array+int | sat | 169.6 | 0.5 | `simplify[blast_select_store=True]>solve-eqs` |

## fp_solve  (N=12)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| fp | fp | sat | 1255.3 | 997.6 | `solve-eqs>simplify` |

## reachability  (N=60)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| unroll_bool | bool | sat | 5.1 | 3.0 | `propagate-values>simplify[blast_select_store=True]` |
| special | relations | unsat | 9.9 | 9.9 | `(none)` |
| unroll_set | set | sat | 59.2 | 41.3 | `simplify>solve-eqs` |

## seq_build  (N=12)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| seq | seq | sat | 13.2 | 7.7 | `simplify>ctx-simplify` |

## string_match  (N=12)

| encoding | theories | result | baseline ms | best ms | best tactic sequence |
|---|---|---|--:|--:|---|
| string | string | sat | 9.7 | 9.7 | `(none)` |
| regex | string+regex | sat | 39.2 | 32.6 | `elim-term-ite>propagate-ineqs` |
