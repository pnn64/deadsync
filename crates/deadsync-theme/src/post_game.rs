//! Presentation of the completed session and its ranking-name editor.

use deadlib_assets::AssetManager;
pub use deadsync_score::name_entry::{TimedEntryPolicy, TimedNameEntry};
pub use deadsync_score::session_summary::{SessionPlayerSummary, SessionSummary};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct SessionEndPolicy {
    pub summary_history: usize,
    pub summary_seconds: f32,
    pub summary_input_seconds: f32,
    pub game_over_seconds: f32,
    pub name_entry: TimedEntryPolicy,
}

pub struct SessionSummaryPage<'a> {
    pub results: &'a SessionSummary,
    pub banners: &'a [Arc<str>],
    pub elapsed: f32,
    pub departure_elapsed: Option<f32>,
}

pub struct NameEntryPage<'a> {
    pub editor: &'a TimedNameEntry,
    pub diagonal: bool,
    pub wide: bool,
    pub records: [&'a [NameEntryRecord]; 2],
}

pub struct NameEntryRecord {
    pub title: Arc<str>,
    pub difficulty: Arc<str>,
    pub rank: u32,
    pub personal: bool,
}

pub trait SessionEndPresentation<A> {
    fn policy(&self) -> SessionEndPolicy;
    fn push_summary(
        &self,
        actors: &mut Vec<A>,
        assets: &AssetManager,
        page: SessionSummaryPage<'_>,
    );
    fn push_name_entry(&self, actors: &mut Vec<A>, page: NameEntryPage<'_>);
    fn push_game_over(&self, actors: &mut Vec<A>, elapsed: f32);
}
