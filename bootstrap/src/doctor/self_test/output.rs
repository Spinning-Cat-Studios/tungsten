//! Output formatting for self-test results (text and JSON).

use super::TestResult;

/// Print a single test result in human-readable text format.
pub(super) fn print_test_result_text(result: &TestResult) {
    for phase in &result.phases {
        let status = if phase.passed { "PASS" } else { "FAIL" };
        eprintln!(
            "  {} {:<8} {} ({}ms)",
            status, phase.phase, result.name, phase.duration_ms,
        );
        if !phase.passed && !phase.detail.is_empty() {
            for line in phase.detail.lines() {
                eprintln!("    {}", line);
            }
        }
    }
}

/// Print self-test results as JSON (per-phase structure per ADR 16.4.26b §2).
pub(super) fn print_json_results(results: &[TestResult], tier: &str, total_ms: u64) {
    let programs_passed = results.iter().filter(|r| r.passed()).count();
    let programs_failed = results.iter().filter(|r| !r.passed()).count();
    let total_phases: usize = results.iter().map(|r| r.phase_count()).sum();
    let passed_phases: usize = results.iter().map(|r| r.passed_phase_count()).sum();

    println!("{{");
    println!("  \"command\": \"doctor self-test\",");
    println!("  \"tier\": \"{tier}\",");
    println!("  \"results\": [");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 < results.len() { "," } else { "" };
        println!("    {{");
        println!("      \"program\": \"{}\",", r.file);
        println!("      \"phases\": {{");
        for (j, p) in r.phases.iter().enumerate() {
            let pcomma = if j + 1 < r.phases.len() { "," } else { "" };
            let status = if p.passed { "pass" } else { "fail" };
            let escaped_detail = p
                .detail
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            println!(
                "        \"{}\": {{ \"status\": \"{}\", \"duration_ms\": {}, \"detail\": \"{}\" }}{}",
                p.phase, status, p.duration_ms, escaped_detail, pcomma,
            );
        }
        println!("      }}");
        println!("    }}{comma}");
    }
    println!("  ],");
    println!("  \"summary\": {{");
    println!("    \"programs_passed\": {programs_passed},");
    println!("    \"programs_failed\": {programs_failed},");
    println!("    \"phases_passed\": {passed_phases},");
    println!("    \"phases_total\": {total_phases},");
    println!("    \"total_ms\": {total_ms}");
    println!("  }}");
    println!("}}");
}
