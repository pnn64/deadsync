// Frozen from 8a2ecf968 / 0.5.1703.
use super::*;
pub(super) struct OriginalScoreProfilePaths {
    profile_dir: PathBuf,
}
impl OriginalScoreProfilePaths {
    #[inline(always)]
    pub fn new(profile_dir: impl Into<PathBuf>) -> Self {
        Self {
            profile_dir: profile_dir.into(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    #[inline(always)]
    #[must_use]
    pub fn scores_dir(&self) -> PathBuf {
        self.profile_dir.join("scores")
    }

    #[inline(always)]
    #[must_use]
    pub fn gs_dir(&self) -> PathBuf {
        self.scores_dir().join("gs")
    }

    #[inline(always)]
    #[must_use]
    pub fn gs_chart_dir(&self, chart_hash: &str) -> PathBuf {
        self.gs_dir().join(score_file_shard(chart_hash))
    }

    #[inline(always)]
    #[must_use]
    pub fn gs_index_path(&self) -> PathBuf {
        self.gs_dir().join("index.bin")
    }

    #[inline(always)]
    #[must_use]
    pub fn ac_dir(&self) -> PathBuf {
        self.scores_dir().join("ac")
    }

    #[inline(always)]
    #[must_use]
    pub fn ac_index_path(&self) -> PathBuf {
        self.ac_dir().join("index.bin")
    }

    #[inline(always)]
    #[must_use]
    pub fn local_dir(&self) -> PathBuf {
        self.scores_dir().join("local")
    }

    #[inline(always)]
    #[must_use]
    pub fn local_index_path(&self) -> PathBuf {
        self.local_dir().join("index.bin")
    }
}
