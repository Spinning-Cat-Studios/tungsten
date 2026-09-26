//! Synthesized structural comparators (ADR 29.6.26f, design doc §T11).
//!
//! The structural `assert_eq` comparator is realised under the bootstrap's
//! authoring/synthesis split (design doc §T11.8): the `CompareResult` ADT is
//! *authored* in `.tg` (`src/compiler/driver/ffi/compare/mod.tg`), while the
//! per-type `compare_T` functions are *synthesized* here as ordinary `CoreDef`s
//! and injected into the codegen units before codegen.
//!
//! This lib module holds the codegen-feature-independent pieces — pure `Term`
//! construction ([`terms`]) — so they are unit-testable host-side without LLVM.
//! The pipeline wiring (discovery over codegen units + injection) lives in the
//! codegen-gated `compile/` bin module and calls into here.

#[cfg(feature = "codegen")]
pub mod codegen_hook;
pub mod context;
pub mod discover;
pub mod eval;
mod expand;
pub mod gate;
pub mod symbols;
pub mod synth;
pub mod terms;

pub use context::ComparatorTypes;
// Re-exported at the old paths: the symbol namespace moved into `symbols/`
// for the directory-size limit, and the split is an organisational one that
// callers have no reason to track.
pub use symbols::{mangling, requests};

/// The polymorphic comparator intrinsic symbol (ADR 29.6.26f §T11.2a / P6′).
///
/// `compare(a, b)` emits `App(App(TyApp(Global(COMPARE_INTRINSIC), T), a), b)`.
/// A reference `TyApp(Global(COMPARE_INTRINSIC), ConcreteT)` is resolved at
/// instantiation to the synthesized `compare_T` — by the monomorphizer (codegen)
/// or by lazy synthesis (evaluator). Single source of truth lives in
/// `tungsten_core` (the evaluator needs it too).
pub use tungsten_core::eval::COMPARE_INTRINSIC;
