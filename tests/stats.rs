use statman::{snapshot::FlushParams, Stats};

#[test]
pub fn test_counter() {
    let stats = Stats::new();
    let counter = stats.counter("mycounter");
    counter.increment();
    counter.add(3);
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .counters
            .get("mycounter")
            .unwrap(),
        4
    );

    // reset_on_flush
    counter.set_reset_on_flush(true);
    counter.add(2);
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .counters
            .get("mycounter")
            .unwrap(),
        6 // was false
    );

    counter.increment();
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .counters
            .get("mycounter")
            .unwrap(),
        1 // was true
    );
}

#[test]
pub fn test_gauge_f64() {
    let stats = Stats::new();
    let gauge = stats.gauge_f64("mygauge");
    gauge.set(3.3);
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .gauges_f64
            .get("mygauge")
            .unwrap(),
        3.3
    );

    // reset_on_flush
    gauge.set_reset_on_flush(true);
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .gauges_f64
            .get("mygauge")
            .unwrap(),
        3.3 // was false
    );

    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .gauges_f64
            .get("mygauge")
            .unwrap(),
        0.0 // was true
    );
}

#[test]
pub fn test_gauge_i64() {
    let stats = Stats::new();
    let gauge = stats.gauge_i64("mygauge");
    gauge.set(3);
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .gauges_i64
            .get("mygauge")
            .unwrap(),
        3
    );

    // reset_on_flush
    gauge.set_reset_on_flush(true);
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .gauges_i64
            .get("mygauge")
            .unwrap(),
        3 // was false
    );

    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .gauges_i64
            .get("mygauge")
            .unwrap(),
        0 // was true
    );
}

#[test]
pub fn test_histograms() {
    let stats = Stats::new();
    let histogram = stats.histogram("myhist");
    histogram.record(3);
    histogram.record(4);
    assert_eq!(
        stats
            .flush(&FlushParams::default())
            .histograms
            .get("myhist")
            .unwrap()
            .count,
        2
    );

    // reset_on_flush
    histogram.set_reset_on_flush(true);
    histogram.record(5);
    assert_eq!(
        stats
            .flush(&FlushParams::default())
            .histograms
            .get("myhist")
            .unwrap()
            .count,
        3 // was false
    );

    histogram.record(1);
    assert_eq!(
        stats
            .flush(&FlushParams::default())
            .histograms
            .get("myhist")
            .unwrap()
            .count,
        1 // was true
    );
}

#[test]
pub fn test_sets() {
    let stats = Stats::new();
    let set = stats.set("myset");
    set.insert("str1");
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .sets
            .get("myset")
            .unwrap(),
        vec!["str1".to_owned()]
    );

    // reset_on_flush
    set.set_reset_on_flush(true);
    set.insert("str2");
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .sets
            .get("myset")
            .unwrap(),
        vec!["str1".to_owned(), "str2".to_owned()] // was false
    );

    set.insert("str3");
    assert_eq!(
        *stats
            .flush(&FlushParams::default())
            .sets
            .get("myset")
            .unwrap(),
        vec!["str3".to_owned()] // was true
    );
}

#[test]
pub fn test_evict() {
    let stats = Stats::new();

    let counter = stats.counter("mycounter");
    counter.increment();
    drop(counter); // drop reference

    // before evict
    assert_eq!(stats.flush(&FlushParams::default()).counters.len(), 1);

    // after evict
    stats.evict();
    assert_eq!(stats.flush(&FlushParams::default()).counters.len(), 0);
}
