//! Codegen type extraction helpers.
//!
//! These methods extract type definitions from the elaboration environment
//! in forms needed by the code generator. Records, ADTs, and type aliases
//! each have different downstream representations.

use tungsten_core::Type;

use super::env::{self, TypeDefKind};
use super::Elaborator;

impl<'a> Elaborator<'a> {
    /// Extract record type definitions for codegen.
    ///
    /// Returns a map from record name to its fields.
    /// This is needed because records are kept as nominal types during elaboration
    /// but codegen needs to expand them to structural product types.
    pub fn get_record_types(&self) -> std::collections::HashMap<String, Vec<(String, Type)>> {
        let mut records = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if let TypeDefKind::Record(fields) = &type_def.kind {
                records.insert(name.clone(), fields.clone());
            }
        }
        records
    }

    /// Extract ADT type definitions for codegen.
    ///
    /// Returns a map from ADT name to (params, constructors).
    /// This is needed to expand `Type::App("Name", args)` to sum/mu types in codegen.
    pub fn get_adt_types(
        &self,
    ) -> std::collections::HashMap<String, (Vec<String>, Vec<env::Constructor>)> {
        let mut adts = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if let TypeDefKind::ADT(constructors) = &type_def.kind {
                adts.insert(
                    name.clone(),
                    (type_def.params.clone(), constructors.clone()),
                );
            }
        }
        adts
    }

    /// Extract type alias definitions for `info` commands.
    ///
    /// Returns a map from alias name to (params, target type).
    pub fn get_type_aliases(&self) -> std::collections::HashMap<String, (Vec<String>, Type)> {
        let mut aliases = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if let TypeDefKind::Alias(target) = &type_def.kind {
                aliases.insert(name.clone(), (type_def.params.clone(), target.clone()));
            }
        }
        aliases
    }

    /// Extract cached type encodings (Phase 1e results).
    ///
    /// Returns a map from type name to its cached encoded Type.
    /// Only non-parameterized types with successful encoding are included.
    pub fn get_encoded_types(&self) -> std::collections::HashMap<String, Type> {
        let mut encoded = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if let Some(ref ty) = type_def.encoded_type {
                encoded.insert(name.clone(), ty.clone());
            }
        }
        encoded
    }

    /// Extract mutual recursion groups (Phase 1c.5 SCC results).
    ///
    /// Returns a map from type name to its full SCC group members.
    /// Only populated for types in SCCs of size > 1.
    pub fn get_mutual_recursion_groups(&self) -> std::collections::HashMap<String, Vec<String>> {
        self.mutual_recursion_groups.clone()
    }

    /// Extract parent type visibilities for all types (ADR 14.5.26c).
    ///
    /// Returns a map from type name → declared visibility. Used by
    /// `info type visibility` to compute effective member visibilities.
    pub fn get_type_visibilities(
        &self,
    ) -> std::collections::HashMap<String, crate::ast::Visibility> {
        self.env
            .types
            .iter()
            .map(|(name, td)| (name.clone(), td.visibility))
            .collect()
    }

    /// Extract per-field visibilities for record types (ADR 14.5.26c).
    ///
    /// Returns a map from record name → per-field visibility overrides.
    /// `None` entries mean "inherit parent type visibility".
    pub fn get_record_field_visibilities(
        &self,
    ) -> std::collections::HashMap<String, Vec<Option<crate::ast::Visibility>>> {
        let mut result = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if matches!(type_def.kind, env::TypeDefKind::Record(_)) {
                result.insert(name.clone(), type_def.field_visibilities.clone());
            }
        }
        result
    }
}
