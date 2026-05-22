//! Display formatting for types.
//!
//! Contains the `Display` trait implementation for `Type` and the
//! `display_detailed` method for debugging type mismatches.

use std::fmt;

use super::Type;

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Base types with fixed display names
        if let Some(name) = self.base_type_name() {
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
    /// Base types with fixed display names.
    pub(super) fn base_type_name(&self) -> Option<&str> {
        match self {
            Type::Bool => Some("Bool"),
            Type::Nat => Some("Nat"),
            Type::Unit => Some("Unit"),
            Type::Void => Some("Void"),
            Type::Prop => Some("Prop"),
            Type::String => Some("String"),
            _ => None,
        }
    }

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
    #[must_use]
    pub fn display_detailed(&self) -> String {
        // Base types: display name matches the type name
        if let Some(name) = self.base_type_name() {
            return name.to_string();
        }
        // Binary type constructors share the same format
        if let Some((name, t1, t2)) = self.detailed_binary_label() {
            return format!(
                "{name}({}, {})",
                t1.display_detailed(),
                t2.display_detailed()
            );
        }
        match self {
            Type::TyVar(v) => format!("TyVar({v})"),
            Type::Forall(v, body) => {
                format!("Forall({}, {})", v, body.display_detailed())
            }
            Type::Eq(ty, t1, t2) => {
                format!("Eq({}, {}, {})", ty.display_detailed(), t1, t2)
            }
            Type::Mu(v, body) => {
                format!("Mu({}, {})", v, body.display_detailed())
            }
            Type::Ptr(inner) | Type::Ref(inner) => {
                let name = if matches!(self, Type::Ptr(_)) {
                    "Ptr"
                } else {
                    "Ref"
                };
                format!("{name}({})", inner.display_detailed())
            }
            Type::App(name, args) => {
                let arg_strs: Vec<String> = args.iter().map(Type::display_detailed).collect();
                format!("App({}, [{}])", name, arg_strs.join(", "))
            }
            Type::Adt(name, type_args, variants) => {
                let arg_strs: Vec<String> = type_args.iter().map(Type::display_detailed).collect();
                let var_strs: Vec<String> = variants
                    .iter()
                    .map(|(ctor, payload)| format!("({}, {})", ctor, payload.display_detailed()))
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
