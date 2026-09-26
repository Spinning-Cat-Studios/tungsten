//! `SelfHostedErrorEntry` data table — the self-hosted compiler's error codes.
//!
//! Split out of the parent module (ADR 24.7.26e review) so the catalogue is
//! pure data: ~260 lines of const entries that would otherwise push the
//! lookup/render logic past the file-size limit and require an exemption.

pub(crate) struct SelfHostedErrorEntry {
    pub(crate) code: &'static str,
    pub(crate) name: &'static str,
    pub(crate) category: &'static str,
    pub(crate) description: &'static str,
}

pub(super) const SELF_HOSTED_ERRORS: &[SelfHostedErrorEntry] = &[
    // Type errors (E0001-E0099)
    SelfHostedErrorEntry {
        code: "E0001",
        name: "ErrTypeMismatch",
        category: "Type Errors",
        description: "Expected one type, found another. The elaborator inferred or expected a specific type but the expression produced a different one.",
    },
    SelfHostedErrorEntry {
        code: "E0002",
        name: "ErrArityMismatch",
        category: "Type Errors",
        description: "Wrong number of arguments. A function or constructor was called with the wrong number of arguments.",
    },
    SelfHostedErrorEntry {
        code: "E0003",
        name: "ErrNotAFunction",
        category: "Type Errors",
        description: "Expected a function type but found something else. An expression was used in a function call position but does not have a function type.",
    },
    SelfHostedErrorEntry {
        code: "E0004",
        name: "ErrNotAType",
        category: "Type Errors",
        description: "Expected a type but found a value or unknown name. A name was used in a type position but does not resolve to a type definition.",
    },
    SelfHostedErrorEntry {
        code: "E0005",
        name: "ErrNotAnAdt",
        category: "Type Errors",
        description: "Expected an algebraic data type (ADT) but found a different kind of type.",
    },
    SelfHostedErrorEntry {
        code: "E0006",
        name: "ErrNotASum",
        category: "Type Errors",
        description: "Expected a sum type but found something else. Match expressions require a sum-typed scrutinee.",
    },
    SelfHostedErrorEntry {
        code: "E0007",
        name: "ErrNotAPair",
        category: "Type Errors",
        description: "Expected a product/pair type but found something else. Tuple indexing requires a product type.",
    },
    SelfHostedErrorEntry {
        code: "E0008",
        name: "ErrCannotInfer",
        category: "Type Errors",
        description: "Cannot infer a type. A type annotation is needed because the elaborator cannot determine the type from context.",
    },
    // Name resolution errors (E0100-E0199)
    SelfHostedErrorEntry {
        code: "E0100",
        name: "ErrUnresolvedType",
        category: "Name Resolution",
        description: "Type name not found in scope. The referenced type is not defined or not imported in the current module.",
    },
    SelfHostedErrorEntry {
        code: "E0101",
        name: "ErrUnresolvedValue",
        category: "Name Resolution",
        description: "Value name not found in scope. The referenced variable, function, or constructor is not defined or not imported.",
    },
    SelfHostedErrorEntry {
        code: "E0102",
        name: "ErrUnresolvedModule",
        category: "Name Resolution",
        description: "Module not found. The referenced module path does not resolve to any known module.",
    },
    SelfHostedErrorEntry {
        code: "E0103",
        name: "ErrDuplicateDef",
        category: "Name Resolution",
        description: "Name already defined. Two definitions in the same scope have the same name.",
    },
    SelfHostedErrorEntry {
        code: "E0104",
        name: "ErrPrivateAccess",
        category: "Name Resolution",
        description: "Accessing a private item. The item exists but is not visible from the current module.",
    },
    SelfHostedErrorEntry {
        code: "E0105",
        name: "ErrAmbiguousName",
        category: "Name Resolution",
        description: "Ambiguous name with multiple candidates. Multiple items with the same name are in scope (e.g., from different glob imports).",
    },
    SelfHostedErrorEntry {
        code: "E0106",
        name: "ErrDuplicateImport",
        category: "Name Resolution",
        description: "Same name imported twice. Two import statements bring the same name into scope.",
    },
    // Item/definition errors (E0200-E0299)
    SelfHostedErrorEntry {
        code: "E0200",
        name: "ErrMissingReturnType",
        category: "Items",
        description: "Function missing return type annotation. The function signature needs an explicit return type.",
    },
    SelfHostedErrorEntry {
        code: "E0201",
        name: "ErrInvalidExtern",
        category: "Items",
        description: "Invalid extern function declaration.",
    },
    SelfHostedErrorEntry {
        code: "E0202",
        name: "ErrImportNotFound",
        category: "Items",
        description: "Import path could not be resolved. The path in a `use` statement does not point to a valid module or item.",
    },
    SelfHostedErrorEntry {
        code: "E0203",
        name: "ErrGlobImportEmpty",
        category: "Items",
        description: "Glob import (`use foo::*`) resolved to zero items.",
    },
    SelfHostedErrorEntry {
        code: "E0204",
        name: "ErrCyclicDependency",
        category: "Items",
        description: "Cycle detected in type definitions. Types cannot reference each other in a way that creates an infinite structure without indirection.",
    },
    SelfHostedErrorEntry {
        code: "E0205",
        name: "ErrInvalidVisibility",
        category: "Items",
        description: "Visibility modifier on an invalid item. `pub` was used on an item that doesn't support visibility.",
    },
    SelfHostedErrorEntry {
        code: "E0206",
        name: "ErrMissingBody",
        category: "Items",
        description: "Function or theorem without a body. Non-extern functions must have an implementation.",
    },
    // Pattern matching errors (E0300-E0399)
    SelfHostedErrorEntry {
        code: "E0300",
        name: "ErrInexhaustiveMatch",
        category: "Patterns",
        description: "Non-exhaustive match. Not all possible values of the scrutinee type are covered by the match arms.",
    },
    SelfHostedErrorEntry {
        code: "E0301",
        name: "ErrUnreachablePattern",
        category: "Patterns",
        description: "Unreachable pattern. A match arm can never be reached because earlier arms already cover all its cases.",
    },
    SelfHostedErrorEntry {
        code: "E0302",
        name: "ErrConstructorNotFound",
        category: "Patterns",
        description: "Constructor not found for the given type. The pattern references a constructor that doesn't exist on the matched type.",
    },
    SelfHostedErrorEntry {
        code: "E0303",
        name: "ErrDuplicateField",
        category: "Patterns",
        description: "Field specified twice in a record pattern or constructor.",
    },
    SelfHostedErrorEntry {
        code: "E0304",
        name: "ErrMissingField",
        category: "Patterns",
        description: "Required field missing from record pattern or constructor.",
    },
    SelfHostedErrorEntry {
        code: "E0305",
        name: "ErrExtraField",
        category: "Patterns",
        description: "Unexpected field in record pattern or constructor.",
    },
    SelfHostedErrorEntry {
        code: "E0306",
        name: "ErrMultiArmMatch",
        category: "Patterns",
        description: "Match with too many arms. Only two-arm matches are supported for ADTs (one per constructor).",
    },
    SelfHostedErrorEntry {
        code: "E0307",
        name: "ErrPatternTooDeep",
        category: "Patterns",
        description: "Pattern nesting exceeds the depth limit.",
    },
    // Proof errors (E0400-E0499)
    SelfHostedErrorEntry {
        code: "E0400",
        name: "ErrProofRequired",
        category: "Proofs",
        description: "Expected a proof term.",
    },
    SelfHostedErrorEntry {
        code: "E0401",
        name: "ErrSorryInProd",
        category: "Proofs",
        description: "`sorry` used in production code. `sorry` is only allowed during development.",
    },
    SelfHostedErrorEntry {
        code: "E0402",
        name: "ErrInvalidRefl",
        category: "Proofs",
        description: "`refl` used on non-equal types. Reflexivity requires both sides of the equality to be the same type.",
    },
    // Reference/mutability errors (E0500-E0599)
    SelfHostedErrorEntry {
        code: "E0500",
        name: "ErrRefInPureContext",
        category: "References",
        description: "Reference used in a pure context. References are not allowed in pure functional code.",
    },
    SelfHostedErrorEntry {
        code: "E0501",
        name: "ErrCannotMutate",
        category: "References",
        description: "Trying to mutate an immutable binding.",
    },
    // Expression errors (E0600-E0699)
    SelfHostedErrorEntry {
        code: "E0600",
        name: "ErrNoReturnContext",
        category: "Control Flow",
        description: "`return` used outside a function body.",
    },
    SelfHostedErrorEntry {
        code: "E0601",
        name: "ErrNoMainFunction",
        category: "Entry Point",
        description: "No `main` function found. Executable files must define a `main` function.",
    },
    SelfHostedErrorEntry {
        code: "E0602",
        name: "ErrInvalidMainSig",
        category: "Entry Point",
        description: "`main` function has an invalid signature. It should take no arguments and return a supported type.",
    },
    // Soundness gates (E0700-E0799): positivity takes E0700-E0709, the
    // termination mirror starts at E0710 (ADR 18.8.26b D4).
    SelfHostedErrorEntry {
        code: "E0700",
        name: "ErrNonStrictlyPositive",
        category: "Soundness",
        description: "A type definition is not strictly positive: one of its own recursive group is reachable from a forbidden position -- left of an arrow at any depth, inside a `Ref`/`Ptr` cell, or in an `Eq` witness. Strict positivity is what makes `structural subterm` well-founded, so without it a diverging term exists at any type with no syntactic recursion for the termination checker to inspect. The rule is the same one the bootstrap raises E0061 for -- both compilers call one shared checker -- so the fix is the same: move the occurrence out of the argument position, storing the result of the function rather than the function itself. There is no annotation that admits it.",
    },
    SelfHostedErrorEntry {
        code: "E0710",
        name: "ErrCannotProveTermination",
        category: "Soundness",
        description: "A recursive definition could not be certified as terminating: no argument it passes to itself is a strict structural subterm of the caller's own decreasing parameter. Without termination a non-terminating definition inhabits every type -- including the empty one -- so every proof the compiler accepts would be contingent on the author not having written a loop. The rule is the same one the bootstrap raises E0062 for; both compilers call one shared checker, so the fix is the same: descend on a subterm of the argument rather than rebuilding it, name the parameter with `#[decreasing(arg)]` when the search is ambiguous, or opt the definition out with `#[partial]` -- which is fine in executable code and inadmissible in a proof (see E0711).",
    },
    SelfHostedErrorEntry {
        code: "E0711",
        name: "ErrPartialInProof",
        category: "Soundness",
        description: "A theorem, lemma or axiom reaches a `#[partial]` definition, directly or through a chain of ordinary ones. `#[partial]` opts a definition out of termination certification, and the opt-out taints transitively: a proof that depends on one is no longer a proof, because the partial definition may not terminate and so inhabits its type vacuously. Wrapping the partial definition in another one does not launder it -- admission rejects references to *tainted* constants, not only to annotated ones. The rule is the same one the bootstrap raises E0063 for. The fix is to remove the dependency, not to annotate the proof.",
    },
    // Other errors (E0900-E0999)
    SelfHostedErrorEntry {
        code: "E0900",
        name: "ErrRecursionLimitExceeded",
        category: "Limits",
        description: "Hit the elaboration recursion limit. The type or expression is too deeply nested.",
    },
    SelfHostedErrorEntry {
        code: "E0901",
        name: "ErrPhase1Violation",
        category: "Phases",
        description: "Violation of Phase 1 rules. An operation was attempted that is not allowed during the current elaboration phase.",
    },
    SelfHostedErrorEntry {
        code: "E0902",
        name: "ErrNotImplemented",
        category: "Internal",
        description: "Feature not yet implemented in the self-hosted compiler.",
    },
    SelfHostedErrorEntry {
        code: "E0999",
        name: "ErrOther",
        category: "Internal",
        description: "Catch-all error for miscellaneous failures. Check the error message for details.",
    },
];
