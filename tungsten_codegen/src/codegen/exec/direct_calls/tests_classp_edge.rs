//! Class-P tail-edge correctness tests (ADR 1.7.26e §2.5/§2.6, R3/R5/R10):
//! simultaneous assignment across multiple indirect buffers, full
//! materialization of self-aliasing next-values, no scratch/forwarded allocas
//! in tail position, entry-block value-context buffers, the mixed
//! flattenable + non-flattenable shape, and the eager-load discipline that
//! keeps buffer addresses from escaping (R7 extern boundary).

use super::decompose::ParamLowering;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::targets::TargetTriple;
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::AddressSpace;
use tungsten_core::types::Type;

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

/// Extract the printed IR body of one function (from its `define` line to the
/// closing `}`) so assertions don't match other functions in the module.
fn fn_ir(ir: &str, name: &str) -> String {
    let needle = format!("@\"{name}\"(");
    let mut out = Vec::new();
    let mut inside = false;
    for l in ir.lines() {
        if l.starts_with("define") && l.contains(&needle) {
            inside = true;
        }
        if inside {
            out.push(l);
            if l == "}" {
                break;
            }
        }
    }
    assert!(!out.is_empty(), "function '{name}' not found in IR:\n{ir}");
    out.join("\n")
}

/// Declare a Class-P entry with `n` nested-struct params (+ an i64 flat), get
/// `$direct_mt`, position the builder in its entry block, and eager-load each
/// indirect buffer (as `bind_decomposed_params` does).
fn setup_class_p<'ctx>(
    cg: &mut CodeGen<'ctx>,
    ctx: &'ctx Context,
    base: &str,
    n_indirect: usize,
) -> (
    inkwell::values::FunctionValue<'ctx>,
    Vec<BasicValueEnum<'ctx>>,
) {
    let ptr_type = ctx.ptr_type(AddressSpace::default());
    let i64_type = ctx.i64_type();
    let nested = nested_struct(ctx);
    let mut params: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = vec![ptr_type.into()];
    for _ in 0..n_indirect {
        params.push(nested.into());
    }
    params.push(i64_type.into());
    let fn_type = i64_type.fn_type(&params, false);
    cg.module
        .add_function(&format!("{base}$direct"), fn_type, None);
    let param_map = cg.declare_decomposed_entry(base, fn_type).unwrap().unwrap();
    assert_eq!(param_map.len(), n_indirect + 1);
    cg.direct_calls.set_decompose_map(base, param_map);

    let mt_fn = cg
        .module
        .get_function(&format!("{base}$direct_mt"))
        .unwrap();
    cg.compilation.current_fn = Some(mt_fn);
    let entry = ctx.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);

    // Eager-load each indirect buffer once at entry (the bind discipline that
    // makes §2.6 simultaneous assignment safe by construction).
    let mut loaded = Vec::new();
    for i in 0..n_indirect {
        let buf = mt_fn.get_nth_param(i as u32).unwrap().into_pointer_value();
        let v = cg
            .builder
            .build_load(nested, buf, &format!("old.{i}"))
            .unwrap();
        loaded.push(v);
    }
    (mt_fn, loaded)
}

/// §2.6 self-aliasing: `ctx'` is derived from old `ctx` (field update). The
/// store-back must be a single store of the fully materialized SSA aggregate —
/// no `getelementptr` partial mutation into the forwarded buffer.
#[test]
fn test_self_aliasing_next_value_fully_materialized() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_self_alias");
    let (mt_fn, loaded) = setup_class_p(&mut cg, &context, "sa", 1);

    // next = old with a field updated (self-aliasing derivation).
    let updated = cg
        .builder
        .build_insert_value(
            loaded[0].into_struct_value(),
            context.ptr_type(AddressSpace::default()).const_null(),
            0,
            "next_ctx",
        )
        .unwrap()
        .as_basic_value_enum();
    let flat = context.i64_type().const_int(1, false).into();

    let emitted = cg
        .try_emit_decomposed_musttail("sa", &[updated, flat])
        .unwrap();
    assert!(emitted.is_some(), "self-aliasing Class-P edge must EMIT");
    cg.emit_return_if_needed(&emitted.unwrap()).unwrap();

    assert!(cg.module.verify().is_ok());
    let ir = cg.module.print_to_string().to_string();
    let body = fn_ir(&ir, "sa$direct_mt");

    // Fully materialized: exactly one store to the buffer, of the derived SSA
    // value; no GEP-based partial mutation of the forwarded buffer.
    let stores: Vec<&str> = body.lines().filter(|l| l.contains("store ")).collect();
    assert_eq!(stores.len(), 1, "exactly one store-back, got:\n{body}");
    assert!(
        stores[0].contains("%next_ctx") || stores[0].contains("insertvalue"),
        "store-back stores the materialized next value, got: {}",
        stores[0]
    );
    assert!(
        !body.contains("getelementptr"),
        "no in-place partial mutation of the forwarded buffer:\n{body}"
    );
    // Read-before-overwrite: the eager load precedes the store-back.
    let load_idx = body.find("%old.0 = load").expect("eager load present");
    let store_idx = body.find("store ").unwrap();
    assert!(
        load_idx < store_idx,
        "load of old value precedes store-back"
    );
}

