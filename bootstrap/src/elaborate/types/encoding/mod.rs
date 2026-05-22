//! ADT and Record Type Encoding
//!
//! This module handles encoding algebraic data types (ADTs) and records
//! into Core calculus types (sums, products, and μ-types).
//!
//! # Encoding Rules
//!
//! ## Records → Products
//!
//! Record fields are encoded as right-nested products:
//!
//! - `record Point { x: Nat, y: Nat }` → `Nat × Nat`
//! - `record Triple { a: Nat, b: Bool, c: String }` → `Nat × (Bool × String)`
//! - Single-field records are bare: `record Wrapper { val: Nat }` → `Nat`
//!
//! Records remain as nominal `TyVar("@RecordName")` during elaboration.
//! Product encoding is deferred to codegen's `expand_type`.
//!
//! ## Constructors → Products
//!
//! Each constructor's fields are encoded as right-nested products:
//!
//! - `Cons(T, List<T>)` → `T × α` (where `α` is the μ-variable)
//! - `Leaf(Nat)` → `Nat`
//! - `Nil` (no fields) → `Unit`
//!
//! ## ADTs → Sums
//!
//! ADT constructors are combined into sums. The policy (ADR 2.2.26):
//!
//! - 0 constructors → `Void`
//! - 1 constructor → bare payload (no Sum wrapper)
//! - 2 constructors → `Sum(ctor1, ctor2)`
//! - 3+ constructors → `Adt(name, type_args, [(ctor_name, payload), ...])`
//!
//! ## Recursive ADTs → μ-types
//!
//! Recursive ADTs (those referencing themselves in constructor fields)
//! are wrapped in `μ`-binders:
//!
//! - `enum List<T> { Nil, Cons(T, List<T>) }` → `μα_List. (Unit + (T × α_List))`
//!
//! The μ-variable uses the convention `α_<AdtName>`.
//! Self-references in the body become `TyVar("α_<AdtName>")`.
//!
//! # Named Type TyVar Convention
//!
//! Named types (records, stubs) use the `@` prefix in TyVar names
//! (e.g., `TyVar("@Token")`). This distinguishes them from genuine
//! type variables (`TyVar("T")`) and μ-variables (`TyVar("α_List")`).
//! See ADR 13.4.26c §2.
//!
//! # Cycle Detection
//!
//! Mutually recursive types (e.g., `type A = ... B ...` and `type B = ... A ...`)
//! would cause infinite recursion during encoding. We prevent this by tracking
//! types currently being encoded and skipping expansion when a cycle is detected.

use std::collections::{HashMap, HashSet};

use crate::elaborate::env::{Constructor, TypeDefKind};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

/// Shared context for encoding constructor fields within an ADT.
///
/// Bundles the parameters that are threaded through every field-encoding
/// call during `encode_adt_type_impl`, avoiding 7-8 parameter signatures.
pub(super) struct FieldSubstCtx<'a, 'b> {
    pub(super) adt_name: &'a str,
    pub(super) adt_params: &'a [String],
    pub(super) subst: &'a HashMap<&'b str, &'b Type>,
    pub(super) is_recursive: bool,
    pub(super) mu_var: String,
    /// μ-variables for other mutual recursion group members (excluding self).
    /// Each entry is (adt_name, mu_var). Empty if not in a mutual group.
    pub(super) group_mu_vars: Vec<(String, String)>,
    /// Whether to emit trace output for encoding decisions.
    pub(super) tracing: bool,
}

impl<'a> Elaborator<'a> {
    /// Encode an ADT as a Core type.
    ///
    /// For non-recursive ADTs:
    /// - `type Unit = ()` → `Unit`
    /// - `type Bool = True | False` → `Unit + Unit`
    /// - `type Option<T> = None | Some(T)` → `Unit + T`
    /// - `type Either<A, B> = Left(A) | Right(B)` → `A + B`
    ///
    /// For recursive ADTs (e.g., List<T>):
    /// - `enum List<T> { Nil, Cons(T, List<T>) }` → `μα. Unit + (T × α)`
    pub(crate) fn encode_adt_type(&mut self, name: &str, type_args: &[Type]) -> ElabResult<Type> {
        let mut mu_encoding_stack = HashSet::new();
        self.encode_adt_type_impl(name, type_args, &mut mu_encoding_stack)
    }

