//! Output formatting for values and types.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::config::MAX_TYPE_DISPLAY_DEPTH;
use tungsten_core::{Term, Type};

// ============================================================================
// Type Name Registry (exact match for non-parameterized types)
// ============================================================================

// Thread-local registry mapping encoded types to their user-defined names.
// This enables reverse lookup during error formatting without threading Env everywhere.
// Uses Vec instead of HashMap because Type doesn't implement Hash.
// The list is small (only non-parameterized types) so linear search is fine.
thread_local! {
    static TYPE_NAME_REGISTRY: RefCell<Vec<(Type, String)>> = RefCell::new(Vec::new());
}

/// Register a type-to-name mapping for reverse lookup in error messages.
///
/// Called during elaboration Phase 1e for non-parameterized types.
pub fn register_type_name(encoded_type: Type, name: String) {
    TYPE_NAME_REGISTRY.with(|registry| {
        let mut reg = registry.borrow_mut();
        // Avoid duplicates
        if !reg.iter().any(|(ty, _)| ty == &encoded_type) {
            reg.push((encoded_type, name));
        }
    });
}

/// Clear the type name registry (for testing or new compilation units).
pub fn clear_type_name_registry() {
    TYPE_NAME_REGISTRY.with(|registry| {
        registry.borrow_mut().clear();
    });
    TYPE_PATTERN_REGISTRY.with(|registry| {
        registry.borrow_mut().clear();
    });
}

/// Look up a user-defined type name from its Core encoding (exact match).
///
/// Returns Some(name) if a non-parameterized type matches exactly.
fn lookup_type_name_exact(ty: &Type) -> Option<String> {
    TYPE_NAME_REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .find(|(encoded, _)| encoded == ty)
            .map(|(_, name)| name.clone())
    })
}

// ============================================================================
// Type Pattern Registry (pattern match for parameterized types)
// ============================================================================

/// A type pattern for reverse lookup of parameterized types.
///
/// For `Option<T>`, the pattern would be `Unit + TyVar("T")`.
/// For `List<T>`, the pattern would be `μα_List. Unit + (TyVar("T") × TyVar("α_List"))`.
#[derive(Debug, Clone)]
pub struct TypePattern {
    /// Type name (e.g., "Option", "List")
    pub name: String,

    /// Parameter names in order (e.g., ["T"])
    pub params: Vec<String>,

    /// The pattern type with parameters as TyVar placeholders
    pub pattern: Type,

    /// The μ-variable name if this is a recursive type (e.g., "α_List")
    pub mu_var: Option<String>,
}

thread_local! {
    static TYPE_PATTERN_REGISTRY: RefCell<Vec<TypePattern>> = RefCell::new(Vec::new());
}

/// Register a type pattern for reverse lookup of parameterized types.
///
/// Called during elaboration Phase 1e for parameterized types like `Option<T>`.
pub fn register_type_pattern(pattern: TypePattern) {
    TYPE_PATTERN_REGISTRY.with(|registry| {
        let mut reg = registry.borrow_mut();
        // Avoid duplicates
        if !reg.iter().any(|p| p.name == pattern.name) {
            reg.push(pattern);
        }
    });
}

