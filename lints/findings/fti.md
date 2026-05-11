# Findings: runtime/src/fti.rs

Reviewed against `lints/rules/` as of HEAD (baf8078).

## Violations of existing rules

None. AP-001 explicitly exempts `fti.rs` from the
no-library-specific-in-language-core rule (the registry mentions
library names intentionally). AP-002–AP-008 are scoped to
`examples/`, `tests/`, or other files and don't apply here.

## Cross-language contract: INSTALLERS ↔ stdlib/runtime.ev

The invariants doc requires every name in `INSTALLERS` to correspond
to a `type X` declaration in `stdlib/runtime.ev`. Verification:

| INSTALLERS entry | `stdlib/runtime.ev` type | Field contract match |
|---|---|---|
| `"FrameClock"`  | `type FrameClock` (line 204)  | install writes `tick_count` (Int); type declares `tick_count ∈ Int`. OK. |
| `"Hostname"`    | `type Hostname` (line 211)    | install writes `name` (String); type declares `name ∈ String`. OK. |
| `"Timer"`       | `type Timer` (line 223)       | install reads `interval_ms` pin, writes `tick_count`; type declares both. OK. |
| `"SDL_Window"`  | `type SDL_Window` (line 240)  | install reads `title`/`width`/`height` pins, writes `handle`/`gl_handle`/`vao`; type declares all six. OK. |
| `"GL_Program"`  | `type GL_Program` (line 259)  | install reads `vertex_src`/`fragment_src` pins, writes `handle`; type declares all three. OK. |

Other types in `stdlib/runtime.ev` that intentionally have NO
INSTALLERS entry, and why:

- `type FrameTimer` (line 189), `type Signal` (line 191) — legacy
  marker-type subscription path, handled outside FTI (per the
  comment "LEGACY" + the CLAUDE.md "marker-type subscription (legacy
  v3 path)" framing).

No drift in either direction.

## File-level invariant compliance

- **"Single dispatch table mapping Evident type names → install fns"** —
  `INSTALLERS: &[(&str, FtiInstallFn)]` at line 38, exactly that shape.
- **"Must NOT contain bridge logic — only the table"** — each
  `install_*` function is plumbing only: read pins, construct a
  bridge struct (defined in `event_sources`), call `start[_inline]`,
  return. No SDL/GL/timer behavior implemented here.
- **"Must NOT build constraints, schedule, perform Effects"** — no Z3,
  no Solver, no Effect dispatch in the file. Confirmed.
- **"Must NOT hold any state — registry is a static &[...]"** — the
  only data at module scope is the `const INSTALLERS`. No `static mut`,
  no `OnceCell`, no `lazy_static`, no interior mutability. Confirmed.
- **Dependencies: ast (Pins), event_sources only** — imports are
  `std::sync::mpsc::Sender`, `crate::ast::Pins`,
  `crate::event_sources::{...}`. The `mpsc::Sender` is the channel
  type that `event_sources` exposes (`Sender<SchedulerEvent>`); it
  is incidental Rust plumbing, not a layering violation. No reach
  into `runtime`, `effect_loop`, `effect_dispatch`, `translate`, or
  `ffi`.

## Candidate new rules

None. The cross-language contract documented in the invariants doc
(every INSTALLERS entry needs a matching `type` in
`stdlib/runtime.ev`, with field shapes that match the install
function's pin reads / writes) is plausibly worth mechanizing — a
script could parse INSTALLERS' `&str` keys, parse `stdlib/runtime.ev`
type declarations, and diff names + fields. But:

- Today the table has 5 entries and `stdlib/runtime.ev` is a single
  small file; drift would be caught by any test that touches an FTI
  type (the bridge would fail to install or write to a non-existent
  field). The cost of a custom AP rule with parser is not yet
  justified.
- If a third file enters the contract (e.g. a FTI types document
  separate from `stdlib/runtime.ev`), or the table grows past ~10
  entries, revisit. Suggested name at that point: AP-009-fti-types-
  match-stdlib-runtime.

Logging this as review-only for now; no rule file added.

## Clean

The file is otherwise clean. No violations of existing rules, no
new rule candidates that clear the bar today.
