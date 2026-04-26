# Example 3: Type Checker — Dependent Claims and Composable Inference

A type checker for a tiny expression language. This example shows:
- Claims parameterized by runtime values (dependent types in spirit)
- How composable claims build complex inference from simple rules
- The "typing context" as an asserted fact, not a passed argument

---

## The tiny language

```evident
type Expr =
    | Lit Nat                      -- numeric literal
    | Bool Bool                    -- boolean literal
    | Add Expr Expr               -- addition
    | If Expr Expr Expr           -- conditional
    | Var String                  -- variable reference
    | Let String Expr Expr        -- let binding
    | Lam String Type Expr        -- lambda (annotated parameter)
    | App Expr Expr               -- function application

type Type =
    | Nat
    | Bool
    | Arrow Type Type             -- function type T1 -> T2
```

---

## Step 0: Declare the typing claim

```evident
claim has_type : Expr -> Type -> Prop
```

No rules yet. The solver knows nothing.

```evident
? has_type (Lit 42) ?t
```

```
-- Solver may return:
t = Nat        -- possible
t = Bool       -- also possible
t = Arrow Nat Bool  -- also possible

-- The literal 42 could be any type. Completely wrong.
```

---

## Step 1: Literal rules

```evident
evident has_type (Lit _) Nat
evident has_type (Bool _) Bool
```

```evident
? has_type (Lit 42) ?t
```

```
t = Nat   ✓

? has_type (Bool true) ?t
t = Bool  ✓

? has_type (Lit 42) Bool
-- Not evident. (42 is not a Bool)
```

---

## Step 2: Addition

```evident
evident has_type (Add e1 e2) Nat
    has_type e1 Nat
    has_type e2 Nat
```

```evident
? has_type (Add (Lit 1) (Lit 2)) ?t
t = Nat  ✓

? has_type (Add (Lit 1) (Bool true)) ?t
-- Not evident. (Bool true is not Nat, so Add fails)
```

---

## Step 3: Conditionals — types must agree

```evident
evident has_type (If cond then_ else_) t
    has_type cond Bool
    has_type then_ t
    has_type else_ t
```

The same `t` appears in both branches — the solver unifies them, ensuring both branches
have the same type. This is structural constraint propagation, not code.

```evident
? has_type (If (Bool true) (Lit 1) (Lit 2)) ?t
t = Nat  ✓

? has_type (If (Lit 1) (Lit 2) (Lit 3)) ?t
-- Not evident. (Lit 1 is Nat, not Bool — condition fails)

? has_type (If (Bool true) (Lit 1) (Bool false)) ?t
-- Not evident. (Nat ≠ Bool — branches must agree)
```

---

## Step 4: Variables — the typing context

Variables require a context: a mapping from names to types. In conventional type checkers,
this is a parameter threaded through every function. In Evident, it is an *asserted fact*.

```evident
-- Assert that in the current context, variable "x" has type Nat
assert var_type "x" Nat
assert var_type "f" (Arrow Nat Nat)
```

```evident
-- The claim: in the current context, a variable reference has the declared type
claim var_type : String -> Type -> semidet  -- declared via assert

evident has_type (Var name) t
    var_type name t
```

```evident
? has_type (Var "x") ?t
t = Nat  ✓

? has_type (Var "y") ?t
-- Not evident. ("y" not in context)

? has_type (Add (Var "x") (Lit 3)) ?t
t = Nat  ✓
```

---

## Step 5: Let bindings — extending the context

`Let "x" e1 e2` binds `x` to the value of `e1` in the body `e2`. The type of `x` in the
body is the type of `e1`.

```evident
evident has_type (Let name e1 e2) t
    has_type e1 t1                   -- infer the type of the bound expression
    with_binding name t1:            -- extend the context temporarily
        has_type e2 t                -- type-check the body in the extended context
```

The `with_binding name t1: ...` is a scoped context extension. It temporarily adds
`var_type name t1` to the evidence base for the sub-derivation, then retracts it.

