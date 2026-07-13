use deadlib_render::{ClockDomainTrace, PresentModeTrace};
use deadsync_assets::noteskin::Noteskin;
use deadsync_profile::PlayerSide;
use std::path::PathBuf;
use std::sync::Arc;

pub use deadsync_config::frame_pacing::VisibleStutterSample;
pub use deadsync_theme::views::{
    AudioTimingView, CourseGraphStageView, CourseStageView, DensityGraphView, EvaluationView,
    FrameStatsSample, FrameStatsSummary, OverlayAnchor, OverlayStyle, SelectedCourseView,
    TimingHealthView,
};

/// Concrete evaluation view used by the Simply Love screens.
pub type ScoreInfo = EvaluationView<Arc<Noteskin>, PlayerSide>;
pub type CourseGraphStage = CourseGraphStageView;
pub type CourseStagePlan = CourseStageView;
pub type SelectedCoursePlan = SelectedCourseView;
pub type DensityGraphSource = DensityGraphView;
pub type TimingHealth = TimingHealthView<PresentModeTrace, ClockDomainTrace, AudioTimingView>;

/// One player's shell-prepared local records and online eligibility state used
/// while constructing an Evaluation screen.
#[derive(Clone, Debug, Default)]
pub struct EvaluationInitPlayerView {
    pub machine_records: Vec<deadsync_score::LeaderboardEntry>,
    pub personal_records: Vec<deadsync_score::LeaderboardEntry>,
    pub groovestats: deadsync_score::GrooveStatsEvalState,
    pub itl: deadsync_score::ItlEvalState,
}

#[derive(Clone, Debug, Default)]
pub struct EvaluationInitView {
    pub players: [EvaluationInitPlayerView; 2],
}

/// Shell-prepared runtime data consumed by Simply Love's Select Music screen.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectMusicRuntimeView {
    pub audio_playback: deadsync_theme::views::AudioPlaybackView,
    pub lobby: SimplyLoveLobbyRuntimeView,
    pub downloads: Vec<SelectMusicDownloadView>,
    /// Beat offset applied to the selection arrow bounce animation.
    pub arrow_bounce_offset: f32,
    pub policy: SelectMusicPolicyView,
    pub unlock_downloads_available: bool,
    pub ready_song_reload_dirs: Vec<PathBuf>,
    pub sync_graph_mode: deadsync_config::prelude::SyncGraphMode,
    pub sync_confidence_percent: u8,
}

impl Default for SelectMusicRuntimeView {
    fn default() -> Self {
        Self {
            audio_playback: Default::default(),
            lobby: Default::default(),
            downloads: Vec::new(),
            arrow_bounce_offset: 0.0,
            policy: Default::default(),
            unlock_downloads_available: false,
            ready_song_reload_dirs: Vec::new(),
            sync_graph_mode: deadsync_config::prelude::SyncGraphMode::PostKernelFingerprint,
            sync_confidence_percent: 80,
        }
    }
}

/// One shell-prepared unlock download row rendered by Select Music.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectMusicDownloadView {
    pub name: String,
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub complete: bool,
    pub error_message: Option<String>,
}

/// Shell-prepared online lobby state consumed by Simply Love lobby-aware screens.
#[derive(Clone, Debug, PartialEq)]
pub struct SimplyLoveLobbyRuntimeView {
    pub snapshot: deadsync_online::lobbies::Snapshot,
    pub reconnect_status_text: Option<String>,
    pub disconnect_hold_seconds: f32,
}

impl Default for SimplyLoveLobbyRuntimeView {
    fn default() -> Self {
        Self {
            snapshot: Default::default(),
            reconnect_status_text: None,
            disconnect_hold_seconds: 5.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SimplyLoveGrooveStatsService {
    #[default]
    GrooveStats,
    BoogieStats,
}

/// One player's shell-prepared score-submission state for Evaluation.
#[derive(Clone, Debug, Default)]
pub struct EvaluationSubmissionView {
    pub groovestats_status: Option<deadsync_score::GrooveStatsSubmitUiStatus>,
    pub arrowcloud_status: Option<deadsync_score::ArrowCloudSubmitUiStatus>,
    pub event_progress: Vec<deadsync_score::EventProgress>,
    pub record_banner: Option<deadsync_score::GrooveStatsSubmitRecordBanner>,
    pub groovestats_next_retry_secs: Option<u32>,
    pub arrowcloud_next_retry_secs: Option<u32>,
    pub groovestats_next_retry_is_auto: bool,
    pub arrowcloud_next_retry_is_auto: bool,
}

/// Shell-prepared runtime data consumed by Simply Love's Evaluation screen.
#[derive(Clone, Debug, Default)]
pub struct EvaluationRuntimeView {
    pub lobby: SimplyLoveLobbyRuntimeView,
    pub groovestats_service: SimplyLoveGrooveStatsService,
    pub submissions: [EvaluationSubmissionView; 2],
    pub leaderboards: [Option<deadsync_score::CachedPlayerLeaderboardData>; 2],
}

/// One playlist file loaded by the shell for Simply Love's playlist wheel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectMusicPlaylistView {
    pub id: String,
    pub owner: Option<String>,
    pub name: String,
    pub text: String,
}

/// Filesystem-derived data prepared once when Select Music is entered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicInitView {
    pub songs_root: PathBuf,
    pub courses_root: PathBuf,
    pub playlists: Vec<SelectMusicPlaylistView>,
}

/// Shell-prepared song packs used by Simply Love's Options import/sync UI.
#[derive(Clone, Debug)]
pub struct OptionsSongPackView {
    pub group_name: String,
    pub display_name: String,
    pub songs: Vec<Arc<deadsync_chart::SongData>>,
}

/// Runtime/config policy used to expose Select Music features and input paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicPolicyView {
    pub dedicated_menu_only: bool,
    pub fsr_profiles: bool,
    pub replays: bool,
    pub profile_switch: bool,
    pub keyboard_features: bool,
}

