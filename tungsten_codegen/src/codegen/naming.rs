//! Symbol naming and lambda name generation.
//!
//! Methods for generating unique names for lambdas, monomorphized
//! instances, and other generated symbols during codegen.

use super::CodeGen;
use super::SymbolEntry;

impl CodeGen<'_> {
    /// Set a module-level prefix for generated symbol names.
    ///
    /// When compiling multiple modules into separate `.ll` files, each
    /// `CodeGen` instance should have a unique prefix to prevent symbol
    /// collisions for generated names (lambdas, monomorphized instances, fix).
    pub fn set_module_prefix(&mut self, prefix: String) {
        self.naming.module_prefix = Some(prefix);
    }

    /// Generate a unique name.
    pub(crate) fn fresh_name(&mut self, prefix: &str) -> String {
        self.naming.counter += 1;
        if let Some(ref module_prefix) = self.naming.module_prefix {
            format!("{}_{}_{}", prefix, module_prefix, self.naming.counter)
        } else {
            format!("{}_{}", prefix, self.naming.counter)
        }
    }

    /// Generate a unique lambda function name.
    ///
    /// When `named_lambdas` is enabled and a binding name is available,
    /// uses the source-level name (module-qualified to avoid collisions).
    /// Otherwise, falls back to `__lambda_N` (or `__<prefix>_lambda_N` if
    /// a module prefix is set).
    pub(crate) fn fresh_lambda_name(&mut self) -> String {
        self.naming.lambda_counter += 1;
        let ir_name = if self.naming.named_lambdas {
            if let Some(ref binding) = self.naming.current_binding_name {
                // Check for collision — if name already exists, add suffix
                let candidate = if let Some(ref prefix) = self.naming.module_prefix {
                    format!("{prefix}__{binding}")
                } else {
                    binding.clone()
                };
                if self.module.get_function(&candidate).is_some() {
                    format!("{}_{}", candidate, self.naming.lambda_counter)
                } else {
                    candidate
                }
            } else {
                self.make_lambda_name()
            }
        } else {
            self.make_lambda_name()
        };

        // Record in symbol map
        self.naming.symbol_map.push(SymbolEntry {
            ir_name: ir_name.clone(),
            source_name: self.naming.current_binding_name.clone(),
            file: None,
            line: None,
        });

        ir_name
    }

    /// Build a lambda name with optional module prefix.
    fn make_lambda_name(&self) -> String {
        if let Some(ref prefix) = self.naming.module_prefix {
            format!("__{}_lambda_{}", prefix, self.naming.lambda_counter)
        } else {
            format!("__lambda_{}", self.naming.lambda_counter)
        }
    }

    /// Get the symbol map (IR name → source name mapping).
    pub fn symbol_map(&self) -> &[SymbolEntry] {
        &self.naming.symbol_map
    }

    /// Enable named lambda mode (source-level names in IR).
    pub fn set_named_lambdas(&mut self, enabled: bool) {
        self.naming.named_lambdas = enabled;
    }
}
