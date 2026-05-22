//! FFI type operations.
//!
//! - `constructors`: Type construction functions (tg_type_nat, tg_type_arrow, etc.)
//! - `predicates`: Type predicate functions (tg_type_is_mu, tg_type_is_sum, etc.)
//! - `accessors`: Type accessors, substitution, and debug (tg_type_get_*, tg_type_substitute, etc.)

pub(super) mod accessors;
pub(super) mod accessors_introspection;
pub(super) mod constructors;
pub(super) mod predicates;
