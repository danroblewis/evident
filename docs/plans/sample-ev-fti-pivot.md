# sample.ev TokenList → FTI pivot — design + migration plan

Memory growth diagnosis (confirmed): every tick the kernel reads each
`TokenList` state-field's value out of the Z3 model, re-emits it as a
nested `(TLCons t1 (TLCons t2 ...))` pin string, and parses it back
into AST nodes which accumulate in Z3's term hash-cons table. For a
real compile (~6000 ticks, ~50 avg list length per field, 33 list
state-fields in sample.smt2), that's ~10M cons-cell ASTs per run —
hundreds of MB of RSS, unbounded over time.

The fix already proven at the runtime layer:
`tests/kernel/wave-5-fti/token_stack_proof.smt2` runs the kernel +
libc + `__mem` chain end-to-end with two Ints in Z3 state (base +
depth) carrying an unbounded token sequence stored in libc memory.

This doc plans the source-level pivot of compiler/sample.ev to use
that pattern. (compiler/compiler.ev has the same shape and needs the
same pivot — see "Cascade" below.)

## Architectural shift

Today every TokenList state-field requires Z3 to materialise the
list each tick. Replace each one with EITHER:

- An (Int, Int) pair: `<name>_base` (malloc'd buffer pointer) +
  `<name>_count` (size), if the field is genuinely accumulative.
- An Int: `<name>_cursor`, if the field is a read-only walk position
  into the SAME buffer another field allocated.
- DELETED: if the field was just a derived peek (e.g. `r_l1` is
  `TLTl(_rem)` — replaceable by `tokens_cursor + 1`).

The lexer's APPEND-then-REVERSE cons-list idiom (prepend during LEX,
walk to reverse in a dedicated REVERSE phase) goes away: FTI is
append-friendly so the buffer is in source order to start with.
Whole `pmode 1 REVERSE` phase gets deleted.

## State-field map

Tabular view of every TokenList field in sample.ev (line numbers from
current HEAD) and its replacement:

| Old field      | Line | Role                                          | New shape                                         |
|----------------|------|-----------------------------------------------|---------------------------------------------------|
| `_tokens`      | 208  | LEX accumulator (reverse-prepend per token)   | `tokens_base ∈ Int` + `tokens_count ∈ Int`        |
| `work_tl`      | 263  | REVERSE phase one-off tail                    | DELETE (REVERSE phase gone)                       |
| `work`         | 272  | REVERSE work cursor                           | DELETE                                            |
| `_work`        | 273  | REVERSE work carry                            | DELETE                                            |
| `_fwd`         | 275  | post-REVERSE forward token stream             | DELETE (use `tokens_base` directly)               |
| `_items`       | 309  | top-level item walker                         | `_items_cursor ∈ Int`                             |
| `items_d1`     | 389  | `TLTl(_items)` — peek                         | DELETE (use `_items_cursor + 1`)                  |
| `items_d2`     | 391  | `TLTl(TLTl(_items))` — peek                   | DELETE (use `_items_cursor + 2`)                  |
| `_rem`         | 395  | membership walk cursor inside a claim         | `_rem_cursor ∈ Int`                               |
| `r_l1`..`r_l4` | 402+ | dispatch peek tails of `_rem`                 | DELETE (cursor + offset)                          |
| `cts_rest`     | 428  | CtorMembershipStep return cursor              | `cts_rest_cursor ∈ Int` (returned by sub-claim)   |
| `step_rest`    | 443  | MembershipStep return cursor                  | `step_rest_cursor ∈ Int`                          |
| `sqs_rest`     | 453  | SeqMembershipStep return cursor               | `sqs_rest_cursor ∈ Int`                           |
| `mts_rest`     | 461  | MatchMembershipStep return cursor             | `mts_rest_cursor ∈ Int`                           |
| `sel_rest`     | 469  | selector across the four above                | `sel_rest_cursor ∈ Int`                           |
| `_plist`       | 529  | enum-parse accumulator                        | `_plist_cursor ∈ Int` (or its own buffer)         |
| `ptail`        | 569  | `_plist` tail peek                            | DELETE (cursor + 1)                               |
| `_skipl`       | 804  | skip-mode token list                          | `_skipl_cursor ∈ Int`                             |
| `skipl_tl`     | 807  | `_skipl` tail peek                            | DELETE                                            |

