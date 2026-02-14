//! AST schema versioning for cache invalidation.
//!
//! This module provides compile-time structural hashing of AST definitions
//! to automatically invalidate caches when AST types change.

/// Current cache format version - bump on incompatible changes to manifest format.
pub const CACHE_FORMAT_VERSION: u32 = 2;

/// Current IR schema version - bump when CoreDef/Type/Term format changes.
pub const IR_SCHEMA_VERSION: u32 = 1;

/// Computed hash of AST struct definitions.
/// Auto-invalidates cache when AST types change (fields added/removed/reordered, type changes).
/// This is computed from AST_SCHEMA_SIGNATURE below.
pub const AST_SCHEMA_HASH: [u8; 32] = compute_schema_hash(AST_SCHEMA_SIGNATURE);

/// String representation of AST schema for hashing.
/// Update this when AST struct definitions change - the hash will auto-invalidate caches.
/// Format: "StructName{field:Type,...};..." for each serialized struct.
pub const AST_SCHEMA_SIGNATURE: &str = concat!(
    "SourceFile{items:Vec<Item>,span:Span};",
    "FunctionDef{visibility:Visibility,name:Ident,type_params:Vec<TypeParam>,params:Vec<Param>,return_type:Option<TypeExpr>,body:Expr,span:Span};",
    "TypeDef{visibility:Visibility,name:Ident,type_params:Vec<TypeParam>,body:TypeBody,span:Span};",
    "TypeAlias{visibility:Visibility,name:Ident,type_params:Vec<TypeParam>,aliased:TypeExpr,span:Span};",
    "TheoremDef{visibility:Visibility,kind:TheoremKind,name:Ident,type_params:Vec<TypeParam>,params:Vec<Param>,prop:TypeExpr,body:Option<Expr>,span:Span};",
    "AxiomDef{visibility:Visibility,name:Ident,type_params:Vec<TypeParam>,params:Vec<Param>,prop:TypeExpr,span:Span};",
    "ExternFnDef{visibility:Visibility,name:Ident,type_params:Vec<TypeParam>,params:Vec<Param>,return_type:TypeExpr,span:Span};",
    "v3"  // Bump this suffix when changing schema but not struct layouts
);

/// Compute hash of the schema signature at compile time.
pub const fn compute_schema_hash(signature: &str) -> [u8; 32] {
    // Simple compile-time hash using FNV-1a style mixing
    // (SHA-256 isn't const-friendly, so we use a simpler deterministic hash)
    let bytes = signature.as_bytes();
    let mut hash = [0u8; 32];
    let mut i = 0;
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV prime
        i += 1;
    }
    // Spread the 64-bit hash across 32 bytes for compatibility with existing code
    hash[0] = (h >> 56) as u8;
    hash[1] = (h >> 48) as u8;
    hash[2] = (h >> 40) as u8;
    hash[3] = (h >> 32) as u8;
    hash[4] = (h >> 24) as u8;
    hash[5] = (h >> 16) as u8;
    hash[6] = (h >> 8) as u8;
    hash[7] = h as u8;
    // Fill rest with hash variations
    let mut j = 8;
    while j < 32 {
        hash[j] = hash[j - 8] ^ ((j as u8).wrapping_mul(0x9e));
        j += 1;
    }
    hash
}

/// Default max cache size in MB.
pub const DEFAULT_MAX_SIZE_MB: u64 = 500;

/// Compiler version string for cache invalidation.
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");