    /// Internal implementation of encode_adt_type with cycle detection.
    /// Public within crate so other modules can pass their mu_encoding_stack through.
    pub(crate) fn encode_adt_type_impl(
        &mut self,
        name: &str,
        type_args: &[Type],
        mu_encoding_stack: &mut HashSet<String>,
    ) -> ElabResult<Type> {
        let tracing = self.should_trace_encoding(name);

        if tracing {
            let args_str = format_type_args(type_args);
            self.trace_encoding("encode", &format!("{name}{args_str}: start"));
            self.trace_encoding(
                "encode",
                &format!("  stack: {{{}}}", format_stack(mu_encoding_stack)),
            );
        }

        // Cycle detection: return a reference if already encoding this type
        if let Some(cycle_result) =
            self.check_encoding_cycle(name, type_args, mu_encoding_stack, tracing)
        {
            return Ok(cycle_result);
        }

        // Pre-insert mutual recursion group members into the encoding stack
        let group_members_inserted = self.push_group_members(name, mu_encoding_stack);

        let result = self.encode_adt_type_core(name, type_args, mu_encoding_stack, tracing);

        // Clean up group members we pre-inserted (always, even on error)
        Self::pop_group_members(mu_encoding_stack, &group_members_inserted);

        result
    }

    /// Pre-insert mutual recursion group members into the encoding stack
    /// (ADR 18.4.26i §5 Step 4). This ensures cross-references within the
    /// group produce μ-variable cycle breaks instead of inlining the full
    /// encoding. We insert BOTH the bare name and the @-prefixed version.
    fn push_group_members(
        &self,
        name: &str,
        mu_encoding_stack: &mut HashSet<String>,
    ) -> Vec<String> {
        self.mutual_recursion_groups
            .get(name)
            .cloned()
            .map(|group| {
                let filtered: Vec<String> = group
                    .into_iter()
                    .filter(|m| m != name && !mu_encoding_stack.contains(m))
                    .collect();
                for m in &filtered {
                    mu_encoding_stack.insert(m.clone());
                    mu_encoding_stack.insert(format!("@{}", m));
                }
                filtered
            })
            .unwrap_or_default()
    }

    /// Remove previously-inserted group members from the encoding stack.
    fn pop_group_members(mu_encoding_stack: &mut HashSet<String>, group_members: &[String]) {
        for member in group_members {
            mu_encoding_stack.remove(member);
            mu_encoding_stack.remove(&format!("@{}", member));
        }
    }

