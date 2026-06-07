# sample.ev gap census — what compiler2's driver must grow to compile the sample corpus

Date: 2026-06-07. Baseline: compiler2 driver post-P3d (22 census
fixtures green; see docs/plans/compiler2-driver-notes.md). Target
corpus: `compiler/sample.ev` plus its full import graph, flattened by
`scripts/flatten-evident.sh compiler/sample.ev` (6,259 lines raw).

## Method

All counts below are actual greps over the flattened corpus with
comment lines stripped (`grep -v '^\s*--'` + trailing `-- …` removed),
yielding 3,997 code lines. Each grep pattern is stated with its count.
Counts are occurrence counts unless marked "distinct" or "lines".
Example lines cite the original source file:line (pre-flatten).

The corpus files and their code-line sizes (non-comment, non-blank):

| file | LOC | | file | LOC |
|---|---|---|---|---|
| compiler/sample.ev | 601 | | compiler/translate_seq.ev | 90 |
| compiler/parse_body.ev | 462 | | compiler/translate.ev | 88 |
| compiler/translate_ctor.ev | 348 | | compiler/translate_ternary.ev | 65 |
| compiler/parser.ev | 292 | | compiler/translate_declare.ev | 29 |
| compiler/translate_scalar_expr.ev | 283 | | stdlib/kernel.ev | 90 |
| compiler/lexer.ev | 215 | | compiler/parse_body_match.ev | 122 |
| compiler/translate_arith.ev | 197 | | compiler/parse_body_ctor.ev | 105 |
| compiler/parse_body_seq.ev | 177 | | compiler/parse_body_call.ev | 112 |
| compiler/translate_bool.ev | 120 | | | |

Top-level shape: 127 `claim` decls (68 bare-head, 59 parametrized),
31 `enum` decls, 0 `type` decls.

## What the corpus does NOT use (verified absent)

These constructs fail under the fossil census but are **not needed**
to compile sample.ev — they can stay descoped for this milestone:

