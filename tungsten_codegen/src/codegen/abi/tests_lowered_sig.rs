//! Tests for the reframed exact `musttail`-compatibility gate over
//! [`LoweredSignature`] and the minimal canonical slot-attribute policy
//! (ADR 1.7.26e §5, R6/R4).

use super::*;
use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;
use inkwell::AddressSpace;

/// A non-flattenable aggregate to sit behind sret / indirect-param slots.
fn agg(ctx: &Context) -> BasicTypeEnum<'_> {
    let i64_ty = ctx.i64_type();
    let inner = ctx.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    ctx.struct_type(&[inner.into(), i64_ty.into()], false)
        .into()
}

/// Build a canonical Class-P sret signature: `void (ptr sret, ptr buf, ptr env, i64 flat)`.
fn sret_class_p<'ctx>(ctx: &'ctx Context) -> LoweredSignature<'ctx> {
    let ptr: BasicTypeEnum<'ctx> = ctx.ptr_type(AddressSpace::default()).into();
    let i64_ty: BasicTypeEnum<'ctx> = ctx.i64_type().into();
    LoweredSignature {
        call_conv: 0,
        is_var_args: false,
        ret: None, // void — sret style
        slots: vec![
            LoweredSlot::sret(ptr, agg(ctx), 8, 64),
            LoweredSlot::indirect_param(ptr, agg(ctx), 8, 48),
            LoweredSlot::env(ptr),
            LoweredSlot::flat(i64_ty),
        ],
    }
}

/// Build a no-sret Class-P scalar-return signature: `i64 (ptr buf, ptr env)` (R4).
fn scalar_class_p<'ctx>(ctx: &'ctx Context) -> LoweredSignature<'ctx> {
    let ptr: BasicTypeEnum<'ctx> = ctx.ptr_type(AddressSpace::default()).into();
    LoweredSignature {
        call_conv: 0,
        is_var_args: false,
        ret: Some(ctx.i64_type().into()),
        slots: vec![
            LoweredSlot::indirect_param(ptr, agg(ctx), 8, 48),
            LoweredSlot::env(ptr),
        ],
    }
}

// ── AC 3: minimal buffer-slot attributes — `noalias` on both ────────────────

/// ADR 17.7.26e: `noalias` sits on the sret slot (fresh out-buffer, §2.5) AND
/// on the indirect-param slots (single-routed buffers, I1–I5). `byval` sits on
/// neither — it would copy at the C ABI and defeat pointer forwarding.
#[test]
fn sret_and_indirect_param_slots_carry_noalias() {
    let sret = SlotAttrs::sret(8, 64);
    assert!(sret.sret, "sret slot marked sret");
    assert!(
        sret.noalias,
        "sret slot retains noalias (fresh buffer, §2.5)"
    );
    assert!(sret.nonnull);
    assert_eq!(sret.align, 8);
    assert_eq!(sret.dereferenceable, 64);
    assert!(!sret.byval);

    let indirect = SlotAttrs::indirect_param(8, 48);
    assert!(indirect.nonnull, "indirect-param slot is nonnull");
    assert_eq!(indirect.align, 8, "indirect-param carries align");
    assert_eq!(
        indirect.dereferenceable, 48,
        "indirect-param carries dereferenceable"
    );
    assert!(
        indirect.noalias,
        "indirect-param slot carries noalias (ADR 17.7.26e, I1–I5)"
    );
    assert!(
        !indirect.byval,
        "indirect-param slot must NOT carry byval (defeats forwarding)"
    );
    assert!(!indirect.sret);
}

/// ADR 17.7.26e: the descriptor can describe its own slots, so
/// `info codegen indirect-abi` reports the ABI contract from the single source
/// of truth rather than from re-derived guesswork.
#[test]
fn slots_describe_their_role_and_attributes() {
    let ctx = Context::create();
    let described = sret_class_p(&ctx).describe_slots();
    assert_eq!(described.len(), 4, "sret + indirect + env + flat");
    assert_eq!(
        described[0],
        "sret: noalias nonnull align 8 dereferenceable(64)"
    );
    assert_eq!(
        described[1],
        "indirect-param: noalias nonnull align 8 dereferenceable(48)"
    );
    assert_eq!(described[2], "env: (none)", "env carries no ABI attributes");
    assert_eq!(described[3], "flat: (none)");
}

