//! Importing ITGmania + Simply Love local profiles into DeadSync.
//!
//! The importer reads an ITGmania `LocalProfiles/<id>/` directory and creates a
//! brand-new DeadSync local profile from it: profile metadata, online keys,
//! avatar, Simply Love player options, and the full offline high-score history.
//!
//! Submodules:
//! * [`xml`] — a tiny dependency-free XML reader for `Stats.xml`.
//! * [`itg`] — readers that turn the ITGmania files into plain structs.

pub mod itg;
pub mod options;
pub mod resolver;
pub mod xml;
