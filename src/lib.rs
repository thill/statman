//! statman: A global manager of atomic statistics
//!
//! # Stats Manager
//!
//! Individual statistics are managed by a [`Stats`] instance.
//! Most use-cases will utilize the global instance by calling [`Stats::global`],
//! however multiple [`Stats`] managers may be instantiated using [`Stats::new()`].
//!
//! Individual stats are managed by name.
//! Stats of the same name and type will update the same underyling value.
//!
//! # Stat impls
//!
//! All stats are produced in an [`Arc`] so they may be written to by multiple thread.
//! - `Counter`: A counter backed by an [`std::sync::atomic::AtomicU64`]
//! - `GaugeF64`: A gauge backed by an [`atomic_float::AtomicF64`]
//! - `GaugeI64`: A gauge backed by an [`std::sync::atomic::AtomicI64`]
//! - `Histogram`: An [`hdrhistogram`] encapsulated with a [`spin`] lock.
//! - `Set`: A set of strings backed by an [`scc::HashSet`]
//!
//! # Scheduler
//!
//! [`Stats`] is a manager of named stats and does not do any scheduling by default.
//! Users may elect to schedule their own calls to [`Stats::flush`] or use the [`scheduler`] module.
//!
//! # Resetting Stats
//!
//! By default, stats do not reset between calls to flush.
//! This behavior can be overwritten for individual stats using `set_reset_on_flush`.
//! It can be overwritten globally using [`Stats`] `set_default_reset_on_flush_for_*` functions.
//!
//! # Example
//!
//! ```
//! use std::{
//!     sync::{
//!         atomic::{AtomicBool, Ordering},
//!         Arc,
//!     },
//!     thread,
//!     time::{Duration, SystemTime},
//! };
//!
//! use chrono::{DateTime, Utc};
//! use statman::{scheduler::FlushScheduler, snapshot::StatsSnapshot, Stats};
//! // signal handling for a graceful exit
//! let runflag = Arc::new(AtomicBool::new(true));
//! {
//!     let runflag = Arc::clone(&runflag);
//!     ctrlc::set_handler(move || {
//!         runflag.store(false, Ordering::Relaxed);
//!     })
//!     .expect("Error setting Ctrl-C handler");
//! }
//!
//! // create and spawn a scheduler that will print stats every 10 minutes
//! FlushScheduler::new(Stats::global())
//!     .with_register_callback(|snapshot: &StatsSnapshot| {
//!         println!(
//!             "{} - {:?}",
//!             DateTime::<Utc>::from(SystemTime::now()).format("%T"),
//!             snapshot
//!         )
//!     })
//!     .with_runflag(Arc::clone(&runflag))
//!     .spawn(Duration::from_secs(10))
//!     .unwrap();
//!
//! // create a histogram stat that resets on every call to flush
//! let hist = Stats::global().histogram("myhist");
//! hist.set_reset_on_flush(true);
//!
//! // record an ever increasing value to the histogram
//! let mut next_record = 0;
//! while runflag.load(Ordering::Relaxed) {
//!     hist.record(next_record);
//!     next_record += 1;
//!     thread::sleep(Duration::from_millis(10));
//! }
//! ```
//!
//! Output:
//! ```text
//! 22:11:00 - StatsSnapshot { counters: {}, gauges_f64: {}, gauges_i64: {}, histograms: {"myhist": HistogramSnapshot { count: 597, percentiles: [(50.0, 298), (90.0, 537), (99.9, 596), (99.99, 596), (100.0, 596)] }}, sets: {} }
//! 22:11:10 - StatsSnapshot { counters: {}, gauges_f64: {}, gauges_i64: {}, histograms: {"myhist": HistogramSnapshot { count: 994, percentiles: [(50.0, 1093), (90.0, 1491), (99.9, 1590), (99.99, 1590), (100.0, 1590)] }}, sets: {} }
//! 22:11:20 - StatsSnapshot { counters: {}, gauges_f64: {}, gauges_i64: {}, histograms: {"myhist": HistogramSnapshot { count: 994, percentiles: [(50.0, 2087), (90.0, 2485), (99.9, 2585), (99.99, 2585), (100.0, 2585)] }}, sets: {} }
//! ```

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use once_cell::sync::OnceCell;
use scc::{ebr::Guard, HashIndex};
use snapshot::{FlushParams, StatsSnapshot};
use stat::*;

