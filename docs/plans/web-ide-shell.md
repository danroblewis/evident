# Evident Web IDE — Shell Design (information architecture)

Status: design pass by `ide-ui-designer` (Mira), 2026-06-25. Companion to
`docs/plans/web-ide.md` (the feature spec). This doc is about the **instrument's
shell** — its layout, regions, control surfaces, and the find-ability of the
~28 diagram views — not about any single feature. Migration slices at the end are
filed as `--tag ui` tasks in `ide/tasks.json`.

The build it describes: `ide/web/static/index.html` (DOM), `ide/web/static/app*.js`
(wiring), `ide/web/render.py` (`ALL_VIEWS` registry → `VIEWS`), `ide/web/analysis.py`
(`_recommend` lead-view picker). Surveyed live at `http://localhost:5173/`.

---

## 0. The one highest-leverage move

**Group the ~28 diagram tabs into four labeled families crossed with a rigor
badge, and give the analysis panel a fixed two-band cockpit grid.** Today the tab
strip renders `VIEWS` in raw registry order — `solution space · terminal map ·
reachable region · time series · state graph · phase portrait · reachability tree …`
— a 28-chip wall in NO order (not even alphabetical). That is the single worst
find-ability problem in the product. Everything else (toolbar crowding, naked
`scope`/`unroll` boxes, the unlabeled vertical stack) is real but secondary; the
gallery is where a user is lost *every single render*.

The fix is cheap because the data is already there: every view has a one-line
caption in `VIEW_CAPTIONS` (`app-symbols.js`) whose trailing "**tells you …**"
clause is a ready-made "what it answers", and `render.py` already partitions views
into rigor classes (`view_rigor` → `proven` / `exhaustive` / `sampled`). We add a
static `family` for each view and let `renderViewTabs` lay them out grouped, with
the existing rigor badge shown per chip. No backend reorder needed.

---

## 1. The shell wireframe (laptop width, ~1280px)

Regions are labeled `[N]` and specced in §2.

