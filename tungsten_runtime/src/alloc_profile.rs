//! Allocation profiler for compiled Tungsten programs.
//!
//! Provides per-function attribution of cumulative allocated bytes.
//! When enabled via `--alloc-profile`, codegen emits a function-entry hook
//! that sets the current function name. The profiling malloc wrapper records
//! bytes allocated under that name. At program exit, a sorted report is printed.
//!
//! See ADR 7.5.26b for design rationale.

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_void};
use core::ptr;

/// Maximum number of distinct functions we track.
/// Uses a fixed array to avoid allocating (which would recurse into the profiler).
const MAX_FUNCTIONS: usize = 4096;

/// A single profiler entry: function name pointer + cumulative bytes.
struct ProfileEntry {
    name: *const c_char,
    bytes: u64,
    count: u64,
}

/// Wrapper for global profiler state.
///
/// Tungsten programs are single-threaded, so no synchronization is needed.
/// `UnsafeCell` avoids the Rust 2024 `static mut` deprecation while making
/// the single-writer invariant explicit.
struct ProfilerCell(UnsafeCell<Profiler>);

// SAFETY: Tungsten programs are single-threaded. No concurrent access.
unsafe impl Sync for ProfilerCell {}

static PROFILER: ProfilerCell = ProfilerCell(UnsafeCell::new(Profiler::new()));

struct Profiler {
    current_fn: *const c_char,
    filter_fn: *const c_char,
    entries: [ProfileEntry; MAX_FUNCTIONS],
    entry_count: usize,
    total_bytes: u64,
    total_count: u64,
}

impl Profiler {
    const fn new() -> Self {
        const EMPTY: ProfileEntry = ProfileEntry {
            name: ptr::null(),
            bytes: 0,
            count: 0,
        };
        Self {
            current_fn: ptr::null(),
            filter_fn: ptr::null(),
            entries: [EMPTY; MAX_FUNCTIONS],
            entry_count: 0,
            total_bytes: 0,
            total_count: 0,
        }
    }

    /// Find or create an entry for the given function name.
    /// Comparison is by pointer equality (all names are string literals in .rodata).
    fn find_or_create(&mut self, name: *const c_char) -> Option<&mut ProfileEntry> {
        // Search existing entries by pointer equality
        for i in 0..self.entry_count {
            if self.entries[i].name == name {
                return Some(&mut self.entries[i]);
            }
        }
        // Create new entry if space available
        if self.entry_count < MAX_FUNCTIONS {
            let idx = self.entry_count;
            self.entries[idx].name = name;
            self.entries[idx].bytes = 0;
            self.entries[idx].count = 0;
            self.entry_count += 1;
            Some(&mut self.entries[idx])
        } else {
            None
        }
    }

    /// Record an allocation of `size` bytes under the current function.
    fn record(&mut self, size: u64) {
        self.total_bytes += size;
        self.total_count += 1;
        if !self.current_fn.is_null() {
            if let Some(entry) = self.find_or_create(self.current_fn) {
                entry.bytes += size;
                entry.count += 1;
            }
        }
    }
}

/// Set the current function name for allocation attribution.
///
/// Called by codegen-emitted hooks at the start of each Tungsten function
/// when `--alloc-profile` is enabled.
///
/// # Safety
///
/// `name` must be a valid, null-terminated C string with static lifetime
/// (codegen emits these as global string constants).
#[no_mangle]
pub unsafe extern "C" fn __tungsten_alloc_profile_set_fn(name: *const c_char) {
    unsafe {
        (*PROFILER.0.get()).current_fn = name;
    }
}

/// Set a filter so only the named function appears in the report.
///
/// # Safety
///
/// `name` must be a valid, null-terminated C string with static lifetime.
#[no_mangle]
pub unsafe extern "C" fn __tungsten_alloc_profile_set_filter(name: *const c_char) {
    unsafe {
        (*PROFILER.0.get()).filter_fn = name;
    }
}

/// Profiling malloc wrapper. Calls libc malloc and records the allocation.
///
/// # Safety
///
/// Same contract as `malloc(3)`.
#[no_mangle]
pub unsafe extern "C" fn __tungsten_alloc_profile_malloc(size: u64) -> *mut c_void {
    let ptr = unsafe { libc::malloc(size as libc::size_t) };
    if !ptr.is_null() {
        unsafe {
            (*PROFILER.0.get()).record(size);
        }
    }
    ptr
}