Net: **20 TokenList state-fields → 2 (buffer base+count) + ~8
cursor Ints + several DELETEs**. Per-tick pin string drops from
"33 nested cons-lists" to "10 Ints".

## Token encoding (mirrors stdlib/fti/token_stack.ev)

Per-token entry: 32 bytes / 4 i64 slots at offset `i * 32`:
- slot 0 (offset +0):  tag (TokenTag_* discriminator)
- slot 1 (offset +8):  payload0 (IntLit value / FloatLit whole / string ptr)
- slot 2 (offset +16): payload1 (FloatLit fractional)
- slot 3 (offset +24): payload2 (FloatLit frac-digits)

Nullary tokens: tag only, rest 0.
IntLit: tag + value in slot 1.
FloatLit: tag + 3 ints in slots 1/2/3.
Ident/StringLit/ErrTok: tag + string ptr in slot 1; string is
malloc'd separately and the pointer captured into slot 1 via a
second `write_long` effect. v1 leaks the strings until process exit.

## Phase changes

### Tick 0 effects

Was: `⟨ReadLine, ReadLine⟩` (read source path + target claim from stdin).

Now: `⟨ReadLine, ReadLine, LibCall("libc", "malloc", ⟨ArgInt(16384)⟩)⟩`.

Tick 1 captures `tokens_base` from `last_results[2]` (the malloc
return) and proceeds with the existing ReadFile + lex setup. 16 KB
buffer = 512 tokens, generous for a typical claim. (TODO: realloc
on overflow — v1 just over-allocates.)

### LEX section (currently lines 191-218)

Per tick, the lex emits 0/1/2 tokens. Replace `TLCons` with
write_long effects at `tokens_base + tokens_count * 32`. The token
constructor stays in Evident (just the tag-to-Int conversion); the
push effects go into the `effects = ⟨…⟩` line.

Sketch for the simplest case (`char_only` → push one nullary
operator token):

```evident
-- New state fields
tokens_base   ∈ Int
_tokens_base  ∈ Int
tokens_count  ∈ Int
_tokens_count ∈ Int

tokens_base = (is_first_tick ? 0
            : (phase = 1 ? ir_at(2) : _tokens_base))

push_count ∈ Int = (lx_comment ? 0
                  : strlit_closing ? 1
                  : finish_str_with_op ? 2
                  : finish_int_with_op ? 2
                  : finish_str_only ? 1
                  : finish_int_only ? 1
                  : char_only ? 1
                  : 0)

tokens_count = (is_first_tick ? 0 : _tokens_count + push_count)
```

The effects emission needs to issue the right write_long calls
based on which branch fires. For one-token branches:

```evident
push_tag ∈ Int = (strlit_closing ? <StringLit_tag>
                : finish_str_only ? <Ident_tag>      -- (or whatever the classified_str token tag is)
                : finish_int_only ? <IntLit_tag>
                : char_only ? <op_tok_tag>
                : 0)

-- Effects: existing tick-2+ filler plus the per-tick token push(es).
```

This expands the `effects = ⟨…⟩` line — currently emits a single
filler `LibCall("libc","getpid",⟨⟩)` on lex ticks — to emit 1-2
write_long calls when pushing, falling back to the filler when
push_count = 0.

### REVERSE phase (currently pmode 1)

DELETED ENTIRELY. The buffer is already in source order because we
append rather than prepend. Code that previously waited for
`entering_parse` after REVERSE completes now triggers on `lex_done`
directly. `_pmode = 1` transition gone; pmode goes 0 → 2 directly.

### PARSE phase

Replace TLHd peeks with cached `cur_tag` state:

```evident
-- New cached state for the current cursor head
items_cur_tag ∈ Int
_items_cur_tag ∈ Int
items_cur_p0  ∈ Int
_items_cur_p0 ∈ Int

-- Refreshed when cursor advances. Read effect emitted previous tick.
items_cur_tag = (is_first_tick ? 0
              : items_cursor_advanced ? ir_at(<read_slot>)
              : _items_cur_tag)
```