This is the one place where Evident's monotonic evidence base needs scoped assertions —
we don't want `var_type name t1` to persist beyond the let-body's derivation.

```evident
? has_type (Let "y" (Lit 5) (Add (Var "y") (Lit 3))) ?t
t = Nat  ✓

-- The derivation:
--   has_type (Lit 5) Nat                         ← type of binding
--   [extend context: var_type "y" Nat]
--     has_type (Var "y") Nat                     ← uses new binding
--     has_type (Lit 3) Nat
--     has_type (Add (Var "y") (Lit 3)) Nat       ← body type
--   [retract var_type "y" Nat]
-- Result: Nat
```

---

## Step 6: Lambda and application — Arrow types

```evident
evident has_type (Lam param param_type body) (Arrow param_type return_type)
    with_binding param param_type:
        has_type body return_type

evident has_type (App fn arg) return_type
    has_type fn (Arrow arg_type return_type)
    has_type arg arg_type
```

The Arrow pattern: `App fn arg` has type `return_type` when `fn` has type
`Arrow arg_type return_type` and `arg` has type `arg_type`. The solver unifies
`arg_type` automatically — no explicit threading.

```evident
? has_type (Lam "n" Nat (Add (Var "n") (Lit 1))) ?t
t = Arrow Nat Nat  ✓

? has_type (App (Var "f") (Lit 3)) ?t
-- Context has: var_type "f" (Arrow Nat Nat)
t = Nat  ✓

? has_type (App (Var "f") (Bool true)) ?t
-- Not evident. (f expects Nat, got Bool)
```

---

## Composability: inference runs in both directions

The type checker as written also works as a **type-directed expression generator**.

```evident
-- What expressions have type Nat in this context?
? all has_type ?e Nat
-- e = Lit 0, Lit 1, ..., Add (Lit 0) (Lit 0), ..., Var "x", ...
-- (infinite — solver would need bounding)

-- What type does this expression have?
? has_type (Add (Lit 1) (Var "x")) ?t
t = Nat  ✓

-- Is there any type this expression can have?
? has_type (Add (Bool true) (Lit 1)) ?t
-- Not evident for any t. (Bool true is not Nat)
```

---

## Reuse: the same rules as a bidirectional type checker

```evident
-- Check: does this expression have this type?
? has_type (Lit 42) Nat    -- Yes ✓
? has_type (Lit 42) Bool   -- No

-- Infer: what type does this expression have?
? has_type (Add (Lit 1) (Lit 2)) ?t    -- t = Nat

-- Elaborate: what must the sub-expression type be?
? has_type (Add ?e (Lit 2)) Nat         -- e must have type Nat
-- solver generates constraints on e
```

The claim `has_type` is not a function from expressions to types. It is a **relation** between
expressions and types. The solver uses it in whichever direction the query requires.

---

## Parametric extension: a polymorphic language

To add parametric polymorphism (like `List[T]`), we extend the type system:

```evident
type Type =
    | Nat
    | Bool
    | Arrow Type Type
    | TypeVar String                  -- e.g. 'a', 'b'
    | ForAll String Type              -- ∀ a. T

-- Type instantiation: substitute a type variable
claim subst : String -> Type -> Type -> Type -> det

evident subst var replacement (TypeVar name) replacement when name = var
evident subst var replacement (TypeVar name) (TypeVar name) when name != var
evident subst var replacement (Arrow t1 t2) (Arrow t1' t2')
    subst var replacement t1 t1'
    subst var replacement t2 t2'
-- etc.

-- Typing a type application (instantiating a polymorphic type)
evident has_type (TypeApp e conc_type) result_type
    has_type e (ForAll var body)
    subst var conc_type body result_type
```

The parametric extension composes naturally with everything already written.
No existing rules need to change — new rules are added, old rules are unaffected.
This is the monotonicity property of Evident's rule system working in practice.
