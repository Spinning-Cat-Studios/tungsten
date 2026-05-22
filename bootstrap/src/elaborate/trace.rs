//! Tracing utilities for the elaborator (--trace-types support).
//!
//! Provides targeted tracing of type elaboration for a specific definition,
//! including semantic provenance annotations from ADR 13.4.26c §5.

use tungsten_core::Type;

use super::Elaborator;

impl<'a> Elaborator<'a> {
    /// Set the trace target for --trace-types (ADR 13.4.26c §5).
    pub fn set_trace_target(&mut self, target: Option<String>) {
        self.trace_target = target;
    }

    /// Check if tracing is active for the current definition.
    pub(crate) fn should_trace(&self) -> bool {
        if let Some(ref target) = self.trace_target {
            if let Some(ref current) = self.current_def_name {
                return current == target;
            }
        }
        false
    }

    /// Emit a trace message if tracing is active.
    pub(crate) fn trace(&self, label: &str, message: &str) {
        if self.should_trace() {
            let def = self.current_def_name.as_deref().unwrap_or("<unknown>");
            eprintln!("[trace] {}: {}", def, label);
            for line in message.lines() {
                eprintln!("  {}", line);
            }
        }
    }

    /// Format a type with semantic annotation from provenance if available.
    ///
    /// For `μα_List. (Unit + (String × α_List))` with provenance, returns
    /// `"μα_List. (Unit + (String × α_List))  (semantic: List<String>)"`.
    pub(crate) fn format_type_with_provenance(&self, ty: &Type) -> String {
        let structural = format!("{}", ty);
        if let Type::Mu(binder, _) = ty {
            if let Some(origin) = self.type_provenance.mu_origins.get(binder) {
                let semantic = if origin.type_args.is_empty() {
                    origin.adt_name.clone()
                } else {
                    let args: Vec<String> =
                        origin.type_args.iter().map(|a| format!("{}", a)).collect();
                    format!("{}<{}>", origin.adt_name, args.join(", "))
                };
                return format!("{}  (semantic: {})", structural, semantic);
            }
        }
        structural
    }

    // ─── Encoding trace (--trace-encoding) ──────────────────────────

    /// Set the trace target for --trace-encoding (ADR 18.4.26h §3).
    pub fn set_trace_encoding(&mut self, target: Option<String>) {
        self.trace_encoding = target;
    }

    /// Check if encoding tracing is active for a given type name.
    pub(crate) fn should_trace_encoding(&self, type_name: &str) -> bool {
        match self.trace_encoding {
            Some(ref target) if target.is_empty() => true, // trace all
            Some(ref target) => target == type_name,
            None => false,
        }
    }

    /// Emit an encoding trace message to stderr.
    pub(crate) fn trace_encoding(&self, tag: &str, message: &str) {
        if self.trace_encoding.is_some() {
            eprintln!("[{}] {}", tag, message);
        }
    }

    // ─── Normalization trace (--trace-normalization) ────────────────

    /// Set the trace target for --trace-normalization (ADR 20.4.26c).
    pub fn set_trace_normalization(&mut self, target: Option<String>) {
        self.trace_normalization = target;
    }

    /// Check if normalization tracing is active for a given type name.
    pub(crate) fn should_trace_normalization(&self, type_name: &str) -> bool {
        match self.trace_normalization {
            Some(ref target) if target.is_empty() => true,
            Some(ref target) => target == type_name,
            None => false,
        }
    }

    /// Emit a normalization trace message to stderr.
    pub(crate) fn trace_normalization(&self, message: &str) {
        if self.trace_normalization.is_some() {
            eprintln!("[norm] {}", message);
        }
    }
}
