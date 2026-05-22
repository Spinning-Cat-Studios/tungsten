//! Item definitions for the Tungsten AST — functions, types, theorems, axioms,
//! modules, use statements, and their supporting structures.

use super::{Expr, Ident, Param, Path, Span, Spanned, TypeExpr, TypeParam, Visibility};
use serde::{Deserialize, Serialize};

/// A function definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Function name
    pub name: Ident,
    /// Type parameters (generics)
    pub type_params: Vec<TypeParam>,
    /// Parameters
    pub params: Vec<Param>,
    /// Return type (optional, can be inferred)
    pub return_type: Option<TypeExpr>,
    /// Function body
    pub body: Expr,
    /// Span of the entire definition
    pub span: Span,
}

/// A type definition (ADT or Record).
///
/// **AST** representation (surface syntax, `TypeExpr`/`TypeParam`).
/// See also: `elaborate::env::definitions::TypeDef` (elaboration representation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Type name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// Type body (sum type with variants, or record type with fields)
    pub body: TypeBody,
    /// Span of the entire definition
    pub span: Span,
}

/// The body of a type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeBody {
    /// Sum type with variants: `| A | B(T) | C(T, U)`
    Sum(Vec<Variant>),
    /// Record type with named fields: `{ x: T, y: U }`
    Record(Vec<RecordField>),
}

/// A field in a record type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordField {
    /// Visibility modifier (None = inherit parent type visibility)
    pub visibility: Option<Visibility>,
    /// Field name
    pub name: Ident,
    /// Field type
    pub ty: TypeExpr,
    /// Span of the field
    pub span: Span,
}

/// A type alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAlias {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Alias name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// The aliased type
    pub ty: TypeExpr,
    /// Span of the entire definition
    pub span: Span,
}

/// A variant of an ADT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    /// Visibility modifier (None = inherit parent type visibility)
    pub visibility: Option<Visibility>,
    /// Variant name
    pub name: Ident,
    /// Fields (empty for unit variants)
    pub fields: Vec<Field>,
    /// Span of the variant
    pub span: Span,
}

/// A field in a variant or struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    /// Optional field name (for named fields)
    pub name: Option<Ident>,
    /// Field type
    pub ty: TypeExpr,
    /// Span of the field
    pub span: Span,
}
/// A theorem definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TheoremDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Theorem name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// Parameters (hypotheses)
    pub params: Vec<Param>,
    /// Proposition to prove
    pub prop: TypeExpr,
    /// Proof body
    pub body: Expr,
    /// Span of the entire definition
    pub span: Span,
}
/// An axiom definition (theorem without proof).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiomDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Axiom name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// Parameters
    pub params: Vec<Param>,
    /// Proposition
    pub prop: TypeExpr,
    /// Span of the entire definition
    pub span: Span,
}
/// An external function definition (FFI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternFnDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Function name (Tungsten side)
    pub name: Ident,
    /// Optional C symbol name (if different from Tungsten name)
    pub symbol: Option<String>,
    /// ABI string (e.g., "C")
    pub abi: String,
    /// Parameters with types
    pub params: Vec<ExternParam>,
    /// Return type
    pub return_type: TypeExpr,
    /// Span of the entire definition
    pub span: Span,
}

/// An external function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternParam {
    /// Parameter name
    pub name: Ident,
    /// Parameter type
    pub ty: TypeExpr,
    /// Span
    pub span: Span,
}

/// A module declaration: `mod foo;` or `pub mod foo;`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDecl {
    /// Visibility (public or private)
    pub visibility: Visibility,
    /// Module name (e.g., "token" for `mod token;`)
    pub name: Ident,
    /// Span of the entire declaration
    pub span: Span,
}

/// A use statement: `use foo::bar;` or `use foo::{a, b};`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseDecl {
    /// Visibility (for future `pub use`)
    pub visibility: Visibility,
    /// The use tree (path and optional grouping)
    pub tree: UseTree,
    /// Span of the entire declaration
    pub span: Span,
}

/// The tree structure of a use statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UseTree {
    /// Simple path: `use foo::bar;`
    Path(Path),
    /// Grouped imports: `use foo::{bar, baz};`
    Group {
        /// Prefix path (e.g., `foo` in `use foo::{bar, baz};`)
        prefix: Path,
        /// Items being imported
        items: Vec<UseTree>,
        /// Span of the group
        span: Span,
    },
    /// Glob import: `use foo::*;`
    Glob {
        /// Module path to import from (e.g., `foo` in `use foo::*;`)
        prefix: Path,
        /// Span of the glob (including the `*`)
        span: Span,
    },
    /// Aliased import: `use foo::bar as baz;`
    Alias {
        /// The original path being imported
        path: Path,
        /// The local alias name
        alias: Ident,
        /// Span covering the full `path as alias`
        span: Span,
    },
}

/// Result of expanding a use tree - either concrete paths or a glob import.
#[derive(Debug, Clone)]
pub enum ExpandedUseTree {
    /// Concrete paths that can be individually imported
    Paths(Vec<Path>),
    /// Glob import that needs special handling
    Glob { prefix: Path, span: Span },
    /// Aliased import: resolve `path` but register under `alias`
    Alias {
        path: Path,
        alias: Ident,
        span: Span,
    },
}

