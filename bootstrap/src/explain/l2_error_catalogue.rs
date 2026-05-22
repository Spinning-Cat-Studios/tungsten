//! L2 error code catalogue for `tungsten explain error --l2`.
//!
//! Maps L2 (self-hosted compiler) error codes to descriptions.
//! L2 uses range-based numbering that differs from L1:
//!   L2 E0001 = `TypeMismatch`  vs  L1 E0010 = `TypeMismatch`
//!   L2 E0101 = `UnresolvedValue` vs L1 E0001 = `UndefinedVariable`
//!
//! See `src/compiler/elab/error/kinds.tg` for the canonical source.

use std::process::ExitCode;

struct L2ErrorEntry {
    code: &'static str,
    name: &'static str,
    category: &'static str,
    description: &'static str,
}

const L2_ERRORS: &[L2ErrorEntry] = &[
    // Type errors (E0001-E0099)
    L2ErrorEntry {
        code: "E0001",
        name: "ErrTypeMismatch",
        category: "Type Errors",
        description: "Expected one type, found another. The elaborator inferred or expected a specific type but the expression produced a different one.",
    },
    L2ErrorEntry {
        code: "E0002",
        name: "ErrArityMismatch",
        category: "Type Errors",
        description: "Wrong number of arguments. A function or constructor was called with the wrong number of arguments.",
    },
    L2ErrorEntry {
        code: "E0003",
        name: "ErrNotAFunction",
        category: "Type Errors",
        description: "Expected a function type but found something else. An expression was used in a function call position but does not have a function type.",
    },
    L2ErrorEntry {
        code: "E0004",
        name: "ErrNotAType",
        category: "Type Errors",
        description: "Expected a type but found a value or unknown name. A name was used in a type position but does not resolve to a type definition.",
    },
    L2ErrorEntry {
        code: "E0005",
        name: "ErrNotAnAdt",
        category: "Type Errors",
        description: "Expected an algebraic data type (ADT) but found a different kind of type.",
    },
    L2ErrorEntry {
        code: "E0006",
        name: "ErrNotASum",
        category: "Type Errors",
        description: "Expected a sum type but found something else. Match expressions require a sum-typed scrutinee.",
    },
    L2ErrorEntry {
        code: "E0007",
        name: "ErrNotAPair",
        category: "Type Errors",
        description: "Expected a product/pair type but found something else. Tuple indexing requires a product type.",
    },
    L2ErrorEntry {
        code: "E0008",
        name: "ErrCannotInfer",
        category: "Type Errors",
        description: "Cannot infer a type. A type annotation is needed because the elaborator cannot determine the type from context.",
    },
    // Name resolution errors (E0100-E0199)
    L2ErrorEntry {
        code: "E0100",
        name: "ErrUnresolvedType",
        category: "Name Resolution",
        description: "Type name not found in scope. The referenced type is not defined or not imported in the current module.",
    },
    L2ErrorEntry {
        code: "E0101",
        name: "ErrUnresolvedValue",
        category: "Name Resolution",
        description: "Value name not found in scope. The referenced variable, function, or constructor is not defined or not imported.",
    },
    L2ErrorEntry {
        code: "E0102",
        name: "ErrUnresolvedModule",
        category: "Name Resolution",
        description: "Module not found. The referenced module path does not resolve to any known module.",
    },
    L2ErrorEntry {
        code: "E0103",
        name: "ErrDuplicateDef",
        category: "Name Resolution",
        description: "Name already defined. Two definitions in the same scope have the same name.",
    },
    L2ErrorEntry {
        code: "E0104",
        name: "ErrPrivateAccess",
        category: "Name Resolution",
        description: "Accessing a private item. The item exists but is not visible from the current module.",
    },
    L2ErrorEntry {
        code: "E0105",
        name: "ErrAmbiguousName",
        category: "Name Resolution",
        description: "Ambiguous name with multiple candidates. Multiple items with the same name are in scope (e.g., from different glob imports).",
    },
    L2ErrorEntry {
        code: "E0106",
        name: "ErrDuplicateImport",
        category: "Name Resolution",
        description: "Same name imported twice. Two import statements bring the same name into scope.",
    },
    // Item/definition errors (E0200-E0299)
    L2ErrorEntry {
        code: "E0200",
        name: "ErrMissingReturnType",
        category: "Items",
        description: "Function missing return type annotation. The function signature needs an explicit return type.",
    },
    L2ErrorEntry {
        code: "E0201",
        name: "ErrInvalidExtern",
        category: "Items",
        description: "Invalid extern function declaration.",
    },
    L2ErrorEntry {
        code: "E0202",
        name: "ErrImportNotFound",
        category: "Items",
        description: "Import path could not be resolved. The path in a `use` statement does not point to a valid module or item.",
    },
    L2ErrorEntry {
        code: "E0203",
        name: "ErrGlobImportEmpty",
        category: "Items",
        description: "Glob import (`use foo::*`) resolved to zero items.",
    },
    L2ErrorEntry {
        code: "E0204",
        name: "ErrCyclicDependency",
        category: "Items",
        description: "Cycle detected in type definitions. Types cannot reference each other in a way that creates an infinite structure without indirection.",
    },
    L2ErrorEntry {
        code: "E0205",
        name: "ErrInvalidVisibility",
        category: "Items",
        description: "Visibility modifier on an invalid item. `pub` was used on an item that doesn't support visibility.",
    },
    L2ErrorEntry {
        code: "E0206",
        name: "ErrMissingBody",
        category: "Items",
        description: "Function or theorem without a body. Non-extern functions must have an implementation.",
    },
    // Pattern matching errors (E0300-E0399)
    L2ErrorEntry {
        code: "E0300",
        name: "ErrInexhaustiveMatch",
        category: "Patterns",
        description: "Non-exhaustive match. Not all possible values of the scrutinee type are covered by the match arms.",
    },
    L2ErrorEntry {
        code: "E0301",
        name: "ErrUnreachablePattern",
        category: "Patterns",
        description: "Unreachable pattern. A match arm can never be reached because earlier arms already cover all its cases.",
    },
    L2ErrorEntry {
        code: "E0302",
        name: "ErrConstructorNotFound",
        category: "Patterns",
        description: "Constructor not found for the given type. The pattern references a constructor that doesn't exist on the matched type.",
    },
    L2ErrorEntry {
        code: "E0303",
        name: "ErrDuplicateField",
        category: "Patterns",
        description: "Field specified twice in a record pattern or constructor.",
    },
    L2ErrorEntry {
        code: "E0304",
        name: "ErrMissingField",
        category: "Patterns",
        description: "Required field missing from record pattern or constructor.",
    },
    L2ErrorEntry {
        code: "E0305",
        name: "ErrExtraField",
        category: "Patterns",
        description: "Unexpected field in record pattern or constructor.",
    },
    L2ErrorEntry {
        code: "E0306",
        name: "ErrMultiArmMatch",
        category: "Patterns",
        description: "Match with too many arms. Only two-arm matches are supported for ADTs (one per constructor).",
    },
    L2ErrorEntry {
        code: "E0307",
        name: "ErrPatternTooDeep",
        category: "Patterns",
        description: "Pattern nesting exceeds the depth limit.",
    },
    // Proof errors (E0400-E0499)
    L2ErrorEntry {
        code: "E0400",
        name: "ErrProofRequired",
        category: "Proofs",
        description: "Expected a proof term.",
    },
    L2ErrorEntry {
        code: "E0401",
        name: "ErrSorryInProd",
        category: "Proofs",
        description: "`sorry` used in production code. `sorry` is only allowed during development.",
    },
    L2ErrorEntry {
        code: "E0402",
        name: "ErrInvalidRefl",
        category: "Proofs",
        description: "`refl` used on non-equal types. Reflexivity requires both sides of the equality to be the same type.",
    },
    // Reference/mutability errors (E0500-E0599)
    L2ErrorEntry {
        code: "E0500",
        name: "ErrRefInPureContext",
        category: "References",
        description: "Reference used in a pure context. References are not allowed in pure functional code.",
    },
    L2ErrorEntry {
        code: "E0501",
        name: "ErrCannotMutate",
        category: "References",
        description: "Trying to mutate an immutable binding.",
    },
    // Expression errors (E0600-E0699)
    L2ErrorEntry {
        code: "E0600",
        name: "ErrNoReturnContext",
        category: "Control Flow",
        description: "`return` used outside a function body.",
    },
    L2ErrorEntry {
        code: "E0601",
        name: "ErrNoMainFunction",
        category: "Entry Point",
        description: "No `main` function found. Executable files must define a `main` function.",
    },
    L2ErrorEntry {
        code: "E0602",
        name: "ErrInvalidMainSig",
        category: "Entry Point",
        description: "`main` function has an invalid signature. It should take no arguments and return a supported type.",
    },
    // Other errors (E0900-E0999)
    L2ErrorEntry {
        code: "E0900",
        name: "ErrRecursionLimitExceeded",
        category: "Limits",
        description: "Hit the elaboration recursion limit. The type or expression is too deeply nested.",
    },
    L2ErrorEntry {
        code: "E0901",
        name: "ErrPhase1Violation",
        category: "Phases",
        description: "Violation of Phase 1 rules. An operation was attempted that is not allowed during the current elaboration phase.",
    },
    L2ErrorEntry {
        code: "E0902",
        name: "ErrNotImplemented",
        category: "Internal",
        description: "Feature not yet implemented in the self-hosted compiler.",
    },
    L2ErrorEntry {
        code: "E0999",
        name: "ErrOther",
        category: "Internal",
        description: "Catch-all error for miscellaneous failures. Check the error message for details.",
    },
];

