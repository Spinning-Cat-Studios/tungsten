//! Shared route-lowering probe (ADR 12.7.26c P3/P4/D5).
//!
//! `doctor check type lowering-consistency` and `info type lowering` both ask
//! the same question — "what LLVM layout does each lowering *route* produce for
//! this type?" — so they share this one probe over `TypeLowering::route_layouts`
//! (the P1/D4 core). The commands cannot disagree because the layouts come from
//! the same place. It lives in the `tungsten_bootstrap` lib (not the bin's
//! `compile` module) so the lib-side doctor check can reach it; the bin-side
//! `info` command imports it as `tungsten_bootstrap::doctor::lowering_probe`.
//!
//! Parameterized ADTs are instantiated with `String` type arguments (the
//! documented best-effort fallback in §2.2). Route *agreement* is size-
//! independent — every route funnels through the shared `tagged_union_blob_type`
//! authority, so any concrete instantiation that reaches all routes exposes a
//! route that bypasses the authority. `String` gives a concrete, non-trivial
//! blob size (the exact quantity that diverged in 16d2f4f1).

use tungsten_codegen::inkwell::context::Context;
use tungsten_codegen::inkwell::targets::{InitializationConfig, Target, TargetMachine};
use tungsten_codegen::{CodegenConstructor, Route, TypeLowering};
use tungsten_core::types::Type;

use crate::driver::ProjectOutput;

/// One type's LLVM layout under a specific lowering route.
#[derive(Debug, Clone)]
pub struct RouteLayout {
    /// The route's short tag (`"named"`, `"app"`, `"structural"`, `"flat-adt"`).
    pub route: &'static str,
    /// The route's full human-facing label.
    pub label: &'static str,
    /// The LLVM layout string this route produced.
    pub layout: String,
}

/// A type whose routes disagree — the split-brain the check exists to catch.
#[derive(Debug, Clone)]
pub struct LoweringDivergence {
    /// The ADT name.
    pub type_name: String,
    /// The concrete type arguments used (empty for a nullary ADT).
    pub args: Vec<String>,
    /// The per-route layouts (at least two distinct).
    pub layouts: Vec<RouteLayout>,
}

/// Convert the driver's ADT table into the codegen constructor form. Mirrors
/// the bin's `compile::convert_adt_types_for_codegen`, inlined here so this
/// lib module has no dependency on the bin's `compile` module.
fn adt_types_for_codegen(
    project: &ProjectOutput,
) -> std::collections::HashMap<String, (Vec<String>, Vec<CodegenConstructor>)> {
    project
        .adt_types
        .iter()
        .map(|(name, (params, ctors))| {
            let codegen_ctors = ctors
                .iter()
                .map(|c| CodegenConstructor {
                    name: c.name.clone(),
                    fields: c.fields.clone(),
                    index: c.index,
                })
                .collect();
            (name.clone(), (params.clone(), codegen_ctors))
        })
        .collect()
}

/// Build a `TypeLowering` for a project: register its ADT + record types and
/// attach native target data so blob sizing is accurate. Shared setup for both
/// P3 and P4.
#[must_use]
pub fn build_project_lowering<'ctx>(
    context: &'ctx Context,
    project: &ProjectOutput,
) -> TypeLowering<'ctx> {
    let mut lowering = TypeLowering::new(context);
    lowering.register_record_types(project.record_types.clone());
    lowering.register_adt_types(adt_types_for_codegen(project));

    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");
    let triple = TargetMachine::get_default_triple();
    if let Ok(target) = Target::from_triple(&triple) {
        if let Some(machine) = target.create_target_machine(
            &triple,
            "generic",
            "",
            tungsten_codegen::inkwell::OptimizationLevel::Default,
            tungsten_codegen::inkwell::targets::RelocMode::PIC,
            tungsten_codegen::inkwell::targets::CodeModel::Default,
        ) {
            lowering.set_target_data(machine.get_target_data());
        }
    }
    lowering
}

/// The (name, params) of every ADT in a project, in sorted order for
/// deterministic output.
#[must_use]
pub fn project_adt_signatures(project: &ProjectOutput) -> Vec<(String, Vec<String>)> {
    let mut sigs: Vec<(String, Vec<String>)> = project
        .adt_types
        .iter()
        .map(|(name, (params, _))| (name.clone(), params.clone()))
        .collect();
    sigs.sort_by(|a, b| a.0.cmp(&b.0));
    sigs
}

/// Instantiate an ADT's type parameters with `String` (the best-effort
/// fallback), yielding the concrete args each route is lowered at.
fn instantiate_args(params: &[String]) -> Vec<Type> {
    params.iter().map(|_| Type::String).collect()
}

/// Lower a named type via every applicable route, returning each route's layout.
/// Recursive ADTs are skipped (they lower uniformly to `ptr` at the value level,
/// so value-level route divergence is impossible — see §2.2 / Non-Goals).
#[must_use]
pub fn type_route_layouts(
    lowering: &mut TypeLowering,
    name: &str,
    params: &[String],
) -> Vec<RouteLayout> {
    if lowering.is_recursive_adt(name) {
        return Vec::new();
    }
    let args = instantiate_args(params);
    lowering
        .route_layouts(name, &args)
        .into_iter()
        .map(|(route, ty): (Route, _)| RouteLayout {
            route: route.short(),
            label: route.label(),
            layout: ty.print_to_string().to_string(),
        })
        .collect()
}

/// Do these route layouts disagree? True when at least two routes produced
/// distinct LLVM layouts.
#[must_use]
pub fn layouts_diverge(layouts: &[RouteLayout]) -> bool {
    layouts.windows(2).any(|w| w[0].layout != w[1].layout)
}

/// Scan every non-recursive ADT in a project for a route-lowering divergence.
#[must_use]
pub fn scan_lowering_consistency(
    lowering: &mut TypeLowering,
    signatures: &[(String, Vec<String>)],
) -> Vec<LoweringDivergence> {
    let mut divergences = Vec::new();
    for (name, params) in signatures {
        let layouts = type_route_layouts(lowering, name, params);
        if layouts.len() >= 2 && layouts_diverge(&layouts) {
            divergences.push(LoweringDivergence {
                type_name: name.clone(),
                args: instantiate_args(params)
                    .iter()
                    .map(|t| format!("{t}"))
                    .collect(),
                layouts,
            });
        }
    }
    divergences
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rl(route: &'static str, layout: &str) -> RouteLayout {
        RouteLayout {
            route,
            label: "route",
            layout: layout.to_string(),
        }
    }

    #[test]
    fn agreeing_layouts_do_not_diverge() {
        let layouts = vec![
            rl("named", "{ i32, [16 x i8] }"),
            rl("app", "{ i32, [16 x i8] }"),
            rl("structural", "{ i32, [16 x i8] }"),
        ];
        assert!(!layouts_diverge(&layouts));
    }

    #[test]
    fn distinct_layouts_diverge() {
        // The 16d2f4f1 shape: named route typed payload vs structural blob.
        let layouts = vec![
            rl("named", "{ i32, { ptr, i64 } }"),
            rl("structural", "{ i32, [40 x i8] }"),
        ];
        assert!(layouts_diverge(&layouts));
    }

    #[test]
    fn instantiate_args_fills_params_with_string() {
        let args = instantiate_args(&["T".to_string(), "E".to_string()]);
        assert_eq!(args, vec![Type::String, Type::String]);
    }
}