/// Simply Love's two density-graph texture targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveDensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}

/// Simply Love compatibility name used inside its concrete screen modules.
pub(crate) type DensityGraphSlot = SimplyLoveDensityGraphSlot;

/// Runtime capabilities used to decide which concrete Simply Love options
/// rows are available on the current host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimplyLoveUpdaterCapabilities {
    pub app_update: bool,
    pub ffmpeg_install: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuGrooveError {
    Disabled,
    MachineOffline,
    CannotConnect,
    TimedOut,
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuGrooveStatus {
    Pending {
        boogie: bool,
    },
    Error {
        boogie: bool,
        kind: MainMenuGrooveError,
    },
    Connected {
        boogie: bool,
        get_scores: bool,
        leaderboard: bool,
        auto_submit: bool,
    },
}

impl Default for MainMenuGrooveStatus {
    fn default() -> Self {
        Self::Pending { boogie: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuArrowCloudError {
    Disabled,
    TimedOut,
    HostBlocked,
    CannotConnect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainMenuArrowCloudStatus {
    #[default]
    Pending,
    Connected,
    Error(MainMenuArrowCloudError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainMenuSmxConflictView {
    pub color_rgb: [f32; 3],
}

/// Shell-prepared runtime data consumed by Simply Love's concrete main menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MainMenuRuntimeView {
    pub allow_shutdown_host: bool,
    pub song_count: usize,
    pub pack_count: usize,
    pub course_count: usize,
    pub groovestats: MainMenuGrooveStatus,
    pub arrowcloud: MainMenuArrowCloudStatus,
    pub smx_conflict: Option<MainMenuSmxConflictView>,
}

/// Coarse updater failure category used by Simply Love's localized overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdateErrorKind {
    Network,
    RateLimited,
    HttpStatus,
    Parse,
    NoAssetForHost,
    Checksum,
    Io,
}

/// Release metadata needed by Simply Love's updater presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveReleaseView {
    pub tag: String,
    pub html_url: String,
    pub published_at: Option<String>,
}

/// Release-asset metadata shown by Simply Love before a download begins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveReleaseAssetView {
    pub size: u64,
    pub digest: Option<String>,
}

/// Prepared app-update state rendered by Simply Love.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdatePhase {
    Idle,
    Checking,
    ConfirmDownload {
        info: SimplyLoveReleaseView,
        asset: SimplyLoveReleaseAssetView,
    },
    UpToDate {
        tag: String,
    },
    RollbackChecking,
    RollbackPick {
        candidates: Vec<SimplyLoveReleaseView>,
        selected: usize,
    },
    RollbackEmpty,
    AvailableNoInstall {
        info: SimplyLoveReleaseView,
    },
    Downloading {
        info: SimplyLoveReleaseView,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
    },
    Ready {
        info: SimplyLoveReleaseView,
    },
    Applying {
        info: SimplyLoveReleaseView,
    },
    AppliedRestartRequired {
        info: SimplyLoveReleaseView,
        detail: String,
    },
    Error {
        kind: SimplyLoveUpdateErrorKind,
        detail: String,
    },
}

/// Prepared FFmpeg-install state rendered by Simply Love.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveFfmpegPhase {
    Idle,
    Checking,
    Confirm {
        version: String,
        origin: String,
        total: Option<u64>,
        already_available: bool,
    },
    Downloading {
        version: String,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
        speed_bps: Option<u64>,
    },
    Extracting {
        version: String,
    },
    Installed {
        version: String,
    },
    Unsupported,
    AlreadyAvailable,
    Error {
        kind: SimplyLoveUpdateErrorKind,
        detail: String,
    },
}

/// One shell-prepared snapshot of the updater services used by Options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveUpdaterView {
    pub update: SimplyLoveUpdatePhase,
    pub ffmpeg: SimplyLoveFfmpegPhase,
}

impl Default for SimplyLoveUpdaterView {
    fn default() -> Self {
        Self {
            update: SimplyLoveUpdatePhase::Idle,
            ffmpeg: SimplyLoveFfmpegPhase::Idle,
        }
    }
}

/// Number of bins in Simply Love's frame-interval histogram.
pub const HISTOGRAM_BINS: usize = 32;

/// Fill `out` with Simply Love's frame-interval histogram. The final bin
/// absorbs overflow.
pub fn frame_histogram(
    samples: &[FrameStatsSample],
    out: &mut [u32; HISTOGRAM_BINS],
    bin_width_us: u32,
) {
    *out = [0; HISTOGRAM_BINS];
    let bin_width_us = bin_width_us.max(1);
    for sample in samples {
        if sample.is_empty() {
            continue;
        }
        let idx = (sample.frame_us / bin_width_us).min(HISTOGRAM_BINS as u32 - 1) as usize;
        out[idx] = out[idx].saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_ignores_empty_samples_and_absorbs_overflow() {
        let samples = [
            FrameStatsSample::empty(),
            FrameStatsSample {
                host_nanos: 1,
                frame_us: 1_500,
                ..FrameStatsSample::empty()
            },
            FrameStatsSample {
                host_nanos: 2,
                frame_us: 100_000,
                ..FrameStatsSample::empty()
            },
        ];
        let mut bins = [0; HISTOGRAM_BINS];
        frame_histogram(&samples, &mut bins, 1_000);
        assert_eq!(bins[1], 1);
        assert_eq!(bins[HISTOGRAM_BINS - 1], 1);
    }
}