/// Print the L2 error listing grouped by category.
pub fn print_l2_error_list() {
    println!("L2 (Self-Hosted Compiler) Error Reference");
    println!("══════════════════════════════════════════");
    println!();
    println!("Note: L2 error codes differ from L1 (Rust bootstrap) codes.");
    println!("L1 uses flat numbering; L2 uses range-based numbering.");
    println!("See `tungsten explain error` for L1 codes.\n");

    let categories = [
        "Type Errors",
        "Name Resolution",
        "Items",
        "Patterns",
        "Proofs",
        "References",
        "Control Flow",
        "Entry Point",
        "Limits",
        "Phases",
        "Internal",
    ];

    for cat in categories {
        let entries: Vec<_> = L2_ERRORS.iter().filter(|e| e.category == cat).collect();
        if entries.is_empty() {
            continue;
        }
        println!("{cat}:");
        for entry in entries {
            println!(
                "  {} {:<30} {}",
                entry.code,
                entry.name,
                short_desc(entry.description)
            );
        }
        println!();
    }

    println!("Use `tungsten explain error --l2 <code>` for detailed explanation.");
    println!("Use `tungsten explain error --l2 <name>` to look up by name.");
}

/// Print a detailed L2 error explanation.
pub fn print_l2_error_explanation(query: &str) -> ExitCode {
    // Try matching by code first, then by name
    let entry = L2_ERRORS
        .iter()
        .find(|e| e.code.eq_ignore_ascii_case(query) || e.name.eq_ignore_ascii_case(query));

    if let Some(entry) = entry {
        println!("L2 Error: {} ({})", entry.code, entry.name);
        println!("{}", "═".repeat(12 + entry.code.len() + entry.name.len()));
        println!();
        println!("Category: {}", entry.category);
        println!();
        println!("Description:");
        for line in entry.description.lines() {
            println!("  {line}");
        }
        println!();
        // Show L1 equivalent if known
        if let Some(l1) = l1_equivalent(entry.code) {
            println!("L1 equivalent: tungsten explain error {l1}");
        }
        println!();
        println!("Note: L2 codes appear in tungsten1/tungsten2 output.");
        println!("L1 codes appear in the Rust bootstrap compiler output.");
        ExitCode::SUCCESS
    } else {
        eprintln!("Unknown L2 error: `{query}`");
        if let Some(suggestion) = fuzzy_match_l2(query) {
            eprintln!("Did you mean `{suggestion}`?");
        }
        eprintln!();
        eprintln!("Run `tungsten explain error --l2` to list all L2 error codes.");
        ExitCode::FAILURE
    }
}

