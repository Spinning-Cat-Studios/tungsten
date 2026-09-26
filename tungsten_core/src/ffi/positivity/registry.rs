//! The registration protocol's state machine (ADR 18.8.26b D1), as a pure
//! value the FFI layer owns one of.
//!
//! The externs in the parent module are thin wrappers over these methods, so
//! the protocol — including every way it can be misused — is assertable
//! without a `.tg` compiler, an arena or a C string.
//!
//! **The grammar has no `def_end`.** A definition is closed by the next
//! `def_begin` or by `check`:
//!
//! ```text
//! reset ( def_begin add_param* ( ctor_begin add_field* )* set_alias_body? )* check
//! ```
//!
//! Every departure from it is an [`ProtocolError`], never a silent skip. A
//! dropped field is a definition that passes positivity *because the offending
//! occurrence never arrived* — a false accept produced by the marshalling
//! rather than by the rule, which is the one failure mode neither compiler
//! could detect on its own.

use std::collections::{BTreeMap, BTreeSet};

use crate::types::positivity::{
    analyze_positivity, FieldRef, PositivityCtor, PositivityDef, PositivityDefs,
    PositivityViolation,
};
use crate::types::Type;

use super::wire::{def_kind_from_code, DefKind, ProtocolError};

/// The constructor currently being filled.
#[derive(Debug)]
struct PendingCtor {
    name: String,
    fields: Vec<(FieldRef, Type)>,
}

/// The definition currently being filled.
#[derive(Debug)]
struct PendingDef {
    name: String,
    kind: DefKind,
    params: Vec<String>,
    ctors: Vec<PositivityCtor>,
    ctor: Option<PendingCtor>,
    alias_body: Option<Type>,
}

/// The ambient definition environment the mirror registers into (D6).
///
/// Ambient rather than handle-addressed because [`PositivityDefs`] is not an
/// arena object — it is a map of owned `String`s, and handing it back as a
/// handle would mean arena-allocating a structure nothing else in the arena
/// reads. That is safe only while self-hosted elaboration of a collection pass
/// is single-threaded and registration is not re-entrant (R2).
#[derive(Debug, Default)]
pub struct PositivityRegistry {
    defs: BTreeMap<String, PositivityDef>,
    aliases: BTreeMap<String, (Vec<String>, Type)>,
    stubs: BTreeSet<String>,
    pending: Option<PendingDef>,
    violations: Vec<PositivityViolation>,
}

impl PositivityRegistry {
    /// Start a fresh environment. Also clears the previous pass's violations,
    /// so a reader between two passes cannot serve a stale answer.
    pub fn reset(&mut self) {
        *self = PositivityRegistry::default();
    }

    /// Open a definition, closing any predecessor (the grammar's implicit
    /// close).
    ///
    /// # Errors
    /// [`ProtocolError::UnknownKind`] when `kind` is outside D1's enum.
    pub fn def_begin(&mut self, name: &str, kind: u64) -> Result<(), ProtocolError> {
        let kind = def_kind_from_code(kind).ok_or(ProtocolError::UnknownKind(kind))?;
        self.close_pending();
        self.pending = Some(PendingDef {
            name: name.to_string(),
            kind,
            params: Vec::new(),
            ctors: Vec::new(),
            ctor: None,
            alias_body: None,
        });
        Ok(())
    }

    /// Add one type parameter, in declaration order.
    ///
    /// # Errors
    /// No open definition, or a constructor has already been opened.
    pub fn add_param(&mut self, name: &str) -> Result<(), ProtocolError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or(ProtocolError::NoCurrentDefinition("add_param"))?;
        if !pending.ctors.is_empty() || pending.ctor.is_some() {
            return Err(ProtocolError::ParamAfterConstructor);
        }
        pending.params.push(name.to_string());
        Ok(())
    }

    /// Open a constructor on the current definition, closing any predecessor.
    ///
    /// # Errors
    /// No open definition, or the definition is an alias or a stub.
    pub fn ctor_begin(&mut self, name: &str) -> Result<(), ProtocolError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or(ProtocolError::NoCurrentDefinition("ctor_begin"))?;
        if matches!(pending.kind, DefKind::Alias | DefKind::Stub) {
            return Err(ProtocolError::ConstructorOnNonAdt);
        }
        close_pending_ctor(pending);
        pending.ctor = Some(PendingCtor {
            name: name.to_string(),
            fields: Vec::new(),
        });
        Ok(())
    }

    /// Add one field. `name` is `None` for a positional ADT field and `Some`
    /// for a named record field.
    ///
    /// # Errors
    /// No open constructor.
    pub fn add_field(&mut self, name: Option<&str>, ty: Type) -> Result<(), ProtocolError> {
        let ctor = self
            .pending
            .as_mut()
            .and_then(|pending| pending.ctor.as_mut())
            .ok_or(ProtocolError::NoCurrentConstructor)?;
        let field = match name {
            Some(name) => FieldRef::Named(name.to_string()),
            None => FieldRef::Index(ctor.fields.len()),
        };
        ctor.fields.push((field, ty));
        Ok(())
    }

    /// Set the current alias's body.
    ///
    /// # Errors
    /// No open definition, or the definition is not an alias.
    pub fn set_alias_body(&mut self, ty: Type) -> Result<(), ProtocolError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or(ProtocolError::NoCurrentDefinition("set_alias_body"))?;
        if pending.kind != DefKind::Alias {
            return Err(ProtocolError::AliasBodyOnNonAlias);
        }
        pending.alias_body = Some(ty);
        Ok(())
    }

    /// Close the stream and run the analysis. Returns the violation count.
    ///
    /// The implicit close happens here too: the **last** definition in the
    /// stream has no successor `def_begin` to close it, and losing it would
    /// silently shrink the corpus.
    pub fn check(&mut self) -> usize {
        self.close_pending();
        let defs = PositivityDefs::new(
            std::mem::take(&mut self.defs),
            &self.aliases,
            self.stubs.clone(),
        );
        self.violations = analyze_positivity(&defs).violations;
        // `PositivityDefs::new` consumed the map; put the definitions back so a
        // second `check` in the same pass is not silently empty.
        self.defs = defs
            .iter()
            .map(|(name, def)| (name.clone(), def.clone()))
            .collect();
        self.violations.len()
    }

    /// How many violations the last [`check`](Self::check) found.
    #[must_use]
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }

    /// The `i`th violation as the wire format [`render_violation`] defines.
    #[must_use]
    pub fn violation_at(&self, index: usize) -> Option<&PositivityViolation> {
        self.violations.get(index)
    }

    /// Fold the open definition into the environment, if there is one.
    fn close_pending(&mut self) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        close_pending_ctor(&mut pending);
        match pending.kind {
            DefKind::Alias => {
                // An alias with no body registered is not an alias the engine
                // can inline; dropping it is right, because leaving a
                // half-built entry would expand references to `Type::Error`.
                if let Some(body) = pending.alias_body {
                    self.aliases.insert(pending.name, (pending.params, body));
                }
            }
            DefKind::Stub => {
                self.stubs.insert(pending.name);
            }
            DefKind::Adt | DefKind::Record => {
                self.defs.insert(
                    pending.name,
                    PositivityDef {
                        params: pending.params,
                        ctors: pending.ctors,
                        is_record: pending.kind == DefKind::Record,
                    },
                );
            }
        }
    }
}

/// Fold the open constructor into its definition, if there is one.
fn close_pending_ctor(pending: &mut PendingDef) {
    if let Some(ctor) = pending.ctor.take() {
        pending.ctors.push(PositivityCtor {
            name: ctor.name,
            fields: ctor.fields,
        });
    }
}
