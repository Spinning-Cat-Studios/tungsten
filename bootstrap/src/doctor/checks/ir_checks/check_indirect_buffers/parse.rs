//! Text-level LLVM function parsing for the indirect-buffer audit.

use crate::doctor::checks::ir_checks::textparse::{split_ir_functions, split_top_level_commas};

use super::track::trailing_ssa_name;

/// One parsed LLVM function: name, header, the SSA names of its **buffer**
/// parameters, and body.
pub(super) struct Func<'a> {
    pub(super) name: String,
    pub(super) header: &'a str,
    /// Parameters carrying a canonical buffer-slot attribute (`sret(…)` /
    /// `dereferenceable(…)`, ADR 1.7.26e §2.1 P6) — the sret out-buffer and the
    /// Class-P indirect-param buffers, and nothing else.
    pub(super) buffer_params: Vec<String>,
    pub(super) body: Vec<&'a str>,
}

impl Func<'_> {
    /// True when the header carries an indirect aggregate / sret buffer
    /// parameter — the signal that the audit is EXPECTED to track at least one
    /// buffer for this function.
    pub(super) fn has_buffer_param(&self) -> bool {
        !self.buffer_params.is_empty()
    }
}

/// Split a module into functions, capturing the `define` header + body lines.
pub(super) fn split_functions(text: &str) -> Vec<Func<'_>> {
    split_ir_functions(text)
        .into_iter()
        .map(|f| Func {
            buffer_params: parse_buffer_params(f.header),
            name: f.name,
            header: f.header,
            body: f.body,
        })
        .collect()
}

/// SSA names of the **buffer** parameters in a `define` header: the `ptr` params
/// carrying `sret(…)` or `dereferenceable(…)`.
///
/// Deliberately *not* every `ptr` param. A flat recursive-ADT (`Mu`) argument is
/// also lowered to a bare `ptr`, but it is an ordinary by-value argument with no
/// buffer contract: handing it to a helper is not an escape, and treating it as
/// one made the R4 audit report 32 phantom escapes over the first full
/// self-compile corpus it was ever pointed at (ADR 17.7.26e P2).
fn parse_buffer_params(def: &str) -> Vec<String> {
    let (Some(open), Some(close)) = (def.find('('), def.rfind(')')) else {
        return Vec::new();
    };
    split_top_level_commas(&def[open + 1..close])
        .into_iter()
        .filter_map(|part| {
            let p = part.trim();
            let is_buffer =
                p.starts_with("ptr") && (p.contains("sret(") || p.contains("dereferenceable("));
            is_buffer.then(|| trailing_ssa_name(p)).flatten()
        })
        .collect()
}
