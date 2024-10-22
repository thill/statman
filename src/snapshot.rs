//! This module provides the data structures used by or generated by the [`crate::Stats::flush`] function.

use std::collections::BTreeMap;

/// Generated by [`crate::Stats::flush`] to provide a snapshot of all current [`crate::stat`] values.
#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    pub counters: BTreeMap<String, u64>,
    pub gauges_f64: BTreeMap<String, f64>,
    pub gauges_i64: BTreeMap<String, i64>,
    pub histograms: BTreeMap<String, HistogramSnapshot>,
    pub sets: BTreeMap<String, Vec<String>>,
}
impl Default for StatsSnapshot {
    fn default() -> Self {
        Self {
            counters: BTreeMap::new(),
            gauges_f64: BTreeMap::new(),
            gauges_i64: BTreeMap::new(),
            histograms: BTreeMap::new(),
            sets: BTreeMap::new(),
        }
    }
}

/// A snapshot of a histogram.
///
/// The populated percentiles are controlled by [`FlushParams::with_histogram_percentiles`].
#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    pub count: u64,
    pub percentiles: Vec<(f64, u64)>,
}

/// Used as an input to [`crate::Stats::flush`] to define flush behavior.
#[derive(Debug, Clone)]
pub struct FlushParams {
    pub histogram_percentiles: Vec<f64>,
}
impl FlushParams {
    pub fn with_histogram_percentiles(mut self, histogram_percentiles: Vec<f64>) -> Self {
        self.histogram_percentiles = histogram_percentiles;
        self
    }
}
impl Default for FlushParams {
    fn default() -> Self {
        Self {
            histogram_percentiles: vec![50.0, 90.0, 99.9, 99.99, 100.0],
        }
    }
}
