use crate::SongData;
use deadsync_rules::course::CourseType;
use std::path::PathBuf;
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

/// Selected course identity, rules, and stages needed to start a game session.
#[derive(Clone, Debug)]
pub struct CourseSessionPlan {
    pub path: PathBuf,
    pub score_hash: String,
    pub course_difficulty_name: String,
    pub course_meter: Option<u32>,
    pub course_type: CourseType,
    pub lives: i32,
    pub stages: Vec<CourseStagePlan>,
}

/// Another prepared sequence for the existing course session.
///
/// Session identity and rules remain those selected at startup; renewal only
/// needs the matching course path and its next resolved stages.
#[derive(Clone, Debug)]
pub struct CourseCyclePlan {
    pub path: PathBuf,
    pub stages: Vec<CourseStagePlan>,
}
