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
| `timing_diagram` | exhaustive* | the same ensemble as EE-style digital/analog waveform lanes. |
| `space_time` | exhaustive* | a Seq-carried state's evolution as a rows=ticks × cols=positions raster (Rule 90 → Sierpiński). |
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
