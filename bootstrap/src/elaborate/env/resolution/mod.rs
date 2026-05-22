//! Path resolution, canonical lookup, and visibility checking for Env.
//!
//! - `resolve`: Qualified path resolution for types, values, constructors
//! - `canonical`: Canonical module resolution following re-export chains
//! - `visibility`: Visibility checking for modules and items

mod canonical;
mod module_items;
mod resolve;
mod visibility;

pub use canonical::CanonicalResolutionError;
