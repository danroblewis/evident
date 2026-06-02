# M8 — the closing demo

The final milestone of the prelude plan. Goal: an Evident program
that uses the prelude (Z3 FTI + Formula + the set-theoretic
extensions from M6) to solve a real constraint problem.

This doc sketches what the demo should look like once M5 + M6 land.

## Candidate puzzles

### 4×4 Sudoku

A 4×4 Sudoku has 16 cells, each holding an integer 1–4, with row,
column, and 2×2-box constraints. Smaller than full 9×9, fits in a
demo.

Pros:
- Familiar to most programmers
- Concrete observable answer (print the grid)
- Exercises arithmetic + distinct + ranges

Cons:
- Easier than 9×9 might feel underwhelming
- The constraint encoding is verbose (16 cells × 3 constraint families)

### Zebra puzzle

The classic logic puzzle with 5 houses, 5 colors, 5 nationalities,
5 pets, 5 drinks, 5 cigarette brands. Constraints like "the
Norwegian lives in the first house," "the Englishman lives in the
red house," etc.

Pros:
- Beautiful test of set-theoretic constraints
- One unique solution, satisfying to compute
- ~14 constraints, manageable size

Cons:
- Verbose to encode
- Needs uniqueness constraints (`distinct` on each attribute family)

### N-queens (4×4)

Place 4 queens on a 4×4 board with no two attacking each other.

Pros:
- Tiny, fits on one screen
- Demonstrates 2D constraint encoding
- The output (queen positions) is small

Cons:
- 4×4 only has 2 solutions; not as satisfying as a unique-solution puzzle

## Recommendation: 4×4 Sudoku

Smallest demo that's still recognizably "a real solver problem."
~30 cells of constraints, output is a 4×4 grid of digits.

## Sketch of the demo program

```
fsm sudoku4()
    z ∈ Z3
    phase ∈ Int

    phase = _phase + 1

    z.formulas = match _phase:
        0 =>
            ; Declare the 16 cell variables c00..c33, each in {1..4}.
            ; Row constraints: each row's 4 cells are distinct.
            ; Column constraints: each column's 4 cells are distinct.
            ; Box constraints: each 2×2 box's 4 cells are distinct.
            ; Plus the given cells from the puzzle.
            [
                ; Range constraints (each cell in 1..4)
                Ge(Var("c00", "Int"), IntLit(1)),
                Le(Var("c00", "Int"), IntLit(4)),
                ; ... 15 more
                ; Distinct constraints — encoded as pairwise inequality
                Not(Eq(Var("c00", "Int"), Var("c01", "Int"))),
                ; ... many more
                ; Given clues
                Eq(Var("c00", "Int"), IntLit(3))
                ; ...
            ]
        _ => _z.formulas

    effects = match z.sat:
        Sat =>
            ; Print the solved grid by reading the model and emitting puts.
            [LibCall("libc", "puts", "i(s)",
                     [ArgStr("SAT — see model")], "", ""),
             LibCall("libc", "puts", "i(s)",
                     [ArgStr(model)], "", "")]
        Unsat => [LibCall("libc", "puts", "i(s)",
                          [ArgStr("UNSAT")], "", "")]
        _ => []
```

Length: probably ~150 lines once written out. The bulk is constraint
enumeration. The constraint generation may benefit from M6's
quantifiers, which would let us write:

```
; Each cell in 1..4
Forall("i", "Int",
    Forall("j", "Int",
        Implies(And(SetMember(Var("i", "Int"), Range(0, 3)),
                    SetMember(Var("j", "Int"), Range(0, 3))),
                And(Ge(Cell(i, j), IntLit(1)),
                    Le(Cell(i, j), IntLit(4))))))
```

But quantified arithmetic over array-indexed variables is hard in
SMT — we may end up unrolling anyway.

For v1, stick to the unrolled form (16 cell vars, 16 range pairs,
many distinct pairs). It's verbose but the demo is about showing
that the prelude WORKS for a real problem, not about elegance.

## What the demo proves

When `examples/sudoku4.ev` solves a real 4×4 Sudoku and prints the
solution, we have:

- Z3 FTI working: declare formulas, get a sat result, read the model.
- Formula datatype rich enough: Var, IntLit, Ge, Le, Not, Eq, And.
- The relational programming model in practice: the user writes
  constraints, Z3 finds the assignment.
- Two-tick latency manageable: a small phase machine drives the
  ask→check→read loop.

All without writing a single libcall by hand at the user-program
level. That's the bar from the prelude plan.

## Acceptance

```
$ python3 src/main.py examples/sudoku4.ev
SAT — see model
(declare-fun c00 () Int)
(declare-fun c01 () Int)
...
(define-fun c00 () Int 3)
(define-fun c01 () Int 1)
...
```

The exact model output format is whatever `Z3_solver_to_string`
produces.

## What's NOT in the demo

- Pretty-printing the grid as a 4×4 visual. The user can do that
  by writing more libcalls; v1 just prints the raw SMT-LIB model.
- Multi-solution enumeration. v1 finds one solution.
- Optimization (find the "best" solution). Out of scope.
- Performance benchmarking. The demo's bar is "solves correctly,"
  not "solves fast."

## Implementation notes

Once M5 and M6 are done:
1. Write `examples/sudoku4.ev` with the puzzle hardcoded.
2. Run it; observe SAT and a model.
3. Manually verify the model satisfies the puzzle.
4. If it does, M8 is done.

Estimated time: ~30 minutes of writing constraints, ~10 minutes of
debugging. Total ~40 minutes.