- Quantifiers: 0 uses (`∀`/`∃` appear only as char literals inside the
  lexer's alphabet strings, e.g. compiler/lexer.ev:221). Census
  038–040 stay out of scope.
- Generics `<T>`: 0. (080 out of scope.)
- Record `type` decls / record lifts / tuples / method dispatch: 0.
  (081–093, 116–138 out of scope.)
- `..Passthrough`: 0. `subclaim` / `external`: 0 (the words appear
  only inside MaybeKeyword's string literals).
- `⇒` as a constraint operator: 0. All 367 non-string-literal `⇒`
  occurrences are match arms (369 total, 2 in char literals).
  Indentation-aware implies blocks stay descoped.
- Real-typed memberships: 0 (`∈ Real` never appears). BUT the Real
  *sort* is still required: `ArgReal(Real)` / `RealResult(Real)` are
  enum payload fields (stdlib/kernel.ev:17,62) — see gap C1.
- FloatLit: 0. Seq concat `++` on Seqs: 0 (every `++` is string
  concat — `grep -cE '(⟩\s*\+\+|\+\+\s*⟨)'` = 0). Multi-name *body*
  memberships (`a, b ∈ Int` as a body line): 0 (multi-name groups
  appear only in first-line param lists — see gap F1). Chained
  membership bounds (`0 < x ∈ Int < 5`): 0.
- `str.replace` / `str.contains` / `str.at` / prefixof / suffixof: 0
  (013/070/073/074–079 out of scope).

## Gap inventory — dependency-ordered build sequence

Difficulty calibration: S ≈ a contained edit on the scale of one P3b
line-shape; M ≈ the whole P3b widening; L ≈ P3c (Pratt FSM) or P3d
(FTI lexer) — a subsystem replacement/addition. "Gates" lists the
conformance census fixtures
(docs/plans/conformance-census-2026-06-07.log, all FAIL under the
fossil unless marked ✓) that exercise the same construct and become
that step's acceptance tests.

### Phase A — lexer fidelity (blocks everything; no dependencies)

**A1. Digit-bearing identifiers.**
The driver lexer's is_alpha class excludes digits, so `t0` lexes as
Ident("t") *dropped* + IntLit(0) (documented fossil quirk,
docs/plans/fti-lexer-notes.md §"Faithfully-carried fossil quirks").
- Example: `t0 ∈ Token` — compiler/parse_body.ev:82.
- Count: **2,159** occurrences of digit-bearing idents, **613
  distinct** (`grep -oE '\b[a-zA-Z_][A-Za-z_]*[0-9][A-Za-z_0-9]*\b'`).
  The corpus is unlexable without this.
- Subsystem: lexer (the per-char scanner FSM's ident-continue class +
  LexFtiPlan's fold rules — a digit while collecting an ident must
  extend the ident, not finish it).
- Difficulty: **S** (one classifier arm + the int/ident boundary rule).
- Gates: none directly (every later phase depends on it); re-run all
  22 green fixtures as regression.

**A2. String-literal escapes.**
The driver lexer has no `\` escape mode; the corpus embeds `\n`, `\t`,
`\"`, `\\` in literals.
- Example: `out = (c = " " ∨ c = "\t" ∨ c = "\n" ∨ c = "")` —
  compiler/lexer.ev:152.
- Count: **100** backslash occurrences on **59** lines
  (`grep -o '\\' | wc -l` / `grep -c '\\'`).
- Subsystem: lexer (escape-pending state in the strlit mode; the
  EscapeChar table at compiler/lexer.ev:280 is the spec).
- Difficulty: **S/M** (one more lexer state; the FTI string payload is
  accumulated then strdup'd at finish, so the buffer just receives the
  translated char).
- Gates: none in census (002 ✓ passes without escapes); lex_fti
  fixture extension.

### Phase B — symbols and strings

**B1. Symbol table beyond 8 slots.**
The driver symtab is 8 fixed slots; `claim main` alone has **303**
memberships (`sed -n '/^claim main/,$p' | grep -cE '^\s+[a-z_]…∈'`)
and **364** distinct lowercase identifiers.
- Example: every body line of compiler/sample.ev:85–1065.
- Subsystem: walker/emit (symtab). The natural shape is a second FTI
  buffer (name-ptr + handle per entry, linear scan) — the P3d pattern
  reapplied.
- Difficulty: **M** (mechanical, but touches every symtab consumer:
  decl, atom resolution, pend plumbing).
- Gates: regression only; first new fixture that needs >8 vars.

**B2. String sort, String memberships, string-literal atoms/pins.**
- Examples: `src_path ∈ String` — compiler/sample.ev:106; string pin
  `target ∈ String = ""` — compiler/sample.ev:123.
- Counts: **506** `∈ String` memberships (`grep -cE '∈ String\b'`);
  **35** `∈ String = "…"` literal pins; StringLit atoms appear
  throughout expressions.
- Subsystem: ZINIT (Z3_mk_string_sort), line classifier + walker
  (decl branch picks the string sort; `Z3_mk_string` for literal atoms
  in the AtomBuildZ3 path), manifest (String state fields).
- Difficulty: **M** (mirrors the existing Int/Bool decl paths; the
  FTI lexer already strdup's StringLit payloads).
- Gates: 019-string-membership; 064 ✓ and 002 ✓ as regressions.

**B3. String builtins and operators in expressions.**
The corpus computes with strings: concat, length, substring, search,
int→string. These need *function-call syntax* in the Pratt FSM
(`f(a, b, c)` shift/reduce with comma args) plus the `#` prefix and
`++` infix.
- Examples: `_partial_strlit ++ cur_char` — compiler/sample.ev:183;
  `cur_char ∈ String = substr(input, pos, 1)` — compiler/sample.ev:142;
  `lx_nl ∈ Int = index_of(input, "\n", pos)` — compiler/sample.ev:245;
  `#input` in `done ∈ Bool = ((pos ≥ #input) ∧ (#input > 0))` —
  compiler/sample.ev:158; `str_from_int(_fcount)` —
  compiler/sample.ev:850.
- Counts: `++` **162**; `substr(` **24**; `str_from_int(` **28**;
  `index_of(` **5**; `#`-cardinality **8** (`grep -cE '#[A-Za-z_]'`).
- Subsystem: Pratt (call syntax, `#` prefix op, `++` infix), walker
  (C2Op arms → the seq.concat/length/extract/indexof/from_int
  builders).
- Difficulty: **M** — call syntax is the only parser-FSM change; each
  builtin is then one C2Op arm.
- Gates: 005/050 (str.++), 011/067 (str.len), 012/068/069
  (str.substr), 014/071/072 (str.indexof), 003 (int→string).

### Phase C — user datatypes (the enum/ctor/match cluster)

**C1. User enum declarations → build-context datatypes.**
The driver currently *skips* enum decls. The corpus declares **31**
enums (`grep -c '^enum '`), with: payload variants up to arity 3
(`FloatLit(Int, Int, Int)` — compiler/lexer.ev:22), self-recursion
(`enum TokenList = TLNil | TLCons(Token, TokenList)` —
compiler/lexer.ev:116), mutual recursion (`EMatch(Expr, MatchArmList)`
— compiler/parser.ev:72, where MatchArm carries Expr back), a Seq
payload (`LibCall(String, String, Seq(LibArg))` —
stdlib/kernel.ev:38 → needs the `__SeqOf_LibArg` cons-helper sort),
and Real payload fields (`ArgReal(Real)` — stdlib/kernel.ev:17 →
needs Z3_mk_real_sort at ZINIT).
- Subsystem: ZINIT/walker — generalize translate2_ctor.ev's
  VariantNameSymStep/…/EnumSortSymStep machinery from the hardcoded
  Exit-only Effect to N enums parsed from the token stream, batched
  Z3_mk_datatypes so forward + mutual refs resolve; a
  sort/ctor/recognizer/accessor registry keyed by name (another
  FTI-buffer candidate).
- Difficulty: **L** — the largest single subsystem add; the mechanics
  are documented in translate2_ctor.ev but the registry +
  mutual-recursion batching is new.
- Gates: 043-enum-declaration, 021-real-membership (the Real sort
  lands here), 066 ✓ regression.

**C2. Enum-typed memberships + manifest fields.**
- Example: `op_tok ∈ Token` — compiler/sample.ev:152.
- Count: **540** enum-typed memberships
  (`grep -oE '∈ [A-Z][A-Za-z_0-9]*'` minus primitives/Seq: TokenList
  221, Token 147, Expr 55, Effect 33, WorkList 15, Op 12, + 17 more
  types).
- Subsystem: line classifier + walker decl path (sort lookup via the
  C1 registry), manifest (enum-typed state fields — the kernel
  already carries them; the P3d oracle manifest had 9 TokenList
  fields).
- Difficulty: **M** (dep: C1).
- Gates: 044-enum-constraint.

**C3. Constructor applications + nullary ctor atoms in expressions.**
- Examples: `int_tok ∈ Token = IntLit(_partial_int)` —
  compiler/sample.ev:200; nullary atom `TLNil` in
  `tokens = (is_first_tick ? TLNil : …)` — compiler/sample.ev:211.
- Counts: **945** capitalized-name-then-paren tokens on non-claim-head
  lines (`grep -vE '^claim ' | grep -oE '\b[A-Z][A-Za-z_0-9]*\('`),
  of which ~325 are composition call heads (gap F2) and ~180 are
  match-arm patterns (gap C5) ⇒ **≈440 expression-position ctor
  applications**; plus **154** nullary ctor sentinel atoms (TLNil,
  EofTok, ENoExpr, …). 11 pins of the form `name ∈ EnumT = Ctor(…)`;
  120 ternary branches yield enum values.
- Subsystem: Pratt (an Ident followed by `(` in operand position
  becomes a call shape — shared with B3; an Ident that resolves in the
  ctor registry instead of the symtab becomes a nullary mk_app),
  walker (generalize CtorArgWriteStep/CtorAppStep from the
  harvested-Exit special case to registry-driven apps with ≤3 args).
- Difficulty: **M/L** (dep: C1; parser work shared with B3).
- Gates: 044, 052 (ctor in seq literal), 006 (partial).

**C4. `matches` recognizers.**
- Example: `head_is_enum ∈ Bool = (items_hd matches KwEnum)` —
  compiler/sample.ev:332.
- Count: **323** ` matches ` occurrences; 109 carry a payload wildcard
  (`matches Ctor(_)`, incl. 9 `(_, _)`), the rest nullary; **0** have
  named binds.
- Subsystem: Pratt (postfix operator: operand `matches` Ctor-pattern;
  patterns' binds are always `_`, so no binding logic), walker
  (recognizer lookup via C1's VariantQueryStep machinery → mk_app of
  the tester).
- Difficulty: **M** (dep: C1). Side effect: retires the driver-notes
  descope "≠ as a membership bound mis-asserts" is unrelated and stays
  a separate S item — not corpus-blocking (corpus `≠` appears only
  inside full expressions, which Pratt already lowers correctly).
- Gates: 006 (partial).

**C5. `match` expressions (pin RHS) with payload binds.**
- Example: compiler/sample.ev:108–110 —
  `path_read ∈ String = match last_results[0]` /
  `    StringResult(s) ⇒ s` / `    _ ⇒ ""`.
- Counts: **185** `= match` pins; **180** payload-bind arms
  (`Ctor(x) ⇒`), **153** wildcard arms; ≤3 arms per match (awk scan
  over the stripped corpus); scrutinees are plain idents except **2**
  `match last_results[0]`.
- Subsystem: line classifier (a pin whose RHS head is KwMatch enters a
  match sub-parse), walker (lower to nested ite over recognizer
  testers — the fossil's documented shape; each bound name `s`
  resolves as the accessor app `(Ctor__f0 scrut)` within that arm's
  body, i.e. a scoped symtab overlay per arm), emit (none).
- Difficulty: **L** — arm bodies are full expressions (ctors, strings,
  arithmetic), and accessor-substitution scoping is a new walker
  concept. The newline-separated arm surface is fine (lexed newlines
  don't exist; an arm ends at the next pattern's `Ctor ( … ) ⇒` /
  `_ ⇒` lookahead).
- Gates: 006-enum-match (the full fixture).

### Phase D — Seq values and the effects channel

**D1. Full Effect enum floor.**
The driver declares an Exit-only Effect. sample.ev's effects use
ReadLine, ReadFile, Exit, and LibCall (and the enum declares
WriteFile), so the build context needs the real 5-variant Effect with
LibCall's `Seq(LibArg)` payload — the LibArg + __SeqOf_LibArg + Effect
multi-datatype batch translate2_ctor.ev documents.
- Example: stdlib/kernel.ev:34–39.
- Count: Effect ctor apps in the corpus: LibCall **41**, ReadFile
  **4**, Exit **4**, WriteFile **3** (`grep -oE '(ReadLine|…)\('`).
- Subsystem: ZINIT — subsumed by C1 if the user-enum path translates
  stdlib/kernel.ev's Effect *as* a user enum (preferred; the kernel
  matches variants by name).
- Difficulty: **M** (mostly falls out of C1).
- Gates: none new in census; every multi-tick seam fixture.

**D2. Conditional effects writer with Seq literals.**
The corpus's single effects constraint is a 5-way nested ternary over
Seq(Effect) literals of differing lengths, with a nested Seq payload
and an empty Seq:
- Example: compiler/sample.ev:1061–1065 —
  `effects ∈ Seq(Effect) = (is_first_tick ? ⟨ReadLine⟩ : ((¬_got_path)
  ? ⟨ReadFile(src_path)⟩ : (emit_now ? ⟨Exit(0)⟩ : (emitting_block ?
  ⟨LibCall("libc", "puts", ⟨ArgStr(emit_payload)⟩)⟩ :
  ⟨LibCall("libc", "getpid", ⟨⟩)⟩))))`.
- Counts: **45** `⟨` tokens; **42** `⟨Ctor(` element heads; **2** `⟨⟩`
  empty literals; **1** effects membership (the only `∈ Seq(` in the
  corpus).
- Subsystem: Pratt (`⟨ … ⟩` literal as an operand shape, elements full
  expressions), walker (today's C2SelectLen handles exactly one
  unconditional 1-element Exit literal; this needs per-branch
  (select effects i) + effects__len lowering composed under ite —
  Seq values become first-class work-item results: an (array-handle,
  len-handle) pair), emit (max-effects from the longest branch, or
  keep the fixed 16, which stays kernel-correct).
- Difficulty: **L** — the (array, len) value-pair plumbing through the
  ternary work items is the novel part.
- Gates: 052-seq-literal, 057-seq-cardinality,
  065-seq-length-contradiction, 051-seq-type-index (the select path).

**D3. `last_results[0]` indexing + Result sort in build context.**
- Example: compiler/sample.ev:108.
- Count: **2** `last_results[0]` — the only indexing in the corpus.
- Subsystem: Pratt (postfix `[ expr ]` → mk_select), ZINIT (Result
  must exist as a real sort in the build context, not just the
  textual prelude, because user constraints name StringResult etc. —
  falls out of C1 translating stdlib/kernel.ev's Result enum, with
  the textual prelude suppressed to avoid the duplicate sort,
  mirroring sample.ev's `_saw_result` dance at lines 894–898).
- Difficulty: **S/M** (dep: C1).
- Gates: none in census; tests/seam fixtures exercise it.

### Phase E — multi-tick state machine support

**E1. Manifest state-fields + `_name` carry declares + effects floor.**
The corpus is a multi-tick FSM: **46** `_name` carry memberships
(`grep -cE '^\s+_[A-Za-z_0-9]+\s+∈'`), **49** distinct `_`-prefixed
idents, **46** `(is_first_tick ? init : …)` carry-update pins in
main, **54** is_first_tick uses.
- Example: `_got_path ∈ Bool` — compiler/sample.ev:103, read back as
  `(¬_got_path)` the next tick.
- Subsystem: emit/manifest — derive state-fields from top-level
  memberships (String + enum types included; `_`-prefixed,
  is_first_tick, effects, last_results excluded per the fossil's
  rules, compiler/parse_body.ev:289–290 and
  compiler/parse_body_ctor.ev:195–197); emit the
  `(assert (>= effects__len 0))` floor; declare the `_<name>` consts
  the kernel asserts into.
- Difficulty: **M** (rule-following; the fossil code is the spec).
- Gates: any kernel-run multi-tick fixture (tests/kernel/*.ev);
  ultimately "driver-compiled sample.ev runs under the kernel".

### Phase F — claim composition (the keystone)

**F1. First-line parameter lists.**
- Example: `claim BuildZ3AstVectorSize(ctx_h, vec_h ∈ Int, eff ∈
  Effect)` — stdlib/kernel.ev:131 (a multi-name group).
- Counts: **59** parametrized claims; **16** with multi-name groups
  (`grep -E '^claim \w+\(' | grep -cE '\([^∈)]*,[^∈)]*∈'`); param
  types include enums (`e ∈ Expr`) and `Seq(LibArg)` shapes.
- Subsystem: top-level dispatch (today: "parametrized ⇒ skip"; needed:
  parse the param list into the callee's declared names + types so F2
  can bind them).
- Difficulty: **M**.
- Gates: 055-schema-params-sat (056 ✓ regression), 049-multi-name.

**F2. Claim-call composition with `slot ↦ value` binding + inlining.**
The single largest semantic gap. Every parse/translate pass in the
corpus is invoked by composition.
- Example: `MembershipStep(rem ↦ _rem, decl ↦ step_decl, …)` —
  compiler/sample.ev:447 (8-slot call); `TLHd(l ↦ _work, out ↦
  work_hd)` — compiler/sample.ev:263.
- Counts: **325** composition call sites
  (`grep -cE '^\s+[A-Z][A-Za-z_0-9]*\(.*↦'`), **65 distinct
  callees**, **10** call sites spanning two lines; composition
  nesting depth ≥5 (SPrim2→SChain1→SPrim1→SChain0→SPrim0 in
  compiler/translate_scalar_expr.ev; RenderExprL3→L2→L1→L0 ×3-wide
  per level in compiler/translate_ctor.ev — the oracle's α-renamed
  expansion of these chains is why sample.smt2 is ~2 MB).
- Subsystem: ALL of: top-level dispatch (build a claim-name →
  buffer-cursor index during the skip pass — the FTI buffer is
  random-access, so a callee body is a (start, end) cursor pair, no
  token copying); walker (an inline stack of (return-cursor,
  substitution-table, α-prefix) frames; a body line that classifies
  as a composition call pushes a frame); symtab (per-site α-renaming
  of callee body locals — the corpus's `ms_`/`sq_`/`l1_` prefixes
  exist precisely because the oracle leaks body locals across
  names-match sites while α-renaming distinct sites; faithful
  compilation must do the per-site rename); Pratt (slot values are
  full expressions — CallArgsStep's `(x > 3)` shapes).
- Difficulty: **L** — the one item that approaches "architecturally
  hard". See the go/no-go.
- Gates: 045-subschema-expansion, 046-subschema-constraint,
  047/048-passthrough, 094–099 bare-claim conjunction, 102–108
  mapped-renames/multi-variable, 109–115 passthrough names-match.

## Summary table

| # | Construct | Count (grep over flattened corpus, comments stripped) | Example | Subsystem | Diff | Census gates |
|---|---|---|---|---|---|---|
| A1 | digit-bearing idents | 2,159 occ / 613 distinct | parse_body.ev:82 `t0 ∈ Token` | lexer | S | (all regressions) |
| A2 | string escapes | 100 occ / 59 lines | lexer.ev:152 `c = "\t"` | lexer | S/M | — |
| B1 | symtab > 8 slots | 364 distinct idents in `main`; 303 memberships | sample.ev:85… | walker/symtab | M | — |
| B2 | String sort + literals | 506 `∈ String`; 35 literal pins | sample.ev:106 | ZINIT·classifier·walker·manifest | M | 019 |
| B3 | string ops + call syntax | `++` 162 · substr 24 · str_from_int 28 · index_of 5 · `#` 8 | sample.ev:142,183,245 | Pratt·walker | M | 003,005,011,012,014,050,067–069,071,072 |
| C1 | user enum decls (rec/mutual/Seq/Real payloads) | 31 enums | lexer.ev:116; kernel.ev:38 | ZINIT·walker (registry) | **L** | 043, 021 |
| C2 | enum-typed memberships | 540 | sample.ev:152 | classifier·walker·manifest | M | 044 |
| C3 | ctor apps + nullary atoms | ≈440 apps + 154 atoms | sample.ev:200,211 | Pratt·walker | M/L | 044, 052 |
| C4 | `matches` recognizers | 323 (109 with `(_)`) | sample.ev:332 | Pratt·walker | M | 006 (part) |
| C5 | `match` pins w/ binds | 185 matches · 180 bind arms · 153 wildcards | sample.ev:108–110 | classifier·walker (scoped subst) | **L** | 006 |
| D1 | full Effect floor | LibCall 41 · ReadFile 4 · Exit 4 · WriteFile 3 | kernel.ev:34–39 | ZINIT (via C1) | M | seam |
| D2 | conditional effects + Seq literals | 45 `⟨` · 42 `⟨Ctor(` · 2 `⟨⟩` · 1 effects line | sample.ev:1061–1065 | Pratt·walker·emit | **L** | 051,052,057,065 |
| D3 | `last_results[i]` select + Result sort | 2 | sample.ev:108 | Pratt·ZINIT | S/M | seam |
| E1 | manifest fields + `_name` carries + floor | 46 carry pairs · 46 carry pins · 54 is_first_tick | sample.ev:103 | emit/manifest | M | kernel fixtures |
| F1 | first-line param lists | 59 claims · 16 multi-name groups | kernel.ev:131 | dispatch | M | 055, 049 |
| F2 | composition `slot ↦ value` inlining | 325 sites · 65 callees · depth ≥5 | sample.ev:447,263 | dispatch·walker·symtab·Pratt | **L** | 045–048, 094–115 |

Out of scope for this corpus (verified 0 uses): quantifiers, generics,
records/tuples/methods, `..` passthrough, subclaim, implies-constraint
lines, chained membership bounds, multi-name body memberships, Seq
concat, FloatLit, str.replace/contains/at/prefixof/suffixof.

## Go/no-go judgment

**GO.** Nothing in the inventory invalidates the driver's
architecture (FTI token buffer + cursor windows + one-action-per-tick
Pratt/walker FSM + work items + last_results handle plumbing). Each
gap extends an existing subsystem along an axis it already has:

- The four L items decompose into patterns already proven: C1
  generalizes translate2_ctor.ev's existing step claims behind a name
  registry (an FTI-buffer reapplication); C5 is nested-ite lowering
  (TernaryBuildZ3 exists) plus a scoped symtab overlay; D2 is the
  work-item value model widened from "one Z3 handle" to "an
  (array, len) handle pair" — plumbing, not redesign.
- F2 (composition inlining) is the only item with architectural
  *tension*: the walker becomes re-entrant (an inline frame stack) and
  the symtab needs per-site α-renaming. Two facts keep it inside the
  current design: (1) the P3d FTI buffer is random-access, so "splice
  the callee body" is a cursor push/pop, not token-list surgery — the
  cursor/window machinery was built for exactly this access pattern;
  (2) call-site arity and slot values are bounded and flat in the
  corpus (≤10 slots, scalar-expression values), so the substitution
  table is the same flat-string-table shape CallArgsStep/SlotSubst
  already prove out in the corpus itself. Largest single work item,
  but laborious rather than architecturally hard.
- The honest risk is **throughput, not expressibility**: P3d compiles
  a ~30-line fixture in ~6,190 ticks / ~4.6 min (per-tick ~26–41 ms,
  functionizer-residual FSM shapes — the known P2 finding). The
  flattened corpus is ~4,000 code lines, and `claim main` alone
  carries ~300 memberships; without per-tick cost reduction
  (functionizing the FSM shapes, batching pure ticks) a full
  sample.ev compile extrapolates to days of wall time. That wall is
  performance engineering on a working pipeline — track it as its own
  line, but it is not a reason to redesign the driver before widening
  it.

Recommended sequencing is the phase order above: A is prerequisite to
lexing the corpus at all; B unblocks the string-heavy half of the
census; C is the largest coherent cluster and gates D; E/F close the
loop to "driver-compiled sample.ev runs under the kernel". Every
phase has named census fixtures as acceptance gates, so each step is
independently testable in the P3b/P3c/P3d batch-acceptance style.