/// Print the allocation profile report to stderr.
///
/// Called at the end of `__tungsten_inner_main` when `--alloc-profile` is enabled.
/// If a filter was set via `__tungsten_alloc_profile_set_filter`, only that
/// function is shown in the report.
#[no_mangle]
pub extern "C" fn __tungsten_alloc_profile_report() {
    unsafe {
        let profiler = &*PROFILER.0.get();
        if profiler.total_bytes == 0 {
            eprintln!("\n  Allocation Profile: no allocations recorded.");
            return;
        }

        // Resolve filter name (if set)
        let filter_name: Option<&str> = if profiler.filter_fn.is_null() {
            None
        } else {
            let s = core::ffi::CStr::from_ptr(profiler.filter_fn)
                .to_str()
                .unwrap_or("");
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };

        // Collect indices and sort by bytes descending
        let mut indices: Vec<usize> = (0..profiler.entry_count).collect();
        indices.sort_by(|&a, &b| profiler.entries[b].bytes.cmp(&profiler.entries[a].bytes));

        // Apply filter if set
        if let Some(filter) = filter_name {
            indices.retain(|&idx| {
                let entry = &profiler.entries[idx];
                if entry.name.is_null() {
                    return false;
                }
                let name = core::ffi::CStr::from_ptr(entry.name).to_str().unwrap_or("");
                name == filter
            });
        }

        let shown = indices.len().min(20);
        eprintln!();
        if let Some(filter) = filter_name {
            eprintln!("  Allocation Profile (filtered: {})", filter);
        } else {
            eprintln!("  Allocation Profile (top {} by bytes)", shown);
        }
        eprintln!("  {}", "─".repeat(64));
        eprintln!("  {:40} {:>14} {:>8}", "Function", "Bytes", "%");
        eprintln!("  {}", "─".repeat(64));

        for &idx in indices.iter().take(20) {
            let entry = &profiler.entries[idx];
            let name = if entry.name.is_null() {
                "<unknown>"
            } else {
                core::ffi::CStr::from_ptr(entry.name)
                    .to_str()
                    .unwrap_or("<invalid utf8>")
            };
            let pct = (entry.bytes as f64 / profiler.total_bytes as f64) * 100.0;
            eprintln!(
                "  {:40} {:>14} ({:>5.2}%)",
                name,
                format_bytes(entry.bytes),
                pct
            );
        }

        eprintln!("  {}", "─".repeat(64));
        eprintln!(
            "  {:40} {:>14} ({} calls)",
            "TOTAL",
            format_bytes(profiler.total_bytes),
            format_count(profiler.total_count)
        );
        eprintln!();
    }
}

/// Format a byte count with comma separators.
fn format_bytes(bytes: u64) -> String {
    let s = bytes.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a count with comma separators.
fn format_count(count: u64) -> String {
    format_bytes(count) // Same formatting logic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0");
        assert_eq!(format_bytes(999), "999");
        assert_eq!(format_bytes(1000), "1,000");
        assert_eq!(format_bytes(1_000_000), "1,000,000");
        assert_eq!(format_bytes(1_432_871_424), "1,432,871,424");
    }

    #[test]
    fn test_profiler_record() {
        let mut profiler = Profiler::new();
        let name = c"test_fn".as_ptr();
        profiler.current_fn = name;
        profiler.record(100);
        profiler.record(200);

        assert_eq!(profiler.total_bytes, 300);
        assert_eq!(profiler.total_count, 2);
        assert_eq!(profiler.entry_count, 1);
        assert_eq!(profiler.entries[0].bytes, 300);
        assert_eq!(profiler.entries[0].count, 2);
    }

    #[test]
    fn test_profiler_multiple_functions() {
        let mut profiler = Profiler::new();
        let fn_a = c"fn_a".as_ptr();
        let fn_b = c"fn_b".as_ptr();

        profiler.current_fn = fn_a;
        profiler.record(100);

        profiler.current_fn = fn_b;
        profiler.record(500);

        profiler.current_fn = fn_a;
        profiler.record(200);

        assert_eq!(profiler.total_bytes, 800);
        assert_eq!(profiler.entry_count, 2);
        assert_eq!(profiler.entries[0].bytes, 300); // fn_a
        assert_eq!(profiler.entries[1].bytes, 500); // fn_b
    }

    #[test]
    fn test_profiler_no_current_fn() {
        let mut profiler = Profiler::new();
        profiler.record(100);

        assert_eq!(profiler.total_bytes, 100);
        assert_eq!(profiler.entry_count, 0); // no attribution
    }

    #[test]
    fn test_profiler_overflow_no_panic() {
        let mut profiler = Profiler::new();
        // Create MAX_FUNCTIONS distinct function name pointers.
        // Each c-string literal at a different address counts as distinct.
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
            profiler.record(10);
        }
        assert_eq!(profiler.entry_count, MAX_FUNCTIONS);
        assert_eq!(profiler.total_bytes, (MAX_FUNCTIONS as u64) * 10);

        // One more should not panic — allocation is tracked in total but not per-fn
        let overflow_name = std::ffi::CString::new("fn_overflow").unwrap();
        profiler.current_fn = overflow_name.as_ptr();
        profiler.record(99);
        assert_eq!(profiler.entry_count, MAX_FUNCTIONS); // still at max
        assert_eq!(profiler.total_bytes, (MAX_FUNCTIONS as u64) * 10 + 99);
    }

    #[test]
    fn test_profiler_filter() {
        let mut profiler = Profiler::new();
        let fn_a = c"fn_a".as_ptr();
        let fn_b = c"fn_b".as_ptr();

        profiler.current_fn = fn_a;
        profiler.record(100);
        profiler.current_fn = fn_b;
        profiler.record(500);

        // Setting a filter stores the pointer for report-time filtering
        profiler.filter_fn = fn_a;
        assert!(!profiler.filter_fn.is_null());
    }
}
