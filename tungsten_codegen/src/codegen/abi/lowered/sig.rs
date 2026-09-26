//! Canonical lowered-signature descriptor + exact `musttail`-compatibility check
//! (ADR 1.7.26e, Decisions/R6 — "one canonical lowered-signature descriptor,
//! single source of truth").
//!
//! [`CodeGen::check_musttail_abi_safety`](super::CodeGen::check_musttail_abi_safety)
//! is the legacy *type-level* gate: it inspects a bare `FunctionType` and rejects
//! any by-value struct return/param. It cannot see parameter **attributes**
//! (`sret`/`align`/`dereferenceable`/`nonnull`/`byval`), which live on the
//! `FunctionValue`, not the `FunctionType` — so it cannot enforce the *exact*
//! signature equality `musttail` actually demands.
//!
//! This module is the **reframed gate** (ADR 1.7.26e §5): a [`LoweredSignature`]
//! carries the LLVM function type **and** its ABI-significant attributes, so the
//! caller side and callee side of a `musttail` self-edge can be compared for
//! exact equality via [`LoweredSignature::musttail_compatible`]. It also enforces
//! the *leading-`ptr`-run invariant*: after indirect lowering, no by-value struct
//! may remain at the return or any parameter position.
//!
//! ## Staging (ADR 1.7.26e is landed P0+P1-first)
//!
//! P1 introduces this descriptor and its exact check as pure, unit-tested data.
//! P2 constructs a `LoweredSignature` at every internal-entry declaration, call,
//! and typed apply thunk and feeds both sides here, replacing the type-level
//! `check_musttail_abi_safety` call sites. Until then the two coexist.

use inkwell::types::BasicTypeEnum;

/// The role a lowered slot plays in the unified indirect-slot ABI (§2.1).
///
/// Slot order is fixed and stable: **sret return (if any) → indirect params
/// (source order) → env → flat/scalar args**. The leading `ptr` run is uniform
/// whether or not an sret slot is present (the no-sret Class-P shape, R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotRole {
    /// `ptr sret(%T)` result-out pointer — present iff the return is a by-value
    /// aggregate lowered indirect. Always the *first* slot when present.
    SretReturn,
    /// `ptr` caller-owned buffer for a non-flattenable struct parameter
    /// (read in place, overwritten + forwarded across the self-tail edge).
    IndirectParam,
    /// The closure environment pointer.
    Env,
    /// A flat scalar / pointer / recursive-ADT (`Mu`) argument passed by value.
    Flat,
}

/// ABI-significant attributes on a single lowered slot (§2.1).
///
/// Deliberately **minimal** — only what ABI correctness and `musttail` matching
/// require (R6). `noalias` sits on **both** buffer slot kinds: the sret slot
/// (justified by fresh value-context buffer allocation) and — since ADR
/// 17.7.26e — the indirect-param slots, whose buffers are *single-routed* by
/// construction (invariants I1–I5: fresh distinct entry-block allocas, no shim-
/// or callee-side second route, pairwise-distinct tail-edge forwarding). That
/// obligation is not folklore: `doctor check ir indirect-buffers` re-proves it
/// over every emitted `.ll` corpus. 1.7.26e R6 withheld the attribute precisely
/// because nobody had stated it — an *unproven* `noalias` is a miscompile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SlotAttrs {
    /// `sret(%T)` — result-out pointer.
    pub sret: bool,
    /// `byval(%T)` — C-ABI by-value aggregate copy. **Never** set on an
    /// indirect-param slot (it would copy and defeat pointer forwarding);
    /// tracked so the exact check can reject an accidental `byval`.
    pub byval: bool,
    /// `noalias` — optimizer disjointness contract: memory reached through this
    /// pointer is reached through no other pointer the callee sees. Set on the
    /// sret slot (fresh out-buffer, §2.5) and on the indirect-param slots (ADR
    /// 17.7.26e, invariants I1–I5). Pointers *loaded from* a buffer (an
    /// aggregate field pointing at the heap) are not "based on" the param, so
    /// heap shared between two params does not violate the contract — it covers
    /// the buffer bytes alone.
    pub noalias: bool,
    /// `nonnull` — the slot pointer is never null.
    pub nonnull: bool,
    /// `align(N)` — target-data-layout alignment of the pointee aggregate.
    /// `0` = attribute absent.
    pub align: u32,
    /// `dereferenceable(N)` — `sizeof` the pointee aggregate. `0` = absent.
    pub dereferenceable: u64,
    /// `signext` on a scalar integer slot.
    pub sign_ext: bool,
    /// `zeroext` on a scalar integer slot.
    pub zero_ext: bool,
}

impl SlotAttrs {
    /// No ABI attributes — a plain scalar / pointer / env slot.
    pub(crate) fn none() -> Self {
        Self::default()
    }

