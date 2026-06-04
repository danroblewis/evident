# Wave 4o — fix `sample.smt2` emit bugs

Closes the two real bugs wave-4m's lex-once `sample` driver shipped with
(its verification phase never ran), plus removes the `/tmp` race the 4m
band-aid (`mkdir` lock, commit `7ecc95e`) only papered over.

## The bugs (reproduced on `tests/lang_tests/test_cons_chain_lit.ev`)

Pre-4o, that 1-claim + 1-enum file produced **6** `;; claim:` markers and
6 `(check-sat)` blocks (bootstrap emits 1):

```
;; claim: sat_user_intlist   ← the real claim (partial body)
;; claim:                    ← 4 empty-name blocks …
;; claim:
;; claim:
;; claim:
;; claim: ICons              ← a stray variant-name block
```

### Root cause (Items 1 + 2 are one bug)

The membership walk stops early on a shape it cannot translate (Wall 2 —
here `nums = ⟨10, 20, 30⟩` on an enum-typed var). On `claim_done` the
driver set `items = _rem`, i.e. the **leftover mid-claim tokens**. The
DISPATCH sub-mode then re-entered claim/skip mode on those leftovers,
emitting one spurious `(push)/(check-sat)/(pop)` block per fragment:
empty-name blocks for the un-named fragments, and a `;; claim: ICons`
block when a fragment's name slot happened to land on the `ICons` ident
from the body's `nums = ICons(…)` line.

The wrapper zips the `;; claim:` markers against z3's sat/unsat lines, so
the extra blocks produced garbage JSON.

### Fix

Only a real top-level keyword head starts an item:

```
head_is_claimkw = items_hd matches (KwClaim | KwType | KwSchema | KwFsm)
enter_claim / enter_skip  now require head_is_claimkw
skip_junk    = in_dispatch ∧ ¬items_nil ∧ ¬head_is_enum ∧ ¬head_is_claimkw
```

`skip_junk` drops ONE leftover token per tick (`items = items_d1`,
`pmode` stays 0) and re-dispatches, draining a partially-walked claim's
tail **without emitting**. Result: exactly one `;; claim: <name>` per
real claim, no enum markers, no empty names. (`compiler/sample.ev` only —
`compiler.ev` emits once at `all_done`, so it never produced markers; its
single-claim corpus walks consume all tokens cleanly, so the leftover
re-dispatch never fires there.)

## Item 3 — the `/tmp/compiler-input.ev` race

Both drivers baked a literal `ReadFile("/tmp/compiler-input.ev")`. Under
parallel `run-lang-tests.sh`, each wrapper's `cp $FLAT /tmp/...` clobbered
every other's mid-run; the 4m band-aid serialized them with a `mkdir`
lock (killing the parallelism win).

Replaced with a tiny **stdin protocol** (option a — per-process path):

- tick 0 (`is_first_tick`) — `⟨ReadLine⟩` (compiler.ev: `⟨ReadLine, ReadLine⟩`).
  stdin line 1 = a per-process flat source path (a wrapper `mktemp` path);
  line 2 (compiler.ev only) = the optional target claim.
- tick 1 (`¬_got_path`) — `⟨ReadFile(src_path)⟩`, a **dynamic** path the
  kernel model-evaluates (`Z3_model_eval` with completion already does
  this for effect args). compiler.ev also captures `target` here.
- tick 2+ (`_got_path`) — `input` = the file content; the lexer runs.

`got_path` is a one-bit carry (false on tick 0, true after), so
`¬_got_path` selects exactly the ReadFile tick. No shared external state →
parallel invocations cannot collide. `compiler.smt2` / `sample.smt2` no
longer contain the `/tmp/compiler-input.ev` string at all.

Wrappers updated to pipe the path instead of `cp`-ing it (and the `mkdir`
lock is gone):

- `scripts/sample-via-smt2.sh` — `printf '%s\n' "$FLAT" | kernel sample.smt2`
- `scripts/evident-self` (`emit_via_smt2_wrapper`) — `printf '%s\n%s\n' "$FLAT" "$CLAIM" | …`
- `scripts/diff-vs-bootstrap.sh` — same (kept consistent; dev tool, not in test.sh)

## Verification

- **Single-file probe — byte-equal to bootstrap.** sample.smt2 on
  `test_cons_chain_lit.ev` now emits exactly one `;; claim: sat_user_intlist`
  block; `sample-via-smt2.sh … --all --json` ⇒ `{"sat_user_intlist":true}`,
  identical to bootstrap.
- **compiler.ev emit via stdin** produced the correct manifest + prelude +
  `(assert (= a b))` for a scalar file (stdin-path protocol works end-to-end).
- **Fixture:** `tests/kernel/test_sample_driver_marker_count.ev` — an exact
  copy of `compiler/sample.ev`, fed a 1-enum + 1-claim source (with a
  trailing unsupported line that exercises `skip_junk`) via stdin (see
  `run-kernel-tests.sh::setup_fixture`). Pins exactly one `;; claim:` block.
- **Full `--lang` under the seam** (`EVIDENT_SELF_VIA_SMT2=1 test.sh --lang`):
  parallel again (the lock is gone); see RESULTS below.

## Results

<!-- FILLED IN AFTER MEASUREMENT -->
