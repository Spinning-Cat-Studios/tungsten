use super::*;

#[test]
fn test_format_bytes() {
    assert_eq!(report::format_bytes(0), "0");
    assert_eq!(report::format_bytes(999), "999");
    assert_eq!(report::format_bytes(1000), "1,000");
    assert_eq!(report::format_bytes(1_000_000), "1,000,000");
    assert_eq!(report::format_bytes(1_432_871_424), "1,432,871,424");
}

#[test]
fn test_profiler_record() {
    let mut profiler = Profiler::new();
    let name = c"test_fn".as_ptr();
    profiler.current_fn = name;
    profiler.record(100, CLASS_MU);
    profiler.record(200, CLASS_MU);

    assert_eq!(profiler.total_bytes, 300);
    assert_eq!(profiler.total_count, 2);
    assert_eq!(profiler.entry_count, 1);
    assert_eq!(profiler.entries[0].bytes, 300);
    assert_eq!(profiler.entries[0].count, 2);
    assert!(profiler.active);
}

#[test]
fn test_profiler_multiple_functions() {
    let mut profiler = Profiler::new();
    let fn_a = c"fn_a".as_ptr();
    let fn_b = c"fn_b".as_ptr();

    profiler.current_fn = fn_a;
    profiler.record(100, CLASS_MU);

    profiler.current_fn = fn_b;
    profiler.record(500, CLASS_ENV);

    profiler.current_fn = fn_a;
    profiler.record(200, CLASS_STRING);

    assert_eq!(profiler.total_bytes, 800);
    assert_eq!(profiler.entry_count, 2);
    assert_eq!(profiler.entries[0].bytes, 300); // fn_a
    assert_eq!(profiler.entries[1].bytes, 500); // fn_b
}

#[test]
fn test_profiler_class_accounting() {
    let mut profiler = Profiler::new();
    let name = c"fn_classes".as_ptr();
    profiler.current_fn = name;

    profiler.record(100, CLASS_MU);
    profiler.record(200, CLASS_ENV);
    profiler.record(300, CLASS_REF);
    profiler.record(400, CLASS_STRING);
    profiler.record(500, CLASS_OTHER);

    assert_eq!(profiler.class_bytes[CLASS_MU as usize], 100);
    assert_eq!(profiler.class_bytes[CLASS_ENV as usize], 200);
    assert_eq!(profiler.class_bytes[CLASS_REF as usize], 300);
    assert_eq!(profiler.class_bytes[CLASS_STRING as usize], 400);
    assert_eq!(profiler.class_bytes[CLASS_OTHER as usize], 500);
    assert_eq!(profiler.class_count[CLASS_MU as usize], 1);

    // Per-function per-class attribution
    assert_eq!(
        profiler.entries[0].bytes_by_class[CLASS_STRING as usize],
        400
    );
    assert_eq!(profiler.entries[0].bytes, 1500);
}

#[test]
fn test_profiler_out_of_range_class_clamps_to_other() {
    let mut profiler = Profiler::new();
    profiler.record(64, 999);
    assert_eq!(profiler.class_bytes[CLASS_OTHER as usize], 64);
}

#[test]
fn test_mu_bucket_boundaries() {
    assert_eq!(mu_bucket(1), 0);
    assert_eq!(mu_bucket(16), 0);
    assert_eq!(mu_bucket(17), 1);
    assert_eq!(mu_bucket(32), 1);
    assert_eq!(mu_bucket(64), 2);
    assert_eq!(mu_bucket(128), 3);
    assert_eq!(mu_bucket(256), 4);
    assert_eq!(mu_bucket(257), 5);
    assert_eq!(mu_bucket(1 << 20), 5);
}

#[test]
fn test_mu_histogram_only_counts_mu_class() {
    let mut profiler = Profiler::new();
    profiler.record(24, CLASS_MU);
    profiler.record(24, CLASS_ENV);
    profiler.record(300, CLASS_MU);

    assert_eq!(profiler.mu_size_hist[1], 1); // 24B → ≤32 bucket
    assert_eq!(profiler.mu_size_hist[NUM_MU_BUCKETS - 1], 1); // 300B → overflow
    let total: u64 = profiler.mu_size_hist.iter().sum();
    assert_eq!(total, 2);
}

#[test]
fn test_profiler_no_current_fn() {
    let mut profiler = Profiler::new();
    profiler.record(100, CLASS_MU);

    assert_eq!(profiler.total_bytes, 100);
    assert_eq!(profiler.entry_count, 0); // no attribution
                                         // class accounting still works without a current function
    assert_eq!(profiler.class_bytes[CLASS_MU as usize], 100);
}

