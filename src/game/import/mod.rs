//! Importing ITGmania + Simply Love local profiles into DeadSync.
//!
//! Pure readers and translators live in `deadsync-import`. This root module
//! keeps the app-state orchestration that writes local profiles, favorites, and
//! scores.

pub mod run;

pub use deadsync_import::{detect, itg, options, resolver, xml};
