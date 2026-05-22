//! Pipeline tests: discovery, ownership assignment, symbols, prelude, determinism.
//!
//! Split into submodules for maintainability:
//! - `discovery`: Mono request discovery from term trees
//! - `ownership`: Owner assignment, prelude, determinism, signature parity
//! - `symbols`: Symbol mangling and validation

use std::path::PathBuf;

use tungsten_bootstrap::elaborate::CoreDef;
use tungsten_bootstrap::Span;
use tungsten_core::terms::{SpannedTerm, Term};
use tungsten_core::types::Type;

use tungsten_bootstrap::driver::ModuleCodegenUnit;

use crate::compile::mono::*;

mod discovery;
mod discovery_filters;
mod ownership;
mod symbols;

/// Helper to create a MonoRequest with type_arg.
pub(super) fn mono_request(key: MonoKey, requester: &str, type_arg: Type) -> MonoRequest {
    MonoRequest {
        key,
        requester_unit: CodegenUnitId(requester.into()),
        type_args: vec![type_arg],
    }
}

pub(super) fn span() -> Span {
    Span::new(0, 0)
}

pub(super) fn spanned(term: Term) -> SpannedTerm {
    SpannedTerm { term, span: None }
}

pub(super) fn make_def(name: &str, term: Term, ty: Type) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty,
        term: spanned(term),
        span: span(),
    }
}

pub(super) fn make_unit(
    module_path: &[&str],
    source: &str,
    defs: Vec<CoreDef>,
) -> ModuleCodegenUnit {
    ModuleCodegenUnit {
        module_path: module_path.iter().map(|s| s.to_string()).collect(),
        source_file: PathBuf::from(source),
        defs,
    }
}
