use crate::SongData;
use std::sync::Arc;

/// One resolved chart in a course selection.
#[derive(Clone, Debug)]
pub struct CourseStagePlan {
    pub song: Arc<SongData>,
    pub chart_hash: String,
    pub modifiers: String,
    pub gain_seconds: f32,
    pub gain_lives: i32,
}