```
┌─[1 IDENTITY]──────────────────┬─[2 FILE]──┬───────────────[3 ACTION CLUSTERS]──────────────┐
│ ◆ Evident   counter.ev  ●edited│ ▾ Files   │  ▶ Run  ⊨ Solve   ⌨ ? │ ⤓ Export▾ │ ⌘K  18ms ✓ │
├───────────────────────────────┴───────────┴──────────────────────────────────────────────┤
│┌─[4 WORKSPACE]──────────┬─[5 EDITOR]───────────────┬─[6 ANALYSIS COCKPIT]─────────────────┐│
││ ▾ my files             │ 1  fsm counter            │┌─[6a SHAPE banner]──────────────────┐││
││   • counter.ev   ●     │ 2    count ∈ Int := 0     ││ ◆ Driven — count advances on its   │││
││   • scratch.ev         │ 3    Δcount = (_count<9?…) ││   own clock (deterministic recur.) │││
││ ▾ samples (read-only)  │ 4    done ∈ Bool=(count≥5) │└────────────────────────────────────┘││
││   FSM ▾                 │                           │┌─[6b VERDICT strip]─────────────────┐││
││     counter             │                           ││ ✓ Terminates  •fixed pt (count=9)  │││
││     vending             │                           ││ ⊑ boundary count∈[0,9] ▶replay …   │││
││     traffic light       │                           │└────────────────────────────────────┘││
││   Solve ▾               │                           │┌─[6c VIEW · the diagram]────────────┐││
││     N-queens            │                           ││ [family tabs ▸ see §3]             │││
││     sudoku              │                           ││ ┌────────────────────────────────┐ │││
││   …                     │                           ││ │  solution space  ·  z3-proven  │ │││
││                         │                           ││ │      [ the rendered figure ]   │ │││
││                         │                           ││ │                                │ │││
││                         │                           ││ └────────────────────────────────┘ │││
││                         │                           ││ caption: "the SOLVED boundary…"    │││
││                         │                           │└────────────────────────────────────┘││
││                         ├─[5b DIAGNOSTICS]──────────┤┌─[6d INTERROGATE drawer]────────────┐││
││                         │ (errors/footguns, hidden  ││ ⊢ verify │ ⊨? query │ ⊨ Solve       │││
││                         │  until present)           ││  [ the active tool's row + result ]│││
│└─────────────────────────┴───────────────────────────┘└────────────────────────────────────┘│
├─[7 STATUS / TICK TRANSPORT]──────────────────────────────────────────────────────────────────┤
│  ⏮ ◀ ▶ ⏭  ●──────○────  tick 4/9   │   honesty: 0 dropped · scope 400 ✓complete   │  history ▸ │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

The move from today: the analysis panel's flat vertical stack (banner → verdict →
verify → query → try-chips → 28-tab wall → figure → caption → honesty → solve) becomes
a **fixed cockpit grid** with four labeled regions (6a shape, 6b verdict, 6c view, 6d
interrogate), the interrogation tools (verify/query/solve) collapse into ONE tabbed
drawer instead of three always-on stacked rows, and the honesty line + scope state move
to the persistent footer where they belong (they describe the *run*, not the *view*).

---

## 2. Per-region spec

| # | Region | Owns | Today | Change |
|---|--------|------|-------|--------|
| 1 | **Identity** | logo, filename, dirty-dot | `◆ Evident IDE` + `fname` | filename becomes the active-file indicator from the workspace (region 4); add a `●` dirty marker (currently no edited-state signal). |
| 2 | **File menu** | open/new/recent | nothing — only a samples `<select>` | a real **Files ▾** menu: New, Open folder, Recent. The samples dropdown is demoted into the workspace tree (region 4) as a read-only folder. |
| 3 | **Action clusters** | the verbs | ~12 loose text buttons | regrouped into **clusters by verb** — see §4. Primary actions (Run/Solve) stay text; the rest demote to icons + an `Export ▾` disclosure. |
| 4 | **Workspace tree** | open files + samples | absent | the headline new affordance: a left rail with **my files** (an opened folder) and **samples (read-only)**, the samples grouped FSM / Solve / showcase (their labels already carry these hints). See §5. |
| 5 | **Editor** | the source + diagnostics | full-height editor, `#errors` toggles | unchanged in spirit; the diagnostics panel (5b) docks *under* the editor instead of replacing the dynamics caption. |
| 6 | **Analysis cockpit** | the four analysis regions | one vertical stack | gridded into 6a–6d (below). The heart of this redesign. |
| 6a | **Shape banner** | one-line model-shape claim | `#banner` | unchanged content; gets its own bordered cell, always top of the cockpit. |
| 6b | **Verdict strip** | terminates/cyclic + boundary + replay | `#structure` | its own labeled cell directly under the banner; these are *claims about the model*, distinct from *a view of it*. |
| 6c | **View** | the diagram + family tabs + caption | `#tabs` + `#view` + `#view-caption` | the family-grouped tab bar (§3) sits above the figure; the per-view rigor badge moves onto the active-view header line; caption stays beneath. |
| 6d | **Interrogate drawer** | verify · query · solve | three always-on stacked rows (`#invariant`, `#query-row`, `#solve`) | **collapsed into one tabbed drawer** with three tabs (⊢ verify / ⊨? query / ⊨ Solve). Only the active tool's controls show; the others are one click away. Reclaims ~3 rows of vertical space that today push the diagram below the fold. |
| 7 | **Status / transport footer** | tick transport, honesty, scope state, history | scattered (`#latency`/`#status` in header; `#honesty` mid-panel; history at panel bottom; no transport yet) | a persistent bottom bar: the FSM tick transport (when an FSM is loaded), the honesty line (dropped count + scope/complete state), latency, and a history disclosure. These are *session-global*, not per-region — they belong in a frame, not buried in the scroll. |

---

## 3. The diagram taxonomy — all ~28 views slotted

Four **analysis-type families** (what question the view answers) crossed with a
**rigor** badge (`proven` = abstract Z3 over all conditions · `exhaustive` = the full
bounded state graph · `sampled` = trajectories / a capped or continuous fallback —
straight from `render.py:view_rigor`). The tab strip groups by family in this order;
within a family, proven/exhaustive views lead, sampled ones follow. Each "what it
answers" is the trailing **"tells you …"** clause already written in `VIEW_CAPTIONS`.

### Family A — SOLUTION SPACE (what states are possible *at all* — no run)
*The abstract, solved view. Answers "what can be true", before any dynamics.*

| view | rigor | what it answers |
|---|---|---|
| `solution_space` | proven* | each variable's full range + the feasible region of the two principal vars; what states are possible at all, with fixed points marked. |
| `solution_structure` | proven | what a claim *determines* vs leaves *free* — the forced backbone (green) + free vars over proven ranges (blue). |

### Family B — TERMINAL / END-STATE (where it can *rest* — abstract & sampled)
*Answers "does it stop, and where".*

