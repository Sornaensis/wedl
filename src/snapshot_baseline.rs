//! Opt-in work counters for the live snapshot baseline runner.
use std::{cell::RefCell, time::Instant};

#[derive(Default, serde::Serialize)]
pub(crate) struct Counters {
    pub source_walks: usize,
    pub walked_entries: usize,
    pub source_file_reads: usize,
    pub source_bytes_read: usize,
    pub source_preflights: usize,
    pub record_parses: usize,
    pub full_validations: usize,
    pub fingerprint_passes: usize,
    pub fingerprinted_source_bytes: usize,
    pub fts_index_probes: usize,
    pub fts_index_quick_checks: usize,
    pub fts_index_matches: usize,
    pub fts_builds: usize,
    pub projected_records: usize,
    pub projected_fts_rows: usize,
    gate_wait_us: u64,
    execution_us: u64,
}

thread_local! {
    static ACTIVE: RefCell<Option<Counters>> = const { RefCell::new(None) };
}

pub(crate) fn record(update: impl FnOnce(&mut Counters)) {
    ACTIVE.with_borrow_mut(|active| {
        if let Some(counters) = active {
            update(counters);
        }
    });
}

/// One synchronous live operation on a blocking worker.
pub(crate) struct Measurement(Instant);

impl Measurement {
    pub(crate) fn start() -> Self {
        ACTIVE.set(Some(Counters::default()));
        Self(Instant::now())
    }

    pub(crate) fn acquired_gate(&self) {
        record(|counters| counters.gate_wait_us = self.elapsed_us());
    }

    fn elapsed_us(&self) -> u64 {
        self.0.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
    }
}

impl Drop for Measurement {
    fn drop(&mut self) {
        if let Some(mut counters) = ACTIVE.take() {
            counters.execution_us = self.elapsed_us().saturating_sub(counters.gate_wait_us);
            eprintln!(
                "snapshot_baseline {}",
                serde_json::to_string(&counters).expect("serialize integer counters"),
            );
        }
    }
}