    /// Canonical **sret** slot attributes (§2.1):
    /// `ptr sret(%T) align noalias nonnull dereferenceable(sizeof %T)`.
    ///
    /// `noalias` is sound here — the value-context caller allocates a **fresh**
    /// out-buffer (§2.5), so it is provably disjoint from callee-visible memory.
    pub(crate) fn sret(align: u32, size: u64) -> Self {
        Self {
            sret: true,
            noalias: true,
            nonnull: true,
            align,
            dereferenceable: size,
            ..Self::none()
        }
    }

    /// Canonical **indirect-param** slot attributes (§2.1, R6; ADR 17.7.26e):
    /// `ptr noalias nonnull align dereferenceable(sizeof %T)`.
    ///
    /// **No `byval`** (would copy at the C ABI and defeat forwarding).
    ///
    /// `noalias` is sound because the buffer bytes are **single-routed**: the
    /// shim allocates a fresh, per-slot entry-block buffer (I1) it never leaks
    /// (I4); the callee eagerly loads the value at entry and writes back only at
    /// the tail edge, never holding the address (I2/I3); and the self-`musttail`
    /// forwards pairwise-distinct buffers positionally, so every subsequent
    /// activation re-enters with I1 intact (I5). `doctor check ir
    /// indirect-buffers` mechanically re-checks I2–I5 over emitted `.ll`.
    pub(crate) fn indirect_param(align: u32, size: u64) -> Self {
        Self {
            noalias: true,
            nonnull: true,
            align,
            dereferenceable: size,
            ..Self::none()
        }
    }
}

/// One lowered parameter slot: its role, LLVM type, and ABI attributes.
///
/// `pointee` carries the aggregate type behind an sret / indirect-param `ptr`
/// slot — needed to emit the `sret(%T)` type attribute and compared by
/// [`LoweredSignature::musttail_compatible`] (the sret pointee is
/// ABI-significant). `None` for env / flat slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoweredSlot<'ctx> {
    pub role: SlotRole,
    pub ty: BasicTypeEnum<'ctx>,
    pub pointee: Option<BasicTypeEnum<'ctx>>,
    pub attrs: SlotAttrs,
}

impl<'ctx> LoweredSlot<'ctx> {
    /// A `ptr` sret return slot with canonical attributes.
    pub(crate) fn sret(
        ptr_ty: BasicTypeEnum<'ctx>,
        pointee: BasicTypeEnum<'ctx>,
        align: u32,
        size: u64,
    ) -> Self {
        Self {
            role: SlotRole::SretReturn,
            ty: ptr_ty,
            pointee: Some(pointee),
            attrs: SlotAttrs::sret(align, size),
        }
    }

    /// A `ptr` indirect-param buffer slot with canonical attributes.
    pub(crate) fn indirect_param(
        ptr_ty: BasicTypeEnum<'ctx>,
        pointee: BasicTypeEnum<'ctx>,
        align: u32,
        size: u64,
    ) -> Self {
        Self {
            role: SlotRole::IndirectParam,
            ty: ptr_ty,
            pointee: Some(pointee),
            attrs: SlotAttrs::indirect_param(align, size),
        }
    }

    /// The closure environment `ptr` slot (no ABI attributes).
    pub(crate) fn env(ptr_ty: BasicTypeEnum<'ctx>) -> Self {
        Self {
            role: SlotRole::Env,
            ty: ptr_ty,
            pointee: None,
            attrs: SlotAttrs::none(),
        }
    }

    /// A flat scalar / pointer / recursive-ADT argument slot.
    pub(crate) fn flat(ty: BasicTypeEnum<'ctx>) -> Self {
        Self {
            role: SlotRole::Flat,
            ty,
            pointee: None,
            attrs: SlotAttrs::none(),
        }
    }
}

/// The canonical descriptor of a lowered internal-entry signature (R6).
///
/// Holds everything a `musttail` self-edge must agree on: calling convention,
/// varargs flag, return type (`None` ⇒ `void`, i.e. sret-style), and the ordered
/// slots with their types + ABI attributes. Constructed once per (function,
/// specialization) and used to emit *and* to check every internal-entry
/// declaration, definition, direct call, and typed apply thunk (P2+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoweredSignature<'ctx> {
    /// LLVM calling-convention id (0 = C / default). Must match across edges.
    pub call_conv: u32,
    /// Varargs flag. The indirect ABI is always `varargs = false`; a varargs
    /// callee can never be a `musttail` edge.
    pub is_var_args: bool,
    /// Return LLVM type; `None` ⇒ `void` (an sret-style, indirect-return entry).
    pub ret: Option<BasicTypeEnum<'ctx>>,
    /// Ordered slots: sret? → indirect params → env → flats.
    pub slots: Vec<LoweredSlot<'ctx>>,
}