/// Try to match a concrete type against a pattern, returning variable bindings.
///
/// For example, matching `Unit + Nat` against `Unit + TyVar("T")` returns `{"T": Nat}`.
fn try_match_pattern<'a>(
    concrete: &Type,
    pattern: &'a Type,
    bindings: &mut HashMap<&'a str, Type>,
    mu_var: Option<&str>,
) -> bool {
    match (concrete, pattern) {
        // Base types must match exactly
        (Type::Nat, Type::Nat) => true,
        (Type::Bool, Type::Bool) => true,
        (Type::Unit, Type::Unit) => true,
        (Type::Void, Type::Void) => true,
        (Type::Prop, Type::Prop) => true,
        (Type::String, Type::String) => true,

        // Pattern variable: bind or check consistency
        (concrete, Type::TyVar(v)) => {
            // If this is the μ-variable, handle specially
            if Some(v.as_str()) == mu_var {
                // In concrete, this should also be a TyVar (the μ-bound variable)
                matches!(concrete, Type::TyVar(_))
            } else {
                // Regular type parameter - bind it
                if let Some(existing) = bindings.get(v.as_str()) {
                    // Must match existing binding
                    concrete == existing
                } else {
                    // New binding
                    bindings.insert(v.as_str(), concrete.clone());
                    true
                }
            }
        }

        // Structural matching for compound types
        (Type::Sum(ca, cb), Type::Sum(pa, pb)) => {
            try_match_pattern(ca, pa, bindings, mu_var)
                && try_match_pattern(cb, pb, bindings, mu_var)
        }
        (Type::Product(ca, cb), Type::Product(pa, pb)) => {
            try_match_pattern(ca, pa, bindings, mu_var)
                && try_match_pattern(cb, pb, bindings, mu_var)
        }
        (Type::Arrow(ca, cb), Type::Arrow(pa, pb)) => {
            try_match_pattern(ca, pa, bindings, mu_var)
                && try_match_pattern(cb, pb, bindings, mu_var)
        }

        // μ-types: match structurally, tracking the μ-variable
        (Type::Mu(cv, cbody), Type::Mu(pv, pbody)) => {
            // The concrete μ-var name may differ from pattern's, but structure should match
            // We track the pattern's μ-var and expect concrete to have a TyVar in same positions
            let _ = cv; // concrete's μ-var name (unused, we match structurally)
            try_match_pattern(cbody, pbody, bindings, Some(pv.as_str()))
        }

        // Forall types
        (Type::Forall(cv, cbody), Type::Forall(pv, pbody)) => {
            // Similar to μ-types - match structurally
            let _ = cv;
            try_match_pattern(cbody, pbody, bindings, Some(pv.as_str()))
        }

        // Pointer and Ref types
        (Type::Ptr(ci), Type::Ptr(pi)) => try_match_pattern(ci, pi, bindings, mu_var),
        (Type::Ref(ci), Type::Ref(pi)) => try_match_pattern(ci, pi, bindings, mu_var),

        // Eq types (rare in user code, but handle for completeness)
        (Type::Eq(cty, ct1, ct2), Type::Eq(pty, pt1, pt2)) => {
            try_match_pattern(cty, pty, bindings, mu_var) && ct1 == pt1 && ct2 == pt2
        }

        // No match
        _ => false,
    }
}

/// Try to match a concrete type against all registered patterns.
///
/// Returns (type_name, type_args) if a pattern matches.
///
/// To avoid false positives (e.g., Stmt matching as Result because both are sums),
/// we ONLY pattern match for Mu types where the μ-variable name matches a registered
/// pattern name. This is conservative but prevents misleading error messages.
fn lookup_type_by_pattern(ty: &Type) -> Option<(String, Vec<Type>)> {
    TYPE_PATTERN_REGISTRY.with(|registry| {
        let patterns = registry.borrow();

        // Only match Mu types where the μ-variable name matches a pattern name.
        // This prevents Sum types like Stmt from being misidentified as Result.
        if let Type::Mu(mu_var, _) = ty {
            if let Some(type_name) = mu_var.strip_prefix("α_") {
                for pattern in patterns.iter() {
                    if pattern.name == type_name {
                        let mut bindings: HashMap<&str, Type> = HashMap::new();
                        if try_match_pattern(
                            ty,
                            &pattern.pattern,
                            &mut bindings,
                            pattern.mu_var.as_deref(),
                        ) {
                            let args: Option<Vec<Type>> = pattern
                                .params
                                .iter()
                                .map(|p| bindings.get(p.as_str()).cloned())
                                .collect();
                            if let Some(args) = args {
                                return Some((pattern.name.clone(), args));
                            }
                        }
                    }
                }
            }
        }

        // NOTE: We intentionally do NOT fall back to structural matching for non-Mu types.
        // Structural matching is too error-prone - types like Stmt (a sum type) would
        // incorrectly match Result's pattern (also a sum type), leading to confusing
        // error messages like "expected List<Stmt>, found List<Result<...>>".
        None
    })
}

