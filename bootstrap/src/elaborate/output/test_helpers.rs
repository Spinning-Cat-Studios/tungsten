use super::{ElabOutput, TypeProvenance};

impl ElabOutput {
    /// Minimal empty output for tests. All collections empty.
    pub fn test_empty() -> Self {
        Self {
            defs: vec![],
            warnings: vec![],
            record_types: std::collections::HashMap::new(),
            adt_types: std::collections::HashMap::new(),
            type_aliases: std::collections::HashMap::new(),
            type_provenance: TypeProvenance::default(),
            encoded_types: std::collections::HashMap::new(),
            mutual_recursion_groups: std::collections::HashMap::new(),
            type_visibilities: std::collections::HashMap::new(),
            record_field_visibilities: std::collections::HashMap::new(),
        }
    }
}
