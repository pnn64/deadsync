//! DeadSync noteskin parsing and resolution.
//!
//! This crate owns renderer-agnostic noteskin data loading. Root gameplay
//! presentation still owns texture registration and actor construction during
//! the migration.

pub mod actor;
pub mod compiled;
pub mod compiler;
pub mod itg;
