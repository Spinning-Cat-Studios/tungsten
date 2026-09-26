//! How a lowered signature describes itself (ADR 17.7.26e).
//!
//! Rendering lives beside the descriptor but not inside it: `sig.rs` defines the
//! ABI contract, this file turns it into the text a human reads. `info codegen
//! indirect-abi` consumes these so "does slot *k* carry `noalias`?" is answered
//! from the single source of truth rather than by grepping emitted `.ll`.

use super::sig::{LoweredSignature, LoweredSlot, SlotAttrs, SlotRole};

impl SlotAttrs {
    /// The attributes as they are emitted, in declaration order — the text a
    /// reader would find on the `.ll` parameter (`noalias nonnull align 8
    /// dereferenceable(24)`). `sret(%T)` carries a type and is rendered by
    /// [`LoweredSlot::describe`], which knows the pointee.
    ///
    /// Exists so `info codegen indirect-abi` can answer "what attributes does
    /// slot *k* carry?" from the canonical descriptor, rather than sending a
    /// reader to `compile --emit-llvm` + grep (ADR 17.7.26e retrospective).
    pub(crate) fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.noalias {
            parts.push("noalias".to_string());
        }
        if self.nonnull {
            parts.push("nonnull".to_string());
        }
        if self.align > 0 {
            parts.push(format!("align {}", self.align));
        }
        if self.dereferenceable > 0 {
            parts.push(format!("dereferenceable({})", self.dereferenceable));
        }
        if self.sign_ext {
            parts.push("signext".to_string());
        }
        if self.zero_ext {
            parts.push("zeroext".to_string());
        }
        if self.byval {
            parts.push("byval".to_string());
        }
        if parts.is_empty() {
            "(none)".to_string()
        } else {
            parts.join(" ")
        }
    }
}

impl LoweredSlot<'_> {
    /// `<role>: <attrs>` for one slot, e.g.
    /// `sret(%T): noalias nonnull align 8 dereferenceable(24)`.
    pub(crate) fn describe(&self) -> String {
        let role = match self.role {
            SlotRole::SretReturn => "sret",
            SlotRole::IndirectParam => "indirect-param",
            SlotRole::Env => "env",
            SlotRole::Flat => "flat",
        };
        format!("{role}: {}", self.attrs.describe())
    }
}

impl LoweredSignature<'_> {
    /// Per-slot `<role>: <attrs>` descriptions in slot order — the ABI contract
    /// the declaration, every call site, and the `musttail` self-edge all share.
    pub(crate) fn describe_slots(&self) -> Vec<String> {
        self.slots.iter().map(LoweredSlot::describe).collect()
    }
}
