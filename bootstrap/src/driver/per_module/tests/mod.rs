mod parallel;
mod phase_a5;
mod stubs;

use super::stubs::*;
use super::*;
use crate::ast::Item;
use crate::elaborate::{Constructor, TypeDef, TypeDefKind, ValueDef};
use tungsten_core::Type;

fn make_parsed_module(items: Vec<Item>) -> ParsedModule {
    ParsedModule {
        path: std::path::PathBuf::from("test.tg"),
        source_file: crate::ast::SourceFile {
            items,
            span: crate::span::Span::new(0, 0),
        },
        submodules: vec![],
        visibility: crate::ast::Visibility::Public,
    }
}
