//! Auto-tuner for the Z3 `smt.arith.solver` parameter.

use std::collections::HashMap;
use std::time::Duration;

/// Candidate `smt.arith.solver` values the runtime will try when it
/// hasn't yet committed to one. 2 is the older Simplex-based path that
/// wins on Z3 4.8.12 for our workload; 6 is the newer default that
/// wins for newer Z3 versions and on different schemas. The auto-tuner
/// runs each one for a window of frames and locks in the faster one.
///
/// Add another value here (e.g. `12` if Z3 ever ships a useful new one)
/// and pricing will pick it up automatically.
pub(super) const ARITH_SOLVER_CANDIDATES: &[u32] = &[2, 6];

/// Number of frames each candidate is timed under during pricing.
/// Long enough to swamp Z3's per-build overhead with steady-state
/// per-frame cost; short enough that pricing finishes well within
/// the warmup window of typical executor sessions.
pub(super) const PRICING_FRAMES_PER_CANDIDATE: u32 = 30;

/// Per-schema history. Drives the auto-tuner. The state machine:
///
///   Pricing { idx } — currently timing candidate ARITH_SOLVER_CANDIDATES[idx].
///                     After PRICING_FRAMES_PER_CANDIDATE frames the runtime
///                     advances `idx` (rebuilding the cache under the next
///                     candidate). After all candidates are timed, transitions
///                     to Locked under the fastest config seen.
///   Locked { config } — pricing complete. All future queries use this config.
///
/// `EVIDENT_Z3_AUTOTUNE=0` skips pricing entirely and locks immediately
/// to the env-specified `EVIDENT_Z3_ARITH_SOLVER` value (default 2).
pub(super) struct SolveHistory {
    state: TunerState,
    /// Mean ms/iter observed for each candidate fully priced. Used to
    /// pick the winner when pricing completes.
    measured: HashMap<u32, f64>,
    /// Solve times for the *current* candidate's pricing window. Cleared
    /// every time we advance to the next candidate.
    current_window: Vec<Duration>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TunerState {
    /// Currently timing `ARITH_SOLVER_CANDIDATES[idx]`.
    Pricing { idx: usize },
    /// Pricing complete; this is the winner.
    Locked { config: u32 },
}

impl SolveHistory {
    /// Initial state. If autotune is disabled, lock immediately to the
    /// env-specified config (default 2). Otherwise start pricing with
    /// the first candidate.
    pub(super) fn new() -> Self {
        let autotune = std::env::var("EVIDENT_Z3_AUTOTUNE").as_deref() != Ok("0");
        if !autotune {
            let initial: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(2);
            return SolveHistory {
                state: TunerState::Locked { config: initial },
                measured: HashMap::new(),
                current_window: Vec::new(),
            };
        }
        SolveHistory {
            state: TunerState::Pricing { idx: 0 },
            measured: HashMap::new(),
            current_window: Vec::with_capacity(PRICING_FRAMES_PER_CANDIDATE as usize),
        }
    }

    /// The `arith_solver` value the cache should be built under right now.
    pub(super) fn current_config(&self) -> u32 {
        match self.state {
            TunerState::Pricing { idx }     => ARITH_SOLVER_CANDIDATES[idx],
            TunerState::Locked  { config }  => config,
        }
    }

    /// Record a solve time. Returns `Some(new_config)` if the tuner
    /// decided to swap configs (caller should evict the cache so the
    /// next query rebuilds under the new value), `None` otherwise.
    pub(super) fn record(&mut self, dt: Duration) -> Option<u32> {
        let TunerState::Pricing { idx } = self.state else { return None; };

        self.current_window.push(dt);
        if self.current_window.len() < PRICING_FRAMES_PER_CANDIDATE as usize {
            return None;
        }

        // Window full — finalize this candidate's measurement.
        let total_ms: f64 = self.current_window.iter()
            .map(|d| d.as_secs_f64() * 1000.0).sum();
        let mean_ms = total_ms / self.current_window.len() as f64;
        let cfg = ARITH_SOLVER_CANDIDATES[idx];
        self.measured.insert(cfg, mean_ms);
        self.current_window.clear();

        let next_idx = idx + 1;
        if next_idx < ARITH_SOLVER_CANDIDATES.len() {
            // More candidates to price.
            self.state = TunerState::Pricing { idx: next_idx };
            let next_cfg = ARITH_SOLVER_CANDIDATES[next_idx];
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] arith.solver={cfg} → {mean_ms:.2} ms/iter; \
                           probing arith.solver={next_cfg} next");
            }
            Some(next_cfg)
        } else {
            // All candidates priced. Pick the fastest.
            let winner = self.measured.iter()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(c, _)| *c)
                .unwrap_or(2);
            self.state = TunerState::Locked { config: winner };
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] pricing complete: {:?}; locking arith.solver={winner}",
                          self.measured);
            }
            // Return Some only if we need to rebuild cache (i.e. we
            // were timing a different config than the winner).
            if winner != cfg { Some(winner) } else { None }
        }
    }
}
