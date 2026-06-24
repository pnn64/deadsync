#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameplayFailType {
    Immediate,
    ImmediateContinue,
}

impl Default for GameplayFailType {
    fn default() -> Self {
        Self::ImmediateContinue
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldToExitKey {
    Start,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayMenuInput {
    P1Start,
    P2Start,
    P1Back,
    P2Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayMenuInputPlan {
    None,
    ArmHold(HoldToExitKey),
    AbortHold(HoldToExitKey),
    BeginExit(ExitTransitionKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutosyncMode {
    Off,
    Song,
    Machine,
}

#[inline(always)]
pub const fn autosync_mode_status_line(mode: AutosyncMode) -> Option<&'static str> {
    match mode {
        AutosyncMode::Off => None,
        AutosyncMode::Song => Some("AutoSync Song"),
        AutosyncMode::Machine => Some("AutoSync Machine"),
    }
}

#[inline(always)]
pub const fn next_autosync_mode(mode: AutosyncMode, course_active: bool) -> AutosyncMode {
    match mode {
        AutosyncMode::Off if course_active => AutosyncMode::Machine,
        AutosyncMode::Off => AutosyncMode::Song,
        AutosyncMode::Song => AutosyncMode::Machine,
        AutosyncMode::Machine => AutosyncMode::Off,
    }
}

#[inline(always)]
pub const fn gameplay_raw_key_plan(
    input: GameplayRawKeyInput,
    pressed: bool,
    allow_commands: bool,
    ctrl_held: bool,
    shift_held: bool,
    autosync_mode: AutosyncMode,
    course_active: bool,
    tick_mode: GameplayTimingTickMode,
    autoplay_enabled: bool,
) -> GameplayRawKeyPlan {
    if !pressed {
        return match input {
            GameplayRawKeyInput::OffsetAdjust(key) => GameplayRawKeyPlan::ClearOffsetAdjust(key),
            _ => GameplayRawKeyPlan::None,
        };
    }
    if !allow_commands {
        return GameplayRawKeyPlan::None;
    }
    match input {
        GameplayRawKeyInput::Restart if ctrl_held => GameplayRawKeyPlan::Restart,
        GameplayRawKeyInput::Autosync => {
            GameplayRawKeyPlan::SetAutosyncMode(next_autosync_mode(autosync_mode, course_active))
        }
        GameplayRawKeyInput::TimingTick => {
            GameplayRawKeyPlan::SetTimingTickMode(next_timing_tick_mode(tick_mode))
        }
        GameplayRawKeyInput::Autoplay => GameplayRawKeyPlan::SetAutoplayEnabled(!autoplay_enabled),
        GameplayRawKeyInput::OffsetAdjust(key) => {
            let target = offset_adjust_target(shift_held, course_active);
            GameplayRawKeyPlan::StartOffsetAdjust { key, target }
        }
        _ => GameplayRawKeyPlan::None,
    }
}

#[inline(always)]
pub const fn gameplay_raw_key_action_for_plan(plan: GameplayRawKeyPlan) -> RawKeyAction {
    match plan {
        GameplayRawKeyPlan::Restart => RawKeyAction::Restart,
        _ => RawKeyAction::None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTurnOption {
    #[default]
    None,
    Mirror,
    LRMirror,
    UDMirror,
    Left,
    Right,
    Shuffle,
    Blender,
    Random,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnScrollFlags {
    pub reverse: bool,
    pub split: bool,
    pub alternate: bool,
    pub cross: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollReverseOptions {
    pub reverse: f32,
    pub split: f32,
    pub alternate: f32,
    pub cross: f32,
}

#[inline(always)]
pub fn scroll_reverse_percent_for_column(
    options: ScrollReverseOptions,
    local_col: usize,
    num_cols: usize,
) -> f32 {
    if num_cols == 0 {
        return 0.0;
    }
    let mut percent = options.reverse;
    if local_col >= num_cols / 2 {
        percent += options.split;
    }
    if (local_col & 1) != 0 {
        percent += options.alternate;
    }
    let first_cross_col = num_cols / 4;
    let last_cross_col = num_cols.saturating_sub(first_cross_col + 1);
    if local_col >= first_cross_col && local_col <= last_cross_col {
        percent += options.cross;
    }
    if percent > 2.0 {
        percent = percent.rem_euclid(2.0);
    }
    if percent > 1.0 {
        return lerp(1.0, 0.0, percent - 1.0);
    }
    percent.clamp(0.0, 1.0)
}

#[inline(always)]
pub fn scroll_reverse_scale_for_column(
    options: ScrollReverseOptions,
    local_col: usize,
    num_cols: usize,
) -> f32 {
    1.0 - 2.0 * scroll_reverse_percent_for_column(options, local_col, num_cols)
}

pub fn column_scroll_dirs_for_flags(flags: ColumnScrollFlags, num_cols: usize) -> [f32; MAX_COLS] {
    let mut dirs = [1.0_f32; MAX_COLS];
    let n = num_cols.min(MAX_COLS);

    if flags.reverse {
        for d in dirs.iter_mut().take(n) {
            *d *= -1.0;
        }
    }
    if flags.split {
        for base in (0..n).step_by(4) {
            if base + 2 < n {
                dirs[base + 2] *= -1.0;
            }
            if base + 3 < n {
                dirs[base + 3] *= -1.0;
            }
        }
    }
    if flags.alternate {
        for base in (0..n).step_by(4) {
            if base + 1 < n {
                dirs[base + 1] *= -1.0;
            }
            if base + 3 < n {
                dirs[base + 3] *= -1.0;
            }
        }
    }
    if flags.cross {
        for base in (0..n).step_by(4) {
            if base + 1 < n {
                dirs[base + 1] *= -1.0;
            }
            if base + 2 < n {
                dirs[base + 2] *= -1.0;
            }
        }
    }
    dirs
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayConfig {
    pub mine_hit_sound: bool,
    pub default_fail_type: GameplayFailType,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub machine_pack_ini_offsets: bool,
    pub machine_default_sync_pref: SyncPref,
    pub machine_allow_per_player_global_offsets: bool,
    pub machine_enable_replays: bool,
    pub center_1player_notefield: bool,
    pub delayed_back: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameplaySetupRuntimeState {
    pub num_cols: usize,
    pub cols_per_player: usize,
    pub num_players: usize,
    pub viewport: GameplayViewport,
    pub session: GameplaySession,
    pub config: GameplayConfig,
}

#[derive(Clone, Debug)]
pub struct GameplaySourceRuntimeState {
    pub song: Arc<SongData>,
    pub charts: [Arc<ChartData>; MAX_PLAYERS],
    pub gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
}

#[inline(always)]
pub fn effective_player_global_offset_seconds(
    global_offset_seconds: f32,
    player_global_offset_shift_seconds: &[f32],
    player_idx: usize,
) -> f32 {
    global_offset_seconds
        + player_global_offset_shift_seconds
            .get(player_idx)
            .copied()
            .unwrap_or(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayOffsetState {
    global_offset_seconds: f32,
    initial_global_offset_seconds: f32,
    player_global_offset_shift_seconds: [f32; MAX_PLAYERS],
    song_offset_seconds: f32,
    initial_song_offset_seconds: f32,
}

impl GameplayOffsetState {
    #[inline(always)]
    pub const fn new(
        global_offset_seconds: f32,
        player_global_offset_shift_seconds: [f32; MAX_PLAYERS],
        song_offset_seconds: f32,
    ) -> Self {
        Self {
            global_offset_seconds,
            initial_global_offset_seconds: global_offset_seconds,
            player_global_offset_shift_seconds,
            song_offset_seconds,
            initial_song_offset_seconds: song_offset_seconds,
        }
    }

    #[inline(always)]
    pub fn global_offset_seconds(&self) -> f32 {
        self.global_offset_seconds
    }

    #[inline(always)]
    pub fn initial_global_offset_seconds(&self) -> f32 {
        self.initial_global_offset_seconds
    }

    #[inline(always)]
    pub fn song_offset_seconds(&self) -> f32 {
        self.song_offset_seconds
    }

    #[inline(always)]
    pub fn initial_song_offset_seconds(&self) -> f32 {
        self.initial_song_offset_seconds
    }

    #[inline(always)]
    pub fn player_global_offset_shift_seconds(&self, player_idx: usize) -> f32 {
        self.player_global_offset_shift_seconds
            .get(player_idx)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn effective_player_global_offset_seconds(&self, player_idx: usize) -> f32 {
        effective_player_global_offset_seconds(
            self.global_offset_seconds,
            &self.player_global_offset_shift_seconds,
            player_idx,
        )
    }

    #[inline(always)]
    pub fn set_global_offset_seconds(&mut self, seconds: f32) {
        self.global_offset_seconds = seconds;
    }

    #[inline(always)]
    pub fn set_song_offset_seconds(&mut self, seconds: f32) {
        self.song_offset_seconds = seconds;
    }

    #[inline(always)]
    pub fn set_player_global_offset_shift_seconds(&mut self, player_idx: usize, seconds: f32) {
        if let Some(shift) = self.player_global_offset_shift_seconds.get_mut(player_idx) {
            *shift = seconds;
        }
    }
}

impl Default for GameplayConfig {
    fn default() -> Self {
        Self {
            mine_hit_sound: true,
            default_fail_type: GameplayFailType::ImmediateContinue,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            machine_pack_ini_offsets: false,
            machine_default_sync_pref: SyncPref::Null,
            machine_allow_per_player_global_offsets: false,
            machine_enable_replays: true,
            center_1player_notefield: false,
            delayed_back: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayMiniIndicatorMode {
    #[default]
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayMiniIndicatorOptions {
    pub requested_mode: GameplayMiniIndicatorMode,
    pub measure_counter_enabled: bool,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
}

#[inline(always)]
pub const fn mini_indicator_mode_for_options(
    options: GameplayMiniIndicatorOptions,
) -> GameplayMiniIndicatorMode {
    match options.requested_mode {
        GameplayMiniIndicatorMode::None if options.subtractive_scoring => {
            GameplayMiniIndicatorMode::SubtractiveScoring
        }
        GameplayMiniIndicatorMode::None if options.pacemaker => {
            GameplayMiniIndicatorMode::Pacemaker
        }
        mode => mode,
    }
}

#[inline(always)]
pub fn mini_indicator_options<Profile: GameplayProfileData>(
    profile: &Profile,
) -> GameplayMiniIndicatorOptions {
    profile.mini_indicator_options()
}

#[inline(always)]
pub fn mini_indicator_mode<Profile: GameplayProfileData>(
    profile: &Profile,
) -> GameplayMiniIndicatorMode {
    mini_indicator_mode_for_options(mini_indicator_options(profile))
}

#[inline(always)]
pub fn needs_stream_data<Profile: GameplayProfileData>(profile: &Profile) -> bool {
    mini_indicator_needs_stream_data(mini_indicator_options(profile))
}

#[inline(always)]
pub const fn mini_indicator_needs_stream_data(options: GameplayMiniIndicatorOptions) -> bool {
    options.measure_counter_enabled
        || !matches!(
            mini_indicator_mode_for_options(options),
            GameplayMiniIndicatorMode::None
        )
}

#[derive(Clone, Debug)]
pub struct GameplayMiniIndicatorData {
    pub personal_best_percent: [Option<f64>; MAX_PLAYERS],
    pub machine_best_percent: [Option<f64>; MAX_PLAYERS],
}

impl Default for GameplayMiniIndicatorData {
    fn default() -> Self {
        Self {
            personal_best_percent: [None; MAX_PLAYERS],
            machine_best_percent: [None; MAX_PLAYERS],
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameplayMiniIndicatorRuntimeState {
    stream_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    total_stream_measures: [f32; MAX_PLAYERS],
    target_score_percent: [f64; MAX_PLAYERS],
    rival_score_percent: [f64; MAX_PLAYERS],
}

impl Default for GameplayMiniIndicatorRuntimeState {
    fn default() -> Self {
        Self {
            stream_segments: std::array::from_fn(|_| Vec::new()),
            total_stream_measures: [0.0; MAX_PLAYERS],
            target_score_percent: [89.0; MAX_PLAYERS],
            rival_score_percent: [0.0; MAX_PLAYERS],
        }
    }
}

impl GameplayMiniIndicatorRuntimeState {
    pub fn new(
        stream_segments: [Vec<StreamSegment>; MAX_PLAYERS],
        total_stream_measures: [f32; MAX_PLAYERS],
        target_score_percent: [f64; MAX_PLAYERS],
        rival_score_percent: [f64; MAX_PLAYERS],
    ) -> Self {
        Self {
            stream_segments,
            total_stream_measures,
            target_score_percent,
            rival_score_percent,
        }
    }

    #[inline(always)]
    pub fn stream_segments(&self, player: usize) -> &[StreamSegment] {
        self.stream_segments.get(player).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn total_stream_measures(&self, player: usize) -> f32 {
        self.total_stream_measures
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn target_score_percent(&self, player: usize) -> f64 {
        self.target_score_percent
            .get(player)
            .copied()
            .unwrap_or(89.0)
    }

    #[inline(always)]
    pub fn rival_score_percent(&self, player: usize) -> f64 {
        self.rival_score_percent.get(player).copied().unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn clear_stream_segments(&mut self) {
        for segments in &mut self.stream_segments {
            segments.clear();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LeadInTiming {
    pub min_seconds_to_step: f32,
    pub min_seconds_to_music: f32,
}

impl Default for LeadInTiming {
    fn default() -> Self {
        Self {
            min_seconds_to_step: MIN_SECONDS_TO_STEP,
            min_seconds_to_music: MIN_SECONDS_TO_MUSIC,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayAudioClockState {
    lead_in_seconds: f32,
    stream_position_seconds: f32,
    output_delay_seconds: f32,
}

impl GameplayAudioClockState {
    #[inline(always)]
    pub const fn new(
        lead_in_seconds: f32,
        stream_position_seconds: f32,
        output_delay_seconds: f32,
    ) -> Self {
        Self {
            lead_in_seconds,
            stream_position_seconds,
            output_delay_seconds,
        }
    }

    #[inline(always)]
    pub fn lead_in_seconds(&self) -> f32 {
        self.lead_in_seconds
    }

    #[inline(always)]
    pub fn positive_lead_in_seconds(&self) -> f32 {
        self.lead_in_seconds.max(0.0)
    }

    #[inline(always)]
    pub fn stream_position_seconds(&self) -> f32 {
        self.stream_position_seconds
    }

    #[inline(always)]
    pub fn output_delay_seconds(&self) -> f32 {
        self.output_delay_seconds
    }

    #[inline(always)]
    pub fn set_lead_in_seconds(&mut self, seconds: f32) {
        self.lead_in_seconds = seconds;
    }

    #[inline(always)]
    pub fn set_stream_position_seconds(&mut self, seconds: f32) {
        self.stream_position_seconds = seconds;
    }

    #[inline(always)]
    pub fn set_output_delay_seconds(&mut self, seconds: f32) {
        self.output_delay_seconds = seconds.max(0.0);
    }

    #[inline(always)]
    pub fn set_audio_snapshot(&mut self, snapshot: GameplayAudioSnapshot) {
        self.stream_position_seconds = snapshot.stream_clock.stream_seconds;
        self.output_delay_seconds = snapshot.output_delay_seconds.max(0.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayMusicRateState {
    rate: f32,
}

impl GameplayMusicRateState {
    #[inline(always)]
    pub fn new(rate: f32) -> Self {
        Self {
            rate: normalized_song_rate(rate),
        }
    }

    #[inline(always)]
    pub fn rate(&self) -> f32 {
        self.rate
    }

    #[inline(always)]
    pub fn set_rate(&mut self, rate: f32) -> bool {
        let normalized = normalized_song_rate(rate);
        if (normalized - self.rate).abs() <= f32::EPSILON {
            return false;
        }
        self.rate = normalized;
        true
    }
}

impl Default for GameplayMusicRateState {
    fn default() -> Self {
        Self { rate: 1.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayClockRuntimeState {
    pub audio_clock: GameplayAudioClockState,
    pub song_position: GameplaySongPositionState,
    pub display_clock: GameplayDisplayClockState,
    pub end_timing: GameplayEndTimingState,
    pub music_rate: GameplayMusicRateState,
    pub offsets: GameplayOffsetState,
    pub visible_timing: GameplayVisibleTimingState,
}

#[derive(Clone, Debug)]
pub struct GameplayTimingRuntimeState {
    pub timing: Arc<TimingData>,
    pub timing_players: [Arc<TimingData>; MAX_PLAYERS],
    pub beat_info_cache: BeatInfoCache,
    pub timing_profile: TimingProfile,
    pub player_judgment_timing: [PlayerJudgmentTiming; MAX_PLAYERS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplaySongPositionState {
    pub current_beat: f32,
    pub current_music_time_ns: SongTimeNs,
    pub current_beat_display: f32,
    pub current_music_time_display: f32,
}

impl GameplaySongPositionState {
    #[inline(always)]
    pub const fn new(
        current_beat: f32,
        current_music_time_ns: SongTimeNs,
        current_beat_display: f32,
        current_music_time_display: f32,
    ) -> Self {
        Self {
            current_beat,
            current_music_time_ns,
            current_beat_display,
            current_music_time_display,
        }
    }

    #[inline(always)]
    pub fn set_music_position(&mut self, current_beat: f32, current_music_time_ns: SongTimeNs) {
        self.current_beat = current_beat;
        self.current_music_time_ns = current_music_time_ns;
    }

    #[inline(always)]
    pub fn set_display_position(
        &mut self,
        current_beat_display: f32,
        current_music_time_display: f32,
    ) {
        self.current_beat_display = current_beat_display;
        self.current_music_time_display = current_music_time_display;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayStreamClockSnapshot {
    pub stream_seconds: f32,
    pub music_nanos: SongTimeNs,
    pub music_seconds_per_second: f32,
    pub has_music_mapping: bool,
    pub valid_at: Instant,
    pub valid_at_host_nanos: u64,
}

impl Default for GameplayStreamClockSnapshot {
    fn default() -> Self {
        Self {
            stream_seconds: 0.0,
            music_nanos: 0,
            music_seconds_per_second: 1.0,
            has_music_mapping: false,
            valid_at: Instant::now(),
            valid_at_host_nanos: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayAudioSnapshot {
    pub stream_clock: GameplayStreamClockSnapshot,
    pub assist_sfx_generation: u64,
    pub output_delay_seconds: f32,
    pub timing_diag_enabled: bool,
    pub timing_diag_callback_gap_ns: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SongClockSnapshot {
    pub song_time_ns: SongTimeNs,
    pub seconds_per_second: f32,
    pub mapped_audio: bool,
    pub valid_at: Instant,
    pub valid_at_host_nanos: u64,
    pub timing_diag_enabled: bool,
    pub timing_diag_callback_gap_ns: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayFrameClockUpdate {
    pub song_clock: SongClockSnapshot,
    pub previous_music_time_ns: SongTimeNs,
    pub music_time_ns: SongTimeNs,
    pub music_time_sec: f32,
    pub display_music_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayFrameBeginUpdate {
    pub clock: GameplayFrameClockUpdate,
    pub hold_to_exit_completed: bool,
}

#[inline(always)]
pub fn current_song_clock_snapshot(
    audio_snapshot: GameplayAudioSnapshot,
    music_rate: f32,
    audio_lead_in_seconds: f32,
    global_offset_seconds: f32,
) -> SongClockSnapshot {
    let stream_clock = audio_snapshot.stream_clock;
    let fallback_rate = normalized_song_rate(music_rate);
    if stream_clock.has_music_mapping {
        return SongClockSnapshot {
            song_time_ns: stream_clock.music_nanos,
            seconds_per_second: if stream_clock.music_seconds_per_second.is_finite()
                && stream_clock.music_seconds_per_second > 0.0
            {
                stream_clock.music_seconds_per_second
            } else {
                fallback_rate
            },
            mapped_audio: true,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
            timing_diag_enabled: audio_snapshot.timing_diag_enabled,
            timing_diag_callback_gap_ns: audio_snapshot.timing_diag_callback_gap_ns,
        };
    }

    let song_time = music_time_from_stream_position(
        stream_clock.stream_seconds,
        audio_lead_in_seconds,
        global_offset_seconds,
        fallback_rate,
    );
    SongClockSnapshot {
        song_time_ns: song_time_ns_from_seconds(song_time),
        seconds_per_second: fallback_rate,
        mapped_audio: false,
        valid_at: stream_clock.valid_at,
        valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        timing_diag_enabled: audio_snapshot.timing_diag_enabled,
        timing_diag_callback_gap_ns: audio_snapshot.timing_diag_callback_gap_ns,
    }
}

#[inline(always)]
pub fn song_clock_music_time_ns(
    snapshot: SongClockSnapshot,
    captured_at: Instant,
    captured_host_nanos: u64,
) -> SongTimeNs {
    let slope = normalized_song_rate(snapshot.seconds_per_second);
    if snapshot.valid_at_host_nanos != 0 && captured_host_nanos != 0 {
        let dt_nanos = captured_host_nanos as i128 - snapshot.valid_at_host_nanos as i128;
        return clamp_song_time_ns(
            i128::from(snapshot.song_time_ns) + scaled_song_delta_ns(dt_nanos, slope),
        );
    }
    let delta_host_nanos = if let Some(age) = snapshot.valid_at.checked_duration_since(captured_at)
    {
        -(age.as_nanos() as i128)
    } else if let Some(lead) = captured_at.checked_duration_since(snapshot.valid_at) {
        lead.as_nanos() as i128
    } else {
        0
    };
    clamp_song_time_ns(
        i128::from(snapshot.song_time_ns) + scaled_song_delta_ns(delta_host_nanos, slope),
    )
}

pub fn music_time_ns_from_song_clock(
    snapshot: SongClockSnapshot,
    captured_at: Instant,
    captured_host_nanos: u64,
) -> SongTimeNs {
    let slope = normalized_song_rate(snapshot.seconds_per_second);
    let snapshot_song_time = song_time_ns_to_seconds(snapshot.song_time_ns);
    if snapshot.valid_at_host_nanos != 0 && captured_host_nanos != 0 {
        let dt_nanos = captured_host_nanos as i128 - snapshot.valid_at_host_nanos as i128;
        if snapshot.timing_diag_enabled {
            log::debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=host callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                dt_nanos as f64 * 1e-6,
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        return song_clock_music_time_ns(snapshot, captured_at, captured_host_nanos);
    }
    if let Some(age) = snapshot.valid_at.checked_duration_since(captured_at) {
        if snapshot.timing_diag_enabled {
            log::debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                -(age.as_secs_f64() * 1000.0),
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
    } else if let Some(lead) = captured_at.checked_duration_since(snapshot.valid_at) {
        if snapshot.timing_diag_enabled {
            log::debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                lead.as_secs_f64() * 1000.0,
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
    } else if snapshot.timing_diag_enabled {
        log::debug!(
            "AUDIO_DIAG snap_age_ms=0.000 path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
            snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
            snapshot_song_time,
            slope,
            snapshot.valid_at_host_nanos,
            captured_host_nanos,
        );
    }
    song_clock_music_time_ns(snapshot, captured_at, captured_host_nanos)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayMusicCut {
    pub start_sec: f64,
    pub length_sec: f64,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}

impl Default for GameplayMusicCut {
    fn default() -> Self {
        Self {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GameplayAudioCommand {
    StopMusic,
    PlayMusic {
        path: PathBuf,
        cut: GameplayMusicCut,
        looping: bool,
        rate: f32,
    },
    PlayPreloadedSfx(&'static str),
    PlayPreloadedAssistTick(&'static str),
    PlayAssistTickAtMusicTime {
        path: &'static str,
        music_seconds: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplaySessionCommand {
    SetTimingTickMode(GameplayTimingTickMode),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameplayCommandQueue {
    audio: Vec<GameplayAudioCommand>,
    session: Vec<GameplaySessionCommand>,
}

impl GameplayCommandQueue {
    #[inline(always)]
    pub fn with_capacity(audio_capacity: usize, session_capacity: usize) -> Self {
        Self {
            audio: Vec::with_capacity(audio_capacity),
            session: Vec::with_capacity(session_capacity),
        }
    }

    #[inline(always)]
    pub fn push_audio(&mut self, command: GameplayAudioCommand) {
        self.audio.push(command);
    }

    #[inline(always)]
    pub fn push_session(&mut self, command: GameplaySessionCommand) {
        self.session.push(command);
    }

    #[inline(always)]
    pub fn drain_audio(&mut self) -> std::vec::Drain<'_, GameplayAudioCommand> {
        self.audio.drain(..)
    }

    #[inline(always)]
    pub fn drain_session(&mut self) -> std::vec::Drain<'_, GameplaySessionCommand> {
        self.session.drain(..)
    }
}