/// Map L2 code → L1 name for cross-reference.
fn l1_equivalent(l2_code: &str) -> Option<&'static str> {
    match l2_code {
        "E0001" => Some("TypeMismatch"),
        "E0002" => Some("ArityMismatch"),
        "E0003" => Some("ExpectedFunction"),
        "E0004" => Some("UndefinedType"),
        "E0005" => Some("ExpectedType"),
        "E0008" => Some("CannotInferType"),
        "E0100" => Some("UndefinedType"),
        "E0101" => Some("UndefinedVariable"),
        "E0102" => Some("ModuleNotFound"),
        "E0103" => Some("DuplicateDefinition"),
        "E0104" => Some("PrivateItem"),
        "E0106" => Some("DuplicateImport"),
        "E0300" => Some("NonExhaustiveMatch"),
        "E0301" => Some("UnreachableArm"),
        "E0307" => Some("PatternTooDeep"),
        "E0601" => Some("NoMainFunction"),
        _ => None,
    }
}

/// Truncate a description for listing display.
fn short_desc(desc: &str) -> &str {
    let end = desc.find('.').map_or(desc.len(), |i| i);
    &desc[..end]
}

/// Fuzzy-match an L2 error code or name.
fn fuzzy_match_l2(input: &str) -> Option<&'static str> {
    let input_lower = input.to_lowercase();

    // Try prefix match on codes
    if input_lower.starts_with('e') {
        for entry in L2_ERRORS {
            if entry.code.to_lowercase().starts_with(&input_lower) {
                return Some(entry.code);
            }
        }
    }

    // Try substring match on names
    for entry in L2_ERRORS {
        if entry.name.to_lowercase().contains(&input_lower) {
            return Some(entry.name);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_by_code() {
        let entry = L2_ERRORS.iter().find(|e| e.code == "E0001");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name, "ErrTypeMismatch");
    }

    #[test]
    fn lookup_by_name() {
        let entry = L2_ERRORS
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case("ErrUnresolvedValue"));
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().code, "E0101");
    }

    #[test]
    fn l1_crossref_exists() {
        assert_eq!(l1_equivalent("E0001"), Some("TypeMismatch"));
        assert_eq!(l1_equivalent("E0101"), Some("UndefinedVariable"));
        assert_eq!(l1_equivalent("E0999"), None);
    }

    #[test]
    fn all_entries_have_unique_codes() {
        let mut codes: Vec<&str> = L2_ERRORS.iter().map(|e| e.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), L2_ERRORS.len());
    }

    #[test]
    fn fuzzy_finds_partial_code() {
        assert_eq!(fuzzy_match_l2("E000"), Some("E0001"));
    }

    #[test]
    fn fuzzy_finds_partial_name() {
        assert_eq!(fuzzy_match_l2("mismatch"), Some("ErrTypeMismatch"));
    }
}