/// Look up a user-defined type name from its Core encoding.
///
/// Tries exact match first (for non-parameterized types),
/// then pattern match (for parameterized types like `Option<T>`).
fn lookup_type_name(ty: &Type) -> Option<String> {
    // First try exact match (non-parameterized types)
    if let Some(name) = lookup_type_name_exact(ty) {
        return Some(name);
    }

    // Then try pattern match (parameterized types)
    if let Some((name, args)) = lookup_type_by_pattern(ty) {
        if args.is_empty() {
            return Some(name);
        } else {
            // Format as Name<Arg1, Arg2, ...>
            // Recursively format args in case they're also user types
            let formatted_args: Vec<String> = args
                .iter()
                .map(|arg| {
                    // Try to look up the arg too (handles nested user types)
                    lookup_type_name(arg).unwrap_or_else(|| format_type_simple(arg))
                })
                .collect();
            return Some(format!("{}<{}>", name, formatted_args.join(", ")));
        }
    }

    None
}

/// Simple type formatting without depth limit (for type arguments).
fn format_type_simple(ty: &Type) -> String {
    match ty {
        Type::Unit => "Unit".to_string(),
        Type::Void => "Void".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Nat => "Nat".to_string(),
        Type::Prop => "Prop".to_string(),
        Type::String => "String".to_string(),
        Type::TyVar(name) => name.clone(),
        Type::Arrow(a, b) => format!("{} -> {}", format_type_simple(a), format_type_simple(b)),
        Type::Product(a, b) => format!("({} × {})", format_type_simple(a), format_type_simple(b)),
        Type::Sum(a, b) => format!("({} + {})", format_type_simple(a), format_type_simple(b)),
        _ => format!("{:?}", ty),
    }
}

/// Format a term as a human-readable value.
pub fn format_value(term: &Term) -> String {
    match term {
        Term::Zero => "0".to_string(),
        Term::Succ(n) => {
            // Try to convert to a number
            if let Some(n) = term_to_nat(term) {
                n.to_string()
            } else {
                format!("succ({})", format_value(n))
            }
        }
        Term::True => "true".to_string(),
        Term::False => "false".to_string(),
        Term::Unit => "()".to_string(),
        Term::Pair(a, b) => format!("({}, {})", format_value(a), format_value(b)),
        Term::Inl(_, t) => format!("inl({})", format_value(t)),
        Term::Inr(_, t) => format!("inr({})", format_value(t)),
        Term::Lambda(_, _, _) => "<function>".to_string(),
        Term::TyAbs(_, _) => "<polymorphic function>".to_string(),
        Term::Refl(_, _) => "refl".to_string(),
        Term::Sorry => "sorry".to_string(),
        // Phase 2A: String literals
        Term::StringLit(s) => format!("\"{}\"", s),
        // Phase 2A: Folded μ-types (e.g., list constructors)
        Term::Fold(_, t) => format_value(t),
        // For stuck terms, show the structure
        _ => format!("{:?}", term),
    }
}

/// Try to convert a Term to a natural number.
fn term_to_nat(term: &Term) -> Option<u64> {
    match term {
        Term::Zero => Some(0),
        Term::Succ(n) => term_to_nat(n).map(|n| n + 1),
        _ => None,
    }
}

/// Format a type as a human-readable string.
pub fn format_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "Unit".to_string(),
        Type::Void => "Void".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Nat => "Nat".to_string(),
        Type::Prop => "Prop".to_string(),
        Type::String => "String".to_string(), // Phase 2A
        Type::TyVar(name) => name.clone(),
        Type::Arrow(a, b) => {
            let a_str = if matches!(**a, Type::Arrow(_, _)) {
                format!("({})", format_type(a))
            } else {
                format_type(a)
            };
            format!("{} -> {}", a_str, format_type(b))
        }
        Type::Product(a, b) => format!("({} × {})", format_type(a), format_type(b)),
        Type::Sum(a, b) => format!("({} + {})", format_type(a), format_type(b)),
        Type::Forall(name, body) => format!("∀ {}. {}", name, format_type(body)),
        Type::Eq(ty, a, b) => format!("Eq<{}, {:?}, {:?}>", format_type(ty), a, b),
        Type::Mu(name, body) => format!("μ{}. {}", name, format_type(body)), // Phase 2A
        Type::Ptr(inner) => format!("*{}", format_type(inner)),              // Phase 3-Prep
        Type::Ref(inner) => format!("Ref<{}>", format_type(inner)),          // Phase 3-Prep
        Type::App(name, args) => {
            // Deferred type application (should be resolved before output)
            let args_str: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, args_str.join(", "))
        }
        // Flat ADT (ADR 2.2.26)
        Type::Adt(name, type_args, _variants) => {
            if type_args.is_empty() {
                name.clone()
            } else {
                let args_str: Vec<String> = type_args.iter().map(format_type).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
        }
    }
}

