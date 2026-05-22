//! Tests for single-owner monomorphization infrastructure.
//!
//! Split into submodules for maintainability:
//! - `data_model`: DefId, MonoKey, MonoRequestTable, MonoOwnershipMap
//! - `pipeline`: Discovery, ownership assignment, symbols, depot
//!   - `pipeline::discovery`: Mono request discovery from term trees
//!   - `pipeline::ownership`: Depot routing, determinism, parity
//!   - `pipeline::symbols`: Symbol mangling and validation

mod data_model;
mod pipeline;
