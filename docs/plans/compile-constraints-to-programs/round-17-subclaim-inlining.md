# Round 17 — Subschema (method-style) call inlining

**Outcome:** STRUCTURAL CHANGE. `inline_positional_calls` now
handles method-style calls `recv.method(args)` by inlining the
called subclaim's body with arg substitution AND field-rebinding
of the receiver's parent-type fields onto bare names.

This unblocks Mario's `display` FSM at the subclaim-dispatch gate.
The next blocker for display is `∀ (p, b) ∈ coindexed(...)` —
∀-over-Seq, which is on the Round 18 list.

## What changed

`inline_positional_calls` previously handled only plain claim
calls. Method-style calls `win.set_draw_color((c), out)` weren't
recognized (the dotted name didn't resolve to a top-level claim)
and stayed in the body, tripping the gate's `body Call` refusal.

Round 17 adds the missing branch:

1. If `name` contains a `.`, split into `recv_name` + `method_name`.
2. Find `recv_name`'s declared type in the outer body's Memberships.
3. Look up the type's `SubclaimDecl` with `method_name`.
4. Extract the subclaim's first N Memberships as positional params.
5. **Field-rebind**: every bare identifier in the subclaim body
   that matches a top-level Membership of the parent type (e.g.,
   `renderer`, `handle`, `keyboard_state` for SDL_Window) gets
   rewritten to `recv_name.<field>`. The Z3 path does this by
   adding bare-name aliases in env; we do it AST-side via
   `substitute(body, field_name, Identifier("recv.field"))`.
6. Param substitution: args replace the param names.
7. Recursively inline calls inside the substituted body.

For `win.set_draw_color((220, 40, 60, 255), out_eff)`:
```text
Inputs:
  win ∈ SDL_Window
  win.set_draw_color(c, out_eff)
  
Subclaim body (set_draw_color):
  out = LibCall(..., renderer, color.red, color.green, …)

After field-rebinding (renderer → win.renderer):
  out = LibCall(..., win.renderer, color.red, …)

After param substitution (color → c, out → out_eff):
  out_eff = LibCall(..., win.renderer, c.red, …)
```

The result is a pure equality that the function-izer's chain
extraction handles.

## Mario rejection progress

```
                Before R17                          After R17
display:    body Call win.set_draw_color      Forall (non-static bounds)
keyboard:   (slow-path classify)              (slow-path classify, same as before)
game:       Forall (non-static bounds)        Forall (non-static bounds)
level_gen:  non-Eq Binary op Implies          non-Eq Binary op Implies
```

Display is now PAST the gate's call-related refusals. It hits the
∀-over-Seq pattern (`∀ (p, b) ∈ coindexed(platforms, plat_effs) :
win.draw_rect(...)`) — same blocker as `game`.

The deeper investigation needed for keyboard's slow-path classify
rejection (Round 16's open question) is unchanged.

## Test impact

- All 444 cargo + 119 conformance pass in both modes.
- Cross-example HIT count unchanged (no demo crossed the new
  threshold — Mario's the only complex enough demo to need
  subclaim dispatch, and it has more blockers behind it).

## Round 18 candidate

**∀-over-Seq unrolling.** With seq-length pinning from body
declarations (`#plat_effs = 4`, etc.), `coindexed(seqA, seqB)`
can unroll into N concrete substitutions. The `expand_foralls`
pass already handles ∀-over-Range; extending it for ∀-over-Seq
+ coindexed requires Field/Index expression evaluation in the
unrolled body (each iteration substitutes `p` with `seq[i]`
which expands to `Index(seq, i)`).

This unblocks both `display` and `game` simultaneously — they
both fail on the same pattern. After it lands, the remaining
Mario blocker is `level_gen`'s `Implies`.
