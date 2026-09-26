//! The `explain error` listing's category table — the grouped `(kind, summary)`
//! data [`super::error_catalogue`] renders, resolves and fuzzy-matches over.
//!
//! Split out of `error_catalogue.rs` so that file stays under the size cap: it
//! is the one part of the catalogue that is pure data, and it grows with every
//! new error kind.

/// All error kinds grouped by category, for listing.
pub(super) struct ErrorCategory {
    pub(super) name: &'static str,
    pub(super) entries: &'static [(&'static str, &'static str)], // (kind_name, summary)
}

pub(super) const CATEGORIES: &[ErrorCategory] = &[
    ErrorCategory {
        name: "Name Resolution",
        entries: &[
            ("UndefinedVariable", "cannot find value in scope"),
            ("UndefinedType", "cannot find type in scope"),
            ("UndefinedConstructor", "cannot find constructor in scope"),
            ("DuplicateDefinition", "name defined multiple times"),
            ("ModuleNotFound", "cannot find referenced module"),
            ("ItemNotFoundInModule", "item not found in module"),
            ("DuplicateImport", "same name imported twice"),
            ("GlobConflict", "glob imports conflict on a name"),
            ("UnresolvedImport", "cannot resolve import path"),
            ("PrivateModule", "module is private"),
            ("PrivateItem", "item is private"),
            ("PublicItemLeak", "public item exposes private type"),
        ],
    },
    ErrorCategory {
        name: "Type Errors",
        entries: &[
            ("TypeMismatch", "expected one type, found another"),
            ("CannotInferType", "type annotation needed"),
            ("CannotInferTypeArg", "cannot infer type argument"),
            ("ArityMismatch", "wrong number of arguments"),
            ("ExpectedFunction", "expected function, found other type"),
            ("ExpectedType", "expected a specific type"),
            ("ComparatorUnavailable", "no comparator for this type"),
            ("IntLiteralOutOfRange", "integer literal does not fit `Int`"),
            (
                "BuiltinTypeRedefined",
                "a `type` under a builtin type's name",
            ),
        ],
    },
    ErrorCategory {
        name: "Phase 1 Restrictions",
        entries: &[
            ("UnsupportedFeature", "feature not yet supported"),
            ("MutabilityNotSupported", "mutable bindings not supported"),
        ],
    },
    ErrorCategory {
        name: "Pattern Matching",
        entries: &[
            ("NonExhaustiveMatch", "match does not cover all cases"),
            ("MatchScrutineeNotAdt", "match arms expect an ADT scrutinee"),
            ("UnreachableArm", "pattern is unreachable"),
            ("PatternTooDeep", "pattern nesting exceeds limit"),
            ("UnsupportedPattern", "pattern form not supported"),
        ],
    },
    ErrorCategory {
        name: "Entry Point",
        entries: &[
            ("NoMainFunction", "no main function found"),
            ("ContainsSorry", "file contains sorry (cannot compile)"),
        ],
    },
    ErrorCategory {
        name: "Control Flow",
        entries: &[
            ("DeadCodeAfterReturn", "unreachable code after return"),
            ("TryOnNonTryType", "? on non-Result/Option type"),
            ("TryReturnMismatch", "? return type mismatch"),
            ("TryOutsideReturnContext", "? outside function body"),
            ("ReturnOutsideFunction", "return with no function in scope"),
            ("ReturnInsideTryBlock", "return inside a try block"),
            (
                "TryBlockRequiresResultType",
                "try block has no Result type in scope",
            ),
            (
                "TryBlockExpectedSumEncoding",
                "try block's Result is not a sum",
            ),
            (
                "TryBlockMissingConstructor",
                "try block's Result lacks a constructor",
            ),
            ("LetElseNonDiverging", "let-else branch does not diverge"),
            ("LetElseIrrefutable", "irrefutable pattern in let-else"),
            ("IfLetIrrefutable", "irrefutable pattern in if let"),
        ],
    },
    ErrorCategory {
        name: "Type Definitions",
        entries: &[
            ("RecursiveAlias", "type alias references itself"),
            ("NonStrictlyPositive", "type is not strictly positive"),
            (
                "NestedRecursiveFamily",
                "recursion nested under a generic parameter",
            ),
        ],
    },
    ErrorCategory {
        name: "Termination",
        entries: &[
            (
                "CannotProveTermination",
                "recursion is not structurally decreasing",
            ),
            ("PartialInProof", "a proof depends on a partial definition"),
        ],
    },
    ErrorCategory {
        name: "Equality Proofs",
        entries: &[
            (
                "ReflExpectedEquality",
                "refl checked against a non-equality type",
            ),
            (
                "InvalidRefl",
                "refl's two sides are not definitionally equal",
            ),
            (
                "SubstExpectedEquality",
                "subst's proof argument is not an equality",
            ),
            ("TransEndpointMismatch", "trans endpoints do not meet"),
            (
                "CongExpectedFunction",
                "cong's first argument is not a function",
            ),
            (
                "MotiveNotPredicate",
                "subst motive is not a predicate lambda",
            ),
            (
                "MotiveDomainMismatch",
                "motive parameter type differs from the base type",
            ),
            ("MotiveBodyNotType", "motive body is a term, not a type"),
            (
                "NatIndMotiveNotNat",
                "natind motive does not range over Nat",
            ),
        ],
    },
    ErrorCategory {
        name: "Named Records",
        entries: &[
            ("NotARecordType", "type is not a record"),
            ("MissingRecordField", "missing field in record constructor"),
            ("ExtraRecordField", "unknown field in record constructor"),
            ("DuplicateRecordField", "field specified twice"),
        ],
    },
];
