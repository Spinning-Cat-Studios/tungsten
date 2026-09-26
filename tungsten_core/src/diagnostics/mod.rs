//! Diagnostic instrumentation support (ADR 8.7.26a).
//!
//! Pure, LLVM-free logic behind the codegen unit-cost census and the
//! stored-type-size metrics:
//!
//! - [`alloc_counter`] — thread-local allocation-volume counter; the bootstrap
//!   binary registers [`alloc_counter::CountingAllocator`] as its global
//!   allocator so per-unit allocated-bytes deltas can be attributed to the
//!   worker thread that compiled the unit.
//! - [`unit_cost`] — census records, threshold parsing, ranking, rendering
//!   (table / JSON / `TUNGSTEN_CODEGEN_SERIAL_UNITS` list), and the gate
//!   verdict for `tungsten doctor check unit-cost`.
//! - [`type_size`] — stored-`Type`-tree metrics (node count, depth, μ-binder
//!   chain, per-binder α-occurrence counts) for `tungsten info type size`.
//! - [`term_shape`] — bounded `Term` node-count + depth-limited shape rendering
//!   for the `tungsten info eval trace` step tracer (ADR 21.7.26j).
//!
//! These live in `tungsten_core` rather than next to their codegen-side glue
//! (`bootstrap/src/compile/unit_cost/`) so the pure ranking/formatting logic
//! sits in the coverage + mutation gate scope (ADR 7.7.26j applies to the
//! LLVM-free crates only — ADR 8.7.26a AC 3).

pub mod alloc_counter;
pub mod term_shape;
pub mod type_size;
pub mod unit_cost;
