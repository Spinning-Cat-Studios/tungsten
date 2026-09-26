//! Default messages for elaboration error kinds.
//!
//! Split from `kind.rs` to keep that file under the 400-line limit. (That
//! file is now `kind/`, split again by ADR 11.8.26c for the same reason.)

use super::kind::ElabErrorKind;

impl ElabErrorKind {
    /// Get the default message for this error kind.
    pub(in crate::elaborate) fn default_message(&self) -> String {
        match self {
            // Name resolution — simple "not found" errors
            ElabErrorKind::UndefinedVariable(name) => {
                format!("cannot find value `{}` in this scope", name)
            }
            ElabErrorKind::UndefinedType(name) => {
                format!("cannot find type `{}` in this scope", name)
            }
            ElabErrorKind::UndefinedConstructor(name) => {
                format!("cannot find constructor `{}` in this scope", name)
            }
            ElabErrorKind::DuplicateDefinition(name) => {
                format!("the name `{}` is defined multiple times", name)
            }

            // Module/import errors (delegated — contain internal branching)
            ElabErrorKind::ModuleNotFound { .. }
            | ElabErrorKind::ItemNotFoundInModule { .. }
            | ElabErrorKind::DuplicateImport { .. }
            | ElabErrorKind::GlobConflict { .. }
            | ElabErrorKind::UnresolvedImport(_)
            | ElabErrorKind::PrivateModule { .. }
            | ElabErrorKind::PrivateItem { .. }
            | ElabErrorKind::PublicItemLeak { .. } => self.format_module_message(),

            // Type errors
            ElabErrorKind::TypeMismatch { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }
            ElabErrorKind::CannotInferType => {
                "cannot infer type; add a type annotation".to_string()
            }
            ElabErrorKind::CannotInferTypeArg(var) => {
                format!(
                    "cannot infer type argument `{}`; provide explicit type arguments",
                    var
                )
            }
            ElabErrorKind::ArityMismatch { expected, found } => {
                format!("expected {} arguments, found {}", expected, found)
            }
            ElabErrorKind::ExpectedFunction(ty) => {
                format!("expected function, found `{}`", ty)
            }
            ElabErrorKind::ExpectedType { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }

            // Phase 1 restrictions
            ElabErrorKind::UnsupportedFeature(feature) => {
                format!("`{}` is not supported", feature)
            }
            ElabErrorKind::MutabilityNotSupported => {
                "mutable bindings are not supported; use shadowing instead".to_string()
            }

            // Pattern matching errors
            ElabErrorKind::NonExhaustiveMatch => "non-exhaustive match patterns".to_string(),
            ElabErrorKind::MatchScrutineeNotAdt {
                adt_name: Some(name),
                found,
            } => format!(
                "match arms use constructors of `{}`, but the scrutinee has type `{}`",
                name, found
            ),
            ElabErrorKind::MatchScrutineeNotAdt {
                adt_name: None,
                found,
            } => format!(
                "match arms are constructor patterns, but the scrutinee has \
                 non-ADT type `{}`",
                found
            ),
            ElabErrorKind::UnreachableArm => "unreachable pattern".to_string(),
            ElabErrorKind::DeadCodeAfterReturn => "unreachable code after `return`".to_string(),
            ElabErrorKind::PatternTooDeep { depth, max } => {
                format!("pattern nesting depth {} exceeds maximum of {}", depth, max)
            }
            ElabErrorKind::UnsupportedPattern(pat) => {
                format!("pattern `{}` is not supported", pat)
            }

            // Try operator errors
            ElabErrorKind::TryOnNonTryType(ty) => {
                format!(
                    "`?` operator requires `Result<T, E>` or `Option<T>`, found `{}`",
                    ty
                )
            }
            ElabErrorKind::TryReturnMismatch {
                operand_type,
                return_type,
            } => {
                format!(
                    "cannot use `?` on `{}` in function returning `{}`",
                    operand_type, return_type
                )
            }
            ElabErrorKind::TryOutsideReturnContext => {
                "`?` can only be used inside a function or closure body with a known return type"
                    .to_string()
            }
            ElabErrorKind::ReturnInsideTryBlock => {
                "`return` inside `try` block is not allowed; use `?` to propagate errors"
                    .to_string()
            }
            ElabErrorKind::TryBlockRequiresResultType => {
                "`try` block requires a `Result` type in scope; add a type annotation".to_string()
            }
            ElabErrorKind::TryBlockExpectedSumEncoding => {
                "expected `Result` type (Sum encoding) for `try` block".to_string()
            }
            ElabErrorKind::TryBlockMissingConstructor(name) => {
                format!(
                    "`Result` type must have `{}` constructor for `try` block",
                    name
                )
            }

            // Let-else errors
            ElabErrorKind::LetElseNonDiverging(ty) => {
                format!(
                    "`else` branch in `let`-`else` must diverge (e.g., `return`), found type `{}`",
                    ty
                )
            }
            ElabErrorKind::LetElseIrrefutable => {
                "irrefutable pattern in `let`-`else`; `else` branch is unreachable".to_string()
            }
            ElabErrorKind::IfLetIrrefutable => {
                "irrefutable pattern in `if let`; condition always matches".to_string()
            }

            // Named record errors
            ElabErrorKind::NotARecordType(name) => {
                format!("`{}` is not a record type", name)
            }
            ElabErrorKind::MissingRecordField { field, type_name } => {
                format!("missing field `{}` for record type `{}`", field, type_name)
            }
            ElabErrorKind::ExtraRecordField { field, type_name } => {
                format!("unknown field `{}` for record type `{}`", field, type_name)
            }
            ElabErrorKind::DuplicateRecordField(name) => {
                format!("duplicate field `{}`", name)
            }

            // Entry point errors
            ElabErrorKind::NoMainFunction => "no `main` function found".to_string(),
            ElabErrorKind::ContainsSorry => "cannot compile file containing `sorry`".to_string(),
            ElabErrorKind::RecursiveAlias(name) => {
                format!("recursive type alias `{}` references itself", name)
            }
            ElabErrorKind::NonStrictlyPositive { .. } => self.format_positivity_message(),

            // Termination (ADR 29.6.26e) — the headline is rendered by the
            // engine in `tungsten_core`, so the gate cannot word a rejection
            // differently from `doctor check type termination`.
            ElabErrorKind::CannotProveTermination { headline, .. } => headline.clone(),
            ElabErrorKind::PartialInProof { proof, tainted } => format!(
                "`{proof}` is a proof, so it may not depend on the partial constant `{tainted}`"
            ),

            // Nested inductive families (ADR 11.8.26c) — delegated, like E0061
            ElabErrorKind::NestedRecursiveFamily { .. } => self.format_nested_family_message(),

            // Equality proof errors (ADR 21.5.26d) — delegated
            ElabErrorKind::ReflExpectedEquality(_)
            | ElabErrorKind::InvalidRefl { .. }
            | ElabErrorKind::SubstExpectedEquality(_)
            | ElabErrorKind::TransEndpointMismatch { .. }
            | ElabErrorKind::CongExpectedFunction(_) => self.format_equality_message(),

            // Motive errors (ADR 21.5.26g)
            ElabErrorKind::MotiveNotPredicate(ty) => {
                format!("`subst` motive must be a predicate lambda `|x: τ| <type>`, but found type `{}`", ty)
            }
            ElabErrorKind::MotiveDomainMismatch { expected, found } => {
                format!(
                    "motive parameter type `{}` does not match equality base type `{}`",
                    found, expected
                )
            }
            ElabErrorKind::MotiveBodyNotType => {
                "motive body must be a type expression, not a term".to_string()
            }

            ElabErrorKind::NatIndMotiveNotNat(found) => {
                format!(
                    "`natind` motive domain must be `Nat`, but found `{}`",
                    found
                )
            }

            ElabErrorKind::ReturnOutsideFunction => {
                "`return` outside of a function body".to_string()
            }

            ElabErrorKind::ComparatorUnavailable(ty) => {
                format!(
                    "no comparator available for type `{}` \
                     (supported: primitives, tuples, sums/ADTs, records, and lists)",
                    ty
                )
            }

            ElabErrorKind::IntLiteralOutOfRange(literal) => {
                format!(
                    "integer literal `{}` is out of range for `Int` \
                     (the signed 64-bit range is -9223372036854775808 to 9223372036854775807)",
                    literal
                )
            }

            ElabErrorKind::BuiltinTypeRedefined(name) => {
                format!(
                    "cannot redefine the builtin type `{}` — builtin lookup precedes \
                     user types, so this definition would be unreachable",
                    name
                )
            }

            ElabErrorKind::InternalError(msg) => {
                format!(
                    "internal compiler error: {} — this is a bug in the Tungsten \
                     compiler, not in your program; please report it",
                    msg
                )
            }

            ElabErrorKind::Other(msg) => msg.clone(),
        }
    }

