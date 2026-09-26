//! Deterministic mangling of a concrete `Type` to a comparator symbol name
//! (ADR 29.6.26f §T11.2).
//!
//! The same function is used at the `__compare` call site (to name the emitted
//! `Global`) and during synthesis (to name the `CoreDef`), so they always agree.
//! Names are structural and collision-free for the supported type shapes; the
//! exact spelling is internal (never parsed back — the request registry carries
//! the real `Type`).

use tungsten_core::Type;

/// Symbol name of the comparator for `ty`, e.g. `compare_Nat`,
/// `compare_PNat_BoolE` for `(Nat × Bool)`.
#[must_use]
pub fn comparator_symbol(ty: &Type) -> String {
    format!("compare_{}", mangle(ty))
}

fn mangle(ty: &Type) -> String {
    // Primitives mangle to their source name (ADR 18.9.26f).
    if let Some(name) = ty.primitive_name() {
        return name.to_string();
    }
    match ty {
        // Balanced `P…E` / `S…E` delimiters keep nestings unambiguous.
        Type::Product(a, b) => format!("P{}_{}E", mangle(a), mangle(b)),
        Type::Sum(a, b) => format!("S{}_{}E", mangle(a), mangle(b)),
        Type::TyVar(n) => format!("Named{}", sanitize(n)),
        Type::App(n, args) if args.is_empty() => format!("Named{}", sanitize(n)),
        Type::App(n, args) => {
            let inner: Vec<String> = args.iter().map(mangle).collect();
            format!("App{}_{}E", sanitize(n), inner.join("_"))
        }
        // ADTs are identified by name + type args (variants are determined by them).
        Type::Adt(n, args, _) => {
            let inner: Vec<String> = args.iter().map(mangle).collect();
            format!("Adt{}_{}E", sanitize(n), inner.join("_"))
        }
        Type::Mu(v, body) => format!("Mu{}_{}E", sanitize(v), mangle(body)),
        other => format!("Unsupported{}", sanitize(&format!("{other:?}"))),
    }
}

/// Replace non-alphanumerics so the result is a valid LLVM symbol fragment.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_symbols_are_stable() {
        assert_eq!(comparator_symbol(&Type::Nat), "compare_Nat");
        assert_eq!(comparator_symbol(&Type::Bool), "compare_Bool");
        assert_eq!(comparator_symbol(&Type::String), "compare_String");
    }

    #[test]
    fn product_symbol_is_structural() {
        let ty = Type::Product(Box::new(Type::Nat), Box::new(Type::Bool));
        assert_eq!(comparator_symbol(&ty), "compare_PNat_BoolE");
    }

    #[test]
    fn nested_product_is_unambiguous() {
        // (Nat × (Bool × Nat)) vs ((Nat × Bool) × Nat) must differ.
        let left = Type::Product(
            Box::new(Type::Nat),
            Box::new(Type::Product(Box::new(Type::Bool), Box::new(Type::Nat))),
        );
        let right = Type::Product(
            Box::new(Type::Product(Box::new(Type::Nat), Box::new(Type::Bool))),
            Box::new(Type::Nat),
        );
        assert_ne!(comparator_symbol(&left), comparator_symbol(&right));
    }
}