/// Format a type for display in error messages with depth-limited truncation.
///
/// Uses MAX_TYPE_DISPLAY_DEPTH to control how deeply nested types are expanded.
/// Types exceeding the depth limit are truncated with "..." to keep error
/// messages readable.
///
/// For non-parameterized user-defined types (ADTs, Records, Aliases), attempts
/// to show the original type name instead of the Core encoding.
pub fn format_type_for_display(ty: &Type) -> String {
    // First, try to find a user-defined type name for this encoding.
    // This handles non-parameterized types like `Color`, `Direction`, etc.
    if let Some(name) = lookup_type_name(ty) {
        return name;
    }

    format_type_with_depth(ty, MAX_TYPE_DISPLAY_DEPTH)
}

/// Format a type with explicit depth limit.
///
/// When depth reaches 0, nested structures are shown as "...".
fn format_type_with_depth(ty: &Type, depth: usize) -> String {
    if depth == 0 {
        return "...".to_string();
    }

    match ty {
        // Base types don't need depth tracking
        Type::Unit => "Unit".to_string(),
        Type::Void => "Void".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Nat => "Nat".to_string(),
        Type::Prop => "Prop".to_string(),
        Type::String => "String".to_string(),
        Type::TyVar(name) => name.clone(),

        // Compound types recurse with decremented depth
        Type::Arrow(a, b) => {
            let a_str = if matches!(**a, Type::Arrow(_, _)) {
                format!("({})", format_type_with_depth(a, depth - 1))
            } else {
                format_type_with_depth(a, depth - 1)
            };
            format!("{} -> {}", a_str, format_type_with_depth(b, depth - 1))
        }
        Type::Product(a, b) => {
            format!(
                "({} × {})",
                format_type_with_depth(a, depth - 1),
                format_type_with_depth(b, depth - 1)
            )
        }
        Type::Sum(a, b) => {
            format!(
                "({} + {})",
                format_type_with_depth(a, depth - 1),
                format_type_with_depth(b, depth - 1)
            )
        }
        Type::Forall(name, body) => {
            format!("∀ {}. {}", name, format_type_with_depth(body, depth - 1))
        }
        Type::Eq(inner_ty, a, b) => {
            // For Eq types, show the type but truncate term details at depth 1
            if depth <= 1 {
                format!("Eq<{}, ...>", format_type_with_depth(inner_ty, depth - 1))
            } else {
                format!(
                    "Eq<{}, {:?}, {:?}>",
                    format_type_with_depth(inner_ty, depth - 1),
                    a,
                    b
                )
            }
        }
        Type::Mu(name, body) => {
            // Try to extract a readable type name from our encoding convention.
            // We use names like "α_List" or "α_Tree" for recursive types.
            if let Some(type_name) = name.strip_prefix("α_") {
                // Detected a named recursive type - show it more readably
                // Check if this type actually has parameters by looking in the registry
                let has_params = TYPE_PATTERN_REGISTRY.with(|registry| {
                    registry
                        .borrow()
                        .iter()
                        .find(|p| p.name == type_name)
                        .map(|p| !p.params.is_empty())
                        .unwrap_or(false) // If not in registry, assume no params
                });

                if has_params {
                    format!("{}<...>", type_name)
                } else {
                    // No type parameters - just show the name
                    type_name.to_string()
                }
            } else {
                // Unknown μ-variable, show raw encoding
                format!("μ{}. {}", name, format_type_with_depth(body, depth - 1))
            }
        }
        Type::Ptr(inner) => {
            // Use Ptr<T> notation for consistency with user-visible syntax
            format!("Ptr<{}>", format_type_with_depth(inner, depth - 1))
        }
        Type::Ref(inner) => {
            format!("Ref<{}>", format_type_with_depth(inner, depth - 1))
        }
        Type::App(name, args) => {
            if args.is_empty() {
                name.clone()
            } else {
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| format_type_with_depth(a, depth - 1))
                    .collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
        }
        // Flat ADT (ADR 2.2.26)
        Type::Adt(name, type_args, _variants) => {
            if type_args.is_empty() {
                name.clone()
            } else {
                let args_str: Vec<String> = type_args
                    .iter()
                    .map(|a| format_type_with_depth(a, depth - 1))
                    .collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
        }
    }
}