impl UseTree {
    /// Expand this tree into paths or glob imports.
    ///
    /// Grouped imports are flattened into individual paths.
    /// Glob imports cannot be statically expanded and are returned as-is.
    ///
    /// NOTE: For groups containing mixed aliases and paths, this only returns
    /// the first non-path item. Use `expand_all()` for complete expansion.
    pub fn expand(&self) -> ExpandedUseTree {
        match self {
            UseTree::Path(path) => ExpandedUseTree::Paths(vec![path.clone()]),
            UseTree::Group { prefix, items, .. } => Self::expand_group(prefix, items),
            UseTree::Glob { prefix, span } => ExpandedUseTree::Glob {
                prefix: prefix.clone(),
                span: *span,
            },
            UseTree::Alias { path, alias, span } => ExpandedUseTree::Alias {
                path: path.clone(),
                alias: alias.clone(),
                span: *span,
            },
        }
    }

    /// Expand this tree into a list of expanded items.
    ///
    /// Unlike `expand()`, this correctly handles groups containing mixed
    /// paths and aliases by returning each as a separate `ExpandedUseTree`.
    pub fn expand_all(&self) -> Vec<ExpandedUseTree> {
        match self {
            UseTree::Path(path) => vec![ExpandedUseTree::Paths(vec![path.clone()])],
            UseTree::Group { prefix, items, .. } => Self::expand_group_all(prefix, items),
            UseTree::Glob { prefix, span } => vec![ExpandedUseTree::Glob {
                prefix: prefix.clone(),
                span: *span,
            }],
            UseTree::Alias { path, alias, span } => vec![ExpandedUseTree::Alias {
                path: path.clone(),
                alias: alias.clone(),
                span: *span,
            }],
        }
    }

    fn expand_group(prefix: &Path, items: &[UseTree]) -> ExpandedUseTree {
        let mut all_paths = Vec::new();
        for item in items {
            match item.expand() {
                ExpandedUseTree::Paths(paths) => {
                    for p in paths {
                        let mut full_segments = prefix.segments.clone();
                        full_segments.extend(p.segments);
                        let span = Span::new(prefix.span.start, p.span.end);
                        all_paths.push(Path {
                            segments: full_segments,
                            span,
                        });
                    }
                }
                ExpandedUseTree::Glob {
                    prefix: glob_prefix,
                    span,
                } => {
                    let mut full_segments = prefix.segments.clone();
                    full_segments.extend(glob_prefix.segments);
                    let full_prefix = Path {
                        segments: full_segments,
                        span: Span::new(prefix.span.start, glob_prefix.span.end),
                    };
                    return ExpandedUseTree::Glob {
                        prefix: full_prefix,
                        span,
                    };
                }
                ExpandedUseTree::Alias {
                    path: alias_path,
                    alias,
                    span,
                } => {
                    let mut full_segments = prefix.segments.clone();
                    full_segments.extend(alias_path.segments);
                    let full_path = Path {
                        segments: full_segments,
                        span: Span::new(prefix.span.start, alias_path.span.end),
                    };
                    return ExpandedUseTree::Alias {
                        path: full_path,
                        alias,
                        span,
                    };
                }
            }
        }
        ExpandedUseTree::Paths(all_paths)
    }

    /// Expand a group into a list of expanded items, correctly handling mixed content.
    fn expand_group_all(prefix: &Path, items: &[UseTree]) -> Vec<ExpandedUseTree> {
        let mut results = Vec::new();
        let mut paths_batch = Vec::new();

        for item in items {
            match item.expand() {
                ExpandedUseTree::Paths(paths) => {
                    for p in paths {
                        let mut full_segments = prefix.segments.clone();
                        full_segments.extend(p.segments);
                        let span = Span::new(prefix.span.start, p.span.end);
                        paths_batch.push(Path {
                            segments: full_segments,
                            span,
                        });
                    }
                }
                ExpandedUseTree::Glob {
                    prefix: glob_prefix,
                    span,
                } => {
                    // Flush accumulated paths
                    if !paths_batch.is_empty() {
                        results.push(ExpandedUseTree::Paths(std::mem::take(&mut paths_batch)));
                    }
                    let mut full_segments = prefix.segments.clone();
                    full_segments.extend(glob_prefix.segments);
                    let full_prefix = Path {
                        segments: full_segments,
                        span: Span::new(prefix.span.start, glob_prefix.span.end),
                    };
                    results.push(ExpandedUseTree::Glob {
                        prefix: full_prefix,
                        span,
                    });
                }
                ExpandedUseTree::Alias {
                    path: alias_path,
                    alias,
                    span,
                } => {
                    // Flush accumulated paths
                    if !paths_batch.is_empty() {
                        results.push(ExpandedUseTree::Paths(std::mem::take(&mut paths_batch)));
                    }
                    let mut full_segments = prefix.segments.clone();
                    full_segments.extend(alias_path.segments);
                    let full_path = Path {
                        segments: full_segments,
                        span: Span::new(prefix.span.start, alias_path.span.end),
                    };
                    results.push(ExpandedUseTree::Alias {
                        path: full_path,
                        alias,
                        span,
                    });
                }
            }
        }

        if !paths_batch.is_empty() {
            results.push(ExpandedUseTree::Paths(paths_batch));
        }

        if results.is_empty() {
            vec![ExpandedUseTree::Paths(Vec::new())]
        } else {
            results
        }
    }
}
