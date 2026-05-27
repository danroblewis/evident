//! The `FsmEngine` trait every engine plugs into, plus the matrix runner that
//! classifies each (engine × fixture) into one honest verdict and renders the
//! pass/fail matrix.
//!
//! ## The contract a new engine signs
//!
//! Implement [`FsmEngine::tick`]: given a [`Fixture`] (its `meta` + the
//! `*.smt2` paths), return what the engine computed for that one tick —
//! [`Outcome::Sat`] (next-state model + maybe effects), [`Outcome::Unsat`], or
//! [`Outcome::Unsupported`] (a documented capability boundary). [`run_matrix`]
//! diffs each against the golden and assigns a [`Verdict`]. The ONLY failing
//! verdict is [`Verdict::Fail`] — a *wrong* answer. Declining a fixture an
//! engine genuinely can't do yet is a [`Verdict::Gap`], which is green: the
//! contract documents boundaries honestly, it does not fake them.

use std::collections::BTreeMap;

use crate::fixture::Fixture;
use crate::value::CVal;

/// What an engine produced for one fixture's tick.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Solve succeeded. `model` covers (at least) the keys the engine could
    /// produce; `effects` is `Some(list)` when the engine surfaced the
    /// dispatched effects, or `None` when it did not compute them for this
    /// fixture (e.g. effects not encoded in the portable SMT). `None` ≠ "empty
    /// effects" — it means "not checked", and is reported as such.
    Sat {
        model: BTreeMap<String, CVal>,
        effects: Option<Vec<CVal>>,
    },
    /// The pinned transition has no model (the genuine witness for a negative
    /// fixture).
    Unsat,
    /// The engine legitimately cannot run this fixture yet (documented boundary).
    Unsupported(String),
}

/// A pluggable per-tick FSM engine. A replacement runtime proves it preserves
/// the captured semantics by implementing this and going green on the matrix.
pub trait FsmEngine {
    fn name(&self) -> &str;
    /// Run one tick of `fx` and report what it produced.
    fn tick(&self, fx: &Fixture) -> Outcome;
}

/// The verdict for one (engine × fixture) cell. Only [`Verdict::Fail`] is red.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Reproduced the golden fully (every checked model key + effects).
    Pass,
    /// State outputs matched; the engine did not surface effects for this
    /// fixture (they aren't in the portable SMT). Honest partial — green.
    PassStateOnly,
    /// Engine cleanly declined a fixture it can't do yet (documented boundary).
    Gap(String),
    /// Engine produced a WRONG answer. The only failing verdict.
    Fail(String),
}

impl Verdict {
    pub fn is_fail(&self) -> bool {
        matches!(self, Verdict::Fail(_))
    }
    /// Compact matrix cell glyph.
    pub fn glyph(&self) -> &'static str {
        match self {
            Verdict::Pass => "✓",
            Verdict::PassStateOnly => "✓ˢ",
            Verdict::Gap(_) => "—",
            Verdict::Fail(_) => "✗",
        }
    }
}

/// Diff one engine outcome against a fixture's golden into a [`Verdict`].
pub fn classify(fx: &Fixture, out: &Outcome) -> Verdict {
    let m = &fx.meta;

    // ── Negative fixtures ──────────────────────────────────────────────────
    if m.expect_unsat {
        return match out {
            Outcome::Unsat => Verdict::Pass,
            Outcome::Unsupported(r) => Verdict::Gap(r.clone()),
            Outcome::Sat { model, .. } => {
                // Witness "transition impossible" as: the forced output differs
                // from each forbidden value (the functionizer-fast-path witness).
                if m.expect_forbidden.is_empty() {
                    return Verdict::Fail("negative fixture: engine returned Sat, no `forbidden` to disprove".into());
                }
                for (k, forbidden) in &m.expect_forbidden {
                    match model.get(k) {
                        Some(got) if got.canonical() == forbidden.canonical() => {
                            return Verdict::Fail(format!(
                                "forbidden transition occurred: {k} = {} (== forbidden)",
                                got.canonical()
                            ));
                        }
                        Some(_) => {} // forced ≠ forbidden ✓
                        None => {
                            return Verdict::Gap(format!(
                                "can't witness negative: did not produce forbidden key `{k}`"
                            ));
                        }
                    }
                }
                Verdict::Pass
            }
        };
    }

    // ── Positive fixtures ──────────────────────────────────────────────────
    let (model, effects) = match out {
        Outcome::Sat { model, effects } => (model, effects),
        Outcome::Unsupported(r) => return Verdict::Gap(r.clone()),
        Outcome::Unsat => {
            return Verdict::Fail("expected SAT, engine reported UNSAT".into())
        }
    };

    let mut mismatches = Vec::new();
    let mut missing = Vec::new();
    for (k, want) in &m.expect_model {
        match model.get(k) {
            Some(got) if got.canonical() == want.canonical() => {}
            Some(got) => mismatches.push(format!(
                "{k}: got {}, want {}",
                got.canonical(),
                want.canonical()
            )),
            None => missing.push(k.clone()),
        }
    }
    if !mismatches.is_empty() {
        return Verdict::Fail(format!("model mismatch: {}", mismatches.join("; ")));
    }

    let golden_lines = fx.expected_effect_lines();
    let effects_verdict = match effects {
        Some(got) => {
            let got_lines: Vec<String> = got.iter().map(|e| e.effect_line()).collect();
            if got_lines == golden_lines {
                EffStatus::Ok
            } else {
                EffStatus::Mismatch(format!(
                    "effects: got {got_lines:?}, want {golden_lines:?}"
                ))
            }
        }
        None => EffStatus::NotChecked,
    };
    if let EffStatus::Mismatch(d) = &effects_verdict {
        return Verdict::Fail(d.clone());
    }

    if !missing.is_empty() {
        return Verdict::Gap(format!("did not produce output(s): {}", missing.join(", ")));
    }

    match effects_verdict {
        EffStatus::Ok => Verdict::Pass,
        EffStatus::NotChecked if golden_lines.is_empty() => Verdict::Pass,
        EffStatus::NotChecked => Verdict::PassStateOnly,
        EffStatus::Mismatch(_) => unreachable!("handled above"),
    }
}