| view | rigor | what it answers |
|---|---|---|
| `terminal_map` | proven* | the abstract terminal set (absorbing states), Z3 over the one-step relation; ∅ ⇒ a daemon that never stops. |
| `reachable_region` | proven* | a bounding box PROVEN to contain the reachable set by k-induction; bounded / provably-unbounded / indeterminate. |
| `fixedpoint_map` | sampled | where the system comes to rest — fixed points as large markers, short cycles as arrowed loops, against the basin. |
| `basin_map` | exhaustive* | which terminal each *starting* state flows to — the basins of attraction. |
| `morse_graph` | exhaustive* | the recurrence skeleton (SCC condensation) — where the dynamics get trapped vs pass through. |

### Family C — DYNAMICS OVER TIME (how it *evolves* — mostly sampled/exhaustive)
*Answers "what does a run, or the ensemble of runs, look like over ticks".*

| view | rigor | what it answers |
|---|---|---|
| `state_graph` | exhaustive* | the reachable state-transition graph — every state the machine can enter and how they connect. |
| `reachability_tree` | exhaustive* | the BFS unfolding from all initial conditions — how many steps reach each state. |
| `time_series` | exhaustive* | every state variable on stacked tracks over ticks, with the reachable-value envelope band. |
| `value_raster` | exhaustive* | every carried scalar/enum/bool leaf as one row × ticks as columns, cell colour = value — the transpose of `time_series` read as a raster; applies to EVERY multi-variable FSM. **Pair it next to `time_series`** (they are transposes). (Iris #443) |
| `timing_diagram` | exhaustive* | the same ensemble as EE-style digital/analog waveform lanes. |
| `space_time` | exhaustive* | a Seq-carried state's evolution as a rows=ticks × cols=positions raster (Rule 90 → Sierpiński) — now ONE INSTANCE of `value_raster` (the indexed/vector flavor), not its own diagram. A 2-D grid-state flavor (#444) renders life.ev / brackets.ev as a field. (Iris #442/#444) |
| `transition_matrix` | exhaustive* | the transition relation as an adjacency-matrix heatmap — does it stay in a mode (block-diagonal) or switch. |
| `phase_portrait` | sampled | the difference-equation vector field — which way the dynamics flow across value-space. |
| `nullcline_field` | sampled | the qualitative sign field + nullclines over two numeric axes; their crossings are fixed points. |
| `cobweb` | sampled | a 1-D map's staircase — whether iterating the scalar converges, cycles, or diverges. |
| `orbit_scatter` | sampled | the orbit's shape over many starts (loop=cycle, pile-up=fixed point). |
| `occupancy_heatmap` | sampled | where the system spends its time — visit-density over two axes. |

### Family D — STRUCTURE / LAW (the *shape of the relation*, not its runs)
*Answers "how is this program built" — variable coupling, correlations, compiled form.*

| view | rigor | what it answers |
|---|---|---|
| `scatter_matrix` | sampled | which variables correlate or separate across the reachable set (all pairs). |
| `parallel_coords` | sampled | which value-combinations cluster per class (Inselberg polylines). |
| `chord_diagram` | sampled | how much flow goes between which categories (room→room, mode→mode). |
| `function_graph` | proven | the compiled data-flow coupling — a feedback cycle vs a driven pipeline DAG. |
| `function_residual` | proven | what compiled to a function vs what stayed a true constraint (the relational residue). |
| `function_guards` | proven | the guard decision trees — the branching each variable's next value is computed by. |
| `function_behavior` | proven | what each compiled function actually computes, sampled over its inputs. |
| `function_complexity` | proven | where the per-tick compute goes — branching + arithmetic cost, ranked. |

`*` = degrades to `sampled` when the model is capped or continuous (`render.py`'s
`_BOUND_VIEWS` / `_ENUMERATE_VIEWS`); the badge follows the chart, never over-claims.

The `function_*` group already has a `⚙` seam in `renderViewTabs`; under this taxonomy
it becomes the tail of **Family D** with a "compiled structure" sub-label, preserving
the existing affordance.

---

## 4. Control surfaces

### 4.1 Action clusters (region 3) — group ~12 buttons by verb

Today, twelve loose buttons (`⌨ symbols`, `? tour`, `⊨ Solve`, `scope`, `unroll`,
`⧉ SMT-LIB`, `💾 Save`, `↧ .ev`, `🔗 Share`, `📌 pin`, `⇄ diff`, `⌘K`) overflow the
toolbar — at 1280px the right half is already clipped off-screen (verified: only up to
`SMT-LIB` is visible at laptop width). Regroup:

| cluster | members | surface |
|---|---|---|
| **Run** (primary) | the live recompute / claim-select | a clear text `▶ Run` (today implicit-on-edit); keep `⊨ Solve` text beside it. These two are the verbs; everything else is secondary. |
| **Learn** | `⌨ symbols`, `? tour` | a single icon pair (`⌨` / `?`), no text labels. |
| **Export** ▾ | `⧉ SMT-LIB`, `↧ .ev`, `🔗 Share`, `💾 Save` | one **Export ▾** disclosure menu — these are all "get the program out", rarely per-edit. SMT-LIB's `unroll` companion box lives *inside* this menu next to it (§4.2). |
| **Compare** | `📌 pin`, `⇄ diff` | an icon pair; `⇄ diff` already auto-hides until a pin exists — keep that. |
| **Palette** | `⌘K` | far-right icon; it's the escape hatch to everything. |
| **Status** | latency, error/ok | far-right dim text, unchanged. |

Net: from ~12 always-visible controls to **4 visible verbs** (Run, Solve, Export ▾,
⌘K) + two small icon pairs. The toolbar stops clipping at laptop width.

### 4.2 The two naked number boxes — `scope` and `unroll`

Both are bare `<input type=number>` with only a tooltip. They are *parameters of an
action*, not standalone widgets — so dock each to the action it modifies:

- **`scope`** (placeholder "400" = the reachable-states exploration bound, Alloy-style)
  is a parameter of *the analysis run*. Move it to the **status/transport footer**
  (region 7) beside the honesty line it governs, as a labeled stepper:
  `exploration scope: [ 400 ] states — ✓ complete / ⚠ capped`. It belongs next to the
  "is this exhaustive?" readout, because that readout is *what raising it changes*.
- **`unroll`** (the BMC k-step depth) is a parameter of *SMT-LIB export only* — nothing
  else reads it. It has no business in the top toolbar. Move it **inside the Export ▾
  menu**, directly under `⧉ SMT-LIB`, as `unroll k: [ ___ ] steps (bounded model check)`.
  Out of the toolbar entirely until you reach for that export.

Both get a real label and live next to their effect — no more orphaned spinners.

---

## 5. The workspace tree (region 4) — files + samples as folders

The owner's pain #1/#2: no open-folder model; samples are a one-off dropdown. Replace
with a left rail with two collapsible roots:

```
▾ my files
   • counter.ev        ●        ← dirty marker
   • scratch.ev
▾ samples (read-only)
   ▾ FSM
       counter · vending · traffic light · oscillator · random walk · …
   ▾ ⊨ Solve
       N-queens · graph coloring · sudoku · subset-sum · topo sort · sort
   ▾ showcase
       Rule 90 · bistable · cobweb · scatter_matrix · timing_diagram
```

The samples already self-describe their bucket in their labels (`… (FSM)`,
`… (⊨ Solve)`, `… (FSM, basin_map)`) — the grouping is *reading the suffix*, no new
metadata. "read-only" is explicit so opening a sample doesn't pretend to be editable
project state; editing one offers "save a copy into my files". This subsumes the
samples `<select>` entirely.

`my files` is an opened local folder (the owner edits real source files). v0 can back
it with the existing named-save slots (`💾 Save`); the folder-open file-system access
is the larger slice.

---

## 6. Migration order (cheap reshuffles first)

Ordered so each slice ships value without waiting on the one after it. Each is filed
as a `--tag ui` task.

1. **Family-group the diagram tabs.** Add a static `view → family` map; teach
   `renderViewTabs` (`app-history.js`) to lay out the four families with a labeled
   header per group and the rigor badge per chip. Pure frontend, no backend touch.
   *Highest leverage, lowest cost — ship first.* (the §0 move)
2. **Collapse verify/query/solve into one tabbed Interrogate drawer (6d).** Three
   always-on stacked rows → one drawer with three tabs. Reclaims the vertical space
   that pushes the diagram below the fold. Frontend-only (toggle visibility + a tab bar).
3. **Group the toolbar into action clusters + Export ▾ menu (§4.1).** Move
   SMT-LIB/.ev/Share/Save behind `Export ▾`; demote symbols/tour/pin/diff to icons.
   Frontend-only.
4. **Re-home `scope` and `unroll` (§4.2).** scope → footer beside honesty; unroll →
   inside Export ▾. Frontend-only; depends on 3 (Export menu) and 7 (footer).
5. **Cockpit grid for the analysis panel (6a–6c) + status/transport footer (7).** CSS
   grid the panel into shape/verdict/view cells; pull honesty + latency into a
   persistent footer. Mostly CSS + DOM reparenting.
6. **Samples-as-read-only-folder in a workspace rail (region 4, samples half).** The
   left rail with the grouped read-only samples tree, replacing the `<select>`.
   Reads the existing sample labels for grouping.
7. **`my files` — opened-folder editing (region 4, files half).** The real
   open-folder model. Largest slice (file-system access / persistence); ship last.

Slices 1–4 are same-day frontend reshuffles. 5 is CSS-heavy. 6–7 are the genuinely new
capability and gate on a file/persistence decision (`docs/plans/web-ide.md` §10.1).

---
---

# Revision — Verdict + Interrogate, where they LIVE (2026-06-25)

Design pass by `ide-ui-designer` (Mira), in response to owner feedback. Companion to
Nadia's parallel pass on *what the verdict terms mean and why they confuse* — this
section is the **layout/IA** half: where Verdict, Interrogate, History, and Pin belong
in the cockpit. It builds on her semantics, doesn't restate them. Surveyed live at
`http://localhost:5173/` with the cockpit grid (§1) already shipped.

## R.0 The single highest-leverage move

**Dissolve the two heaviest cockpit regions into surfaces that already exist.** The
Verdict strip and the Interrogate panel are both *permanent real estate* spent on
things that aren't permanent. Verdict is a set of *claims about the model* — it belongs
as **header chips next to the title**, where a one-line status always lives in a real
app. Interrogate is *a diagram computed under extra constraints* — it belongs **in the
gallery as two new view families**, where every other "render me a picture of the model"
lives. After this move the cockpit has exactly THREE bands — shape · view · (footer) —
and every analysis, interrogation included, is reached the same way: pick a view.

That is the reframe the owner reached independently ("verify + query are really a
visualizer computed under an EXTRA set of constraints"), and it's correct. The win isn't
cosmetic: it collapses **two distinct interaction models into one**. Today a user learns
"diagrams are tabs up there, but verification is a console down here" — two mental models
for the same act of *asking the model a question*. Unifying them means the gallery
taxonomy (§3) becomes the *complete* index of what you can ask, verify and query
included.

```
TODAY                                    PROPOSED
┌─ banner ──────────────┐                ┌─ HEADER: title  · [✓ chips] [⛨] [?]─┐
├─ VERDICT strip ███████┤  ← cramped     ├─ banner ────────────────────────────┤
├─ view (tabs+figure) ──┤     jargon     ├─ view (tabs+figure) ────────────────┤
├─ INTERROGATE █████████┤  ← big, fixed  │   gallery now includes:             │
│   verify / query / …  │     distracting│    family E ⊢ verify · family A ⊨?   │
├─ footer (history…) ───┤                ├─ footer (scope · honesty · 🕮 hist) ─┤
└───────────────────────┘                └─────────────────────────────────────┘
   pin = toolbar button                     pin = footer "compare ▸" affordance
   history = footer thumbnails              history = 🕮 modal
```

## R.1 Verdict → header chips (right of the pin slot)

The Verdict strip today is one `#structure` row that crams: the verdict word + note,
fixed points, the boundary, forced-equal / forced-different, implied relations
(clickable proofs), Farkas certificates, `▶ replay a dodging loop`, `▶ replay path to
rest`, and `⛨ verify soundness` (read off `app-structure.js:renderStructure`). That's
**nine kinds of thing on one line** — the cramping the owner feels, and the jargon
(`dodging loop`, `path to rest`, `verify soundness`) with no in-place definition.

The fix is a hierarchy, not a wrap. A verdict has a *headline* (one word: does it stop?)
and a *dossier* (the evidence: bounds, fixed points, relations, the replayable
witnesses). The headline belongs in the header as a glanceable chip; the dossier belongs
in a modal you open from it.

### The header treatment

```
┌─[IDENTITY]──────────┬─[verbs]──┬────────────────────[VERDICT chips]──────────────┐
│ ◆ Evident  pick.ev ●│ ▶ ⊨ ⤓ ⌘K │  📌  │ ✓ Terminates  ⊏ x∈[0,9]  ⑂ free  │ ⛨ ? │
└─────────────────────┴──────────┴───────────────────────────────────────────────┘
                                    ↑pin    ↑click any chip → Verdict dossier modal
```

- **One headline chip** — the verdict, colored by class: `✓ Terminates` /
  `↻ Cyclic` / `⑂ Nondeterministic` / `· Settles` (the existing `VERDICTS` icon+word,
  `app-symbols.js:222`). This is the always-glance answer to *"does it stop?"*.
- **Up to two evidence chips, compact** — the boundary (`⊏ x∈[0,9]`) and, when present,
  a single relations marker (`⊢ 2 implied` / `= x=y`). Capped at two so the header never
  wraps; everything else lives in the dossier. Each chip is monochrome, smaller than the
  headline, no buttons inline.
- **A `⛨` soundness affordance** — the fabrication self-check, as a lone glyph button
  (not the verbose `⛨ verify soundness` text). Its result paints the headline chip with a
  tiny `✓`/`✗` corner badge so "has this been cross-checked?" is glanceable.
- **A `?` about-this-verdict affordance** — opens the help/about for the vocabulary
  (Nadia owns the copy). This is where `dodging loop`, `path to rest`, `fixed point`,
  `boundary` get their one-line definitions — *attached to the thing they describe*, not
  buried in the global tour.

**Click any chip → the Verdict dossier modal.** A chip is a glance; the dossier is the
read. It's a centered modal (same chrome as the History modal, §R.3) titled with the
headline verdict, laying the evidence out as labeled rows instead of a crammed strip:

```
┌─ Verdict — ✓ Terminates ──────────────────────────────────[?]─[✕]─┐
│  the orbit converges to a fixed point                              │
│                                                                    │
│  ● fixed point      (count = 9)                                    │
│  ⊏ boundary         count ∈ [0, 9]      x ∈ [0, 9]                 │
│  ⊢ implied          x = y   ›  click for proof (unsat-core)        │
│  = forced equal     a = b                                          │
│                                                                    │
│  ── witnesses you can replay ────────────────────────────────────  │
│  ▶ replay path to rest        init → … → (count=9)   [open in view]│
│  ▶ replay a dodging loop      (only if a lasso exists)             │
│                                                                    │
│  ⛨ verify soundness   ✓ cross-checked against brute-force enum     │
└────────────────────────────────────────────────────────────────────┘
```

Three labeled groups: **what's true** (verdict + fixed points + boundary), **what's
forced** (the relations/certs, each still click-for-proof — the #341/#345/#348
interrogability is preserved, just given room), and **witnesses you can replay** (the two
`▶ replay` buttons, which drive the existing trace scrubber on the live view; `[open in
view]` routes the replay onto whatever diagram is showing). The soundness check sits at
the bottom as the audit line. Nothing is removed — it's the *same* `renderStructure`
content, re-laid from a one-line pile into a scannable dossier, with the jargon now one
`?` away from its definition.

**Why header, not a cockpit band:** a verdict is a *property of the whole model*, like a
filename or a dirty-dot — session-level status, not one of N views. Status lives in the
frame (header/footer), per the same logic that put honesty + scope in the footer (§2,
region 7). The header already has the `📌` pin slot the owner named as the anchor; the
chips sit immediately right of it.

## R.2 Interrogate → two new diagram families (a view that takes extra constraints)

The owner's reframe: verify and query are *"a visualizer computed under an EXTRA set of
constraints."* That is exactly right, and the machinery already fits it — both
`/api/query` and `/api/temporal`/`/api/invariant` already return a **trace** that
`showTrace` renders as a scrubbable path on the live view (`app-verify.js`). They are
*already* visual; they're just trapped in a console. Promote them to views.

This introduces a new cockpit primitive — **a view with an input affordance** — and two
new families in the §3 taxonomy.

### The primitive: a diagram cell that takes a user constraint

Most views render the model as-is. An *interrogable* view renders the model **plus a
predicate the user supplies**, with the predicate input docked into the view region's
header (not a separate panel). The shape:

```
┌─[VIEW]───────────────────────────────────────────────────────────┐
│ [family tabs … ⊨? query  ⊢ verify …]                              │
│ ┌─ input affordance (only for interrogable views) ──────────────┐ │
│ │ ⊨?  light = Green ∧ timer = 2          [find]  [assert ⊢+]    │ │  ← the extra
│ └────────────────────────────────────────────────────────────────┘ │    constraints
│ ┌────────────────────────────────────────────────────────────┐   │
│ │   [ the figure: the found values / the counterexample path ]│   │
│ └────────────────────────────────────────────────────────────┘   │
│ caption: "the reachable state(s) satisfying your query"            │
└───────────────────────────────────────────────────────────────────┘
```

The input row appears **only when an interrogable view is active**, anchored to the view
header — so the editor isn't permanently shadowed by a console, but the affordance is
right where the result draws. This is the general answer to the owner's "how does a
diagram that takes extra user constraints work in the cockpit": *the predicate input is
part of the view chrome, revealed with the view, the way an axis selector (#421) or the
`all initial conditions` toggle already rides a specific view.*

### (a) `⊨? query` → a new SOLUTION-SPACE family view

**Family A (Solution space)** is where "what states are possible" lives — query is the
*interactive* member: "what states are possible **and also satisfy this**." Its figure is
the found witness state(s) plotted into the solution-space picture — the witness as a
**marked point in the feasible region**, with its full record shown, and (when the query
walked a path from init) the `init → witness` trace drawn as the route to it. Multiple
witnesses (the existing `assert ⊢+` stack / enumerate) plot as a *small set of marked
points* — literally a scatter of "here's where your condition can hold," which IS a
solution-space picture. The assumption-stack chips (`#query-stack`) move into the input
affordance as removable tokens.

| view | family | rigor | input | what it answers |
|---|---|---|---|---|
| `query` | A · solution space | exhaustive\* | a conjunction (`light=Green ∧ timer=2`) | the reachable state(s) satisfying your condition — marked in the solution space, with the path that reaches one. |

### (b) `⊢ verify` → a new STRUCTURE/LAW family view (with a custom property UI)

Verify asks *"is this property true over **all** runs?"* — that's a statement about the
*law* of the system, so it joins **Family D (Structure / law)**. Its figure is the
**counterexample trace** when the property fails (the scrubbable `init → … → violation`
path the tool already draws, now as the view's own figure, on a faint render of the state
graph), or a clean **"✓ holds"** proof card when it passes. The property input is richer
than query's — it needs the safety/liveness modality vocabulary and the fairness toggle —
so its input affordance is a small purpose-built row:

```
┌─ ⊢ verify ── property ─────────────────────────────────────────────┐
│ [safety ▾]  var ≤ 5            [□ WF]   [check]                     │
│   modality:  safety □ · eventually ◇ · always-eventually □◇ · P⤳Q  │
└────────────────────────────────────────────────────────────────────┘
        figure: ✗ violated — counterexample trace (scrubbable)
                or ✓ holds — proof card
```

A modality picker (`safety □` / `◇` / `□◇` / `P ⤳ Q`) replaces the bare-input "you must
know the symbols" gate; the `WF` fairness toggle rides alongside (it only applies to
liveness). On failure the counterexample is the figure and the step scrubber is the
view's transport. On success it's a one-line proof card. This is the "custom UI for
entering the property + the counterexample-trace UI fitting a diagram cell" the owner
asked for.

| view | family | rigor | input | what it answers |
|---|---|---|---|---|
| `verify` | D · structure/law | proven\* | a property (modality picker + expr + WF) | whether a safety/liveness property holds over ALL runs — a proof card, or a scrubbable counterexample trace. |

\* both degrade to `sampled` on a capped/continuous model, badge follows the chart, same
rule as every other bound-view (§3).

### Where they sit in the gallery

Two new chips join the family-grouped tab strip from §3 — `⊨? query` as the tail of
Family A, `⊢ verify` as the tail of Family D — each carrying the `⊨` proven-best badge.
A user hunting "can it ever reach Green?" finds `query` under *solution space*; "does it
always stay ≤ 5?" finds `verify` under *structure/law*. The `⊨ Solve` button stays a
header verb (it's an action that *populates* the witness gallery, not a standing view),
but the **Interrogate cockpit band is deleted entirely** — its three rows become these
two views plus the existing Solve flow.

## R.3 History → a modal, off the footer

Session history is a horizontal strip of thumbnails (`#history`) permanently parked in
the footer. It's a *look-back* affordance — used occasionally, not per-edit — so it
shouldn't hold standing real estate. Demote it to a **🕮 history button** in the footer
that opens a modal grid:

```
footer:  scope [400] ✓complete  ·  ✓ 0 dropped  ·  vars: x        🕮 history (12) ▸

          click 🕮  →
┌─ Session history ───────────────────────────────────────[✕]─┐
│  most recent first · click a card to restore that analysis  │
│  ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐                  │
│  │ ▦  │ │ ▦  │ │ ▦  │ │ ▦  │ │ ▦  │ │ ▦  │   each card:      │
│  │claim│ │time│ │state│ │phase│ │ … │ │ … │   thumbnail +    │
│  └────┘ └────┘ └────┘ └────┘ └────┘ └────┘   view name +     │
│  pick.ev    counter    counter   …            source label    │
└──────────────────────────────────────────────────────────────┘
```

The footer keeps only the count + button (`🕮 history (12) ▸`); the grid — larger,
labeled cards with view name and the source they ran against — opens on demand. Same
modal chrome as the Verdict dossier and any future modal, so the IDE grows one consistent
overlay pattern.

## R.4 Pin — where it belongs in the IA (fix tracked separately)

Pin is "freeze result A so the next render lands beside it" (compare two constraints, or
two diagrams of one program). The owner reports it's **mostly broken** — that's a defect
for the builder/Nadia to diagnose, not an IA question. The IA answer: **pin belongs as a
`compare ▸` affordance in the footer**, beside the view, not as a top-toolbar button. It
acts on *the current view's result*, so it lives with the view/session readouts, not up
in the authoring verbs. Concretely:

- In the footer (region 7), next to honesty: `compare ▸` — pins the current result as **A**
  and arms "next render lands beside it as B."
- Once a pin exists, the view region splits A | B (the existing diff/compare path), and a
  small `A ✕` token in the footer clears it. The `⇄ diff` model-diff stays auto-revealed
  only when a pin exists (today's behavior — keep).
- This frees the top toolbar of `📌 pin` / `⇄ diff` (folding into §4.1's plan), and puts
  "compare" next to the thing compared.

The pin defect itself is filed for the builder; this section only fixes its *home*.

## R.5 Revised cockpit wireframe (after R.1–R.4)

```
┌─[IDENTITY]──────────┬─[verbs]──┬───────────────────[VERDICT chips]──────────────┐
│ ◆ Evident pick.ev ● │ ▶ ⊨ ⤓ ⌘K │ 📌│ ✓ Terminates  ⊏ x∈[0,9]  ⊢ 2 implied │⛨ ? │
├─────────────────────┴──────────┴─────────────────────────────────────────────────┤
│┌─[WORKSPACE]───┬─[EDITOR]──────────┬─[ANALYSIS COCKPIT — now 3 bands]────────────┐│
││ ▾ my files    │ 1 claim pick      │┌─ model shape ────────────────────────────┐││
││   pick.ev   ● │ 2  x ∈ Int        ││ ◆ a claim — its SOLUTION SPACE, solved   │││
││ ▾ samples     │ 3  0 ≤ x ≤ 9      │└──────────────────────────────────────────┘││
││   FSM ▾       │ 4  x ≠ 4          │┌─ view ───────────────────────────────────┐││
││   ⊨ Solve ▾   │                   ││ [A: space│struct│⊨?query]  [D:…│⊢verify] │││
││   showcase ▾  │                   ││ ┌ input (interrogable views only) ──────┐│││
││               │                   ││ │ ⊨? light=Green ∧ timer=2  [find][⊢+] ││││
││               │                   ││ └───────────────────────────────────────┘│││
││               │                   ││ ┌──────────────────────────────────────┐ │││
││               │                   ││ │   [ the figure ]                     │ │││
││               │                   ││ └──────────────────────────────────────┘ │││
││               │                   ││ caption + rigor badge                    │││
│└───────────────┴───────────────────┴──────────────────────────────────────────┘│
├─[FOOTER]──────────────────────────────────────────────────────────────────────────┤
│ ⏮◀▶⏭ tick 4/9 │ scope[400]✓ · ✓0 dropped · vars:x │ compare ▸ │ 🕮 history(12) ▸ │
└────────────────────────────────────────────────────────────────────────────────────┘
```

The Verdict strip and Interrogate band are GONE from the cockpit. The cockpit is now
**model shape → view → (footer)**. Verdict rides the header; verify/query are views;
history and compare are footer affordances that open on demand. Every "ask the model
something" — render a diagram, query a state, verify a property — is now one gesture:
pick a view (and, for the two interrogable ones, type into the input that rides it).

## R.6 Migration order for this revision

These slot AFTER the §6 slices (the cockpit grid, taxonomy, and footer must exist first —
they do). Cheapest first.

1. **Verdict headline chip → header (R.1, glance only).** Move the verdict word+note +
   one boundary chip into the header right of `📌`; keep the full `#structure` strip as
   the dossier *content* but render it into a modal opened by the chip. Reuses
   `renderStructure`'s HTML almost verbatim, re-parented into a modal. Frontend-only.
2. **Verdict dossier modal + `?` about (R.1).** The labeled-group layout + the
   vocabulary help/about (Nadia's copy). Preserves the click-for-proof relation handlers.
3. **History → modal (R.3).** Replace the footer `#history` strip with a `🕮` button +
   a modal grid reusing the existing thumbnail render. Frontend-only, mechanical.
4. **`compare ▸` re-home of pin (R.4).** Move `📌`/`⇄ diff` out of the toolbar into a
   footer `compare ▸` affordance. (The pin *defect* is a separate builder task.)
5. **Interrogable-view primitive + `⊨? query` view (R.2a).** The bigger slice: a view
   that carries an input affordance; route `/api/query` results into a solution-space
   figure (witness points + trace). Deletes the `#query-row`/`#query-stack` band.
6. **`⊢ verify` view + property UI (R.2b).** The modality picker + WF toggle as the
   view's input; the counterexample trace becomes the figure; deletes `#invariant`.
   Largest of the set — depends on 5 (the primitive) landing first.

Slices 1–4 are mechanical reshuffles (a day each). 5–6 are the real work: building the
"view that takes a constraint" primitive and re-routing two API results into figures —
but they delete an entire cockpit band and a second interaction model in exchange.
