# FTI lexer — spike results + driver-integration contract

Status: INTEGRATED (P3d, 2026-06-07). `compiler2/lex_fti.ev` +
`tests/kernel/compiler2/lex_fti_fixture.ev` proved the FTI token
buffer from `docs/plans/sample-ev-fti-pivot.md` end-to-end on the
driver's own lexer FSM shape; compiler2/driver.ev now uses it (the
cons-list lexer + REVERSE phase are deleted; the parse phases read
the buffer through an 8-token cursor window). See the P3d section
of docs/plans/compiler2-driver-notes.md for the integration design
+ the 22-fixture acceptance run. The recipe below is kept as the
record of what was de-risked ahead of that wiring.

## What was proven (all kernel-run, oracle-built)

- **lex_fti_fixture.ev: PASS, exit 0.** Lexes an 8-line snippet
  (keyword, idents, IntLits, negative fold `= -3`, binary `10 - 2`
  NOT folded, ASCII ops, unicode glyphs ∈ ≥ ∧ ≠ ⟨ ⟩, parens, a
  StringLit, a comment line) into a calloc'd FTI buffer — 41 tokens
  + EofTok sentinel — then walks the buffer with read_long /
  __cstr.copy and verifies every (tag, payload) entry against the
  hand-tokenized expectation, including the 15 string payloads
  verbatim. 209 ticks, ~0.75 s wall (functionizer-residual shapes,
  the known P2 finding).
- **Negative controls: PASS.** Corrupting one expected tag → exit
  113 (= 100 + k); one int payload → 164 (= 150 + k); one string →
  232 (= 200 + k). Exit-code map: 99 count mismatch, 100+k tag,
  150+k payload0, 200+k string, 0 all-match.
- **Tick/wall vs the cons-list lexer** (same snippet, lex-only
  variants): FTI 110 ticks / 0.40 s vs fossil cons + REVERSE
  151 ticks / 0.43 s. The 41-tick delta is EXACTLY the REVERSE pop
  loop (one tick per token) — append order eliminates the phase.
  Per-tick cost is comparable at this scale; the asymptotic win
  (Z3 term-store/pin-string growth on long token lists) is the
  pivot doc's motivation and is not exercised by a 41-token
  snippet. The cons variant's own count check (41) independently
  cross-validated the expectation table.
- Supporting probes: unicode string literals are CODE-POINT indexed
  through oracle + kernel + Z3 (substr/#/index_of consistent with
  per-char `=` against glyph literals); `\n` and `\"` escapes work
  in oracle-compiled literals; `LibCall("libc","strdup",⟨ArgStr(s)⟩)`
  returns a usable char* the kernel marshals; `last_results[1]`
  capture works; a 26-slot claim composition (LexFtiPlan) translates
  correctly, twice in one host claim.

## Buffer layout (per sample-ev-fti-pivot.md, unchanged)

One calloc'd region. Token k lives at `base + k*32`, 4 i64 slots:

| offset | slot | content |
|--------|------|---------|
| +0  | tag      | TokenTag_* integer (stdlib/fti/token_stack.ev) |
| +8  | payload0 | IntLit value, OR malloc'd char* (Ident=1, StringLit=3) |
| +16 | payload1 | reserved (FloatLit fractional — not produced yet) |
| +24 | payload2 | reserved (FloatLit frac-digit-count) |

- **calloc, not malloc** — zeroed slots make nullary-token payloads
  and past-the-end peeks deterministic (tag 0 = "nothing").
- Tags: token_stack.ev's table, now complete for the driver's
  single-char set (Pipe=65, Question=66, Hash=67, Colon=68 appended).
  The driver lexer emits NO two-char tokens (`<=` is Lt,Eq) and NO
  Newline/Indent tokens — same as the fossil cons lexer it replaces.
- An **EofTok (13) sentinel** is written at entry `tokens_count` on
  the lex_done tick; it is NOT counted in `tokens_count`. Parser
  peeks ≤ 1 entry past the end read 13; further past read 0. The
  robust parser pattern is still `k < tokens_count ? tag_at(k) : 13`.