enum EffStatus {
    Ok,
    NotChecked,
    Mismatch(String),
}

/// One engine's verdict for every fixture, in fixture order.
pub struct EngineColumn {
    pub engine: String,
    pub verdicts: Vec<Verdict>,
}

/// The full matrix: fixtures (rows) × engines (columns).
pub struct MatrixReport {
    pub fixtures: Vec<String>,
    pub columns: Vec<EngineColumn>,
}

impl MatrixReport {
    pub fn any_fail(&self) -> bool {
        self.columns.iter().any(|c| c.verdicts.iter().any(|v| v.is_fail()))
    }

    /// All `(engine, fixture, reason)` failures — for an assert message.
    pub fn failures(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for c in &self.columns {
            for (i, v) in c.verdicts.iter().enumerate() {
                if let Verdict::Fail(r) = v {
                    out.push((c.engine.clone(), self.fixtures[i].clone(), r.clone()));
                }
            }
        }
        out
    }

    /// Per-engine `(pass, state-only, gap, fail)` tallies.
    pub fn tallies(&self) -> Vec<(String, usize, usize, usize, usize)> {
        self.columns
            .iter()
            .map(|c| {
                let mut p = 0;
                let mut s = 0;
                let mut g = 0;
                let mut f = 0;
                for v in &c.verdicts {
                    match v {
                        Verdict::Pass => p += 1,
                        Verdict::PassStateOnly => s += 1,
                        Verdict::Gap(_) => g += 1,
                        Verdict::Fail(_) => f += 1,
                    }
                }
                (c.engine.clone(), p, s, g, f)
            })
            .collect()
    }

    /// Render the matrix + a reasons appendix as GitHub-flavored markdown.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        // Header row.
        out.push_str("| Fixture |");
        for c in &self.columns {
            out.push_str(&format!(" {} |", c.engine));
        }
        out.push('\n');
        out.push_str("|---|");
        for _ in &self.columns {
            out.push_str("---|");
        }
        out.push('\n');
        // Body.
        for (i, fname) in self.fixtures.iter().enumerate() {
            out.push_str(&format!("| `{fname}` |"));
            for c in &self.columns {
                out.push_str(&format!(" {} |", c.verdicts[i].glyph()));
            }
            out.push('\n');
        }
        // Tallies.
        out.push_str("\n**Tallies** (✓ full · ✓ˢ state-only · — gap · ✗ fail):\n\n");
        for (e, p, s, g, f) in self.tallies() {
            out.push_str(&format!(
                "- **{e}**: {p} ✓, {s} ✓ˢ, {g} —, {f} ✗\n"
            ));
        }
        // Reasons appendix (gaps + fails).
        out.push_str("\n**Notes** (gaps & fails, by engine):\n\n");
        for c in &self.columns {
            let mut lines = Vec::new();
            for (i, v) in c.verdicts.iter().enumerate() {
                match v {
                    Verdict::Gap(r) => lines.push(format!("  - `{}` — gap: {r}", self.fixtures[i])),
                    Verdict::Fail(r) => lines.push(format!("  - `{}` — FAIL: {r}", self.fixtures[i])),
                    Verdict::PassStateOnly => {
                        lines.push(format!("  - `{}` — state verified; effects not in portable SMT (checked vs runtime engine)", self.fixtures[i]))
                    }
                    Verdict::Pass => {}
                }
            }
            if !lines.is_empty() {
                out.push_str(&format!("- {}:\n{}\n", c.engine, lines.join("\n")));
            }
        }
        out
    }

    /// Render compactly for a test log (`--nocapture`).
    pub fn to_text(&self) -> String {
        self.to_markdown()
    }
}

/// Run every engine over every fixture and build the matrix.
pub fn run_matrix(engines: &[&dyn FsmEngine], fixtures: &[Fixture]) -> MatrixReport {
    let columns = engines
        .iter()
        .map(|eng| {
            let verdicts = fixtures
                .iter()
                .map(|fx| {
                    let out = eng.tick(fx);
                    classify(fx, &out)
                })
                .collect();
            EngineColumn { engine: eng.name().to_string(), verdicts }
        })
        .collect();
    MatrixReport {
        fixtures: fixtures.iter().map(|f| f.name.clone()).collect(),
        columns,
    }
}