    /// Format messages for equality proof error kinds (ADR 21.5.26d).
    /// Message for E0061 (ADR 7.8.26e §2.2).
    ///
    /// The type, constructor/record, field, offending occurrence **and** the
    /// inherited-through chain all live in the primary message rather than in
    /// notes, because the ariadne renderer surfaces only the last note — and
    /// the intermediate type and parameter are the one part of a D2-shaped
    /// violation the reader cannot see in their own source.
    fn format_positivity_message(&self) -> String {
        let ElabErrorKind::NonStrictlyPositive {
            type_name,
            ctor_name,
            is_record,
            field,
            occurrence,
            via,
        } = self
        else {
            return String::new();
        };
        let position = if *is_record {
            format!("record `{type_name}`, {field}")
        } else {
            format!("constructor `{ctor_name}`, {field}")
        };
        let through: String = via
            .iter()
            .map(|(intermediate, param)| format!(" through `{intermediate}`'s parameter `{param}`"))
            .collect();
        format!(
            "`{type_name}` is not strictly positive: `{occurrence}` reaches a \
             forbidden position in {position}{through}"
        )
    }

    /// Message for E0064 (ADR 11.8.26c §2.2).
    ///
    /// The **binder** is named in the primary message rather than left to a
    /// tool, because the tool that would print it — `info type type-encoding` —
    /// is blocked by this very gate, exactly as `doctor check type positivity`
    /// is by E0061. `doctor tool-reachability` carries that pairing so the
    /// wording cannot drift back to "inspect it with …".
    fn format_nested_family_message(&self) -> String {
        let ElabErrorKind::NestedRecursiveFamily {
            type_name,
            binder,
            nested_under,
        } = self
        else {
            return String::new();
        };
        let nesting = nested_under
            .as_ref()
            .map(|generic| format!(" under generic type `{generic}`"))
            .unwrap_or_default();
        format!(
            "cannot match on `{type_name}`: its recursive occurrence is nested{nesting}, \
             so its μ-encoding (binder `{binder}`) does not unfold to a structural type"
        )
    }

    fn format_equality_message(&self) -> String {
        match self {
            ElabErrorKind::ReflExpectedEquality(ty) => {
                format!("`refl` can only be checked against an equality type, but the expected type was `{}`", ty)
            }
            ElabErrorKind::InvalidRefl { left, right } => {
                format!(
                    "`refl` requires both sides to be equal, but found `{}` and `{}`",
                    left, right
                )
            }
            ElabErrorKind::SubstExpectedEquality(ty) => {
                format!(
                    "`subst` proof argument must have an equality type, but found `{}`",
                    ty
                )
            }
            ElabErrorKind::TransEndpointMismatch { left, right } => {
                format!("`trans` endpoint mismatch: first proof ends with `{}`, second starts with `{}`", left, right)
            }
            ElabErrorKind::CongExpectedFunction(ty) => {
                format!(
                    "`cong` first argument must be a function, but found type `{}`",
                    ty
                )
            }
            _ => unreachable!("format_equality_message called with non-equality error"),
        }
    }

    // `format_module_message`: `messages_modules.rs`.
}
