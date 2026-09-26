//! Class-P ABI-surface tests (ADR 1.7.26e §2.1/§2.3, R6/R8): canonical slot
//! attributes on declarations and call sites, env-ordering for the three slot
//! shapes, per-specialization lowering, shim buffer discipline (entry-block,
//! distinct, lifetime-marked, target-aligned), and a native backend
//! object-emission probe for the attributed musttail edge.

use super::decompose::ParamLowering;
use crate::codegen::abi::SlotRole;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::targets::TargetTriple;
use inkwell::AddressSpace;

/// Build a nested (non-flattenable) struct type `{ptr, {i64,i64}}`.
fn nested_struct(ctx: &Context) -> inkwell::types::StructType<'_> {
    let ptr = ctx.ptr_type(AddressSpace::default());
    let i64_ty = ctx.i64_type();
    let inner = ctx.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    ctx.struct_type(&[ptr.into(), inner.into()], false)
}

fn codegen_aarch64<'ctx>(context: &'ctx Context, name: &str) -> CodeGen<'ctx> {
    let mut cg = CodeGen::new(context, name);
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    cg
}

/// §2.1 canonical attributes on the DECLARATION: the sret slot carries
/// `sret(%T) noalias nonnull align dereferenceable`; the indirect-param slot
/// carries `noalias nonnull align dereferenceable` (ADR 17.7.26e flipped
/// `noalias` on, 1.7.26e R6 had withheld it) and never `byval`.
#[test]
fn test_canonical_attrs_on_mt_declaration() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_attrs");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("at$direct", fn_type, None);
    cg.declare_decomposed_entry("at", fn_type).unwrap().unwrap();

    let ir = cg.module.print_to_string().to_string();
    let decl = ir
        .lines()
        .find(|l| l.starts_with("declare") && l.contains(r#"@"at$direct_mt""#))
        .expect("$direct_mt declaration in IR");

    assert!(
        decl.contains("sret({ i64, ptr })"),
        "sret(%T) on slot 0: {decl}"
    );
    // One sret slot + one indirect-param slot, and `noalias` on each
    // (ADR 17.7.26e) — the count is per-slot, not a blanket "exactly one".
    let sig = cg.direct_calls.lowered_sig("at").expect("sig stored");
    let indirect_params = sig
        .slots
        .iter()
        .filter(|s| s.role == SlotRole::IndirectParam)
        .count();
    assert_eq!(indirect_params, 1, "one non-flattenable struct param");
    assert_eq!(
        decl.matches("noalias").count(),
        1 + indirect_params,
        "noalias on the sret slot AND every indirect-param slot (17.7.26e): {decl}"
    );
    assert_eq!(
        decl.matches("nonnull").count(),
        2,
        "nonnull on sret + indirect slots: {decl}"
    );
    assert_eq!(
        decl.matches("dereferenceable(").count(),
        2,
        "dereferenceable on sret + indirect slots: {decl}"
    );
    assert!(!decl.contains("byval"), "no byval anywhere (R6): {decl}");

    // The stored canonical descriptor agrees with the emitted attributes.
    assert!(sig.slots[0].attrs.noalias && sig.slots[0].attrs.sret);
    assert!(sig.slots[1].attrs.noalias && !sig.slots[1].attrs.byval);
    assert!(sig.slots[1].attrs.nonnull && sig.slots[1].attrs.dereferenceable > 0);
}

/// Env-ordering (R8) for the three slot shapes: **sret + indirect + env**,
/// **indirect-only + env**, and **env-only** (decompose-only) — the env
/// position is computed from the slot layout, never a hardcoded index.
#[test]
fn test_env_ordering_three_slot_shapes() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_env_order");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    let flat2 = context.struct_type(&[i64_type.into(), i64_type.into()], false);

    // Shape A: sret + indirect + env.
    let a_ty = ret_struct.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("shape_a$direct", a_ty, None);
    cg.declare_decomposed_entry("shape_a", a_ty)
        .unwrap()
        .unwrap();
    // Shape B: indirect-only + env (+ trailing flat).
    let b_ty = i64_type.fn_type(&[ptr_type.into(), nested.into(), i64_type.into()], false);
    cg.module.add_function("shape_b$direct", b_ty, None);
    cg.declare_decomposed_entry("shape_b", b_ty)
        .unwrap()
        .unwrap();
    // Shape C: env-only leading run (flattenable struct → decompose, env first).
    let c_ty = i64_type.fn_type(&[ptr_type.into(), flat2.into()], false);
    cg.module.add_function("shape_c$direct", c_ty, None);
    cg.declare_decomposed_entry("shape_c", c_ty)
        .unwrap()
        .unwrap();

    let roles = |name: &str| -> Vec<SlotRole> {
        cg.direct_calls
            .lowered_sig(name)
            .unwrap()
            .slots
            .iter()
            .map(|s| s.role)
            .collect()
    };
    assert_eq!(
        roles("shape_a"),
        vec![SlotRole::SretReturn, SlotRole::IndirectParam, SlotRole::Env],
        "shape A: sret leads, indirect next, env after"
    );
    assert_eq!(
        roles("shape_b"),
        vec![SlotRole::IndirectParam, SlotRole::Env, SlotRole::Flat],
        "shape B: indirect run leads at 0, env after, flats last"
    );
    assert_eq!(
        roles("shape_c"),
        vec![SlotRole::Env, SlotRole::Flat, SlotRole::Flat],
        "shape C: env-only leading run (pre-1.7.26e layout preserved)"
    );

    // The declared LLVM types agree with the slot plans (call lowering reads
    // the same plan — see shim/recurse — so a stale env-first index cannot
    // survive these shapes).
    for (name, n_params) in [("shape_a", 3u32), ("shape_b", 3), ("shape_c", 4)] {
        let mt = cg
            .module
            .get_function(&format!("{name}$direct_mt"))
            .unwrap();
        assert_eq!(
            mt.count_params(),
            if name == "shape_c" {
                n_params - 1
            } else {
                n_params
            },
            "{name}: param count matches slot plan"
        );
    }
}

