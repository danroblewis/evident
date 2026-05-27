//! N4a transition cache — memoizes tick solves by (FSM identity + prev + given).
//!
//! A tick is a pure function: given the same FSM transition relation, the same
//! previous-state values, and the same given inputs, Z3 will always return the
//! same next-state and effects. An FSM that loops through a finite set of states
//! (a cycle, a toggle, a steady counter) would otherwise re-invoke Z3 for every
//! visit. This module memoizes those calls so repeated identical inputs cost only
//! a HashMap lookup.
//!
//! ## Cache key design
//!
//! The key combines:
//!   - `fsm.name`       — identifies *which* FSM (human-readable name)
//!   - `fsm.transition` — the full SMT-LIB transition text; two FSMs with the
//!                        same name but different transitions (e.g. after a
//!                        reload) do not collide.
//!   - the `prev` map   — formatted via `{:?}` (BTreeMap Debug is sorted and
//!                        stable, so this is canonical).
//!   - the `given` map  — same.
//!
//! A U+001F UNIT SEPARATOR (unlikely to appear in content) joins the four parts.

use std::collections::{BTreeMap, HashMap};

use crate::spec::{FsmSpec, TickModel};
use crate::tick::{solve_tick, TickError};
use crate::z3c::Value;

// ---------------------------------------------------------------------------
// Public type
// ---------------------------------------------------------------------------

/// Memoizes tick solves by (FSM identity, prev-state, given inputs).
///
/// The cache is a pure optimization: every result it returns is identical to
/// what a direct `solve_tick` call would return. Errors are NOT cached — a
/// failing solve returns the error immediately and leaves the table unchanged.
#[derive(Default)]
pub struct TickCache {
    table: HashMap<String, TickModel>,
    hits: u64,
    misses: u64,
}

impl TickCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        TickCache { table: HashMap::new(), hits: 0, misses: 0 }
    }

    /// Solve a tick, returning the cached result when possible.
    ///
    /// On a **cache hit** the stored [`TickModel`] is cloned and returned;
    /// `hits` is incremented and Z3 is NOT called.
    ///
    /// On a **cache miss** `solve_tick` is called. On success the result is
    /// stored and `misses` is incremented. On error the cache is left
    /// unchanged and the error is returned as-is.
    pub fn solve(
        &mut self,
        fsm: &FsmSpec,
        prev: &BTreeMap<String, Value>,
        given: &BTreeMap<String, Value>,
    ) -> Result<TickModel, TickError> {
        let key = cache_key(fsm, prev, given);
        if let Some(model) = self.table.get(&key) {
            self.hits += 1;
            return Ok(model.clone());
        }
        // Miss — call Z3.
        let model = solve_tick(fsm, prev, given)?;
        self.table.insert(key, model.clone());
        self.misses += 1;
        Ok(model)
    }

    /// Number of cache hits since creation.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Number of cache misses (= distinct Z3 solves) since creation.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Total number of distinct (FSM, prev, given) triples currently cached.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// True if no entries have been stored yet.
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Key construction
// ---------------------------------------------------------------------------

/// Build a deterministic String key for `(fsm, prev, given)`.
///
/// `BTreeMap`'s `{:?}` output is stable and sorted, so it is safe to use as a
/// canonical representation of the map content. A U+001F UNIT SEPARATOR
/// separates the four components (name, transition, prev, given).
fn cache_key(
    fsm: &FsmSpec,
    prev: &BTreeMap<String, Value>,
    given: &BTreeMap<String, Value>,
) -> String {
    const SEP: char = '\u{1f}';
    format!("{}{SEP}{}{SEP}{:?}{SEP}{:?}", fsm.name, fsm.transition, prev, given)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::load_str;

    // A minimal countdown FSM — simple enough that Z3 solves it deterministically
    // without any effect/halt machinery.
    const FIX: &str = "\
; @meta
; { \"fsms\": [ { \"name\": \"cd\",
;   \"state\": [{\"prev\":\"_count\",\"next\":\"count\",\"sort\":\"Int\",\"init\":3}] } ] }
; @end
; @transition cd
(declare-const _count Int)
(declare-const count Int)
(assert (= count (- _count 1)))
";

    fn fsm() -> crate::spec::FsmSpec {
        load_str(FIX).unwrap().fsms.pop().unwrap()
    }

    fn prev(n: i64) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("_count".to_string(), Value::Int(n));
        m
    }

    // --- basic: first solve is a miss, result is correct --------------------

    #[test]
    fn first_solve_is_miss() {
        let fsm = fsm();
        let mut cache = TickCache::new();

        // Fresh cache must be empty.
        assert!(cache.is_empty());

        let result = cache.solve(&fsm, &prev(3), &BTreeMap::new()).unwrap();

        // count = _count - 1 = 3 - 1 = 2
        assert_eq!(result.next_value("count"), Some(&Value::Int(2)));
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), 1);
    }

    // --- second identical solve is a hit, same result, table unchanged ------

    #[test]
    fn second_identical_solve_is_hit() {
        let fsm = fsm();
        let mut cache = TickCache::new();

        let r1 = cache.solve(&fsm, &prev(3), &BTreeMap::new()).unwrap();
        let r2 = cache.solve(&fsm, &prev(3), &BTreeMap::new()).unwrap();

        assert_eq!(r1, r2);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.len(), 1, "no new entry on a hit");
    }

    // --- different prev is a new miss, new entry in the table ---------------

    #[test]
    fn different_prev_is_new_miss() {
        let fsm = fsm();
        let mut cache = TickCache::new();

        let r3 = cache.solve(&fsm, &prev(3), &BTreeMap::new()).unwrap();
        let r5 = cache.solve(&fsm, &prev(5), &BTreeMap::new()).unwrap();

        assert_eq!(r3.next_value("count"), Some(&Value::Int(2)));
        assert_eq!(r5.next_value("count"), Some(&Value::Int(4)));
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), 2);
    }

    // --- hot-loop: 10 alternating solves collapse to 2 misses + 8 hits ------
    //
    // This is the GATE: a real-world loop revisiting {count:0, count:1}
    // alternately should hit the cache for every revisit after the first two.

    #[test]
    fn hot_loop_collapses_to_cache_hits() {
        let fsm = fsm();
        let mut cache = TickCache::new();

        // 10 calls alternating between two inputs → 2 misses, 8 hits.
        for i in 0..10_u64 {
            let n = (i % 2) as i64; // alternates 0, 1, 0, 1, …
            cache.solve(&fsm, &prev(n), &BTreeMap::new()).unwrap();
        }

        assert_eq!(cache.misses(), 2, "only 2 distinct inputs ever seen");
        assert_eq!(cache.hits(), 8, "the remaining 8 calls were cache hits");
        assert_eq!(cache.len(), 2, "exactly 2 entries in the table");
    }

    // --- cache is transparent: matches a direct solve_tick call -------------

    #[test]
    fn cached_result_matches_direct_solve() {
        let fsm = fsm();
        let mut cache = TickCache::new();
        let p = prev(7);

        let cached = cache.solve(&fsm, &p, &BTreeMap::new()).unwrap();
        let direct = solve_tick(&fsm, &p, &BTreeMap::new()).unwrap();

        assert_eq!(cached, direct, "cache must be transparent w.r.t. solve_tick");
    }

    // --- is_empty() on a fresh cache ----------------------------------------

    #[test]
    fn is_empty_on_fresh_cache() {
        let cache = TickCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 0);
    }
}
