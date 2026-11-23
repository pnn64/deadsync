use std::sync::Arc;

use deadsync_core::song_time::SongTimeNs;
use deadsync_rules::{note::Note, timing::TimingData};

use crate::views::AttractRecords;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttractMedia {
    pub movie: Option<&'static str>,
    pub music: Option<&'static str>,
    pub loop_music: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttractDemoRequest {
    pub game: &'static str,
    pub chart_type: &'static str,
    pub max_difficulty: usize,
    pub force_uncentered_notefield: bool,
    pub hide_gameplay_hud: bool,
}

pub struct AttractDemoChart<'a> {
    pub notes: &'a [Note],
    pub note_times: &'a [SongTimeNs],
    pub timing: &'a TimingData,
}

pub trait AttractPresentation<A> {
    fn reset(&mut self);
    fn record_limits(&self) -> (usize, usize);
    fn start_idle(&mut self, records: AttractRecords);
    fn advance(&mut self, delta_time: f32) -> bool;
    fn media(&self) -> AttractMedia;
    fn playback_time(&self) -> f32;
    fn demonstration(&self) -> Option<AttractDemoRequest>;
    fn prepare_demonstration(&mut self, chart: Option<AttractDemoChart<'_>>);
    fn finish_demonstration(&mut self);
    fn push_actors(&self, actors: &mut Vec<A>, video_key: Option<&Arc<str>>);
    fn push_demo_underlay(&self, actors: &mut Vec<A>, music_time: f32);
    fn push_demo_overlay(&self, actors: &mut Vec<A>, music_time: f32);
}
