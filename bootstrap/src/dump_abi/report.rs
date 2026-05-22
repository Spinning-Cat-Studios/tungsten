//! Output formatting for ABI layout reports.

use super::{FunctionAbi, LlcRegisterInfo, ParamAbi, PassingMode, AAPCS64_DIRECT_THRESHOLD};

/// Format and print a full ABI report for one function.
pub(crate) fn format_report(func: &FunctionAbi, llc_info: Option<&LlcRegisterInfo>) {
    // Function header
    let params_str: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            if p.ty.starts_with('%') || p.ty.contains('{') {
                format!("{}: {}", p.name, p.ty)
            } else {
                p.ty.clone()
            }
        })
        .collect();
    println!(
        "{}({}) -> {}",
        func.name,
        params_str.join(", "),
        func.ret.ty
    );

    // Parameters
    if func.params.is_empty() {
        println!("  Parameters: (none)");
    } else {
        println!("  Parameters:");
        for param in &func.params {
            format_param(param, llc_info);
        }
    }

    // Return type
    println!("  Return:");
    format_return(&func.ret, llc_info);

    println!();
}

/// Format a single parameter's ABI info.
fn format_param(param: &ParamAbi, llc_info: Option<&LlcRegisterInfo>) {
    let passing_str = match param.passing {
        PassingMode::Direct => "DIRECT",
        PassingMode::Indirect => "INDIRECT",
    };

    match &param.layout {
        Some(layout) => {
            println!("    {}: {} = {}", param.name, param.ty, layout.type_str);
            for field in &layout.fields {
                println!(
                    "      Field {}: {}  offset={}  size={}  align={}",
                    field.offset / field.align.max(1), // field index approximation
                    field.ty,
                    field.offset,
                    field.size,
                    field.align,
                );
            }
            println!(
                "      Total: {} bytes (padding: {})",
                layout.total_size, layout.padding
            );

            if layout.total_size <= AAPCS64_DIRECT_THRESHOLD {
                println!(
                    "      AAPCS64: ≤ {} bytes → {} (register decomposition)",
                    AAPCS64_DIRECT_THRESHOLD, passing_str
                );
            } else {
                println!(
                    "      AAPCS64: > {} bytes → {} (by pointer)",
                    AAPCS64_DIRECT_THRESHOLD, passing_str
                );
            }
        }
        None => {
            println!("    {}: {} → {}", param.name, param.ty, passing_str);
        }
    }

    // Tier 2: show llc register info if available
    if let Some(info) = llc_info {
        if let Some(reg_line) = find_register_for_param(&param.name, &info.raw_output) {
            println!("      llc: {}", reg_line.trim());
        }
    }
}

/// Format return type ABI info.
fn format_return(ret: &ParamAbi, llc_info: Option<&LlcRegisterInfo>) {
    let passing_str = match ret.passing {
        PassingMode::Direct => "DIRECT",
        PassingMode::Indirect => "INDIRECT",
    };

    match &ret.layout {
        Some(layout) => {
            println!("    {} = {} → {}", ret.ty, layout.type_str, passing_str);
            if layout.total_size > 0 {
                println!(
                    "    Total: {} bytes (padding: {})",
                    layout.total_size, layout.padding
                );
            }
        }
        None => {
            println!("    {} → {} (register)", ret.ty, passing_str);
        }
    }

    if let Some(info) = llc_info {
        if let Some(reg_line) = find_register_for_return(&info.raw_output) {
            println!("    llc: {}", reg_line.trim());
        }
    }
}

/// Search llc debug output for register info about a specific parameter.
fn find_register_for_param(param_name: &str, output: &str) -> Option<String> {
    // Extract the arg index from "argN"
    let idx = param_name.strip_prefix("arg")?.parse::<usize>().ok()?;

    // Look for patterns like "Arg N" or "IncomingArg" in the debug output
    for line in output.lines() {
        let trimmed = line.trim();
        // Common patterns in llc debug output
        if trimmed.contains(&format!("Arg #{idx}"))
            || trimmed.contains(&format!("arg #{idx}"))
            || trimmed.contains(&format!("IncomingArg #{idx}"))
        {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Search llc debug output for return value register info.
fn find_register_for_return(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("ret ") || trimmed.contains("return ") {
            return Some(trimmed.to_string());
        }
    }
    None
}
