//! Auto-tuner for the Z3 `smt.arith.solver` parameter.

use std::collections::HashMap;
use std::time::Duration;

/// `smt.arith.solver` candidates: 2 = Simplex (wins on Z3 4.8.12), 6 = newer default.
/// Add values here and the auto-tuner picks them up automatically.
pub(super) const ARITH_SOLVER_CANDIDATES: &[u32] = &[2, 6];

/// Frames timed per candidate — enough to swamp Z3 warmup, short enough to finish quickly.
pub(super) const PRICING_FRAMES_PER_CANDIDATE: u32 = 30;

/// Per-schema solve history. `Pricing { idx }` → times candidates → `Locked { config }`.
/// `EVIDENT_Z3_AUTOTUNE=0` skips to locked with `EVIDENT_Z3_ARITH_SOLVER` (default 2).
pub(super) struct SolveHistory {
    state: TunerState,
    /// Mean ms/iter per fully-priced candidate; winner is the minimum.
    measured: HashMap<u32, f64>,
    /// Solve times for the current candidate's window; cleared on advance.
    current_window: Vec<Duration>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TunerState {
    Pricing { idx: usize },
    Locked { config: u32 },
}

impl SolveHistory {
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

    /// The arith_solver value the cache should use right now.
    pub(super) fn current_config(&self) -> u32 {
        match self.state {
            TunerState::Pricing { idx }     => ARITH_SOLVER_CANDIDATES[idx],
            TunerState::Locked  { config }  => config,
        }
    }

    /// Record a solve time; returns `Some(new_config)` when the tuner swaps (caller must
    /// evict the cache), `None` otherwise.
    pub(super) fn record(&mut self, dt: Duration) -> Option<u32> {
        let TunerState::Pricing { idx } = self.state else { return None; };

        self.current_window.push(dt);
        if self.current_window.len() < PRICING_FRAMES_PER_CANDIDATE as usize {
            return None;
        }

        // Window full — record this candidate's mean.
        let total_ms: f64 = self.current_window.iter()
            .map(|d| d.as_secs_f64() * 1000.0).sum();
        let mean_ms = total_ms / self.current_window.len() as f64;
        let cfg = ARITH_SOLVER_CANDIDATES[idx];
        self.measured.insert(cfg, mean_ms);
        self.current_window.clear();

        let next_idx = idx + 1;
        if next_idx < ARITH_SOLVER_CANDIDATES.len() {
            self.state = TunerState::Pricing { idx: next_idx };
            let next_cfg = ARITH_SOLVER_CANDIDATES[next_idx];
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] arith.solver={cfg} → {mean_ms:.2} ms/iter; \
                           probing arith.solver={next_cfg} next");
            }
            Some(next_cfg)
        } else {
            let winner = self.measured.iter()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(c, _)| *c)
                .unwrap_or(2);
            self.state = TunerState::Locked { config: winner };
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] pricing complete: {:?}; locking arith.solver={winner}",
                          self.measured);
            }
            // Only signal a swap when the winner differs from the last-timed config.
            if winner != cfg { Some(winner) } else { None }
        }
    }
}
