//! Retained text for the Ctrl+F setting search overlays.
//!
//! Player Options and Options rank their own rows with [`super::fuzzy`]; these
//! helpers cache the overlay's query, result and detail text so typing only
//! allocates when the visible text changes.

pub mod frame;
pub mod ranking;
pub mod text;