// ── AC 2: exact lowered-signature check — positive cases ────────────────────

#[test]
fn identical_sret_class_p_signatures_are_compatible() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let callee = sret_class_p(&ctx);
    assert_eq!(caller.musttail_compatible(&callee), Ok(()));
}

#[test]
fn identical_scalar_class_p_signatures_are_compatible() {
    // R4: no-sret Class-P shape (scalar return, leading indirect-param run).
    let ctx = Context::create();
    let caller = scalar_class_p(&ctx);
    let callee = scalar_class_p(&ctx);
    assert_eq!(caller.musttail_compatible(&callee), Ok(()));
}

// ── AC 2: exact lowered-signature check — negative cases ─────────────────────

#[test]
fn mismatched_sret_attribute_is_blocker() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    // Flip the sret slot's noalias — an ABI-significant attribute mismatch.
    callee.slots[0].attrs.noalias = false;
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::SlotAttrs { index: 0 })
    );
}

#[test]
fn mismatched_indirect_param_align_is_blocker() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    // Indirect-param slot is at index 1; perturb its align.
    callee.slots[1].attrs.align = 16;
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::SlotAttrs { index: 1 })
    );
}

#[test]
fn varargs_callee_is_blocker() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    callee.is_var_args = true;
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::VarArgs)
    );
}

#[test]
fn calling_convention_mismatch_is_blocker() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    callee.call_conv = 8; // e.g. fastcc
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::CallConv {
            caller: 0,
            callee: 8
        })
    );
}

#[test]
fn return_type_mismatch_is_blocker() {
    let ctx = Context::create();
    let caller = scalar_class_p(&ctx);
    let mut callee = scalar_class_p(&ctx);
    callee.ret = Some(ctx.i32_type().into());
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::ReturnType)
    );
}

#[test]
fn slot_count_mismatch_is_blocker() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    callee.slots.pop();
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::SlotCount {
            caller: 4,
            callee: 3
        })
    );
}

#[test]
fn slot_type_mismatch_is_blocker() {
    let ctx = Context::create();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    // Change the trailing flat scalar's type (index 3).
    callee.slots[3].ty = ctx.i32_type().into();
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::SlotType { index: 3 })
    );
}

#[test]
fn slot_role_mismatch_is_blocker() {
    let ctx = Context::create();
    let ptr: BasicTypeEnum = ctx.ptr_type(AddressSpace::default()).into();
    let caller = sret_class_p(&ctx);
    let mut callee = sret_class_p(&ctx);
    // Relabel the env slot (index 2) as an indirect param — same ptr type, so
    // only the role differs. Give it matching attrs so attrs don't trip first.
    callee.slots[2] = LoweredSlot {
        role: SlotRole::IndirectParam,
        ty: ptr,
        pointee: None,
        attrs: SlotAttrs::none(),
    };
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::SlotRole { index: 2 })
    );
}

// ── Leading-ptr-run invariant: residual by-value aggregate is a hard blocker ─

#[test]
fn residual_by_value_struct_return_is_blocker() {
    let ctx = Context::create();
    let i64_ty = ctx.i64_type();
    let struct_ret = ctx.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    let mut caller = scalar_class_p(&ctx);
    caller.ret = Some(struct_ret.into());
    let callee = scalar_class_p(&ctx);
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::ByValueAggregate {
            at_return: true,
            index: 0
        })
    );
}

#[test]
fn residual_by_value_struct_param_is_blocker() {
    let ctx = Context::create();
    let i64_ty = ctx.i64_type();
    let struct_param = ctx.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    let mut caller = scalar_class_p(&ctx);
    // Replace the env slot's type with a by-value struct (indirect lowering failed).
    caller.slots[1].ty = struct_param.into();
    let callee = scalar_class_p(&ctx);
    assert_eq!(
        caller.musttail_compatible(&callee),
        Err(MusttailIncompat::ByValueAggregate {
            at_return: false,
            index: 1
        })
    );
}
