//! Importing ITGmania + Simply Love profile data into DeadSync DTOs.
//!
//! This crate reads ITGmania profile files and translates their data into
//! DeadSync profile, score, and chart-resolution structures. It does not write
//! local profiles or mutate global game state; root import orchestration owns
//! that boundary.

mod ini;

pub mod detect;
pub mod itg;
pub mod options;
pub mod resolver;
pub mod xml;

#[cfg(test)]
mod pipeline_tests;