/// Per-specialization (§2.2): the same generic shape monomorphized twice —
/// non-flattenable in one specialization, scalar in the other — gets indirect
/// internal-entry lowering in the former only; the internal signatures differ.
#[test]
fn test_per_specialization_indirect_lowering() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_per_spec");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);

    // Specialization 1: T = nested struct → indirect.
    let spec1 = i64_type.fn_type(&[ptr_type.into(), nested.into(), i64_type.into()], false);
    cg.module.add_function("f_nested$direct", spec1, None);
    let map1 = cg.declare_decomposed_entry("f_nested", spec1).unwrap();
    assert_eq!(
        map1,
        Some(vec![ParamLowering::Indirect, ParamLowering::Passthrough])
    );
    let sig1 = cg.direct_calls.lowered_sig("f_nested").unwrap();
    assert!(sig1.slots.iter().any(|s| s.role == SlotRole::IndirectParam));

    // Specialization 2: T = i64 → no struct params at all → no $direct_mt.
    let spec2 = i64_type.fn_type(&[ptr_type.into(), i64_type.into(), i64_type.into()], false);
    cg.module.add_function("f_scalar$direct", spec2, None);
    let map2 = cg.declare_decomposed_entry("f_scalar", spec2).unwrap();
    assert_eq!(map2, None, "scalar specialization keeps the plain entry");
    assert!(
        cg.direct_calls.lowered_sig("f_scalar").is_none(),
        "no indirect internal-entry signature for the scalar specialization"
    );
    assert!(cg.module.get_function("f_scalar$direct_mt").is_none());
}

