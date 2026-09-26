//! FFI type operations.
//!
//! - `nodes`: handle-children arena representation + import/materialize (ADR 2.7.26a §4)
//! - `node_equality`: α-equivalence directly on the node DAG
//! - `node_substitute`: τ[α := τ'] on the node DAG with structural sharing
//! - `constructors`: Type construction functions (tg_type_nat, tg_type_arrow, etc.)
//! - `predicates`: Type predicate functions (tg_type_is_mu, tg_type_is_sum, etc.)
//! - `accessors`: Type accessors, substitution, and debug (tg_type_get_*, tg_type_substitute, etc.)

pub(crate) mod accessors;
pub(crate) mod accessors_introspection;
pub(crate) mod constructors;
pub(crate) mod node_equality;
pub(crate) mod node_substitute;
pub(crate) mod nodes;
pub(crate) mod predicates;
