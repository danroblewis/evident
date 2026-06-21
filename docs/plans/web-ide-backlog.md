# Evident Web IDE — Feature Backlog

A living pool of features, wider than the committed plan. `web-ide.md` is **the spec**
(what we've decided to build, M0–M5); this is **the backlog** — proposed, triaged,
parked. The `ide-feature-designer` agent (Iris) appends `Proposed` batches grounded in
what only Evident can do; we promote winners into the spec and park the rest.

**Legend** — priority: `★` killer · `⬆` strong · `•` nice · `?` speculative ·
effort: `S/M/L` · depends: `FE` frontend / `BE` backend / `RT` runtime.

---

## Planned (already in the spec, M0–M5)

Iris should NOT re-propose these — they're committed.

- `[M0]` live write→see loop — edit a constraint, diagram updates ≲300 ms · M · FE+BE
- `[M0]` model-shape banner (driven / relational / nondeterministic) · S · BE
- `[M0]` dropped-constraint honesty line · S · BE
- `[M1]` interactive `time_series` scrubber + tick transport · M · FE
- `[M1]` `state_graph` click → inspector + constraint provenance · M · FE+RT
- `[M1]` brushing-and-linking across views (one state, many lenses) · M · FE
- `[M2]` language server: diagnostics + footgun detectors, hover types, completion · L · BE
- `[M2]` Unicode input method (`\in`→∈) · S · FE
- `[M2]` `⟦solve⟧` / `⟦run⟧` codelens · S · FE+BE
- `[M3]` solver console: claim SAT/witness · UNSAT/core · solve-for-X · pin-and-explore · M · BE
- `[M4]` full 16-view gallery interactive + export + deep-link-to-a-state · L · FE
- `[M5]` (hosted) projects, persistence, multi-file, share links, sandbox · L · FE+BE

---

## Proposed (from Iris — triage these into the spec)

_Empty. Run `ide-feature-designer` to populate the first batch._

<!-- Iris appends dated batches below this line. Format per item:
### <name>   ★|⬆|•|?   · effort S/M/L · depends FE|BE|RT
<one-line pitch>. **Only-Evident:** <why this exploits the solver / relational model /
the live diagram, not a generic IDE feature>. **Lens:** <solver | direct-manipulation |
steal-from-masters | lower-the-floor | critic-pain>. <2–3 sentences of substance.>
-->

---

## Parked / out of scope (with the reason)

_None yet._
