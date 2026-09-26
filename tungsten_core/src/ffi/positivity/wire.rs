//! The two wire formats the positivity seam uses (ADR 18.8.26b D1/D6).
//!
//! Both are here rather than beside their callers because they are the parts
//! the `.tg` side has to agree with byte for byte: the `kind` code going in,
//! and the violation text coming out. A drift in either is invisible to every
//! Rust test that does not name the encoding, so the encoding is named once.

use std::fmt::Write as _;

use crate::types::positivity::PositivityViolation;

/// What kind of definition is being registered.
///
/// The wire encoding is a small `Nat`, and **an unrecognised code is a hard
/// error rather than a default**: silently defaulting to `Adt` would turn a
/// record into an ADT and change the answer, in the direction of accepting
/// something the bootstrap rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Adt,
    Record,
    Alias,
    Stub,
}

/// Decode D1's `kind` wire value. `None` is the hard error, not a fallback.
#[must_use]
pub fn def_kind_from_code(code: u64) -> Option<DefKind> {
    match code {
        0 => Some(DefKind::Adt),
        1 => Some(DefKind::Record),
        2 => Some(DefKind::Alias),
        3 => Some(DefKind::Stub),
        _ => None,
    }
}

/// A call that the grammar does not allow at this point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// `kind` was not one of D1's four values.
    UnknownKind(u64),
    /// A call that needs an open definition arrived without one.
    NoCurrentDefinition(&'static str),
    /// `add_field` with no open constructor.
    NoCurrentConstructor,
    /// `add_param` after the first `ctor_begin`: parameters are declaration
    /// order, and one arriving late would be indexed against the wrong
    /// occurrence set.
    ParamAfterConstructor,
    /// `ctor_begin` on an alias or a stub, neither of which has constructors.
    ConstructorOnNonAdt,
    /// `set_alias_body` on something that is not an alias.
    AliasBodyOnNonAlias,
    /// `add_field` with a field name that is neither the `INVALID_HANDLE`
    /// sentinel nor a readable C string — a null pointer most of all, since
    /// `0` is a valid arena index and must never read as "unnamed".
    UnreadableFieldName,
}

impl ProtocolError {
    /// The message the FFI stores for `tg_get_last_error`.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            ProtocolError::UnknownKind(code) => {
                format!("positivity: unknown definition kind {code} (expected 0..=3)")
            }
            ProtocolError::NoCurrentDefinition(call) => {
                format!("positivity: `{call}` with no definition open")
            }
            ProtocolError::NoCurrentConstructor => {
                "positivity: `add_field` with no constructor open".to_string()
            }
            ProtocolError::ParamAfterConstructor => {
                "positivity: `add_param` after the definition's first constructor".to_string()
            }
            ProtocolError::ConstructorOnNonAdt => {
                "positivity: `ctor_begin` on an alias or stub".to_string()
            }
            ProtocolError::AliasBodyOnNonAlias => {
                "positivity: `set_alias_body` on a non-alias definition".to_string()
            }
            ProtocolError::UnreadableFieldName => {
                "positivity: `add_field` field name is neither INVALID_HANDLE nor a \
                 readable C string"
                    .to_string()
            }
        }
    }
}

/// One violation as the two-part wire format `tg_positivity_violation_render`
/// hands across the seam: the **type name**, a newline, then the message.
///
/// The name travels beside the message rather than only inside it because the
/// caller needs it as *data* — it is how the self-hosted elaborator finds the
/// span to report at, and parsing it back out of an English sentence would be
/// a second thing that can disagree with the first.
#[must_use]
pub fn render_violation(violation: &PositivityViolation) -> String {
    let position = if violation.is_record {
        format!("record `{}`, {}", violation.type_name, violation.field)
    } else {
        format!("constructor `{}`, {}", violation.ctor_name, violation.field)
    };
    let mut through = String::new();
    for link in &violation.via {
        // Writing into a String is infallible, so the result is dropped.
        let _ = write!(
            through,
            " through `{}`'s parameter `{}`",
            link.type_name, link.param
        );
    }
    format!(
        "{}\n`{}` is not strictly positive: `{}` reaches a forbidden position in {position}{through}",
        violation.type_name, violation.type_name, violation.occurrence
    )
}