pub extern crate hdrhistogram;
pub type HdrHistogram = crate::hdrhistogram::Histogram<u64>;

pub mod scheduler;
pub mod snapshot;
pub mod stat;

/// Manages underlying named [`stat`] instances.
///
/// ## Instances
///
/// Most use-cases will make use of the global instance obtained using the [`Stats::global`] function.
/// However, it is also possible to create additional [`Stats`] instances on demand with [`Stats::new`].
///
/// ## Stat Lifecycle
///
/// - If a [`stat`] with the given name did not already exist, it will be created and returned.
/// - If a [`stat`] with the same name already existed, the shared instance will be returned via a cloned `Arc`.
/// - The [`stat`] will be tracked until all references to the `Arc` are dropped and [`Stats::evict`] is called.
/// - Stats are managed by name _per stat type_, which means stats of different types and the same name will be tracked as different stats.
///
/// ## Reset on Flush
///
/// Each individual stat maintains a `reset_on_flush` flag, which determines if its state should be reset on calls to flush.
/// This flag may be set using a `set_reset_on_flush` function provided by each individual [`stat`] type.
///
/// By default, stats are not reset when flush is called, but this default can be overwritten using the `set_default_reset_on_flush_for_*` functions.
/// Note that it is *not* appropriate for common libraries to change the behavior of defaults on the [`Stats::global`] instance.
/// That decision should be left up to the final binary application.
#[derive(Debug)]
pub struct Stats {
    counters: HashIndex<String, Arc<Counter>>,
    gauges_f64: HashIndex<String, Arc<GaugeF64>>,
    gauges_i64: HashIndex<String, Arc<GaugeI64>>,
    histograms: HashIndex<String, Arc<Histogram>>,
    sets: HashIndex<String, Arc<Set>>,
    default_reset_on_flush_for_counters: AtomicBool,
    default_reset_on_flush_for_gauges: AtomicBool,
    default_reset_on_flush_for_histograms: AtomicBool,
    default_reset_on_flush_for_sets: AtomicBool,
}
impl Stats {
    /// Get the global [`Stats`] instance, which is lazily evaluated with a `OnceCell`.
    pub fn global() -> &'static Arc<Stats> {
        static INSTANCE: OnceCell<Arc<Stats>> = OnceCell::new();
        INSTANCE.get_or_init(|| Arc::new(Stats::new()))
    }

    /// Creat a new [`Stats`] instance, which is to be managed by the user
    pub fn new() -> Self {
        Self {
            counters: HashIndex::new(),
            gauges_f64: HashIndex::new(),
            gauges_i64: HashIndex::new(),
            histograms: HashIndex::new(),
            sets: HashIndex::new(),
            default_reset_on_flush_for_counters: AtomicBool::new(false),
            default_reset_on_flush_for_gauges: AtomicBool::new(false),
            default_reset_on_flush_for_histograms: AtomicBool::new(false),
            default_reset_on_flush_for_sets: AtomicBool::new(false),
        }
    }

    /// Create a named [`Counter`] statistic, which is tracked using an underlying `AtomicU64`.
    pub fn counter<'a, N: Into<String>>(&self, name: N) -> Arc<Counter> {
        Arc::clone(
            &self
                .counters
                .entry(name.into())
                .or_insert_with(|| {
                    Arc::new(Counter::new(
                        self.default_reset_on_flush_for_counters
                            .load(Ordering::Relaxed),
                    ))
                })
                .get(),
        )
    }

    /// Create a named [`GaugeF64`] statistic, which is tracked using an underlying `AtomicF64`.
    pub fn gauge_f64<'a, N: Into<String>>(&self, name: N) -> Arc<GaugeF64> {
        Arc::clone(
            &self
                .gauges_f64
                .entry(name.into())
                .or_insert_with(|| {
                    Arc::new(GaugeF64::new(
                        self.default_reset_on_flush_for_gauges
                            .load(Ordering::Relaxed),
                    ))
                })
                .get(),
        )
    }

    /// Create a named [`GaugeI64`] statistic, which is tracked using an underlying `AtomicI64`.
    pub fn gauge_i64<'a, N: Into<String>>(&self, name: N) -> Arc<GaugeI64> {
        Arc::clone(
            &self
                .gauges_i64
                .entry(name.into())
                .or_insert_with(|| {
                    Arc::new(GaugeI64::new(
                        self.default_reset_on_flush_for_gauges
                            .load(Ordering::Relaxed),
                    ))
                })
                .get(),
        )
    }

    /// Create a named [`Histogram`] statistic, which is tracked using an underlying [`HdrHistogram`] wrapped in a [`spin`] lock.
    ///
    /// ## Contention
    ///
    /// Note that since the current implementation utilizes an internal spin lock, contention should be avoided to the extent possible.
    /// Given this implementation detail, it is currently best practice for only one thread to write to an [`Arc`] of a [`Histogram`] instance.
    /// This will ensure the only time there ever may be spinlock contention is when a snapshot is being generated.
    pub fn histogram<'a, N: Into<String>>(&self, name: N) -> Arc<Histogram> {
        Arc::clone(
            &self
                .histograms
                .entry(name.into())
                .or_insert_with(|| {
                    Arc::new(Histogram::new(
                        self.default_reset_on_flush_for_histograms
                            .load(Ordering::Relaxed),
                    ))
                })
                .get(),
        )
    }

    /// Create a named [`Set`] statistic, which is tracked using an underlying [`scc::HashSet`].
    pub fn set<'a, N: Into<String>>(&self, name: N) -> Arc<Set> {
        Arc::clone(
            &self
                .sets
                .entry(name.into())
                .or_insert_with(|| {
                    Arc::new(Set::new(
                        self.default_reset_on_flush_for_sets.load(Ordering::Relaxed),
                    ))
                })
                .get(),
        )
    }

    /// Flush the current state and generate a [`StatsSnapshot`] of the current state of all managed statistics.
    ///
    /// It is up to the user to call this function at a regular interval to generate snapshots and reset stats.
    /// By default, a [`Stats`] instance simply manages shared underlying [`stat`] instances, and does not automatically flush or generate any snapshots.
    pub fn flush(&self, params: &FlushParams) -> StatsSnapshot {
        let mut snapshot = StatsSnapshot::default();
        let guard = Guard::new();
        self.counters.iter(&guard).for_each(|(k, v)| {
            snapshot.counters.insert(k.clone(), v.flush());
        });
        self.gauges_f64.iter(&guard).for_each(|(k, v)| {
            snapshot.gauges_f64.insert(k.clone(), v.flush());
        });
        self.gauges_i64.iter(&guard).for_each(|(k, v)| {
            snapshot.gauges_i64.insert(k.clone(), v.flush());
        });
        self.histograms.iter(&guard).for_each(|(k, v)| {
            snapshot
                .histograms
                .insert(k.clone(), v.flush(&params.histogram_percentiles));
        });
        self.sets.iter(&guard).for_each(|(k, v)| {
            snapshot.sets.insert(k.clone(), v.flush());
        });
        snapshot
    }

    /// Attempt to evict any [`stat`] instances whos references have all been dropped.
    ///
    /// It is up to the user to call this function at a regular interval to evict dropped stats.
    pub fn evict(&self) {
        self.counters.retain(|_, v| Arc::strong_count(v) > 1);
        self.gauges_f64.retain(|_, v| Arc::strong_count(v) > 1);
        self.gauges_i64.retain(|_, v| Arc::strong_count(v) > 1);
        self.histograms.retain(|_, v| Arc::strong_count(v) > 1);
        self.sets.retain(|_, v| Arc::strong_count(v) > 1);
    }

    /// Set the `reset_on_flush` behavior for any newly created [`Counter`]
    pub fn set_default_reset_on_flush_for_counters(&self, val: bool) {
        self.default_reset_on_flush_for_counters
            .store(val, Ordering::Relaxed);
    }

    /// Set the `reset_on_flush` behavior for any newly created [`GaugeF64`] or [`GaugeI64`]
    pub fn set_default_reset_on_flush_for_gauges(&self, val: bool) {
        self.default_reset_on_flush_for_gauges
            .store(val, Ordering::Relaxed);
    }

    /// Set the `reset_on_flush` behavior for any newly created [`Histogram`]
    pub fn set_default_reset_on_flush_for_histograms(&self, val: bool) {
        self.default_reset_on_flush_for_histograms
            .store(val, Ordering::Relaxed);
    }

    /// Set the `reset_on_flush` behavior for any newly created [`Set`]
    pub fn set_default_reset_on_flush_for_sets(&self, val: bool) {
        self.default_reset_on_flush_for_sets
            .store(val, Ordering::Relaxed);
    }
}