/// A structured reason two lowered signatures are not `musttail`-compatible.
///
/// Surfaces to the coverage/trace reporting as [`ReasonCode::AbiSignatureMismatch`]
/// (or [`ReasonCode::NonFlattenableParam`] / [`ReasonCode::StructReturn`] for a
/// residual by-value aggregate). See [`super::musttail_report::ReasonCode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MusttailIncompat {
    /// Calling conventions differ.
    CallConv { caller: u32, callee: u32 },
    /// Either side is varargs.
    VarArgs,
    /// Return types differ (or one is `void` and the other is not).
    ReturnType,
    /// Slot counts differ.
    SlotCount { caller: usize, callee: usize },
    /// Slot LLVM types differ at `index`.
    SlotType { index: usize },
    /// Slot roles differ at `index`.
    SlotRole { index: usize },
    /// Slot ABI attributes differ at `index`.
    SlotAttrs { index: usize },
    /// A by-value aggregate remains at the return or a parameter position — the
    /// indirect lowering failed and the leading-`ptr`-run invariant is broken.
    /// `at_return` distinguishes the return from a param at `index`.
    ByValueAggregate { at_return: bool, index: usize },
}

impl MusttailIncompat {
    /// Human-readable reason for `--trace-musttail` / SKIP diagnostics.
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::CallConv { caller, callee } => {
                format!("calling convention mismatch ({caller} vs {callee})")
            }
            Self::VarArgs => "varargs (musttail incompatible)".to_string(),
            Self::ReturnType => "return type mismatch".to_string(),
            Self::SlotCount { caller, callee } => {
                format!("arg slot count mismatch ({caller} vs {callee})")
            }
            Self::SlotType { index } => format!("arg slot {index} type mismatch"),
            Self::SlotRole { index } => format!("arg slot {index} role mismatch"),
            Self::SlotAttrs { index } => format!("arg slot {index} ABI-attribute mismatch"),
            Self::ByValueAggregate {
                at_return: true, ..
            } => "struct return (musttail incompatible in LLVM 18)".to_string(),
            Self::ByValueAggregate { index, .. } => {
                format!("by-value struct at slot {index} (musttail incompatible in LLVM 18)")
            }
        }
    }
}

impl<'ctx> LoweredSignature<'ctx> {
    /// Exact `musttail`-compatibility check (ADR 1.7.26e §5, the reframed gate).
    ///
    /// `self` is the caller side of a `musttail` self-edge; `callee` is the
    /// callee side. `musttail` demands **exact** signature equality, so this
    /// compares calling convention, varargs, return type, and each slot's type,
    /// role, and ABI-significant attributes — and enforces the leading-`ptr`-run
    /// invariant (no by-value struct at return or param on either side).
    ///
    /// Returns `Ok(())` when a `musttail` call between the two is legal, or the
    /// first structured [`MusttailIncompat`] otherwise.
    pub(crate) fn musttail_compatible(&self, callee: &Self) -> Result<(), MusttailIncompat> {
        // (1) Leading-ptr-run invariant: no by-value struct may survive on EITHER
        //     side. A struct return/param means indirect lowering did not fire.
        for sig in [self, callee] {
            if let Some(ret) = sig.ret {
                if ret.is_struct_type() {
                    return Err(MusttailIncompat::ByValueAggregate {
                        at_return: true,
                        index: 0,
                    });
                }
            }
            if let Some((i, _)) = sig
                .slots
                .iter()
                .enumerate()
                .find(|(_, s)| s.ty.is_struct_type())
            {
                return Err(MusttailIncompat::ByValueAggregate {
                    at_return: false,
                    index: i,
                });
            }
        }

        // (2) Calling convention, varargs, return type.
        if self.call_conv != callee.call_conv {
            return Err(MusttailIncompat::CallConv {
                caller: self.call_conv,
                callee: callee.call_conv,
            });
        }
        if self.is_var_args || callee.is_var_args {
            return Err(MusttailIncompat::VarArgs);
        }
        if self.ret != callee.ret {
            return Err(MusttailIncompat::ReturnType);
        }

        // (3) Slot-by-slot: count, then type/role/attributes at each index.
        if self.slots.len() != callee.slots.len() {
            return Err(MusttailIncompat::SlotCount {
                caller: self.slots.len(),
                callee: callee.slots.len(),
            });
        }
        for (i, (a, b)) in self.slots.iter().zip(callee.slots.iter()).enumerate() {
            if a.ty != b.ty {
                return Err(MusttailIncompat::SlotType { index: i });
            }
            if a.role != b.role {
                return Err(MusttailIncompat::SlotRole { index: i });
            }
            // Pointee is ABI-significant via `sret(%T)`/`dereferenceable` — a
            // mismatch is an attribute-level incompatibility.
            if a.attrs != b.attrs || a.pointee != b.pointee {
                return Err(MusttailIncompat::SlotAttrs { index: i });
            }
        }
        Ok(())
    }
}
