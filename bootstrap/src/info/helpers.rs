//! Formatting and encoding helpers for info commands.

use tungsten_bootstrap::elaborate::TypeProvenance;
use tungsten_core::types::Type;

/// Format a type for short display (truncate very long types).
pub fn format_type_short(ty: &Type) -> String {
    let s = format!("{ty}");
    if s.len() > 60 {
        format!("{}...", &s[..57])
    } else {
        s
    }
}

/// Format a type using semantic ADT names from provenance where available.
///
/// Returns `Some("List<String> -> ...")` when μ-binders have provenance,
/// or `None` if no semantic info was found.
pub fn format_semantic_type(ty: &Type, provenance: &TypeProvenance) -> Option<String> {
    fn fmt(ty: &Type, provenance: &TypeProvenance) -> (String, bool) {
        match ty {
            Type::Mu(binder, _body) => {
                if let Some(origin) = provenance.mu_origins.get(binder) {
                    let sem = if origin.type_args.is_empty() {
                        origin.adt_name.clone()
                    } else {
                        let args: Vec<String> = origin
                            .type_args
                            .iter()
                            .map(|a| {
                                let (s, _) = fmt(a, provenance);
                                s
                            })
                            .collect();
                        format!("{}<{}>", origin.adt_name, args.join(", "))
                    };
                    (sem, true)
                } else {
                    (format!("{ty}"), false)
                }
            }
            Type::Arrow(t1, t2) => {
                let (s1, h1) = fmt(t1, provenance);
                let (s2, h2) = fmt(t2, provenance);
                let left = if matches!(t1.as_ref(), Type::Arrow(_, _)) {
                    format!("({s1})")
                } else {
                    s1
                };
                (format!("{left} -> {s2}"), h1 || h2)
            }
            Type::Product(t1, t2) => {
                let (s1, h1) = fmt(t1, provenance);
                let (s2, h2) = fmt(t2, provenance);
                (format!("({s1} × {s2})"), h1 || h2)
            }
            Type::Sum(t1, t2) => {
                let (s1, h1) = fmt(t1, provenance);
                let (s2, h2) = fmt(t2, provenance);
                (format!("({s1} + {s2})"), h1 || h2)
            }
            _ => (format!("{ty}"), false),
        }
    }

    let (result, has_semantic) = fmt(ty, provenance);
    if has_semantic {
        Some(result)
    } else {
        None
    }
}

/// Describe a constructor's fields as an encoded type string.
pub fn encode_ctor_fields(ctor: &tungsten_bootstrap::elaborate::Constructor) -> String {
    if ctor.fields.is_empty() {
        "Unit".to_string()
    } else if ctor.fields.len() == 1 {
        format_type_short(&ctor.fields[0])
    } else {
        ctor.fields
            .iter()
            .map(format_type_short)
            .collect::<Vec<_>>()
            .join(" × ")
    }
}

/// Build a description of the encoding body for an ADT.
pub fn encode_body_description(
    constructors: &[tungsten_bootstrap::elaborate::Constructor],
    _mu_var: &str,
) -> String {
    match constructors.len() {
        0 => "Void".to_string(),
        1 => encode_ctor_fields(&constructors[0]),
        _ => {
            let parts: Vec<String> = constructors.iter().map(encode_ctor_fields).collect();
            parts.join(" + ")
        }
    }
}
