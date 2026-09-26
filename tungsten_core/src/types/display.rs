//! Display formatting for types.
//!
//! Contains the `Display` trait implementation for `Type` and the
//! `display_detailed` method for debugging type mismatches.

use std::fmt;

use super::Type;

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Primitive types display as their source-level name (ADR 18.9.26f)
        if let Some(name) = self.primitive_name() {
            return write!(f, "{name}");
        }
        // Binary type constructors: (t1 OP t2)
        if let Some((t1, t2, op)) = self.fmt_binary_type_op() {
            return write!(f, "({t1} {op} {t2})");
        }
        match self {
            Type::TyVar(v) => {
                // Strip @-prefix from named types for display (ADR 13.4.26c §2)
                let display_name = v.strip_prefix('@').unwrap_or(v);
                write!(f, "{display_name}")
            }
            // Binding forms: ∀v. body / μv. body
            Type::Forall(v, body) => write!(f, "∀{v}. {body}"),
            Type::Mu(v, body) => write!(f, "μ{v}. {body}"),
            Type::Eq(ty, t1, t2) => write!(f, "Eq {ty} {t1} {t2}"),
            Type::Ptr(inner) | Type::Ref(inner) => {
                let name = if matches!(self, Type::Ptr(_)) {
                    "Ptr"
                } else {
                    "Ref"
                };
                write!(f, "{name}<{inner}>")
            }
            Type::App(name, args) => fmt_type_app(f, name, args),
            Type::Adt(name, type_args, variants) => fmt_type_adt(f, name, type_args, variants),
            Type::Error => write!(f, "<type error>"),
            // Handled by helpers above
            Type::Bool
            | Type::Nat
            | Type::Int
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Arrow(..)
            | Type::Product(..)
            | Type::Sum(..) => unreachable!(),
        }
    }
}

/// Format `Name<arg1, arg2, ...>`.
fn fmt_type_app(f: &mut fmt::Formatter<'_>, name: &str, args: &[Type]) -> fmt::Result {
    write!(f, "{name}<")?;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{arg}")?;
    }
    write!(f, ">")
}

/// Format `Name[<type_args> Ctor1(payload) | Ctor2 | ...]`.
fn fmt_type_adt(
    f: &mut fmt::Formatter<'_>,
    name: &str,
    type_args: &[Type],
    variants: &[(String, Type)],
) -> fmt::Result {
    write!(f, "{name}[")?;
    if !type_args.is_empty() {
        write!(f, "<")?;
        for (i, arg) in type_args.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{arg}")?;
        }
        write!(f, ">")?;
    }
    for (i, (ctor, payload)) in variants.iter().enumerate() {
        if i > 0 {
            write!(f, " | ")?;
        }
        if *payload == Type::Unit {
            write!(f, "{ctor}")?;
        } else {
            write!(f, "{ctor}({payload})")?;
        }
    }
    write!(f, "]")
}

impl Type {
    /// Identify binary type constructors and return (lhs, rhs, operator string).
    fn fmt_binary_type_op(&self) -> Option<(&Type, &Type, &str)> {
        match self {
            Type::Arrow(a, b) => Some((a, b, "→")),
            Type::Product(a, b) => Some((a, b, "×")),
            Type::Sum(a, b) => Some((a, b, "+")),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 1 Diagnostics: Detailed type display
// ─────────────────────────────────────────────────────────────────────────────

impl Type {
    /// Display the type in detailed form showing full structure.
    /// Useful for debugging type mismatches.
    ///
    /// Unbounded — a large type produces a large string. Callers rendering into
    /// a *diagnostic* should prefer [`Type::display_detailed_to_depth`], which
    /// bounds the output (ADR 21.7.26f).
    #[must_use]
    pub fn display_detailed(&self) -> String {
        self.display_detailed_to_depth(usize::MAX)
    }

    /// Display the type in detailed form, elided past `max_depth` nesting levels.
    ///
    /// Structure below the cut-off renders as `…`. This exists because
    /// [`Type::display_detailed`] is unbounded, and the stored encodings it is
    /// most useful on (recursive ADTs, constructor tables) are exactly the ones
    /// large enough to swamp a diagnostic. Unlike the user-facing
    /// `format_type_for_display`, this keeps the *structural* spelling
    /// (`App(...)` vs `Adt(...)`) — which is the whole point when two types
    /// render identically but differ structurally.
    #[must_use]
    pub fn display_detailed_to_depth(&self, max_depth: usize) -> String {
        if max_depth == 0 {
            return "…".to_string();
        }
        let inner = max_depth - 1;

        // Primitive types: display name matches the type name
        if let Some(name) = self.primitive_name() {
            return name.to_string();
        }
        // Binary type constructors share the same format
        if let Some((name, t1, t2)) = self.detailed_binary_label() {
            return format!(
                "{name}({}, {})",
                t1.display_detailed_to_depth(inner),
                t2.display_detailed_to_depth(inner)
            );
        }
        match self {
            Type::TyVar(v) => format!("TyVar({v})"),
            Type::Forall(v, body) => {
                format!("Forall({}, {})", v, body.display_detailed_to_depth(inner))
            }
            Type::Eq(ty, t1, t2) => {
                format!(
                    "Eq({}, {}, {})",
                    ty.display_detailed_to_depth(inner),
                    t1,
                    t2
                )
            }
            Type::Mu(v, body) => {
                format!("Mu({}, {})", v, body.display_detailed_to_depth(inner))
            }
            Type::Ptr(inner_ty) | Type::Ref(inner_ty) => {
                let name = if matches!(self, Type::Ptr(_)) {
                    "Ptr"
                } else {
                    "Ref"
                };
                format!("{name}({})", inner_ty.display_detailed_to_depth(inner))
            }
            Type::App(name, args) => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| a.display_detailed_to_depth(inner))
                    .collect();
                format!("App({}, [{}])", name, arg_strs.join(", "))
            }
            Type::Adt(name, type_args, variants) => {
                let arg_strs: Vec<String> = type_args
                    .iter()
                    .map(|a| a.display_detailed_to_depth(inner))
                    .collect();
                let var_strs: Vec<String> = variants
                    .iter()
                    .map(|(ctor, payload)| {
                        format!("({}, {})", ctor, payload.display_detailed_to_depth(inner))
                    })
                    .collect();
                format!(
                    "Adt({}, [{}], [{}])",
                    name,
                    arg_strs.join(", "),
                    var_strs.join(", ")
                )
            }
            Type::Error => "Error".to_string(),
            // Handled by helpers above
            Type::Bool
            | Type::Nat
            | Type::Int
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Arrow(..)
            | Type::Product(..)
            | Type::Sum(..) => unreachable!(),
        }
    }

    /// Identify binary type constructors for display_detailed format.
    fn detailed_binary_label(&self) -> Option<(&str, &Type, &Type)> {
        match self {
            Type::Arrow(a, b) => Some(("Arrow", a, b)),
            Type::Product(a, b) => Some(("Product", a, b)),
            Type::Sum(a, b) => Some(("Sum", a, b)),
            _ => None,
        }
    }
}