/// §2.6 simultaneous assignment: two indirect params where `next_a` is derived
/// from old `b` (cross-param) and `next_b` is old `a` (swap). Every read of an
/// old buffer must precede every store-back, and each buffer is forwarded in
/// its own slot. R5/R10: the tail edge introduces NO allocas (no type-pooled
/// scratch, no per-iteration slot).
#[test]
fn test_two_indirect_params_swap_and_cross_param() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_swap");
    let (mt_fn, loaded) = setup_class_p(&mut cg, &context, "sw", 2);

    // next_a = old_b with a field updated (cross-param derivation);
    // next_b = old_a (swap).
    let next_a = cg
        .builder
        .build_insert_value(
            loaded[1].into_struct_value(),
            context.ptr_type(AddressSpace::default()).const_null(),
            0,
            "next_a",
        )
        .unwrap()
        .as_basic_value_enum();
    let next_b = loaded[0];
    let flat = context.i64_type().const_int(1, false).into();

    let emitted = cg
        .try_emit_decomposed_musttail("sw", &[next_a, next_b, flat])
        .unwrap();
    assert!(emitted.is_some(), "two-indirect-param swap edge must EMIT");
    cg.emit_return_if_needed(&emitted.unwrap()).unwrap();

    assert!(cg.module.verify().is_ok());
    let ir = cg.module.print_to_string().to_string();
    let body = fn_ir(&ir, "sw$direct_mt");
    let lines: Vec<&str> = body.lines().collect();

    // All old-buffer reads precede all store-backs (simultaneous assignment).
    let last_load = lines
        .iter()
        .rposition(|l| l.contains("= load") && l.contains("%old."))
        .expect("eager loads present");
    let first_store = lines
        .iter()
        .position(|l| l.trim_start().starts_with("store "))
        .expect("store-backs present");
    assert!(
        last_load < first_store,
        "every old-buffer read precedes every store-back:\n{body}"
    );

    // Both buffers stored and forwarded in their own slots (%0, %1).
    let stores: Vec<&&str> = lines
        .iter()
        .filter(|l| l.trim_start().starts_with("store "))
        .collect();
    assert_eq!(
        stores.len(),
        2,
        "one store-back per indirect param:\n{body}"
    );
    assert!(stores.iter().any(|l| l.contains("ptr %0")));
    assert!(stores.iter().any(|l| l.contains("ptr %1")));
    assert!(
        body.contains(r#"musttail call i64 @"sw$direct_mt"(ptr"#),
        "musttail forwards the buffers:\n{body}"
    );
    let musttail_line = lines.iter().find(|l| l.contains("musttail call")).unwrap();
    assert!(
        musttail_line.contains("%0") && musttail_line.contains("%1"),
        "both incoming buffer pointers forwarded unchanged: {musttail_line}"
    );

    // R5/R10: same-typed params, and still no scratch alloca at the tail edge —
    // next values are SSA, buffers distinct by construction.
    assert!(
        !body.contains("alloca"),
        "tail edge must not introduce allocas (R5 scratch / R10 forwarded):\n{body}"
    );
    let _ = mt_fn;
}

/// §2.6 duplicate actual: the SAME old value stored into both buffers — the
/// two buffers stay distinct, each getting its own store of the shared value.
#[test]
fn test_duplicate_actual_stores_same_value_to_distinct_buffers() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_dup");
    let (_mt_fn, loaded) = setup_class_p(&mut cg, &context, "dp", 2);

    let flat = context.i64_type().const_int(1, false).into();
    let emitted = cg
        .try_emit_decomposed_musttail("dp", &[loaded[0], loaded[0], flat])
        .unwrap();
    assert!(emitted.is_some());
    cg.emit_return_if_needed(&emitted.unwrap()).unwrap();

    assert!(cg.module.verify().is_ok());
    let ir = cg.module.print_to_string().to_string();
    let body = fn_ir(&ir, "dp$direct_mt");
    let stores: Vec<&str> = body
        .lines()
        .filter(|l| l.trim_start().starts_with("store "))
        .collect();
    assert_eq!(stores.len(), 2);
    // Same materialized value, distinct destination buffers.
    assert!(stores.iter().all(|l| l.contains("%old.0")));
    assert!(stores.iter().any(|l| l.contains("ptr %0")));
    assert!(stores.iter().any(|l| l.contains("ptr %1")));
}

