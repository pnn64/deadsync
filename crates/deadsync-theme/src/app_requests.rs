//! Requests and events exchanged with the application by theme controllers.

use crate::Screen;
use crate::color::DifficultyColorScheme;
use crate::profile_picker::ManageLocalProfilesView;
use crate::select_music::DensityGraphSlot;
use crate::views::DensityGraphView;
use crate::{AudioRequest, GraphicsRequest, PlatformRequest};
use deadlib_platform::input::KeyCode;
#[cfg(target_os = "windows")]
use deadlib_platform::input::WindowsPadBackend;
use deadsync_config::audio::NoteScrollClock;
use deadsync_config::coin::CoinMode;
use deadsync_config::options::SmxPackName;
use deadsync_config::theme::{
    BreakdownStyle, DefaultFailType, DefaultSyncOffset, GameFlag, GameplayBannerMode, LanguageFlag,
    LogLevel, MachineBarColor, MachineEvaluationStyle, MachineFont, MachinePreferredPlayMode,
    MachinePreferredPlayStyle, NewPackMode, RandomBackgroundMode, SelectMusicDefaultSort,
    SelectMusicItlRankMode, SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement,
    SelectMusicSeriesSource, SelectMusicSongSelectBgMode, SelectMusicSort,
    SelectMusicStepArtistBoxMode, SelectMusicWheelScoreMode, SelectMusicWheelScoreType,
    SelectMusicWheelStyle, SrpgVariant, TournamentScoringSystem, VersionOverlaySide, VisualStyle,
};
use deadsync_gameplay::GameplayAction;
use deadsync_gameplay::exit_route::{
    GameplayExitRoute, GameplayExitRouteContext, resolve_gameplay_exit_route,
};
use deadsync_input::devices::SmxPadPreset;
use deadsync_input::{InputBinding, VirtualAction};
use deadsync_profile::PlayerSide;
pub use deadsync_profile::requests::ProfileRequest;
use deadsync_simfile::sync_offset::SongOffsetSyncChange;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum MediaRequest {
    Screenshot(Option<PlayerSide>),
    Banner(Option<PathBuf>),
    CdTitle(Option<PathBuf>),
    PackBanner(Option<PathBuf>),
    WheelItemBackgrounds(Vec<PathBuf>),
    DensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphView>,
    },
}

