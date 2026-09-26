//! The poison model's producers and consumers (ADR 14.8.26g, 15.8.26d):
//! the collection pass's conditional deferral and its two producers, and the
//! construction/destruction boundaries that pass the poison through.
//!
//! Grouped into a subdirectory by 15.8.26d — a second file on the subject
//! took `tests/` over the directory cap.

mod collection_deferral;
mod construction_site_must_fail;
mod construction_site_poison;