/// Mixed flattenable + non-flattenable (§2.2/§2.4): the non-flattenable param
/// goes indirect, the flattenable one keeps 18.5.26a scalar decomposition, and
/// the self-tail edge lives only in `$direct_mt → $direct_mt` (no bounce).
#[test]
fn test_mixed_flattenable_and_indirect_no_bounce() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_mixed");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let flat2 = context.struct_type(&[i64_type.into(), i64_type.into()], false);
    // i64 @mx$direct(ptr env, {ptr,{i64,i64}} ctx, {i64,i64} pair)
    let fn_type = i64_type.fn_type(&[ptr_type.into(), nested.into(), flat2.into()], false);
    cg.module.add_function("mx$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("mx", fn_type).unwrap().unwrap();
    assert_eq!(
        param_map,
        vec![ParamLowering::Indirect, ParamLowering::Decompose(2)],
        "non-flattenable goes indirect; flattenable keeps decomposition"
    );
    cg.direct_calls.set_decompose_map("mx", param_map);

    let mt_fn = cg.module.get_function("mx$direct_mt").unwrap();
    // Layout: [0]=ctx buf, [1]=env, [2..4]=decomposed pair fields → i64 return.
    assert_eq!(mt_fn.count_params(), 4);
    cg.compilation.current_fn = Some(mt_fn);
    let entry = context.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);
    let old = cg
        .builder
        .build_load(
            nested,
            mt_fn.get_nth_param(0).unwrap().into_pointer_value(),
            "old.0",
        )
        .unwrap();
    let pair = flat2.get_undef().into();

    let emitted = cg.try_emit_decomposed_musttail("mx", &[old, pair]).unwrap();
    assert!(emitted.is_some(), "mixed Class-P edge must EMIT");
    cg.emit_return_if_needed(&emitted.unwrap()).unwrap();

    assert!(cg.module.verify().is_ok());
    let ir = cg.module.print_to_string().to_string();
    let body = fn_ir(&ir, "mx$direct_mt");
    let mt_line = body
        .lines()
        .find(|l| l.contains(r#"musttail call i64 @"mx$direct_mt"("#))
        .expect("musttail self edge present");
    assert!(
        mt_line.contains("%0") && mt_line.contains("ptr null"),
        "self edge forwards buffer + env, decomposed scalars after: {mt_line}"
    );
    // No bounce through the by-value `$direct` shim.
    let bounced = body.lines().any(|l| {
        let t = l.trim_start();
        t.contains("call") && t.contains(r#"@"mx$direct"("#)
    });
    assert!(
        !bounced,
        "self edge must not bounce through $direct:\n{body}"
    );
}

/// R7 extern boundary (P0 escape-audit consequence): `bind` eager-loads the
/// indirect param to an SSA **value**, so every body consumer — extern calls
/// included — receives the aggregate value through the existing by-value path;
/// the buffer address itself never flows onward.
#[test]
fn test_bind_eager_loads_value_not_buffer_address() {
    let context = Context::create();
    let mut cg = codegen_aarch64(&context, "test_bind_load");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // Core type Nat × (Nat × Nat) lowers to a nested, non-flattenable struct.
    let core_ty = Type::product(Type::Nat, Type::product(Type::Nat, Type::Nat));
    let lowered = cg.types.lower_type(&core_ty);
    assert!(lowered.is_struct_type(), "fixture must lower to a struct");
    let fn_type = i64_type.fn_type(&[ptr_type.into(), lowered.into()], false);
    cg.module.add_function("eb$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("eb", fn_type).unwrap().unwrap();
    assert_eq!(param_map, vec![ParamLowering::Indirect]);

    let mt_fn = cg.module.get_function("eb$direct_mt").unwrap();
    cg.compilation.current_fn = Some(mt_fn);
    let entry = context.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);

    cg.bind_decomposed_params(
        mt_fn,
        "eb$direct_mt",
        &["ctx".to_string()],
        &[&core_ty],
        &param_map,
    )
    .unwrap();

    let (bound, _ty) = cg.compilation.env.get("ctx").expect("ctx bound").clone();
    assert!(
        bound.is_struct_value(),
        "bind binds the loaded aggregate VALUE, not the buffer ptr: {bound:?}"
    );
    let ir = cg.module.print_to_string().to_string();
    assert!(
        ir.contains("ctx.indirect.load = load"),
        "eager load-through-buffer at entry:\n{ir}"
    );
}