/// Song/course cache work requested by a theme and executed by the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentRequest {
    ReloadLibrary {
        songs_root: PathBuf,
        courses_root: PathBuf,
    },
    ReloadSongDirs {
        songs_root: PathBuf,
        pack_dirs: Vec<PathBuf>,
    },
    ReloadSong {
        simfile_path: PathBuf,
    },
    DeleteSong {
        simfile_path: PathBuf,
    },
    /// Permanently delete an installed pack.
    ///
    /// Carries the group name and never a path: the shell resolves it against
    /// the live catalog, so nothing a screen can compose reaches the
    /// filesystem.
    DeletePack {
        group_name: String,
    },
    /// Measure a library pack with Null-or-Die from the Content Browser:
    /// the shell gathers its songs and opens the pack sync review on them.
    MeasurePackSync {
        group_name: String,
    },
    /// Write a `Pack.ini` `SyncOffset` for an installed pack, so the engine
    /// knows whether it was authored ITG-synced or null-synced.
    ///
    /// Same rule as the delete above: a group name, never a path.
    SetPackSync {
        group_name: String,
        /// `true` writes `SyncOffset=ITG`, `false` writes `SyncOffset=NULL`.
        itg: bool,
    },
    /// Ask the shell to cut short the in-progress startup `ReplayGain` analysis
    /// so the loading screen can advance without waiting for every song.
    SkipReplayGain,
    /// Analyze EBU R128 loudness for the whole song library and populate the
    /// `ReplayGain` cache. Progress is reported via
    /// the ReplayGain progress stream. Cancel an in-flight
    /// run with [`ContentRequest::SkipReplayGain`].
    ApplyReplayGain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItgProfileCandidate {
    pub dir: PathBuf,
    pub display_name: String,
    pub imported_as: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItgImportSummary {
    pub profile_id: String,
    pub display_name: String,
    pub scores_total: usize,
    pub scores_imported: usize,
    pub charts_song_not_found: usize,
    pub charts_chart_not_found: usize,
    pub scores_unmapped: usize,
    pub favorites_total: usize,
    pub favorites_imported: usize,
    pub itl_entries_imported: usize,
    pub simply_love_options_imported: bool,
    pub groovestats_imported: bool,
    pub arrowcloud_imported: bool,
    pub avatar_imported: bool,
    pub canceled: bool,
    pub already_imported_as: Option<String>,
}

impl ItgImportSummary {
    #[must_use]
    pub const fn online_keys_imported(&self) -> bool {
        self.groovestats_imported || self.arrowcloud_imported
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileImportEvent {
    Candidates {
        candidates: Vec<ItgProfileCandidate>,
        browsed_dir: Option<PathBuf>,
    },
    BrowseCanceled,
    Progress {
        done: usize,
        total: usize,
        label: String,
    },
    Finished(Result<ItgImportSummary, String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalProfileEvent {
    Created {
        result: Result<String, ()>,
        view: ManageLocalProfilesView,
    },
    Renamed {
        profile_id: String,
        result: Result<(), ()>,
        view: ManageLocalProfilesView,
    },
    DefaultSet {
        view: ManageLocalProfilesView,
    },
    Deleted {
        result: Result<(), ()>,
        view: ManageLocalProfilesView,
    },
}

#[derive(Clone, Debug)]
pub enum OnlineRequest {
    Reinitialize,
    Lobby(LobbyRequest),
    StartScoreImport(ScoreImportRequest),
    CancelScoreImport,
    StartQrLogin(QrLoginRequest),
    CancelQrLogin(QrLoginService),
    LinkArrowCloud {
        profile_id: String,
        display_name: String,
    },
    LinkGrooveStats {
        profile_id: String,
        display_name: String,
    },
    RefreshPlayerLeaderboard {
        chart_hash: String,
        side: PlayerSide,
        max_entries: usize,
    },
    LoadMachineReplays {
        chart_hash: String,
        max_entries: usize,
    },
    RefreshSrpgShop {
        side: PlayerSide,
    },
    DownloadSrpgShopUnlock {
        shop_id: u32,
        name: String,
        url: String,
    },
    RetryUnlockDownloads,
    EnsureStepManiaOnlineCatalog,
    RefreshStepManiaOnlineCatalog,
    /// Ask the catalogue for the next page of packs, newest first.
    LoadMoreStepManiaOnlinePacks,
    DownloadStepManiaOnlinePack {
        pack_id: u64,
    },
    PurchaseSrpgShopItem {
        shop_id: u32,
        item_id: String,
        type_id: u8,
    },
    FetchGrade(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LobbyRequest {
    Search,
    Create {
        password: String,
    },
    Join {
        code: String,
        password: String,
    },
    Leave,
    SelectSong(deadsync_online::lobbies::LobbySongInfo),
    UpdateMachineState {
        screen_name: &'static str,
        ready: bool,
    },
    UpdateMachineStats {
        screen_name: &'static str,
        p1_ready: bool,
        p2_ready: bool,
        p1_stats: Option<deadsync_online::lobbies::MachinePlayerStats>,
        p2_stats: Option<deadsync_online::lobbies::MachinePlayerStats>,
    },
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrLoginService {
    ArrowCloud,
    GrooveStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrLoginSlotAvailability {
    NotJoined,
    Guest,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrLoginSlot {
    pub side: PlayerSide,
    pub availability: QrLoginSlotAvailability,
    pub display_name: String,
    pub had_existing_key: bool,
    pub target_profile_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrLoginRequest {
    pub service: QrLoginService,
    pub slots: [QrLoginSlot; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QrLoginEvent {
    Started {
        service: QrLoginService,
        side: PlayerSide,
        short_code: String,
        verification_url: String,
    },
    Succeeded {
        service: QrLoginService,
        side: PlayerSide,
        display_name: String,
    },
    Failed {
        service: QrLoginService,
        side: PlayerSide,
        reason: String,
    },
}

impl QrLoginEvent {
    #[must_use]
    pub const fn service(&self) -> QrLoginService {
        match self {
            Self::Started { service, .. }
            | Self::Succeeded { service, .. }
            | Self::Failed { service, .. } => *service,
        }
    }
}

#[derive(Clone)]
pub struct ScoreImportProfile {
    pub id: String,
    pub display_name: String,
    pub groovestats_api_key: String,
    pub groovestats_username: String,
    pub arrowcloud_api_key: String,
}

impl std::fmt::Debug for ScoreImportProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScoreImportProfile")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("groovestats_api_key", &"<redacted>")
            .field("groovestats_username", &self.groovestats_username)
            .field("arrowcloud_api_key", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct ScoreImportRequest {
    pub endpoint: deadsync_score::ScoreImportEndpoint,
    pub profile: ScoreImportProfile,
    pub pack_groups: Vec<String>,
    pub only_missing_groovestats_scores: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreImportProgress {
    pub processed_charts: usize,
    pub total_charts: usize,
    pub imported_scores: usize,
    pub missing_scores: usize,
    pub failed_requests: usize,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreImportSummary {
    pub requested_charts: usize,
    pub imported_scores: usize,
    pub missing_scores: usize,
    pub failed_requests: usize,
    pub rate_limit_per_second: u32,
    pub elapsed_seconds: f32,
    pub canceled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScoreImportEvent {
    Progress(ScoreImportProgress),
    Finished(Result<ScoreImportSummary, String>),
}

#[derive(Clone, Debug)]
pub enum SyncRequest {
    StartAnalysis {
        owner: SyncOwner,
        targets: Vec<SyncTarget>,
        emit_freq_delta: bool,
    },
    CancelAnalysis(SyncOwner),
    ApplySongOffset {
        simfile_path: PathBuf,
        delta_seconds: f32,
    },
    ApplySongOffsetBatch {
        owner: SyncOwner,
        changes: Vec<SongOffsetSyncChange>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncOwner {
    SelectMusicSong,
    SelectMusicPack,
    OptionsPack,
    /// The Content Browser's Installed tab.
    ContentBrowserPack,
}

/// Song-search work run off the render thread by the shell's `song_search`
/// service, which owns the index: the screen hands over its catalog once, then
/// only sends queries. `generation` lets it discard stale rankings.
#[derive(Clone, Debug)]
pub enum SongSearchRequest {
    /// Re-index; sent when Select Music is built or reloaded.
    BuildIndex {
        entries: Arc<[deadsync_gameplay::music_wheel::MusicWheelEntry]>,
    },
    /// Rank a query against the service's index.
    Rank {
        generation: u64,
        query: String,
        scope: deadsync_simfile::song_search_index::SongSearchScope,
        chart_type: &'static str,
    },
}

/// A ranking, applied only while its `generation` is still current.
#[derive(Clone, Debug)]
pub struct SongSearchResult {
    pub generation: u64,
    pub matches: Vec<deadsync_simfile::song_search_index::SongSearchMatch>,
}

#[derive(Clone, Debug)]
pub struct SyncTarget {
    pub song: Arc<deadsync_chart::SongData>,
    pub chart_ix: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncKernelTarget {
    Digest,
    Accumulator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncKernel {
    Rising,
    Loudest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SyncStreamEvent {
    Init {
        cols: usize,
        freq_rows: usize,
        planned_beats: usize,
        kernel_target: SyncKernelTarget,
        kernel: SyncKernel,
        times_ms: Vec<f64>,
    },
    Beat {
        beat_seq: usize,
        digest_row: Vec<f64>,
        freq_delta: Option<Vec<f64>>,
    },
    Convolution {
        rows: usize,
        post_kernel: Vec<f64>,
        convolution: Vec<f64>,
        edge_discard: usize,
    },
    Done(SyncResult),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SyncPlotView {
    pub freq_rows: usize,
    pub digest_rows: usize,
    pub cols: usize,
    pub post_rows: usize,
    pub freq_domain: Vec<f64>,
    pub beat_digest: Vec<f64>,
    pub post_kernel: Vec<f64>,
    pub convolution: Vec<f64>,
    pub times_ms: Vec<f64>,
    pub edge_discard: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SyncSongResult {
    pub estimate: SyncResult,
    pub plot: SyncPlotView,
    pub cached: bool,
}

pub enum SyncEvent {
    SongStream(SyncStreamEvent),
    SongFinished(Result<SyncSongResult, String>),
    RowStarted {
        index: usize,
    },
    RowInit {
        index: usize,
        total_beats: usize,
    },
    RowBeat {
        index: usize,
        beats_processed: usize,
        total_beats: usize,
    },
    RowCached {
        index: usize,
        result: SyncResult,
        applied: bool,
    },
    RowFinished {
        index: usize,
        result: Result<SyncResult, String>,
    },
    BatchSaveFinished {
        summary: deadsync_simfile::sync_offset::SongOffsetSaveSummary,
    },
    Finished,
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncResult {
    pub bias_ms: f64,
    pub confidence: f64,
}

/// Select Music preferences chosen by a theme and persisted by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMusicConfigRequest {
    ShowBanners(bool),
    ShowVideoBanners(bool),
    ShowBreakdown(bool),
    BreakdownStyle(BreakdownStyle),
    TranslatedTitles(bool),
    WheelSwitchSpeed(u8),
    WheelStyle(SelectMusicWheelStyle),
    DifficultyColors(DifficultyColorScheme),
    HideInactiveSeries(bool),
    DefaultSort(SelectMusicDefaultSort),
    LastSort(SelectMusicSort),
    SeriesSource(SelectMusicSeriesSource),
    SongSelectBackground(SelectMusicSongSelectBgMode),
    AllowProfileSwitch(bool),
    ShowCdTitles(bool),
    ShowWheelGrades(bool),
    ShowWheelLamps(bool),
    ItlRankMode(SelectMusicItlRankMode),
    WheelScoreMode(SelectMusicWheelScoreMode),
    WheelScoreType(SelectMusicWheelScoreType),
    WheelShowFails(bool),
    WheelItlPoints(bool),
    NewPackMode(NewPackMode),
    ShowGolfPars(bool),
    ShowFolderStats(bool),
    PatternInfoMode(SelectMusicPatternInfoMode),
    StepArtistBoxMode(SelectMusicStepArtistBoxMode),
    ShowPreviews(bool),
    ShowPreviewMarker(bool),
    PreviewLoop(bool),
    PreviewStartsImmediately(bool),
    ShowGameplayTimer(bool),
    ShowStageDisplay(bool),
    ShowScorebox(bool),
    ScoreboxPlacement(SelectMusicScoreboxPlacement),
    ScoreboxEventsFirst(bool),
    ChartInfoMask(u8),
}

/// Input mappings chosen by a theme and persisted by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingsConfigRequest {
    BindKeyboard {
        action: VirtualAction,
        index: usize,
        code: KeyCode,
    },
    BindGamepad {
        action: VirtualAction,
        index: usize,
        binding: InputBinding,
    },
    Clear {
        action: VirtualAction,
        index: usize,
    },
}

/// Machine preferences chosen by a theme and persisted by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineConfigRequest {
    ShowSelectProfile(bool),
    ShowSelectColor(bool),
    ShowSelectStyle(bool),
    PreferredPlayStyle(MachinePreferredPlayStyle),
    ShowSelectPlayMode(bool),
    PreferredPlayMode(MachinePreferredPlayMode),
    Font(MachineFont),
    BarColor(MachineBarColor),
    EvaluationStyle(MachineEvaluationStyle),
    ShowEvaluationSummary(bool),
    EasterEggs(bool),
    NiceSound(bool),
    ShowNameEntry(bool),
    ShowGameover(bool),
    MenuMusic(bool),
    VisualStyle(VisualStyle),
    SrpgVariant(SrpgVariant),
    EnableReplays(bool),
    EnableHeartRateMonitors(bool),
    AllowPerPlayerGlobalOffsets(bool),
    PackIniOffsets(bool),
    DefaultSyncOffset(DefaultSyncOffset),
    KeyboardFeatures(bool),
    ShowVideoBackgrounds(bool),
    RandomBackgroundMode(RandomBackgroundMode),
    ShowVersionOverlay(bool),
    VersionOverlaySide(VersionOverlaySide),
    ShowLocalIp(bool),
    WriteCurrentScreen(bool),
}

/// Coin, stage, and timed-session preferences chosen in Arcade Options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoinConfigRequest {
    Mode(CoinMode),
    CoinsPerCredit(u8),
    SongsPerPlay(u8),
    EventMode(bool),
    PremiumFreeMinutes(u8),
    PremiumFreeGraceSeconds(u16),
    ContinueOnGiveUp(bool),
    LongSongSeconds(u16),
    MarathonSongSeconds(u16),
}

/// Advanced loading preferences chosen by a theme and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvancedConfigRequest {
    DefaultFailType(DefaultFailType),
    BannerCache(bool),
    CdTitleCache(bool),
    SongParsingThreads(u8),
    CacheSongs(bool),
    FastLoad(bool),
    AllowSongDeletion(bool),
}

/// Course preferences chosen by a theme and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourseConfigRequest {
    ShowRandom(bool),
    ShowMostPlayed(bool),
    ShowIndividualScores(bool),
    AutosubmitIndividual(bool),
    AutosubmitPostFailPasses(bool),
}

/// Gameplay presentation preferences chosen by a theme and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayConfigRequest {
    BackgroundBrightnessTenths(u8),
    CenterPlayerOneNotefield(bool),
    NoteScrollClock(NoteScrollClock),
    SongLuaMode(deadsync_config::options::SongLuaMode),
    BannerMode(GameplayBannerMode),
    ZmodRatingBoxText(bool),
    ShowBpmDecimal(bool),
    BpmNearField(bool),
    DelayedBack(bool),
    AutoScreenshotMask(u8),
}

/// Tournament policy chosen by the operator and persisted by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TournamentConfigRequest {
    Enabled(bool),
    ScoringSystem(TournamentScoringSystem),
    StepStats(bool),
    EnforceNoCmod(bool),
}

/// System, input, and SMX preferences chosen in Options and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsConfigRequest {
    Game(GameFlag),
    Theme(deadsync_config::theme::ThemeFlag),
    Language(LanguageFlag),
    LogLevel(LogLevel),
    LogToFile(bool),
    GfxDebug(bool),
    #[cfg(target_os = "windows")]
    WindowsPadBackend(WindowsPadBackend),
    AxisFix(bool),
    UseFsrs(bool),
    ThreeKeyNavigation(bool),
    ArcadeOptionsNavigation(bool),
    OnlyDedicatedMenuButtons(bool),
    SmxInput(bool),
    SmxPanelLights(bool),
    SmxManagesPadConfig(bool),
    SmxDefaultPadConfig(SmxPadPreset),
    SmxDefaultLightBrightness(u8),
    SmxPadGifsPack(SmxPackName),
    SmxJudgeGifsPack(SmxPackName),
    SmxIdleLightsBlack(bool),
    VisualDelayMillis(i32),
    InputDebounceMillis(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullOrDieGraph {
    Frequency,
    BeatIndex,
    PostKernelFingerprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphOrientation {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphOrigin {
    Bottom,
    Top,
}

/// Null-or-Die preferences chosen by a theme and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullOrDieConfigRequest {
    SyncGraph(NullOrDieGraph),
    GraphOrientation(GraphOrientation),
    GraphOrigin(GraphOrigin),
    ConfidencePercent(u8),
    CacheResults(bool),
    PackSyncThreads(u8),
    FingerprintTenths(i32),
    WindowTenths(i32),
    StepTenths(i32),
    MagicOffsetTenths(i32),
    KernelTarget(SyncKernelTarget),
    Kernel(SyncKernel),
    FullSpectrogram(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrLoginPolicy {
    Always,
    Sometimes,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SrpgShopFolder {
    Unlocks,
    Shops,
    Faction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightsDriver {
    Off,
    Snek,
    Litboard,
    Win32Serial,
    Fusion,
    Gpb,
    PacDrive,
    PiuioLeds,
    Itgio,
    HidBlueDot,
    Stac2,
    MinimaidHid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayPadLights {
    Input,
    Chart,
}

/// Cabinet light preferences chosen by a theme and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightsConfigRequest {
    Driver(LightsDriver),
    GameplayPadLights(GameplayPadLights),
    SimplifyBass(bool),
}

/// Online preferences chosen by a theme and persisted by shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnlineConfigRequest {
    TwitchChat(deadsync_config::streaming::TwitchOptions),
    EnableGrooveStats(bool),
    ShowSrpgShop(bool),
    SrpgShopFolder(SrpgShopFolder),
    EnableBoogieStats(bool),
    AutoPopulateScores(bool),
    AutoDownloadUnlocks(bool),
    SeparateUnlocksByPlayer(bool),
    GrooveStatsQrLogin(QrLoginPolicy),
    /// `GrooveStats` scorebox boards: ITG (bit 0), EX (bit 1), events (bit 2).
    GrooveStatsScoreboxBoards(u8),
    EnableArrowCloud(bool),
    EnableGolf(bool),
    ArrowCloudRegularBoards(bool),
    ArrowCloudEventBoards(bool),
    SubmitArrowCloudFails(bool),
    ShowArrowCloudResultDialogs(bool),
    ArrowCloudQrLogin(QrLoginPolicy),
    /// Rip Packs' rips listed on itgcontent.net, and kept ones marked kept.
    ShareRips(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigRequest {
    ShowOverlay(u8),
    MouseCursorHidden(bool),
    PersistColor(i32),
    Overscan {
        translate_x: i32,
        translate_y: i32,
        add_width: i32,
        add_height: i32,
    },
    Advanced(AdvancedConfigRequest),
    Coin(CoinConfigRequest),
    Course(CourseConfigRequest),
    Gameplay(GameplayConfigRequest),
    Lights(LightsConfigRequest),
    Machine(MachineConfigRequest),
    Mappings(MappingsConfigRequest),
    NullOrDie(NullOrDieConfigRequest),
    Online(OnlineConfigRequest),
    Options(OptionsConfigRequest),
    SelectMusic(SelectMusicConfigRequest),
    Tournament(TournamentConfigRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardwareRequest {
    TestLightsAuto,
    StepTestCabinet(i8),
    StepTestButton(i8),
    AssignSmxPads {
        p1_serial: Option<String>,
        p2_serial: Option<String>,
    },
    SwapSmxPads,
    SetSmxUnderglowTheme(bool),
    SetSmxUnderglowGrb(bool),
    ApplySmxPadPreset {
        pad: usize,
        name: String,
    },
    ApplySmxPadConfig {
        pad: usize,
        name: String,
    },
    CaptureSmxPadConfig {
        pad: usize,
        name: String,
        set_default: bool,
        overwrite: bool,
    },
    RenameSmxPadConfig {
        serial: String,
        old_name: String,
        new_name: String,
        set_default: bool,
    },
    SetSmxPadConfigDefault {
        serial: String,
        name: String,
    },
    DeleteSmxPadConfig {
        name: String,
    },
    SetSmxPlayerLights([Option<[u8; 3]>; 2]),
    ReenableSmxAutoLights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugRequest {
    WriteFsrDump,
}

/// Updater work requested by a theme and executed by the process shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdaterRequest {
    InstallWorkshop,
    DismissWorkshop,
    CheckForUpdates,
    CheckForRollback,
    DownloadUpdate,
    ApplyUpdate,
    DismissUpdate,
    CancelUpdate,
    MoveRollback(i32),
    ConfirmRollback,
    CheckFfmpegAvailability,
    ConfirmFfmpegInstall,
    DismissFfmpeg,
    CancelFfmpegCheck,
    CancelFfmpegDownload,
}

/// Runtime work requested by a theme in response to input or a screen update.
#[derive(Clone, Debug)]
pub enum RuntimeRequest {
    /// Preserve the game action until the shell supplies current session facts.
    GameplayAction(deadsync_gameplay::GameplayAction),
    SongLaunch(deadsync_gameplay::song_launch::SongLaunchRequest),
    CourseLaunch(
        deadsync_gameplay::stage_launch::StageLaunchRequest<
            deadsync_simfile::course_selection::ConfirmedCourse,
        >,
    ),
    StartReplay(deadsync_score::StageReplayRequest),
    RestartGameplay,
    /// Replace the gameplay screen without committing its stage or Lua VM.
    ReloadSongLua,
    Audio(AudioRequest),
    Media(MediaRequest),
    Content(ContentRequest),
    Profile(ProfileRequest),
    Online(OnlineRequest),
    Graphics(GraphicsRequest),
    Platform(PlatformRequest),
    Sync(SyncRequest),
    Config(ConfigRequest),
    JudgmentPalettes(deadsync_config::judgment_palettes::JudgmentPaletteCatalog),
    Hardware(HardwareRequest),
    Debug(DebugRequest),
    Updater(UpdaterRequest),
    /// Rank the song search off the render thread.
    SongSearch(SongSearchRequest),
}

pub type AppEffect = crate::ThemeEffect<Screen, RuntimeRequest>;
pub type AppInputResult = crate::ThemeInputResult<Screen, RuntimeRequest>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectRouteContext {
    pub current_screen: Screen,
    pub restart_pending: bool,
    pub course_active: bool,
    pub course_has_next_stage: bool,
    pub gameplay_failed: bool,
}

#[derive(Clone, Debug)]
pub struct EffectRoutePlan {
    pub action: AppEffect,
    pub clear_restart_pending: bool,
}

#[must_use]
pub const fn gameplay_exit_screen(
    destination: deadsync_gameplay::exit_route::GameplayExitDestination,
) -> Screen {
    use deadsync_gameplay::exit_route::GameplayExitDestination;

    match destination {
        GameplayExitDestination::Results => Screen::Evaluation,
        GameplayExitDestination::SongSelection => Screen::SelectMusic,
        GameplayExitDestination::CourseSelection => Screen::SelectCourse,
        GameplayExitDestination::NextCourseStage | GameplayExitDestination::RestartStage => {
            Screen::Gameplay
        }
    }
}

/// Interpret legacy screen destinations that also have a gameplay meaning.
#[must_use]
pub const fn gameplay_action_for_navigation(
    screen: Screen,
    immediate: bool,
) -> Option<deadsync_gameplay::GameplayAction> {
    use deadsync_gameplay::{GameplayAction, GameplayExit};

    let exit = match screen {
        Screen::Evaluation => GameplayExit::Complete,
        Screen::SelectMusic => GameplayExit::Cancel,
        _ => return None,
    };
    Some(if immediate {
        GameplayAction::NavigateNoFade(exit)
    } else {
        GameplayAction::Navigate(exit)
    })
}

/// Read semantic gameplay actions and legacy gameplay navigation requests.
#[must_use]
pub fn gameplay_action_for_effect(effect: &AppEffect) -> Option<GameplayAction> {
    match effect {
        AppEffect::Runtime(RuntimeRequest::GameplayAction(action)) => Some(*action),
        AppEffect::Navigate(screen) => gameplay_action_for_navigation(*screen, false),
        AppEffect::NavigateNoFade(screen) => gameplay_action_for_navigation(*screen, true),
        _ => None,
    }
}

/// Map an already resolved game destination to a shared screen role.
#[must_use]
pub const fn effect_for_gameplay_exit(route: GameplayExitRoute) -> AppEffect {
    let screen = gameplay_exit_screen(route.destination);
    if route.immediate {
        AppEffect::NavigateNoFade(screen)
    } else {
        AppEffect::Navigate(screen)
    }
}

/// Compatibility adapter for callers that resolve effects through the theme.
/// Gameplay destinations are decided by the game-owned exit policy.
#[must_use]
pub fn resolve_effect_route(effect: AppEffect, context: EffectRouteContext) -> EffectRoutePlan {
    let Some(action) = gameplay_action_for_effect(&effect) else {
        return EffectRoutePlan {
            action: effect,
            clear_restart_pending: false,
        };
    };
    let (action, clear_restart_pending) = match action {
        GameplayAction::None => (AppEffect::None, false),
        GameplayAction::Restart => (AppEffect::Runtime(RuntimeRequest::RestartGameplay), false),
        action => {
            let route = resolve_gameplay_exit_route(
                action,
                GameplayExitRouteContext {
                    from_gameplay: context.current_screen == Screen::Gameplay,
                    restart_pending: context.restart_pending,
                    course_active: context.course_active,
                    course_has_next_stage: context.course_has_next_stage,
                    all_joined_players_failed: context.gameplay_failed,
                },
            )
            .expect("gameplay navigation has an exit route");
            (
                effect_for_gameplay_exit(route),
                route.consume_restart_pending,
            )
        }
    };
    EffectRoutePlan {
        action,
        clear_restart_pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_gameplay::GameplayExit;
    use deadsync_gameplay::exit_route::GameplayExitDestination;

    #[test]
    fn gameplay_action_adapter_preserves_semantic_requests() {
        for action in [
            GameplayAction::None,
            GameplayAction::Restart,
            GameplayAction::Navigate(GameplayExit::Complete),
            GameplayAction::NavigateNoFade(GameplayExit::Complete),
            GameplayAction::Navigate(GameplayExit::Cancel),
            GameplayAction::NavigateNoFade(GameplayExit::Cancel),
        ] {
            let effect = AppEffect::Runtime(RuntimeRequest::GameplayAction(action));
            assert_eq!(gameplay_action_for_effect(&effect), Some(action));
        }
        for effect in [
            AppEffect::Navigate(Screen::Gameplay),
            AppEffect::NavigateNoFade(Screen::SelectCourse),
            AppEffect::Runtime(RuntimeRequest::RestartGameplay),
            AppEffect::Runtime(RuntimeRequest::ReloadSongLua),
            AppEffect::None,
        ] {
            assert_eq!(gameplay_action_for_effect(&effect), None);
        }
    }

    #[test]
    fn gameplay_exit_adapter_preserves_destination_and_transition_mode() {
        for (destination, screen) in [
            (GameplayExitDestination::Results, Screen::Evaluation),
            (GameplayExitDestination::SongSelection, Screen::SelectMusic),
            (
                GameplayExitDestination::CourseSelection,
                Screen::SelectCourse,
            ),
            (GameplayExitDestination::NextCourseStage, Screen::Gameplay),
            (GameplayExitDestination::RestartStage, Screen::Gameplay),
        ] {
            for immediate in [false, true] {
                let effect = effect_for_gameplay_exit(GameplayExitRoute {
                    destination,
                    immediate,
                    consume_restart_pending: false,
                });
                let expected = if immediate {
                    crate::ThemeFlowEvent::NavigateNoFade(screen)
                } else {
                    crate::ThemeFlowEvent::Navigate(screen)
                };
                assert_eq!(effect.flow_event(), Some(expected));
            }
        }
    }

    #[test]
    fn legacy_and_semantic_gameplay_requests_share_compatibility_resolution() {
        let context = EffectRouteContext {
            current_screen: Screen::Gameplay,
            restart_pending: true,
            course_active: true,
            course_has_next_stage: true,
            gameplay_failed: false,
        };
        for (legacy, action) in [
            (
                AppEffect::Navigate(Screen::Evaluation),
                GameplayAction::Navigate(GameplayExit::Complete),
            ),
            (
                AppEffect::NavigateNoFade(Screen::Evaluation),
                GameplayAction::NavigateNoFade(GameplayExit::Complete),
            ),
            (
                AppEffect::Navigate(Screen::SelectMusic),
                GameplayAction::Navigate(GameplayExit::Cancel),
            ),
            (
                AppEffect::NavigateNoFade(Screen::SelectMusic),
                GameplayAction::NavigateNoFade(GameplayExit::Cancel),
            ),
        ] {
            assert_eq!(gameplay_action_for_effect(&legacy), Some(action));
            let legacy = resolve_effect_route(legacy, context);
            let semantic = resolve_effect_route(
                AppEffect::Runtime(RuntimeRequest::GameplayAction(action)),
                context,
            );
            assert_eq!(legacy.action.flow_event(), semantic.action.flow_event());
            assert_eq!(legacy.clear_restart_pending, semantic.clear_restart_pending);
        }
        let none = resolve_effect_route(
            AppEffect::Runtime(RuntimeRequest::GameplayAction(GameplayAction::None)),
            context,
        );
        assert!(matches!(none.action, AppEffect::None));
        assert!(!none.clear_restart_pending);
        let restart = resolve_effect_route(
            AppEffect::Runtime(RuntimeRequest::GameplayAction(GameplayAction::Restart)),
            context,
        );
        assert!(matches!(
            restart.action,
            AppEffect::Runtime(RuntimeRequest::RestartGameplay)
        ));
        assert!(!restart.clear_restart_pending);
        let named = resolve_effect_route(AppEffect::NavigateNoFade(Screen::Menu), context);
        assert!(matches!(
            named.action,
            AppEffect::NavigateNoFade(Screen::Menu)
        ));
        assert!(!named.clear_restart_pending);
    }
}