#[test]
fn test_profiler_last_entry_fast_path() {
    let mut profiler = Profiler::new();
    let fn_a = c"fn_a".as_ptr();
    let fn_b = c"fn_b".as_ptr();

    profiler.current_fn = fn_a;
    profiler.record(10, CLASS_MU);
    profiler.current_fn = fn_b;
    profiler.record(20, CLASS_MU);
    // Back to fn_a — must find the existing entry, not create a duplicate
    profiler.current_fn = fn_a;
    profiler.record(30, CLASS_MU);

    assert_eq!(profiler.entry_count, 2);
    assert_eq!(profiler.entries[0].bytes, 40);
    assert_eq!(profiler.entries[1].bytes, 20);
}

#[test]
fn test_profiler_overflow_no_panic() {
    let mut profiler = Profiler::new();
    // Create MAX_FUNCTIONS distinct function name pointers.
    let names: Vec<*const c_char> = (0..MAX_FUNCTIONS)
        .map(|i| {
            // Use heap-allocated CStrings to guarantee distinct pointers
            let s = std::ffi::CString::new(format!("fn_{}", i)).unwrap();
            let ptr = s.as_ptr();
            std::mem::forget(s); // leak to keep pointer valid
            ptr
        })
        .collect();

    for &name in &names {
        profiler.current_fn = name;
        profiler.record(10, CLASS_MU);
    }
    assert_eq!(profiler.entry_count, MAX_FUNCTIONS);
    assert_eq!(profiler.total_bytes, (MAX_FUNCTIONS as u64) * 10);

    // One more should not panic — allocation is tracked in total but not per-fn
    let overflow_name = std::ffi::CString::new("fn_overflow").unwrap();
    profiler.current_fn = overflow_name.as_ptr();
    profiler.record(99, CLASS_MU);
    assert_eq!(profiler.entry_count, MAX_FUNCTIONS); // still at max
    assert_eq!(profiler.total_bytes, (MAX_FUNCTIONS as u64) * 10 + 99);
}

#[test]
fn test_profiler_filter() {
    let mut profiler = Profiler::new();
    let fn_a = c"fn_a".as_ptr();
    let fn_b = c"fn_b".as_ptr();

    profiler.current_fn = fn_a;
    profiler.record(100, CLASS_MU);
    profiler.current_fn = fn_b;
    profiler.record(500, CLASS_MU);

    // Setting a filter stores the pointer for report-time filtering
    profiler.filter_fn = fn_a;
    assert!(!profiler.filter_fn.is_null());
}

#[test]
fn test_external_record_inactive_is_noop() {
    // The global profiler starts inactive; external records must be dropped
    // so non-profiled builds (e.g. the L1 bootstrap linking tungsten_core)
    // never accumulate state. Note: uses the global PROFILER — other tests
    // in this file use local Profiler instances so there is no interference.
    alloc_profile_record_external(1234, CLASS_STRING);
    unsafe {
        let profiler = &*PROFILER.0.get();
        assert!(!profiler.active);
        assert_eq!(profiler.total_bytes, 0);
    }
}

#[test]
fn test_interim_dump_threshold_advances() {
    let mut profiler = Profiler::new();
    profiler.dump_interval_initialized = true;
    profiler.dump_interval = 1000;
    profiler.next_dump_bytes = 1000;
    profiler.current_fn = c"fn_dump".as_ptr();

    // Crossing multiple intervals in one jump must advance past total_bytes
    profiler.record(3500, CLASS_MU);
    assert!(profiler.next_dump_bytes > profiler.total_bytes);
    assert_eq!(profiler.next_dump_bytes, 4000);
}

#[test]
fn test_interim_dump_disabled() {
    let mut profiler = Profiler::new();
    profiler.dump_interval_initialized = true;
    profiler.dump_interval = 0;
    profiler.record(10_000, CLASS_MU);
    assert_eq!(profiler.next_dump_bytes, 0);
}

/// ADR 14.9.26b AC 8: the bump tail's exact text — the keys the
/// `selfcompiled-profile` parser is promised, the mode rendered as `off`/`on`,
/// and thousands separators on the byte counts.
#[test]
fn bump_fields_render_mode_and_counts() {
    use crate::alloc::{ArenaStatsOut, MODE_BUMP, MODE_OFF};
    let on = ArenaStatsOut {
        mode: MODE_BUMP,
        chunks: 3,
        reserved: 5_097_152,
        used: 3_025_188,
        high_water: 3_025_188,
    };
    assert_eq!(
        report::bump_fields(&on),
        "bump=on chunks=3 reserved=5,097,152 used=3,025,188 hw=3,025,188"
    );
    let off = ArenaStatsOut {
        mode: MODE_OFF,
        ..ArenaStatsOut::default()
    };
    assert_eq!(
        report::bump_fields(&off),
        "bump=off chunks=0 reserved=0 used=0 hw=0"
    );
    assert_eq!(
        report::bump_summary(&off),
        "arena: bump=off chunks=0 reserved=0 used=0 hw=0 (heap allocations through the runtime symbol only; stack folds excluded)"
    );
}
