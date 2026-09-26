//! The call graph the checker reasons over, and its strongly connected
//! components.
//!
//! Grouped because the two are only ever used together and only by the phase
//! that runs before any descent reasoning: build the graph, take its SCCs, hand
//! the recursive ones to `descent`. Keeping them beside `size_env`/`descent`
//! made the parent directory read as six peers when it is really two stages.

pub mod occurrence;
pub mod reachability;
pub mod scc;

pub use occurrence::{peel_spine, transparent, OccurrenceGraph};
pub use reachability::{callers_of, invert, reachable_from, unreachable_from};
pub use scc::{is_recursive, tarjan_scc, Adjacency};
