//! Revenant engine: archive timestamps, the capture index, and the fidelity classifier.
//!
//! This crate has no network or framework code. Archive access sits behind a
//! later `SnapshotSource` trait. See `.kiro/specs/revenant-core-engine/`.

pub mod annotate;
pub mod autopsy;
pub mod fidelity;
pub mod index;
pub mod package;
pub mod pipeline;
pub mod rewrite;
pub mod time;
