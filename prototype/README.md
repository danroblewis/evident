# prototype — Python over Z3

Strategic restart (branch `prototype-z3-python`). We stripped the
`Evident → smt2 → Z3 → kernel` stack down to its bottom two layers and are
prototyping them directly in **Python controlling Z3**, keeping the Python as
thin as possible so the ideas can move *up* the stack later.

```
  (Evident surface)        ← later
        ↓
  (smt2 lowering)          ← later
        ↓
  Z3 + a small runtime     ← HERE, in Python, kept minimal
```

## What we're exploring

The constraint-modeling core from `../docs/`:

- **relations as sets of tuples** (`../docs/plans/relations-as-tuple-sets.md`) —
  dispatch / mappings / grammars as membership in a set, not control flow.
- **claims as sets** + the set algebra (`../docs/plans/claims-as-sets.md`).
- **under-determined, bounded solution spaces** — partial constraint, the
  solver fills the rest; lazy function-images (project, don't expand).
- **the runtime loop** — `init + Done → solve → effect trace` (bounded model
  checking / planning-as-SAT), the layer that decomposes large models so Z3
  doesn't have to swallow them whole.

## Verified (see `00_smoke.py`)

Python Z3 4.15.4 is installed and the pieces we care about work: under-determined
solves return a witness; sets are native (`SetSort`, `EmptySet`, `SetAdd`,
`SetUnion`/`Intersect`/`Difference`, `IsMember`, `IsSubset`); relations solve.

## Run

```
python3 prototype/00_smoke.py
```

(`z3` Python bindings via the system package; the `z3` CLI is also on PATH.)
