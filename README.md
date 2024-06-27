# statman

statman: A global manager of atomic statistics

## Stats Manager

Individual statistics are managed by a [`Stats`] instance.
Most use-cases will utilize the global instance by calling [`Stats::global`],
however multiple [`Stats`] managers may be instantiated using [`Stats::new()`].

Individual stats are managed by name.
Stats of the same name and type will update the same underyling value.

## Stat impls

All stats are produced in an [`Arc`] so they may be written to by multiple thread.
- `Counter`: A counter backed by an [`std::sync::atomic::AtomicU64`]
- `GaugeF64`: A gauge backed by an [`atomic_float::AtomicF64`]
- `GaugeI64`: A gauge backed by an [`std::sync::atomic::AtomicI64`]
- `Histogram`: An [`hdrhistogram`] encapsulated with a [`spin`] lock.
- `Set`: A set of strings backed by an [`scc::HashSet`]

## Scheduler

[`Stats`] is a manager of named stats and does not do any scheduling by default.
Users may elect to schedule their own calls to [`Stats::flush`] or use the [`scheduler`] module.

## Resetting Stats

By default, stats do not reset between calls to flush.
This behavior can be overwritten for individual stats using `set_reset_on_flush`.
It can be overwritten globally using [`Stats`] `set_default_reset_on_flush_for_*` functions.

## Example

```rust
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};
use statman::{scheduler::FlushScheduler, snapshot::StatsSnapshot, Stats};
// signal handling for a graceful exit
let runflag = Arc::new(AtomicBool::new(true));
{
    let runflag = Arc::clone(&runflag);
    ctrlc::set_handler(move || {
        runflag.store(false, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl-C handler");
}

// create and spawn a scheduler that will print stats every 10 minutes
FlushScheduler::new(Stats::global())
    .with_register_callback(|snapshot: &StatsSnapshot| {
        println!(
            "{} - {:?}",
            DateTime::<Utc>::from(SystemTime::now()).format("%T"),
            snapshot
        )
    })
    .with_runflag(Arc::clone(&runflag))
    .spawn(Duration::from_secs(10))
    .unwrap();

// create a histogram stat that resets on every call to flush
let hist = Stats::global().histogram("myhist");
hist.set_reset_on_flush(true);

// record an ever increasing value to the histogram
let mut next_record = 0;
while runflag.load(Ordering::Relaxed) {
    hist.record(next_record);
    next_record += 1;
    thread::sleep(Duration::from_millis(10));
}
```

Output:
```
22:11:00 - StatsSnapshot { counters: {}, gauges_f64: {}, gauges_i64: {}, histograms: {"myhist": HistogramSnapshot { count: 597, percentiles: [(50.0, 298), (90.0, 537), (99.9, 596), (99.99, 596), (100.0, 596)] }}, sets: {} }
22:11:10 - StatsSnapshot { counters: {}, gauges_f64: {}, gauges_i64: {}, histograms: {"myhist": HistogramSnapshot { count: 994, percentiles: [(50.0, 1093), (90.0, 1491), (99.9, 1590), (99.99, 1590), (100.0, 1590)] }}, sets: {} }
22:11:20 - StatsSnapshot { counters: {}, gauges_f64: {}, gauges_i64: {}, histograms: {"myhist": HistogramSnapshot { count: 994, percentiles: [(50.0, 2087), (90.0, 2485), (99.9, 2585), (99.99, 2585), (100.0, 2585)] }}, sets: {} }
```

License: MIT OR Apache-2.0
