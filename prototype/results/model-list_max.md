# list_max — iterative max over [3, 1, 4, 1, 5, 9, 2, 6]

Each sub-model is shown on its own (symbolic interface), then the **combined** model the runtime actually solves. The recursion is owned by the unroller, not Python's stack.

## sub-model `at`  (`idx: Int`)

```
if idx = 0 then 3
  else if idx = 1 then 1
    else if idx = 2 then 4
      else if idx = 3 then 1
        else if idx = 4 then 5
          else if idx = 5 then 9 else if idx = 6 then 2 else 6
```

## transition `list_max`  (one step: state → state′) — composes `at`

fields: `idx: Int`, `best: Int`

```
if idx = 8 then idx′ = idx ∧ best′ = best
  else idx′ = idx + 1 ∧ best′ = (if s1 > best then s1 else best)
where
  s1 = if idx = 0 then 3
      else if idx = 1 then 1
        else if idx = 2 then 4
          else if idx = 3 then 1
            else if idx = 4 then 5
              else if idx = 5 then 9 else if idx = 6 then 2 else 6
```

## combined model — `list_max` unrolled to 8 steps

This is what the **one-shot** strategy solves (18 variables — grows with depth).

```
0 = idx0
-999 = best0
if idx0 = 8 then idx1 = idx0 ∧ best1 = best0
  else idx1 = idx0 + 1 ∧ best1 = (if s8 > best0 then s8 else best0)
if idx1 = 8 then idx2 = idx1 ∧ best2 = best1
  else idx2 = idx1 + 1 ∧ best2 = (if s7 > best1 then s7 else best1)
if idx2 = 8 then idx3 = idx2 ∧ best3 = best2
  else idx3 = idx2 + 1 ∧ best3 = (if s6 > best2 then s6 else best2)
if idx3 = 8 then idx4 = idx3 ∧ best4 = best3
  else idx4 = idx3 + 1 ∧ best4 = (if s5 > best3 then s5 else best3)
if idx4 = 8 then idx5 = idx4 ∧ best5 = best4
  else idx5 = idx4 + 1 ∧ best5 = (if s4 > best4 then s4 else best4)
if idx5 = 8 then idx6 = idx5 ∧ best6 = best5
  else idx6 = idx5 + 1 ∧ best6 = (if s3 > best5 then s3 else best5)
if idx6 = 8 then idx7 = idx6 ∧ best7 = best6
  else idx7 = idx6 + 1 ∧ best7 = (if s2 > best6 then s2 else best6)
if idx7 = 8 then idx8 = idx7 ∧ best8 = best7
  else idx8 = idx7 + 1 ∧ best8 = (if s1 > best7 then s1 else best7)
where
  s8 = if idx0 = 0 then 3
      else if idx0 = 1 then 1
        else if idx0 = 2 then 4
          else if idx0 = 3 then 1
            else if idx0 = 4 then 5
              else if idx0 = 5 then 9 else if idx0 = 6 then 2 else 6
  s7 = if idx1 = 0 then 3
      else if idx1 = 1 then 1
        else if idx1 = 2 then 4
          else if idx1 = 3 then 1
            else if idx1 = 4 then 5
              else if idx1 = 5 then 9 else if idx1 = 6 then 2 else 6
  s6 = if idx2 = 0 then 3
      else if idx2 = 1 then 1
        else if idx2 = 2 then 4
          else if idx2 = 3 then 1
            else if idx2 = 4 then 5
              else if idx2 = 5 then 9 else if idx2 = 6 then 2 else 6
  s5 = if idx3 = 0 then 3
      else if idx3 = 1 then 1
        else if idx3 = 2 then 4
          else if idx3 = 3 then 1
            else if idx3 = 4 then 5
              else if idx3 = 5 then 9 else if idx3 = 6 then 2 else 6
  s4 = if idx4 = 0 then 3
      else if idx4 = 1 then 1
        else if idx4 = 2 then 4
          else if idx4 = 3 then 1
            else if idx4 = 4 then 5
              else if idx4 = 5 then 9 else if idx4 = 6 then 2 else 6
  s3 = if idx5 = 0 then 3
      else if idx5 = 1 then 1
        else if idx5 = 2 then 4
          else if idx5 = 3 then 1
            else if idx5 = 4 then 5
              else if idx5 = 5 then 9 else if idx5 = 6 then 2 else 6
  s2 = if idx6 = 0 then 3
      else if idx6 = 1 then 1
        else if idx6 = 2 then 4
          else if idx6 = 3 then 1
            else if idx6 = 4 then 5
              else if idx6 = 5 then 9 else if idx6 = 6 then 2 else 6
  s1 = if idx7 = 0 then 3
      else if idx7 = 1 then 1
        else if idx7 = 2 then 4
          else if idx7 = 3 then 1
            else if idx7 = 4 then 5
              else if idx7 = 5 then 9 else if idx7 = 6 then 2 else 6
```

## run result

- **one-shot** (unroll all, one solve): final = `{'idx': 8, 'best': 9}`  [18 vars]
- **incremental** (one step at a time, memory reuse): final = `{'idx': 8, 'best': 9}`  [4 vars, constant]

state trace (incremental):

```
{'idx': 0, 'best': -999} → {'idx': 1, 'best': 3} → {'idx': 2, 'best': 3} → {'idx': 3, 'best': 4} → {'idx': 4, 'best': 4} → {'idx': 5, 'best': 5} → {'idx': 6, 'best': 9} → {'idx': 7, 'best': 9} → {'idx': 8, 'best': 9}
```
