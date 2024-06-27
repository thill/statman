//! This module provides implementations of different trackable statistics

use std::{
    collections::BTreeSet,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc,
    },
};

use atomic_float::AtomicF64;

use crate::{snapshot::HistogramSnapshot, HdrHistogram};

/// A counter stat, which may optionally reset to `0` when flushed
pub struct Counter {
    value: AtomicU64,
    reset_on_flush: AtomicBool,
}
impl Counter {
    pub(crate) fn new(reset_on_flush: bool) -> Self {
        Self {
            value: AtomicU64::new(0),
            reset_on_flush: AtomicBool::new(reset_on_flush),
        }
    }

    /// Set the flag that will determine if the state is reset to `0` when `flush` is called
    pub fn set_reset_on_flush(&self, reset_on_flush: bool) {
        self.reset_on_flush.store(reset_on_flush, Ordering::Relaxed)
    }

    /// Increment the underlying count by 1
    pub fn increment(&self) {
        self.add(1)
    }

    /// Increase the underlying count by the given value
    pub fn add(&self, val: u64) {
        self.value.fetch_add(val, Ordering::Relaxed);
    }

    /// Get the current value, resetting the state based on the `reset_on_flush` flag.
    pub(crate) fn flush(&self) -> u64 {
        if self.reset_on_flush.load(Ordering::Relaxed) {
            self.value.swap(0, Ordering::Relaxed)
        } else {
            self.value.load(Ordering::Relaxed)
        }
    }
}
impl Debug for Counter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Counter")
            .field("value", &self.value)
            .finish()
    }
}

/// A floating point gauge, which may optionally reset to `0.0` when flushed
pub struct GaugeF64 {
    value: Arc<AtomicF64>,
    reset_on_flush: AtomicBool,
}
impl GaugeF64 {
    pub(crate) fn new(reset_on_flush: bool) -> Self {
        Self {
            value: Arc::new(AtomicF64::new(0.0)),
            reset_on_flush: AtomicBool::new(reset_on_flush),
        }
    }

    /// Set the flag that will determine if the state is reset to `0.0` when `flush` is called
    pub fn set_reset_on_flush(&self, reset_on_flush: bool) {
        self.reset_on_flush.store(reset_on_flush, Ordering::Relaxed)
    }

    /// Set the gauge value
    pub fn set(&self, val: f64) {
        self.value.store(val, Ordering::Relaxed)
    }

    /// Get the current value, resetting the state based on the `reset_on_flush` flag.
    pub(crate) fn flush(&self) -> f64 {
        if self.reset_on_flush.load(Ordering::Relaxed) {
            self.value.swap(0.0, Ordering::Relaxed)
        } else {
            self.value.load(Ordering::Relaxed)
        }
    }
}
impl Debug for GaugeF64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GaugeF64")
            .field("value", &self.value)
            .finish()
    }
}

/// A signed integer gauge, which may optionally reset to `0` when flushed
pub struct GaugeI64 {
    value: Arc<AtomicI64>,
    reset_on_flush: AtomicBool,
}
impl GaugeI64 {
    pub(crate) fn new(reset_on_flush: bool) -> Self {
        Self {
            value: Arc::new(AtomicI64::new(0)),
            reset_on_flush: AtomicBool::new(reset_on_flush),
        }
    }

    /// Set the flag that will determine if the state is reset to `0.0` when `flush` is called
    pub fn set_reset_on_flush(&self, reset_on_flush: bool) {
        self.reset_on_flush.store(reset_on_flush, Ordering::Relaxed)
    }

    /// Set the gauge value
    pub fn set(&self, val: i64) {
        self.value.store(val, Ordering::Relaxed)
    }

    /// Get the current value, resetting the state based on the `reset_on_flush` flag.
    pub(crate) fn flush(&self) -> i64 {
        if self.reset_on_flush.load(Ordering::Relaxed) {
            self.value.swap(0, Ordering::Relaxed)
        } else {
            self.value.load(Ordering::Relaxed)
        }
    }
}
impl Debug for GaugeI64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GaugeI64")
            .field("value", &self.value)
            .finish()
    }
}

/// A histogram of `u64` values, which may optionally reset state when flushed
pub struct Histogram {
    hist: spin::Mutex<HdrHistogram>,
    reset_on_flush: AtomicBool,
}
impl Histogram {
    pub(crate) fn new(reset_on_flush: bool) -> Self {
        Self {
            hist: spin::Mutex::new(HdrHistogram::new(3).expect("hdrhistogram with 3 sigfigs")),
            reset_on_flush: AtomicBool::new(reset_on_flush),
        }
    }

    /// Set the flag that will determine if the state is reset to `0.0` when `flush` is called
    pub fn set_reset_on_flush(&self, reset_on_flush: bool) {
        self.reset_on_flush.store(reset_on_flush, Ordering::Relaxed)
    }

    /// Record a value
    pub fn record(&self, val: u64) {
        let mut hist = self.hist.lock();
        hist.record(val).ok();
    }

    /// Clear the state, retruning a summary of the prior state
    pub(crate) fn flush(&self, percentiles: &Vec<f64>) -> HistogramSnapshot {
        let mut hist = self.hist.lock();
        let count = hist.len();
        let mut percentile_values = Vec::new();
        if count > 0 {
            for p in percentiles {
                let p = *p;
                let v = if p == 0.0 {
                    hist.min()
                } else if p == 100.0 {
                    hist.max()
                } else {
                    hist.value_at_percentile(p)
                };
                percentile_values.push((p, v));
            }
        }
        if self.reset_on_flush.load(Ordering::Relaxed) {
            hist.clear();
        }
        HistogramSnapshot {
            count,
            percentiles: percentile_values,
        }
    }
}
impl Debug for Histogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hist = self.hist.lock();
        let count = hist.len();
        if count == 0 {
            f.debug_struct("Histogram").field("count", &count).finish()
        } else {
            f.debug_struct("Histogram")
                .field("count", &count)
                .field("min", &hist.min())
                .field("max", &hist.max())
                .field("p50", &hist.value_at_percentile(50.0))
                .finish()
        }
    }
}

/// A set of strings, which may optionally be cleared when flushed
pub struct Set {
    values: Arc<scc::HashSet<String>>,
    reset_on_flush: AtomicBool,
}
impl Set {
    pub(crate) fn new(reset_on_flush: bool) -> Self {
        Self {
            values: Arc::new(scc::HashSet::new()),
            reset_on_flush: AtomicBool::new(reset_on_flush),
        }
    }

    /// Set the flag that will determine if the state is reset to `0.0` when `flush` is called
    pub fn set_reset_on_flush(&self, reset_on_flush: bool) {
        self.reset_on_flush.store(reset_on_flush, Ordering::Relaxed)
    }

    /// Clear all values from the set
    pub fn clear(&self) {
        self.values.clear()
    }

    /// Insert a value into the set
    pub fn insert<T: Into<String>>(&self, val: T) {
        self.values.insert(val.into()).ok();
    }

    /// Remove a value from the set
    pub fn remove(&self, val: &str) {
        self.values.remove(val);
    }

    /// Return a copy of the current values in the set without changing the current state
    pub(crate) fn flush(&self) -> Vec<String> {
        let mut result = BTreeSet::new();
        self.values.scan(|x| {
            result.insert(x.clone());
        });
        if self.reset_on_flush.load(Ordering::Relaxed) {
            for v in result.iter() {
                self.values.remove(v);
            }
        }
        result.into_iter().collect()
    }
}
impl Debug for Set {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Set").field("values", &self.values).finish()
    }
}