Each `TLHd(_rem, out ↦ r_t0)` site reads from `_items_cur_tag` instead
of pattern-matching a Token datatype.

Each `TLTl` (advancing the cursor) becomes
`items_cursor = _items_cursor + 1` plus a read effect for the new
position emitted this tick (captured next tick).

This DOUBLES tick count in the parse phase (one tick per token
advance instead of same-tick TLTl). Accept it; the savings on the
LEX side dominate.

### Per-step rest cursors

`CtorMembershipStep`, `MembershipStep`, `SeqMembershipStep`,
`MatchMembershipStep` claims (in parse_body_ctor.ev,
parse_body.ev, parse_body_seq.ev, parse_body_match.ev) currently
take `rem ∈ TokenList` and return `rest ∈ TokenList`. They need to
be rewritten to take `rem_cursor ∈ Int` and return
`rest_cursor ∈ Int`. The actual peeks inside those claims become
`__mem.read_long` effects at offsets from the buffer base.

**This cascades.** The membership-step sub-claims need their own
pivot before sample.ev can fully use them. Order matters:

1. Pivot the leaf sub-claims first (parse_body.ev,
   parse_body_ctor.ev, parse_body_seq.ev, parse_body_match.ev) to
   take cursor Ints.
2. Then pivot sample.ev's body to use the cursor-returning versions.

### Halt path

On the terminal tick (where sample.ev emits `⟨puts(smtlib), Exit(0)⟩`),
add `BuildTokenStackFree(_tokens_base)` to the effect Seq. This was
the third honesty-audit gap; v1 of the pivot fixes it.

## Cascade — compiler.ev needs the same treatment

compiler/sample.ev and compiler/compiler.ev share the lex+parse
shape; both have the same TokenList state-fields. The pivot here is
for sample.ev because lang_tests is the most common path through
the seam. The compiler.ev pivot is identical in shape and must
follow.

ORDER MATTERS: pivot compiler.ev FIRST, rebuild compiler.smt2 once
(the painful slow rebuild). Then sample.ev's rebuild uses the new
compiler.smt2 (cheap). Otherwise the sample.ev rebuild still uses
the old leaky compiler.smt2 and OOMs.

## Stages

1. **stdlib/fti/token_stack.ev**: DONE (commit 3898806).
2. **Token encoding tag table**: DONE (in stdlib/fti/token_stack.ev).
3. **kernel/src/libcall.rs `__mem`**: DONE (existing).
4. **Pivot the four membership-step sub-claims** to take/return
   cursor Ints instead of TokenList. (Next session.)
5. **Pivot compiler.ev** (the bootstrap-equivalent — generates
   compiler.smt2).
6. **One painful rebuild** of compiler.smt2 via the existing
   compiler.smt2 (hours, OOM-prone — accept it).
7. **Pivot sample.ev** with the new compiler.smt2 available.
8. **Cheap rebuild** of sample.smt2 (the new compiler.smt2 has FTI
   lex, so its own memory is bounded).

## Validation

After each pivot stage, run:
- `tests/kernel/wave-5-fti/token_stack_proof.smt2` — the FTI runtime is sound.
- `scripts/run-seam-smoke.sh` — basic kernel + compiler.smt2 still emits.
- A specific TokenList-heavy fixture (e.g.
  `tests/kernel/test_compiler_driver_canonical_*.ev`) — exercises a
  full lex+parse+translate roundtrip.

If a rebuild OOMs, that's information: the pivot at THAT stage
wasn't aggressive enough; iterate.

## Out of scope (for this pivot)

- String content lifetime tracking. Strings malloc'd by Ident/
  StringLit/ErrTok leak until process exit. The kernel process is
  short-lived for the compile flow so this is bounded by source
  size. Proper ownership (free on pop, free on stack-teardown) is
  a v2 polish task.
- Buffer growth (realloc on overflow). v1 over-allocates a fixed
  size (16 KB / 512 tokens). Realloc support is a v2 task.