## How the parser reads token k

- tag:      `__mem.read_long(tokens_base + k*32)`
- payload0: `__mem.read_long(tokens_base + k*32 + 8)`
- string payload (tags 1, 3): payload0 is a NUL-terminated char* —
  `LibCall("__cstr","copy",⟨ArgInt(p0)⟩)` → StringResult next tick.

Each read is an effect with next-tick latency (results land in
`last_results` in effect order). A cursor advance therefore costs
one read-tick before the tag is branchable — the pivot doc's known
cost (its "TokenWindow" caching discussion applies to the parser
side, not to this lexer).

## Z3-carried lexer state (the whole point)

Five Ints replace the TokenList: `tokens_base`, `tokens_count`,
`last_tag`, `prev_tag` (negative-fold cache), `pend` (pending
string-pointer slot address, -1 = none). Plus the fossil scanner's
existing pos/partial_* fields, unchanged. Zero datatype state; zero
cons-list pin strings.

## Driver integration recipe (post-Pratt-merge)

`compiler2/lex_fti.ev` exports `LexCharTag`, `LexKeywordTag` (Int-tag
twins of SingleCharTok/MaybeKeyword) and `LexFtiPlan` (the per-tick
push plan). The fixture's `lex_fti_main` is the wiring template —
the driver edit is mechanical:

1. Tick-0 effects gain the buffer alloc:
   `⟨ReadLine, ReadLine⟩` stays; the calloc can ride the ReadFile
   tick or its own ZINIT step (`calloc(512, 32)` = 512 tokens for a
   flattened claim file; v1 over-allocates, no realloc). Capture
   `tokens_base` from last_results like every other ZINIT handle.
2. Keep the fossil scanner block (pos / partial_* / branch bools)
   verbatim. Replace `SingleCharTok` with `LexCharTag` (recognized
   is identical; the Token value was only consumed by the cons
   push). Delete the `tokens` cons update and the tk_p1/tk_p2
   fold peeks — `LexFtiPlan` decides the fold from last_tag/prev_tag.
3. Bind `LexFtiPlan` once; carry count/last_tag/prev_tag/pend per
   the host contract in lex_fti.ev's header; add the kind-indexed
   effect branches (7 fixed-shape seq literals + the pend prepend)
   to the driver's effects ternary, replacing the lex filler branch.
   Ordering invariants the verification leaned on: strdup is always
   effects[0] of its tick; a pending write can only co-occur with
   kind 0/1 ticks and is emitted before that tick's tag write.
4. On the lex_done tick: flush `pend` if set, write the EofTok
   sentinel. DELETE the whole REVERSE phase (`work`/`fwd`/pmode-1);
   `entering_parse` triggers on lex_done directly (phase 0 → 2).
5. PARSE reads tokens by cursor (`_items_cursor` etc. per the pivot
   doc's state-field map) instead of TLHd/TLTl matches: one
   read-effect tick per peek, cached tag/payload fields per the
   pivot doc's stage-4 discussion.

Step 5 is the real work (the membership-step sub-claims still
`match Token`); steps 1-4 are contained in driver_main's LEX/REVERSE
sections and this spike has de-risked all of them.

## Faithfully-carried fossil quirks (NOT fixed here)

The spike replicates the driver lexer's semantics bit-for-bit,
including: digit-bearing idents lex wrong (`p0` → Ident("p") dropped
+ IntLit(0) — is_alpha excludes digits, the ident is silently
discarded at the digit); an ident directly followed by `"` is
dropped; no `\` escapes inside string literals; FloatLit not lexed
(`3.14` → IntLit·Dot·IntLit, the 021 descope). Fixing any of these
belongs to the driver lexer proper, not the FTI plumbing — the plan
claims only consume the branch bools.

## v1 limitations (inherited stances)

- strdup'd string payloads leak until process exit (token_stack.ev
  v1 stance; bounded by source size).
- Fixed buffer capacity, no realloc (pivot doc v1 stance).
- The buffer itself is freed never in the fixture (process exit);
  the driver should `free(tokens_base)` on its terminal tick per
  the pivot doc's halt-path note.
