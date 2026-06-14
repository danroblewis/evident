# sum_to — tail-recursive accumulator (sum 1..5)

Each sub-model is shown on its own (symbolic interface), then the **combined** model the runtime actually solves. The recursion is owned by the unroller, not Python's stack.

## transition `sum_to`  (one step: state → state′)

fields: `i: Int`, `acc: Int`

```
if i = 0 then i′ = i ∧ acc′ = acc else i′ = i − 1 ∧ acc′ = acc + i
```

## combined model — `sum_to` unrolled to 5 steps

This is what the **one-shot** strategy solves (12 variables — grows with depth).

```
5 = i0
0 = acc0
if i0 = 0 then i1 = i0 ∧ acc1 = acc0 else i1 = i0 − 1 ∧ acc1 = acc0 + i0
if i1 = 0 then i2 = i1 ∧ acc2 = acc1 else i2 = i1 − 1 ∧ acc2 = acc1 + i1
if i2 = 0 then i3 = i2 ∧ acc3 = acc2 else i3 = i2 − 1 ∧ acc3 = acc2 + i2
if i3 = 0 then i4 = i3 ∧ acc4 = acc3 else i4 = i3 − 1 ∧ acc4 = acc3 + i3
if i4 = 0 then i5 = i4 ∧ acc5 = acc4 else i5 = i4 − 1 ∧ acc5 = acc4 + i4
```

## run result

- **one-shot** (unroll all, one solve): final = `{'i': 0, 'acc': 15}`  [12 vars]
- **incremental** (one step at a time, memory reuse): final = `{'i': 0, 'acc': 15}`  [4 vars, constant]

state trace (incremental):

```
{'i': 5, 'acc': 0} → {'i': 4, 'acc': 5} → {'i': 3, 'acc': 9} → {'i': 2, 'acc': 12} → {'i': 1, 'acc': 14} → {'i': 0, 'acc': 15}
```
