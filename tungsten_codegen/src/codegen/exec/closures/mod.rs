//! Closure conversion and function application.
//!
//! Lambdas are converted to closures represented as:
//! ```text
//! { fn_ptr: fn(env*, param) -> ret, env_ptr: env* }
//! ```
//!
//! The environment struct contains all free variables captured by the lambda.
//!
//! This module is split into:
//! - [`lambda`]: Lambda compilation and closure creation
//! - [`application`]: Function application (calling closures)
//! - [`fix`]: Fixed point / general recursion compilation

mod application;
mod fix;
mod lambda;

use crate::analysis::free_vars;
use inkwell::basic_block::BasicBlock;
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue};
use std::collections::HashMap;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

// ============================================================================
// Helper types for lambda compilation
// ============================================================================

/// Saved codegen state during lambda body compilation.
///
/// Lambda compilation temporarily switches to a new function context.
/// This struct captures the state that must be restored afterwards.
pub(super) struct SavedLambdaState<'ctx> {
    pub(super) current_fn: Option<FunctionValue<'ctx>>,
    pub(super) env: HashMap<String, (BasicValueEnum<'ctx>, Type)>,
    pub(super) insert_block: Option<BasicBlock<'ctx>>,
    pub(super) in_tail_position: bool,
}

/// Information about captured variables for a lambda.
pub(super) struct CaptureInfo<'ctx> {
    /// Sorted list of captured variable names (for consistent ordering)
    pub(super) names: Vec<String>,
    /// LLVM types for each captured variable
    pub(super) field_types: Vec<BasicTypeEnum<'ctx>>,
    /// The struct type for the environment
    pub(super) env_struct_type: StructType<'ctx>,
}

// ============================================================================
// Helper functions for closure operations
// ============================================================================

/// Check if a function term represents a known noreturn function.
///
/// Returns true for functions named "exit" or containing "`tg_exit`",
/// which are wrappers around the exit system call.
pub(super) fn is_noreturn_function_name(func: &Term) -> bool {
    match func {
        Term::Var(name) | Term::Global(name) => name == "exit" || name.contains("tg_exit"),
        _ => false,
    }
}

/// Collect and sort free variables from a lambda body, excluding the parameter.
pub(super) fn collect_free_variables(body: &Term, param_name: &str) -> Vec<String> {
    let mut fv = free_vars(body);
    fv.remove(param_name);
    let mut sorted: Vec<_> = fv.into_iter().collect();
    sorted.sort();
    sorted
}

// ============================================================================
// Tests for shared helpers
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::terms::Term;
    use tungsten_core::types::Type;

    // ========================================================================
    // Tests for free variable collection
    // ========================================================================

    #[test]
    fn test_collect_free_variables_no_captures() {
        // λx. x has no free variables (x is the parameter)
        let body = Term::Var("x".to_string());
        let fv = collect_free_variables(&body, "x");
        assert!(fv.is_empty());
    }

    #[test]
    fn test_collect_free_variables_single_capture() {
        // λx. y captures y
        let body = Term::Var("y".to_string());
        let fv = collect_free_variables(&body, "x");
        assert_eq!(fv, vec!["y".to_string()]);
    }

    #[test]
    fn test_collect_free_variables_multiple_captures_sorted() {
        // λx. (z, y, a) should capture [a, y, z] in sorted order
        let body = Term::Pair(
            Box::new(Term::Var("z".to_string())),
            Box::new(Term::Pair(
                Box::new(Term::Var("y".to_string())),
                Box::new(Term::Var("a".to_string())),
            )),
        );
        let fv = collect_free_variables(&body, "x");
        assert_eq!(fv, vec!["a".to_string(), "y".to_string(), "z".to_string()]);
    }

    #[test]
    fn test_collect_free_variables_excludes_parameter() {
        // λx. (x, y) should only capture y, not x
        let body = Term::Pair(
            Box::new(Term::Var("x".to_string())),
            Box::new(Term::Var("y".to_string())),
        );
        let fv = collect_free_variables(&body, "x");
        assert_eq!(fv, vec!["y".to_string()]);
    }

    // ========================================================================
    // Tests for noreturn function detection
    // ========================================================================

    #[test]
    fn test_is_noreturn_function_name_exit() {
        assert!(is_noreturn_function_name(&Term::Var("exit".to_string())));
        assert!(is_noreturn_function_name(&Term::Global("exit".to_string())));
    }

    #[test]
    fn test_is_noreturn_function_name_tg_exit_variants() {
        assert!(is_noreturn_function_name(&Term::Var("tg_exit".to_string())));
        assert!(is_noreturn_function_name(&Term::Global(
            "my_tg_exit_wrapper".to_string()
        )));
        assert!(is_noreturn_function_name(&Term::Var(
            "prefix_tg_exit_suffix".to_string()
        )));
    }

    #[test]
    fn test_is_noreturn_function_name_regular_functions() {
        assert!(!is_noreturn_function_name(&Term::Var("print".to_string())));
        assert!(!is_noreturn_function_name(&Term::Global("foo".to_string())));
        assert!(!is_noreturn_function_name(&Term::Var("main".to_string())));
    }

    #[test]
    fn test_is_noreturn_function_name_non_var_terms() {
        // Lambda and other term types should return false
        assert!(!is_noreturn_function_name(&Term::Lambda(
            "x".to_string(),
            Type::Unit,
            Box::new(Term::Var("x".to_string()))
        )));
        assert!(!is_noreturn_function_name(&Term::Unit));
        assert!(!is_noreturn_function_name(&Term::NatLit(42)));
    }
}
