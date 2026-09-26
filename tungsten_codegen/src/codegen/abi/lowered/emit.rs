//! Emission-side companions to [`LoweredSignature`] (ADR 1.7.26e P2/P6, R6):
//! construction from LLVM function types and attachment of the canonical slot
//! attributes (§2.1) to function declarations and call sites.
//!
//! The descriptor is the single source of truth: `declare_decomposed_entry`
//! builds one per Class-P/decomposed entry, derives the LLVM function type from
//! it ([`LoweredSignature::fn_type`]), attaches its attributes to the declared
//! function, and every call site (the `$direct` shim, the self-tail edge)
//! attaches the same attributes — so caller and callee cannot drift.

use super::sig::{LoweredSignature, LoweredSlot, SlotAttrs};
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::context::Context;
use inkwell::types::{AnyType, BasicMetadataTypeEnum, BasicType, FunctionType};
use inkwell::values::{CallSiteValue, FunctionValue};

impl<'ctx> LoweredSignature<'ctx> {
    /// Describe a plain by-value function type as a lowered signature: param 0
    /// is the closure env, all other params are flat by-value args, and the
    /// return stays by value. This is the descriptor for a function that has
    /// **no** indirect lowering — used to run the exact `musttail` gate on
    /// ordinary `$direct` self-edges and closure applications. A by-value
    /// struct anywhere surfaces as [`MusttailIncompat::ByValueAggregate`]
    /// through the leading-`ptr`-run invariant.
    ///
    /// [`MusttailIncompat::ByValueAggregate`]: super::sig::MusttailIncompat::ByValueAggregate
    pub(crate) fn from_flat_fn_type(fn_type: FunctionType<'ctx>) -> Self {
        let slots = fn_type
            .get_param_types()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    LoweredSlot::env(*p)
                } else {
                    LoweredSlot::flat(*p)
                }
            })
            .collect();
        Self {
            call_conv: 0,
            is_var_args: fn_type.is_var_arg(),
            ret: fn_type.get_return_type(),
            slots,
        }
    }

    /// Derive the LLVM function type from the descriptor (R6: declarations are
    /// emitted *from* the signature, never hand-assembled beside it).
    pub(crate) fn fn_type(&self, ctx: &'ctx Context) -> FunctionType<'ctx> {
        let params: Vec<BasicMetadataTypeEnum<'ctx>> =
            self.slots.iter().map(|s| s.ty.into()).collect();
        match self.ret {
            Some(r) => r.fn_type(&params, self.is_var_args),
            None => ctx.void_type().fn_type(&params, self.is_var_args),
        }
    }

    /// Attach the canonical slot attributes to a declared function.
    pub(crate) fn attach_to_function(&self, ctx: &'ctx Context, f: FunctionValue<'ctx>) {
        for (i, slot) in self.slots.iter().enumerate() {
            for attr in slot_attributes(ctx, slot) {
                f.add_attribute(AttributeLoc::Param(i as u32), attr);
            }
        }
    }

    /// Attach the same canonical slot attributes to a call site. `musttail`
    /// (and, for `sret`, ABI lowering on every target) requires call-site and
    /// callee attributes to match exactly.
    pub(crate) fn attach_to_call_site(&self, ctx: &'ctx Context, cs: CallSiteValue<'ctx>) {
        for (i, slot) in self.slots.iter().enumerate() {
            for attr in slot_attributes(ctx, slot) {
                cs.add_attribute(AttributeLoc::Param(i as u32), attr);
            }
        }
    }
}

/// Build the LLVM attribute list for one lowered slot from its [`SlotAttrs`]
/// (§2.1 canonical policy: `sret` slot = `sret(%T) noalias nonnull align
/// dereferenceable`; indirect-param slot = `noalias nonnull align
/// dereferenceable`, **no** `byval` — R6 as amended by ADR 17.7.26e).
fn slot_attributes<'ctx>(ctx: &'ctx Context, slot: &LoweredSlot<'ctx>) -> Vec<Attribute> {
    let a: &SlotAttrs = &slot.attrs;
    let mut attrs = Vec::new();
    let type_attr = |name: &str| {
        slot.pointee.map(|p| {
            ctx.create_type_attribute(
                Attribute::get_named_enum_kind_id(name),
                p.as_any_type_enum(),
            )
        })
    };
    let enum_attr = |name: &str, val: u64| {
        ctx.create_enum_attribute(Attribute::get_named_enum_kind_id(name), val)
    };

    if a.sret {
        attrs.extend(type_attr("sret"));
    }
    if a.byval {
        attrs.extend(type_attr("byval"));
    }
    if a.noalias {
        attrs.push(enum_attr("noalias", 0));
    }
    if a.nonnull {
        attrs.push(enum_attr("nonnull", 0));
    }
    if a.align > 0 {
        attrs.push(enum_attr("align", u64::from(a.align)));
    }
    if a.dereferenceable > 0 {
        attrs.push(enum_attr("dereferenceable", a.dereferenceable));
    }
    if a.sign_ext {
        attrs.push(enum_attr("signext", 0));
    }
    if a.zero_ext {
        attrs.push(enum_attr("zeroext", 0));
    }
    attrs
}
