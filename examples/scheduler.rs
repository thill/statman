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

fn main() {
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
}