/// Shim buffer discipline (§2.5/§2.6, R8/R10): distinct entry-block buffers per
/// indirect param, none aliasing the sret buffer, each wrapped in
/// `llvm.lifetime.start/end`.
#[test]
fn test_shim_distinct_entry_block_buffers_with_lifetimes() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_shim_bufs");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    // {i64,ptr} @sb$direct(ptr env, nested a, nested b) — sret + 2 indirect.
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), nested.into(), nested.into()], false);
    cg.module.add_function("sb$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("sb", fn_type).unwrap().unwrap();
    let mt_fn = cg.module.get_function("sb$direct_mt").unwrap();
    cg.compile_decompose_shim("sb", "sb$direct_mt", mt_fn, &param_map)
        .unwrap();

    assert!(cg.module.verify().is_ok(), "shim IR must verify");
    let ir = cg.module.print_to_string().to_string();
    let shim: Vec<&str> = ir
        .lines()
        .skip_while(|l| !(l.starts_with("define") && l.contains(r#"@"sb$direct"("#)))
        .take_while(|l| *l != "}")
        .collect();
    let body = shim.join("\n");

    // Three distinct buffers: sret out-buffer + one per indirect param.
    for buf in ["%sret_buf", "%indirect_buf.0", "%indirect_buf.1"] {
        assert!(
            body.contains(&format!("{buf} = alloca")),
            "distinct buffer {buf} allocated:\n{body}"
        );
    }
    // Entry-block allocas (R10): the shim is a single entry block, and every
    // alloca precedes the $direct_mt call.
    let call_pos = body.find(r#"@"sb$direct_mt"("#).expect("shim call present");
    let last_alloca = body.rfind("= alloca").unwrap();
    assert!(
        last_alloca < call_pos,
        "all buffers allocated before the call"
    );
    // Distinct slots at the call: sret, buf a, buf b are different pointers.
    let call_line = body
        .lines()
        .find(|l| l.contains(r#"@"sb$direct_mt"("#))
        .unwrap();
    assert!(call_line.contains("%sret_buf"));
    assert!(call_line.contains("%indirect_buf.0"));
    assert!(call_line.contains("%indirect_buf.1"));
    // Lifetime markers around each buffer (R8).
    assert_eq!(
        body.matches("llvm.lifetime.start").count(),
        3,
        "lifetime.start per buffer:\n{body}"
    );
    assert_eq!(
        body.matches("llvm.lifetime.end").count(),
        3,
        "lifetime.end per buffer:\n{body}"
    );
}

/// Buffer alignment (R8): an aggregate with 16-byte ABI alignment (i128 field)
/// carries `align 16` on its indirect slot, from the target data layout.
#[test]
fn test_indirect_slot_align_from_target_layout() {
    let context = Context::create();
    // Native triple: CodeGen::new wires real TargetData into TypeLowering.
    let mut cg = CodeGen::new(&context, "test_align16");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let i128_type = context.i128_type();
    let inner = context.struct_type(&[i64_type.into(), i64_type.into()], false);
    // {i128, {i64,i64}} — nested (non-flattenable) with 16-byte ABI alignment.
    let big = context.struct_type(&[i128_type.into(), inner.into()], false);
    let fn_type = i64_type.fn_type(&[ptr_type.into(), big.into()], false);
    cg.module.add_function("al$direct", fn_type, None);
    cg.declare_decomposed_entry("al", fn_type).unwrap().unwrap();

    let sig = cg.direct_calls.lowered_sig("al").unwrap();
    assert_eq!(sig.slots[0].role, SlotRole::IndirectParam);
    assert_eq!(
        sig.slots[0].attrs.align, 16,
        "indirect slot align comes from target ABI alignment"
    );
    let ir = cg.module.print_to_string().to_string();
    let decl = ir
        .lines()
        .find(|l| l.starts_with("declare") && l.contains(r#"@"al$direct_mt""#))
        .unwrap();
    assert!(decl.contains("align 16"), "align 16 emitted: {decl}");
}

/// Backend probe (R9 precursor): the attributed musttail self-edge — explicit
/// `sret(%T)` + indirect-param attributes — must survive not just the IR
/// verifier but native object emission (`SelectionDAGISel` was the historical
/// crash site for musttail ABI violations).
#[test]
fn test_backend_emits_object_for_attributed_musttail_sret_edge() {
    use inkwell::targets::{
        CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
    };

    let context = Context::create();
    // Native triple end-to-end (CodeGen::new already set it).
    let mut cg = CodeGen::new(&context, "test_backend_probe");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("bp$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("bp", fn_type).unwrap().unwrap();
    cg.direct_calls.set_decompose_map("bp", param_map.clone());

    // Build the $direct_mt body: eager-load, then the attributed musttail edge.
    let mt_fn = cg.module.get_function("bp$direct_mt").unwrap();
    cg.compilation.current_fn = Some(mt_fn);
    let entry = context.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);
    let old = cg
        .builder
        .build_load(
            nested,
            mt_fn.get_nth_param(1).unwrap().into_pointer_value(),
            "old",
        )
        .unwrap();
    let emitted = cg.try_emit_decomposed_musttail("bp", &[old]).unwrap();
    assert!(emitted.is_some(), "attributed sret+indirect edge must EMIT");
    cg.builder.build_return(None).unwrap();
    // And the by-value shim (a non-musttail attributed call site).
    let mt_fn = cg.module.get_function("bp$direct_mt").unwrap();
    cg.compile_decompose_shim("bp", "bp$direct_mt", mt_fn, &param_map)
        .unwrap();

    assert!(cg.module.verify().is_ok(), "module must verify");

    Target::initialize_native(&InitializationConfig::default()).unwrap();
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).unwrap();
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            inkwell::OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .expect("native target machine");
    let obj = tm.write_to_memory_buffer(&cg.module, FileType::Object);
    assert!(
        obj.is_ok(),
        "backend must lower the attributed musttail sret edge: {:?}",
        obj.err()
    );
    assert!(!obj.unwrap().as_slice().is_empty(), "non-empty object");
}
