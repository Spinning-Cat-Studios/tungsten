//! Type definition management methods for Env.
//!
//! Handles registration, lookup, and iteration of type definitions
//! (ADTs, type aliases, stubs) including canonical cross-module lookup.

use super::definitions::{ConstructorInfo, TypeDef, TypeDefKind};
use super::Env;
use super::{ModuleContents, ModulePath};
use tungsten_core::Type;

impl Env {
    // ─────────────────────────────────────────────────────────────────────────
    // Type definitions
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a type name as a stub (for Phase 1a).
    ///
    /// This makes the type name available for import resolution before
    /// the type body is fully elaborated. The stub will be replaced
    /// by the full definition in Phase 1c.
    ///
    /// The `params` argument captures the type parameter names so that
    /// forward references to generic types (e.g., `Forest<T>` referenced
    /// before `Forest` is elaborated) have the correct arity.
    pub fn register_type_stub(
        &mut self,
        name: &str,
        params: Vec<String>,
        visibility: crate::ast::Visibility,
        span: crate::span::Span,
    ) {
        // Create a placeholder type definition with correct arity
        // The kind doesn't matter since it will be replaced
        let stub = TypeDef {
            name: name.to_string(),
            params,
            kind: TypeDefKind::Stub,
            visibility,
            span,
            defining_module: None, // Local stub, will be replaced with real def
            encoded_type: None,
            field_visibilities: Vec::new(),
        };
        self.types.insert(name.to_string(), stub);
    }

    /// Define a new type (with optional module context).
    pub fn define_type_in_module(&mut self, def: TypeDef, module: ModulePath) {
        let name = def.name.clone();

        // Register constructors if this is an ADT
        if let TypeDefKind::ADT(ref ctors) = def.kind {
            for ctor in ctors {
                if self.trace_ctor_registration {
                    eprintln!(
                        "[ctor-reg] register {} (parent={}, index={}) via define_type_in_module",
                        ctor.name, def.name, ctor.index
                    );
                }
                self.constructors.insert(
                    ctor.name.clone(),
                    ConstructorInfo {
                        type_name: def.name.clone(),
                        index: ctor.index,
                        arity: ctor.fields.len(),
                        visibility: ctor.visibility,
                        defining_module: Some(module.clone()),
                    },
                );
                // Track constructor's module
                self.item_modules.insert(ctor.name.clone(), module.clone());
                if let Some(contents) = self.modules.get_mut(&module) {
                    contents.constructors.push(ctor.name.clone());
                }
            }
        }

        // Track type's module
        self.item_modules.insert(name.clone(), module.clone());
        if let Some(contents) = self.modules.get_mut(&module) {
            contents.types.push(name.clone());
        }

        self.types.insert(name, def);
    }

    /// Define a new type.
    pub fn define_type(&mut self, def: TypeDef) {
        // Register constructors if this is an ADT
        if let TypeDefKind::ADT(ref ctors) = def.kind {
            for ctor in ctors {
                if self.trace_ctor_registration {
                    eprintln!(
                        "[ctor-reg] register {} (parent={}, index={}) via define_type",
                        ctor.name, def.name, ctor.index
                    );
                }
                self.constructors.insert(
                    ctor.name.clone(),
                    ConstructorInfo {
                        type_name: def.name.clone(),
                        index: ctor.index,
                        arity: ctor.fields.len(),
                        visibility: ctor.visibility,
                        defining_module: None, // No module context in simple define_type
                    },
                );
            }
        }
        self.types.insert(def.name.clone(), def);
    }

    /// Look up a type definition by name.
    pub fn lookup_type(&self, name: &str) -> Option<&TypeDef> {
        self.types.get(name)
    }

    /// Look up a type definition by name, following canonical module references.
    ///
    /// This is the ADR 31 canonical lookup that handles cross-module generic types.
    /// If the type is a stub with a defining_module, looks up the real definition
    /// from that module's types.
    ///
    /// For pattern matching on imported ADTs, use this instead of `lookup_type`.
    pub fn lookup_type_canonical(&self, name: &str) -> Option<&TypeDef> {
        let typedef = self.types.get(name)?;

        // If this is a stub with a canonical defining module, try to find the real def
        if matches!(typedef.kind, TypeDefKind::Stub) {
            if let Some(ref defining_module) = typedef.defining_module {
                // Look in the defining module's contents for the real type
                if let Some(contents) = self.modules.get(defining_module) {
                    // Check if the module actually defines this type (not just imports it)
                    if contents.types.contains(&name.to_string())
                        && !contents.imported_types.contains_key(name)
                    {
                        // The type should be in the global types map under the same name
                        // but might have been elaborated by now
                        if let Some(real_def) = self.types.get(name) {
                            if !matches!(real_def.kind, TypeDefKind::Stub) {
                                return Some(real_def);
                            }
                        }
                    }
                }
            }
        }

        Some(typedef)
    }

    /// Look up a type name by its encoded representation.
    ///
    /// This enables reverse lookup from Core types to user-defined type names
    /// for better error messages. Only works for non-parameterized types that
    /// have their `encoded_type` cached.
    pub fn lookup_type_name_by_encoding(&self, encoded: &Type) -> Option<&str> {
        for (name, def) in &self.types {
            if let Some(ref cached) = def.encoded_type {
                if cached == encoded {
                    return Some(name);
                }
            }
        }
        None
    }

    /// Iterate over all type definitions.
    pub fn iter_types(&self) -> impl Iterator<Item = (&String, &TypeDef)> {
        self.types.iter()
    }

    /// Check if a type name is defined.
    pub fn has_type(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
}
