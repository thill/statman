//! This module provides an optional utility that may be used to regularly schedule calls to [`Stats::flush`].

use std::{
    any::Any,
    cmp::min,
    io,
    ops::Add,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, park_timeout, JoinHandle},
    time::Duration,
};

use chrono::{Timelike, Utc};

use crate::{
    snapshot::{FlushParams, StatsSnapshot},
    Stats,
};

/// A callback that may be registered to a [`FlushScheduler`].
///
/// A default impl for any `FnMut(&StatsSnapshot)` is defined.
pub trait SnapshotCallback {
    fn on_snapshot(&mut self, snapshot: &StatsSnapshot);
}
impl<F: FnMut(&StatsSnapshot)> SnapshotCallback for F {
    fn on_snapshot(&mut self, snapshot: &StatsSnapshot) {
        self(snapshot)
    }
}

/// A confligurable [`Stats::flush`] scheduler, which calls [`Stats::flush`] on a given interval,
/// dispatching a reference of the [`StatsSnapshot`] to all registered [`SnapshotCallback`] instances.
pub struct FlushScheduler {
    stats: Arc<Stats>,
    callbacks: Arc<Mutex<Vec<Box<dyn SnapshotCallback + Send + 'static>>>>,
    runflag: Arc<AtomicBool>,
    max_park_duration: Duration,
    flush_params: FlushParams,
}
impl FlushScheduler {
    /// Create a new [`FlushScheduler`], which may be spawned using [`FlushScheduler::spawn`]
    pub fn new(stats: &Arc<Stats>) -> Self {
        Self {
            stats: Arc::clone(stats),
            callbacks: Arc::new(Mutex::new(Vec::new())),
            runflag: Arc::new(AtomicBool::new(true)),
            max_park_duration: Duration::from_secs(1),
            flush_params: FlushParams::default(),
        }
    }

    /// Set an atomic boolean that will be continuously checked to determine if the underlying thread should keep running.
    pub fn with_runflag(mut self, runflag: Arc<AtomicBool>) -> Self {
        self.runflag = runflag;
        self
    }

    /// Set the maximum amount of time the scheduler will park before checking the `runflag` again.
    pub fn with_max_park_duration(mut self, max_park_duration: Duration) -> Self {
        self.max_park_duration = max_park_duration;
        self
    }

    /// Define the parameters passed to each call to [`Stats::flush`]
    pub fn with_flush_params(mut self, flush_params: FlushParams) -> Self {
        self.flush_params = flush_params;
        self
    }

    /// Register a callback by adding it to the callbacks collection.
    ///
    /// Callbacks can be registered prior to spawning using this function.
    /// To register a callback after spawning, see [`FlushSchedulerHandle::register_callback`]
    pub fn with_register_callback<T: SnapshotCallback + Send + 'static>(self, callback: T) -> Self {
        self.callbacks
            .lock()
            .expect("callbacks lock")
            .push(Box::new(callback));
        self
    }

    /// Spawn a [`std::thread`] that will call [`Stats::flush`] at the given interval,
    /// returning a [`FlushSchedulerHandle`] that can be used to control the thread or register additional callbacks.
    ///
    /// # Interval Alignment
    ///
    /// The "top" of the "current hour" will be used as an anchor for where scheduling should being.
    /// This ensures that flushes are aligned to predictable and regular timestamps.
    pub fn spawn(self, interval: Duration) -> Result<FlushSchedulerHandle, io::Error> {
        if interval.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "duration must be non-zero",
            ));
        }
        let mut next_flush = Utc::now()
            .with_minute(0)
            .expect("chrono with_minute")
            .with_second(0)
            .expect("chrono with_second")
            .with_nanosecond(0)
            .expect("chrono with_nanosecond");
        let now = Utc::now();
        while next_flush < now {
            next_flush = next_flush.add(interval);
        }
        let callbacks = Arc::clone(&self.callbacks);
        let stats = self.stats;
        let max_park_duration = self.max_park_duration;
        let runflag = Arc::clone(&self.runflag);
        let flush_params = self.flush_params;
        let join_handle = thread::Builder::new()
            .name("statman".to_owned())
            .spawn(move || {
                while runflag.load(Ordering::Relaxed) {
                    let now = Utc::now();
                    if now >= next_flush {
                        let snapshot = stats.flush(&flush_params);
                        let mut callbacks = callbacks.lock().expect("callbacks lock");
                        let callbacks = &mut callbacks;
                        for callback in callbacks.iter_mut() {
                            callback.on_snapshot(&snapshot);
                        }
                        while next_flush < now {
                            // this while loop avoids triggering multiple calls to flush in the event the laptop went into hibernation
                            next_flush = next_flush.add(interval);
                        }
                    }
                    let remaining_nanos = (next_flush - Utc::now())
                        .num_nanoseconds()
                        .unwrap_or(max_park_duration.as_nanos() as i64);
                    let remaining_duration = if remaining_nanos < 0 {
                        Duration::ZERO
                    } else {
                        Duration::from_nanos(remaining_nanos as u64)
                    };
                    let park_duration = min(max_park_duration, remaining_duration);
                    if !park_duration.is_zero() {
                        park_timeout(max_park_duration)
                    }
                }
            })?;
        Ok(FlushSchedulerHandle {
            callbacks: self.callbacks,
            join_handle,
            runflag: self.runflag,
        })
    }
}

/// The result of successfully spawning a [`FlushScheduler`].
/// This handle gives the user the ability to control the spawned thread and register additional callbacks.
pub struct FlushSchedulerHandle {
    callbacks: Arc<Mutex<Vec<Box<dyn SnapshotCallback + Send + 'static>>>>,
    join_handle: JoinHandle<()>,
    runflag: Arc<AtomicBool>,
}
impl FlushSchedulerHandle {
    /// Get a reference the the `Arc` of the underlying `runflag` so that it may be integrated directly into a user's shutdown coordination
    pub fn runflag<'a>(&'a self) -> &'a Arc<AtomicBool> {
        &self.runflag
    }

    /// Gracefully stop the [`FlushScheduler`] thread by setting the underlying `runflag` to false.
    pub fn stop(&self) {
        self.runflag.store(false, Ordering::Relaxed);
    }

    /// Register an additional callback.
    pub fn register_callback<T: SnapshotCallback + Send + 'static>(&self, callback: T) {
        self.callbacks
            .lock()
            .expect("callbacks lock")
            .push(Box::new(callback));
    }

    /// Join the underlying [`FlushScheduler`] thread until it has exited.
    pub fn join(self) -> Result<(), Box<(dyn Any + Send + 'static)>> {
        self.join_handle.join()
    }
}
