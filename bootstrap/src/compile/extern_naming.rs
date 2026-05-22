//! Extern symbol naming for `.tg` definitions that wrap C extern calls.
//!
//! When a definition is a pure extern wrapper (nested lambdas around ExternCall),
//! it gets a special `__wrap_` LLVM name to avoid collisions with the C symbol.

use tungsten_core::Term;

/// Check if a term is a pure extern wrapper (lambdas wrapping ExternCall).
pub(crate) fn get_extern_symbol(term: &Term) -> Option<String> {
    match term {
        Term::ExternCall(symbol, _) => Some(symbol.clone()),
        Term::Lambda(_, _, body) => get_extern_symbol(body),
        _ => None,
    }
}

/// Compute the extern wrapper LLVM name for a term, if applicable.
///
/// If the term is an extern wrapper (lambdas wrapping ExternCall with a `__c_` prefix),
/// returns `Some((original_llvm_name, __wrap_name))`. Otherwise returns `None`.
///
/// This centralizes the extern name conversion pattern that was previously duplicated
/// across single-module and per-module codegen paths.
pub(crate) fn extern_wrap_name(def_name: &str, term: &Term) -> Option<(String, String)> {
    let extern_symbol = get_extern_symbol(term)?;
    let raw = extern_symbol.strip_prefix("__c_").unwrap_or(&extern_symbol);
    let wrap_name = format!("__wrap_{}", raw);
    let original = super::def_llvm_name(def_name);
    Some((original, wrap_name))
}