    /// Core ADT encoding logic, called after cycle detection and group member setup.
    fn encode_adt_type_core(
        &mut self,
        name: &str,
        type_args: &[Type],
        mu_encoding_stack: &mut HashSet<String>,
        tracing: bool,
    ) -> ElabResult<Type> {
        // Check cache for non-parameterized types (no type_args to substitute)
        if type_args.is_empty() {
            if let Some(type_def) = self.env.lookup_type(name) {
                if let Some(ref cached) = type_def.encoded_type {
                    return Ok(cached.clone());
                }
            }
        }

        let type_def = self.env.lookup_type(name).cloned();
        let Some(type_def) = type_def else {
            return Err(self.undefined_type_error(crate::span::Span::new(0, 0), name));
        };

        let TypeDefKind::ADT(ref constructors) = type_def.kind else {
            return Err(ElabError::new(
                type_def.span,
                ElabErrorKind::Other(format!("`{}` is not an ADT", name)),
            ));
        };

        // Add to encoding stack before processing
        mu_encoding_stack.insert(name.to_string());

        if tracing {
            self.trace_encoding(
                "encode",
                &format!(
                    "  push stack: {name} → {{{}}}",
                    format_stack(mu_encoding_stack)
                ),
            );
        }

        let is_recursive = self.adt_is_recursive(name, constructors);

        // Check if this type is in a mutual recursion group (ADR 18.4.26i §5 Step 4).
        let group_mu_vars: Vec<(String, String)> = self
            .mutual_recursion_groups
            .get(name)
            .map(|group| {
                group
                    .iter()
                    .filter(|m| *m != name)
                    .map(|m| (m.clone(), format!("α_{}", m)))
                    .collect()
            })
            .unwrap_or_default();
        let in_mutual_group = !group_mu_vars.is_empty();
        let is_recursive = is_recursive || in_mutual_group;

        if tracing {
            self.trace_encoding("encode", &format!("  is_recursive: {is_recursive}"));
            if in_mutual_group {
                let members: Vec<&str> = group_mu_vars.iter().map(|(n, _)| n.as_str()).collect();
                self.trace_encoding(
                    "encode",
                    &format!("  mutual group: [{}]", members.join(", ")),
                );
            }
        }

        // Build substitution map for type parameters
        let subst: std::collections::HashMap<&str, &Type> = type_def
            .params
            .iter()
            .zip(type_args.iter())
            .map(|(p, a)| (p.as_str(), a))
            .collect();

        let mu_var = format!("α_{}", name);

        let ctx = FieldSubstCtx {
            adt_name: name,
            adt_params: &type_def.params,
            subst: &subst,
            is_recursive,
            mu_var: mu_var.clone(),
            group_mu_vars,
            tracing,
        };

        // Encode each constructor as a product of its fields
        let constructor_types = self.encode_constructors(constructors, &ctx, mu_encoding_stack);

        // Build sum type from constructors (ADR 2.2.26 policy)
        let body = Self::build_adt_sum_body(constructor_types, constructors, name, type_args);

        mu_encoding_stack.remove(name);

        // Wrap in μ-type if recursive, recording provenance
        self.finalize_adt_encoding(type_args, constructors, body, &ctx)
    }

    /// Check if we're already encoding this type (cycle detection).
    /// Returns `Some(type)` if a cycle is detected, `None` otherwise.
    ///
    /// For types in a mutual recursion group, returns `TyVar("α_Name")`
    /// (μ-variable) instead of bare `TyVar("Name")`, since the cross-reference
    /// will be bound by a nested μ-binder in the final encoding.
    fn check_encoding_cycle(
        &mut self,
        name: &str,
        type_args: &[Type],
        mu_encoding_stack: &HashSet<String>,
        tracing: bool,
    ) -> Option<Type> {
        if !mu_encoding_stack.contains(name) {
            return None;
        }

        // Determine if this cycle break is for a mutual recursion group member.
        // Group members use μ-variable names (α_Name) for the cycle break.
        // Strip @-prefix for named type references (ADR 13.4.26c §2).
        let bare_name = name.strip_prefix('@').unwrap_or(name);
        let is_group_member = self.mutual_recursion_groups.contains_key(bare_name);

        if tracing {
            self.trace_encoding(
                "cycle",
                &format!("⚠ \"{name}\" already in stack → cycle detected"),
            );
            let result_desc = if type_args.is_empty() {
                if is_group_member {
                    format!("→ TyVar(\"α_{bare_name}\") (mutual group cycle break)")
                } else {
                    format!("→ TyVar(\"{name}\") (cycle break)")
                }
            } else {
                format!("→ App(\"{name}\", [...]) (cycle break)")
            };
            self.trace_encoding("encode", &result_desc);
        }

        if type_args.is_empty() {
            let var_name = if is_group_member {
                format!("α_{}", bare_name)
            } else {
                name.to_string()
            };
            Some(Type::TyVar(var_name))
        } else {
            Some(Type::app(name.to_string(), type_args.to_vec()))
        }
    }
}

/// Format an encoding stack as a sorted, comma-separated string (for trace output).
fn format_stack(stack: &HashSet<String>) -> String {
    let mut names: Vec<&str> = stack.iter().map(|s| s.as_str()).collect();
    names.sort();
    names.join(", ")
}

/// Format type arguments for trace output (e.g., `<Nat, Bool>` or empty string).
fn format_type_args(type_args: &[Type]) -> String {
    if type_args.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = type_args.iter().map(|a| format!("{a}")).collect();
        format!("<{}>", parts.join(", "))
    }
}

mod constructors;
