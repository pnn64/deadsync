#[derive(Clone, Debug, PartialEq)]
pub struct ChartAttackWindow {
    pub start_second: f32,
    pub len_seconds: f32,
    pub mods: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayAttackMode {
    #[default]
    Off,
    On,
    Random,
}

pub const RANDOM_ATTACK_RUN_TIME_SECONDS: f32 = 6.0;
pub const RANDOM_ATTACK_OVERLAP_SECONDS: f32 = 0.5;
pub const RANDOM_ATTACK_START_SECONDS_INIT: f32 = -1.0;
pub const RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS: f32 = 1.0;

// Mirrors ITGmania Data/RandomAttacks.txt categories for mods deadsync currently supports.
pub const RANDOM_ATTACK_MOD_POOL: [&str; 29] = [
    "0.5x",
    "1x",
    "1.5x",
    "2x",
    "boost",
    "brake",
    "wave",
    "expand",
    "drunk",
    "dizzy",
    "confusion",
    "65% mini",
    "20% flip",
    "30% invert",
    "30% tornado",
    "tipsy",
    "beat",
    "bumpy",
    "50% hidden",
    "50% sudden",
    "30% blink",
    "30% reverse",
    "reverse",
    "centered",
    "hallway",
    "space",
    "incoming",
    "overhead",
    "distant",
];

#[inline(always)]
pub fn chart_effects_from_profile<Profile: GameplayProfileData>(
    profile: &Profile,
) -> ChartAttackEffects {
    profile.chart_effects()
}

#[inline(always)]
pub fn perspective_effects_from_profile<Profile: GameplayProfileData>(
    profile: &Profile,
) -> PerspectiveEffects {
    profile.perspective_effects()
}

#[inline(always)]
pub fn scroll_effects_from_flags(
    reverse: bool,
    split: bool,
    alternate: bool,
    cross: bool,
    centered: bool,
) -> ScrollEffects {
    ScrollEffects::from_flags(reverse, split, alternate, cross, centered)
}

#[inline(always)]
pub fn base_appearance_effects<Profile: GameplayProfileData>(
    profile: &Profile,
) -> AppearanceEffects {
    profile.appearance_effects()
}

#[inline(always)]
pub fn base_visual_effects<Profile: GameplayProfileData>(profile: &Profile) -> VisualEffects {
    profile.visual_effects()
}

#[inline(always)]
pub fn build_attack_mask_windows_for_player(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<AttackMaskWindow> {
    build_attack_mask_windows_for_mode(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    )
}

#[inline(always)]
pub fn player_changes_chart<Profile: GameplayProfileData>(
    chart: &GameplayChartData,
    profile: &Profile,
) -> bool {
    player_chart_changes_for_options(
        profile.chart_effects().has_note_masks(),
        profile.turn_option(),
        chart.chart_attacks.as_deref(),
        profile.attack_mode(),
    )
}

pub fn parse_chart_attack_windows(raw: &str) -> Vec<ChartAttackWindow> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let upper = raw.to_ascii_uppercase();
    let mut starts = Vec::with_capacity(8);
    let mut scan = 0usize;
    while let Some(pos) = upper[scan..].find("TIME=") {
        let idx = scan + pos;
        starts.push(idx);
        scan = idx.saturating_add(5);
        if scan >= raw.len() {
            break;
        }
    }
    if starts.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(starts.len());
    for (i, start) in starts.iter().copied().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(raw.len());
        let chunk = &raw[start..end];
        let mut time = None;
        let mut len = None;
        let mut end_time = None;
        let mut mods = None;

        for part in chunk.split(':') {
            let part = part.trim();
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            let key = k.trim().to_ascii_uppercase();
            let value = v.trim().trim_end_matches(',').trim();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "TIME" => time = value.parse::<f32>().ok(),
                "LEN" => len = value.parse::<f32>().ok(),
                "END" => end_time = value.parse::<f32>().ok(),
                "MODS" => mods = Some(value.to_string()),
                _ => {}
            }
        }

        let (Some(start_second), Some(mods)) = (time, mods) else {
            continue;
        };
        if !start_second.is_finite() || mods.is_empty() {
            continue;
        }
        let mut len_seconds = len.unwrap_or(0.0);
        if let Some(end_second) = end_time
            && end_second.is_finite()
        {
            len_seconds = end_second - start_second;
        }
        if !len_seconds.is_finite() || len_seconds < 0.0 {
            len_seconds = 0.0;
        }
        out.push(ChartAttackWindow {
            start_second,
            len_seconds,
            mods,
        });
    }

    out
}

#[inline(always)]
pub fn random_attack_seed(base_seed: u64, player: usize, attacks_len: usize) -> u64 {
    base_seed
        ^ (0xC2B2_AE3D_27D4_EB4F_u64.wrapping_mul(player as u64 + 1))
        ^ (attacks_len as u64).wrapping_mul(0x9E37_79B9_u64)
}

pub fn build_random_attack_windows(
    song_length_seconds: f32,
    player: usize,
    base_seed: u64,
) -> Vec<ChartAttackWindow> {
    if !song_length_seconds.is_finite() || song_length_seconds <= 0.0 {
        return Vec::new();
    }
    let period = (RANDOM_ATTACK_RUN_TIME_SECONDS - RANDOM_ATTACK_OVERLAP_SECONDS).max(0.0);
    if period <= f32::EPSILON || RANDOM_ATTACK_MOD_POOL.is_empty() {
        return Vec::new();
    }
    let first_start =
        (period + RANDOM_ATTACK_START_SECONDS_INIT).max(RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS);
    if first_start >= song_length_seconds {
        return Vec::new();
    }

    let max_windows = ((song_length_seconds - first_start) / period)
        .floor()
        .max(0.0) as usize
        + 1;
    let mut out = Vec::with_capacity(max_windows);
    let mut rng = TurnRng::new(random_attack_seed(base_seed, player, max_windows));
    let mut start = first_start;
    while start < song_length_seconds {
        let mod_idx = rng.gen_range(RANDOM_ATTACK_MOD_POOL.len());
        out.push(ChartAttackWindow {
            start_second: start,
            len_seconds: RANDOM_ATTACK_RUN_TIME_SECONDS,
            mods: RANDOM_ATTACK_MOD_POOL[mod_idx].to_string(),
        });
        start += period;
    }
    out
}

pub fn build_attack_windows_for_mode(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<ChartAttackWindow> {
    match attack_mode {
        GameplayAttackMode::Off => Vec::new(),
        GameplayAttackMode::On => chart_attacks
            .map(parse_chart_attack_windows)
            .unwrap_or_default(),
        GameplayAttackMode::Random => {
            build_random_attack_windows(song_length_seconds, player, base_seed)
        }
    }
}

pub fn build_attack_mask_windows_for_mode(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let attacks = build_attack_windows_for_mode(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if attacks.is_empty() {
        return Vec::new();
    }
    build_attack_mask_windows(&attacks)
}

#[derive(Clone, Copy, Debug)]
pub struct ParsedAttackMods {
    pub insert_mask: u8,
    pub remove_mask: u8,
    pub holds_mask: u8,
    pub turn_option: GameplayTurnOption,
    pub clear_all: bool,
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub visual_speed: VisualOverrides,
    pub appearance: AppearanceOverrides,
    pub appearance_speed: AppearanceOverrides,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub scroll_approach_speed: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
    pub mini_speed: Option<f32>,
}

impl Default for ParsedAttackMods {
    fn default() -> Self {
        Self {
            insert_mask: 0,
            remove_mask: 0,
            holds_mask: 0,
            turn_option: GameplayTurnOption::None,
            clear_all: false,
            accel: AccelOverrides::default(),
            visual: VisualOverrides::default(),
            visual_speed: VisualOverrides::default(),
            appearance: AppearanceOverrides::default(),
            appearance_speed: AppearanceOverrides::default(),
            visibility: VisibilityOverrides::default(),
            scroll: ScrollOverrides::default(),
            scroll_approach_speed: ScrollOverrides::default(),
            perspective: PerspectiveOverrides::default(),
            scroll_speed: None,
            mini_percent: None,
            mini_speed: None,
        }
    }
}

impl ParsedAttackMods {
    #[inline(always)]
    pub fn has_chart_effect(self) -> bool {
        self.insert_mask != 0
            || self.remove_mask != 0
            || self.holds_mask != 0
            || self.turn_option != GameplayTurnOption::None
    }

    #[inline(always)]
    pub fn has_runtime_mask_effect(self) -> bool {
        self.clear_all
            || self.accel.any()
            || self.visual.any()
            || self.appearance.any()
            || self.visibility.any()
            || self.scroll.any()
            || self.perspective.any()
            || self.scroll_speed.is_some()
            || self.mini_percent.is_some()
    }
}

pub fn chart_attacks_enabled_for_mode(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
) -> bool {
    match attack_mode {
        GameplayAttackMode::Off => false,
        GameplayAttackMode::On => chart_attacks.is_some_and(|raw| !raw.trim().is_empty()),
        GameplayAttackMode::Random => true,
    }
}

pub fn player_chart_changes_for_options(
    has_uncommon_masks: bool,
    turn_option: GameplayTurnOption,
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
) -> bool {
    has_uncommon_masks
        || turn_option != GameplayTurnOption::None
        || chart_attacks_enabled_for_mode(chart_attacks, attack_mode)
}

pub fn begin_outro_attack_visual_clear(
    attacks_cleared_for_outro: &mut bool,
    num_players: usize,
    active_attack_visual: &[VisualOverrides; MAX_PLAYERS],
    outro_attack_visual: &mut [VisualOverrides; MAX_PLAYERS],
) {
    if *attacks_cleared_for_outro {
        return;
    }
    *attacks_cleared_for_outro = true;
    let player_count = num_players.min(MAX_PLAYERS);
    outro_attack_visual[..player_count]
        .copy_from_slice(&active_attack_visual[..player_count]);
}

#[derive(Clone, Copy, Debug)]
pub struct AttackMaskWindow {
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub persist_after_end: bool,
    pub clear_all: bool,
    pub chart: ChartAttackEffects,
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub visual_speed: VisualOverrides,
    pub appearance: AppearanceOverrides,
    pub appearance_speed: AppearanceOverrides,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub scroll_approach_speed: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
    pub mini_mode: MiniAttackMode,
    pub mini_speed: Option<f32>,
}

pub fn build_song_lua_constant_attack_mask_window(
    start_second: f32,
    end_second: f32,
    mods: &str,
) -> Option<AttackMaskWindow> {
    if end_second <= start_second {
        return None;
    }
    let mods = parse_song_lua_runtime_mods(mods);
    if !mods.has_runtime_mask_effect() {
        return None;
    }
    Some(AttackMaskWindow {
        start_second,
        end_second,
        sustain_end_second: f32::MAX,
        persist_after_end: true,
        clear_all: mods.clear_all,
        chart: ChartAttackEffects::default(),
        accel: mods.accel,
        visual: mods.visual,
        visual_speed: mods.visual_speed,
        appearance: mods.appearance,
        appearance_speed: mods.appearance_speed,
        visibility: mods.visibility,
        scroll: mods.scroll,
        scroll_approach_speed: mods.scroll_approach_speed,
        perspective: mods.perspective,
        scroll_speed: mods.scroll_speed,
        mini_percent: mods.mini_percent,
        mini_mode: MiniAttackMode::Delta,
        mini_speed: mods.mini_speed,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaEaseMaskTarget {
    AccelBoost,
    AccelBrake,
    AccelWave,
    AccelExpand,
    AccelBoomerang,
    VisualDrunk,
    VisualDizzy,
    VisualConfusion,
    VisualConfusionOffset,
    VisualConfusionOffsetColumn(usize),
    VisualFlip,
    VisualInvert,
    VisualTornado,
    VisualTipsy,
    VisualTiny,
    VisualBumpy,
    VisualBumpyOffset,
    VisualBumpyPeriod,
    VisualBumpyColumn(usize),
    VisualTinyColumn(usize),
    VisualMoveXColumn(usize),
    VisualMoveYColumn(usize),
    VisualPulseInner,
    VisualPulseOuter,
    VisualPulsePeriod,
    VisualPulseOffset,
    VisualBeat,
    AppearanceHidden,
    AppearanceSudden,
    AppearanceStealth,
    AppearanceBlink,
    AppearanceRandomVanish,
    VisibilityDark,
    VisibilityBlind,
    VisibilityCover,
    ScrollReverse,
    ScrollSplit,
    ScrollAlternate,
    ScrollCross,
    ScrollCentered,
    PerspectiveTilt,
    PerspectiveSkew,
    ScrollSpeedX,
    ScrollSpeedC,
    ScrollSpeedM,
    MiniPercent,
    PlayerX,
    PlayerY,
    PlayerZ,
    PlayerRotationX,
    PlayerRotationZ,
    PlayerRotationY,
    PlayerSkewX,
    PlayerSkewY,
    PlayerZoom,
    PlayerZoomX,
    PlayerZoomY,
    PlayerZoomZ,
    ConfusionYOffsetY,
}

#[derive(Clone, Debug)]
pub struct SongLuaEaseMaskWindow {
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub target: SongLuaEaseMaskTarget,
    pub from: f32,
    pub to: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaRuntimeEaseTarget<'a> {
    Mod(&'a str),
    Player(SongLuaEaseMaskTarget),
    Function,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SongLuaRuntimeEaseTargetOwned {
    Mod(String),
    Player(SongLuaEaseMaskTarget),
    Function,
}

pub trait SongLuaRuntimeEaseTargetLike {
    fn as_runtime_ease_target(&self) -> SongLuaRuntimeEaseTarget<'_>;
}

impl<'a> SongLuaRuntimeEaseTargetLike for SongLuaRuntimeEaseTarget<'a> {
    #[inline(always)]
    fn as_runtime_ease_target(&self) -> SongLuaRuntimeEaseTarget<'_> {
        *self
    }
}

impl SongLuaRuntimeEaseTargetLike for SongLuaRuntimeEaseTargetOwned {
    fn as_runtime_ease_target(&self) -> SongLuaRuntimeEaseTarget<'_> {
        match self {
            SongLuaRuntimeEaseTargetOwned::Mod(target_name) => {
                SongLuaRuntimeEaseTarget::Mod(target_name.as_str())
            }
            SongLuaRuntimeEaseTargetOwned::Player(target) => {
                SongLuaRuntimeEaseTarget::Player(*target)
            }
            SongLuaRuntimeEaseTargetOwned::Function => SongLuaRuntimeEaseTarget::Function,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaRuntimeEaseAppend {
    Appended,
    Unsupported,
    Ignored,
}

#[derive(Clone, Debug)]
pub struct SongLuaColumnOffsetWindowRuntime {
    pub column: usize,
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub from_y: f32,
    pub to_y: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

pub fn build_song_lua_column_offset_window_runtime(
    column: usize,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    from_y: f32,
    to_y: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> SongLuaColumnOffsetWindowRuntime {
    SongLuaColumnOffsetWindowRuntime {
        column,
        start_second,
        end_second,
        sustain_end_second,
        from_y,
        to_y,
        easing: easing.map(ToString::to_string),
        opt1,
        opt2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaNoteHideWindowRuntime {
    pub column: usize,
    pub start_beat: f32,
    pub end_beat: f32,
}

#[inline(always)]
pub const fn build_song_lua_note_hide_window_runtime(
    column: usize,
    start_beat: f32,
    end_beat: f32,
) -> SongLuaNoteHideWindowRuntime {
    SongLuaNoteHideWindowRuntime {
        column,
        start_beat,
        end_beat,
    }
}

pub fn build_song_lua_note_hide_windows_for_players(
    hides: impl IntoIterator<Item = (usize, usize, f32, f32)>,
) -> [Vec<SongLuaNoteHideWindowRuntime>; MAX_PLAYERS] {
    let mut out: [Vec<SongLuaNoteHideWindowRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    for (player, column, start_beat, end_beat) in hides {
        if player < MAX_PLAYERS {
            out[player].push(build_song_lua_note_hide_window_runtime(
                column, start_beat, end_beat,
            ));
        }
    }
    out
}

pub fn build_song_lua_hidden_players(flags: &[bool]) -> [bool; MAX_PLAYERS] {
    let mut out = [false; MAX_PLAYERS];
    out[..flags.len().min(MAX_PLAYERS)].copy_from_slice(&flags[..flags.len().min(MAX_PLAYERS)]);
    out
}

pub fn apply_song_lua_player_actor_overrides<CapturedActor: Clone>(
    player_actors: &mut [CapturedActor; MAX_PLAYERS],
    overrides: &[CapturedActor],
) {
    let count = overrides.len().min(MAX_PLAYERS);
    player_actors[..count].clone_from_slice(&overrides[..count]);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaOverlayMessageRuntime {
    pub event_second: f32,
    pub command_index: usize,
}

#[inline(always)]
pub const fn build_song_lua_overlay_message_runtime(
    event_second: f32,
    command_index: usize,
) -> SongLuaOverlayMessageRuntime {
    SongLuaOverlayMessageRuntime {
        event_second,
        command_index,
    }
}

pub fn build_song_lua_message_command_indices<'a>(
    commands: impl IntoIterator<Item = (usize, &'a str)>,
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (idx, command) in commands {
        out.entry(command.to_ascii_lowercase()).or_insert(idx);
    }
    out
}

#[inline(always)]
pub fn song_lua_message_command_index(
    indices: &BTreeMap<String, usize>,
    message: &str,
) -> Option<usize> {
    const STACK_MESSAGE_BYTES: usize = 128;

    if !message.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return indices.get(message).copied();
    }
    if message.len() <= STACK_MESSAGE_BYTES {
        let mut normalized = [0u8; STACK_MESSAGE_BYTES];
        normalized[..message.len()].copy_from_slice(message.as_bytes());
        normalized[..message.len()].make_ascii_lowercase();
        let normalized = std::str::from_utf8(&normalized[..message.len()])
            .expect("ASCII case folding preserves UTF-8");
        return indices.get(normalized).copied();
    }
    indices.get(&message.to_ascii_lowercase()).copied()
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn song_lua_message_command_index_legacy_for_bench(
    indices: &BTreeMap<String, usize>,
    message: &str,
) -> Option<usize> {
    indices.get(&message.to_ascii_lowercase()).copied()
}

pub fn build_song_lua_message_seconds(
    beats: impl IntoIterator<Item = f32>,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<Option<f32>> {
    beats
        .into_iter()
        .map(|beat| song_lua_message_second(beat, timing_player, global_offset_seconds))
        .collect()
}

pub fn build_song_lua_actor_message_events_with_seconds<'a>(
    messages: impl IntoIterator<Item = (usize, &'a str)>,
    message_seconds: &[Option<f32>],
    commands: impl IntoIterator<Item = (usize, &'a str)>,
) -> Vec<SongLuaOverlayMessageRuntime> {
    let command_indices = build_song_lua_message_command_indices(commands);
    let mut out = Vec::new();
    for (idx, message) in messages {
        let Some(event_second) = message_seconds.get(idx).copied().flatten() else {
            continue;
        };
        let Some(command_index) = song_lua_message_command_index(&command_indices, message) else {
            continue;
        };
        out.push(build_song_lua_overlay_message_runtime(
            event_second,
            command_index,
        ));
    }
    out
}

pub fn build_song_lua_player_message_events<Actor>(
    actors: &[Actor],
    mut events_for_actor: impl FnMut(&Actor) -> Vec<SongLuaOverlayMessageRuntime>,
) -> [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] {
    let mut out: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    for (player, actor) in actors.iter().take(MAX_PLAYERS).enumerate() {
        out[player] = events_for_actor(actor);
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct SongLuaOverlayEaseWindowRuntime<StateDelta> {
    pub overlay_index: usize,
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub cutoff_second: Option<f32>,
    pub from: StateDelta,
    pub to: StateDelta,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SongLuaVisualLayerRuntime<OverlayActor, CapturedActor, StateDelta> {
    pub start_second: f32,
    pub screen_width: f32,
    pub screen_height: f32,
    pub overlays: Vec<OverlayActor>,
    pub overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    pub overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    pub overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    pub song_foreground: CapturedActor,
    pub song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
}

#[derive(Clone, Debug)]
pub struct SongLuaRuntimeVisuals<OverlayActor, CapturedActor, StateDelta> {
    pub overlays: Vec<OverlayActor>,
    pub overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    pub overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    pub overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    pub background_visual_layers:
        Vec<SongLuaVisualLayerRuntime<OverlayActor, CapturedActor, StateDelta>>,
    pub foreground_visual_layers:
        Vec<SongLuaVisualLayerRuntime<OverlayActor, CapturedActor, StateDelta>>,
    pub player_actors: [CapturedActor; MAX_PLAYERS],
    pub player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS],
    pub song_foreground: CapturedActor,
    pub song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
    pub hidden_players: [bool; MAX_PLAYERS],
    pub note_hides: [Vec<SongLuaNoteHideWindowRuntime>; MAX_PLAYERS],
    pub column_offsets: [Vec<SongLuaColumnOffsetWindowRuntime>; MAX_PLAYERS],
    pub screen_width: f32,
    pub screen_height: f32,
}

pub type SongLuaRuntimeBuildOutput<OverlayActor, CapturedActor, StateDelta> = (
    [Vec<AttackMaskWindow>; MAX_PLAYERS],
    [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS],
    SongLuaRuntimeVisuals<OverlayActor, CapturedActor, StateDelta>,
);

pub fn build_song_lua_runtime_visuals<OverlayActor, CapturedActor, StateDelta>(
    overlays: Vec<OverlayActor>,
    overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    background_visual_layers: Vec<
        SongLuaVisualLayerRuntime<OverlayActor, CapturedActor, StateDelta>,
    >,
    foreground_visual_layers: Vec<
        SongLuaVisualLayerRuntime<OverlayActor, CapturedActor, StateDelta>,
    >,
    player_actors: [CapturedActor; MAX_PLAYERS],
    player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS],
    song_foreground: CapturedActor,
    song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
    hidden_players: [bool; MAX_PLAYERS],
    note_hides: [Vec<SongLuaNoteHideWindowRuntime>; MAX_PLAYERS],
    column_offsets: [Vec<SongLuaColumnOffsetWindowRuntime>; MAX_PLAYERS],
    screen_width: f32,
    screen_height: f32,
) -> SongLuaRuntimeVisuals<OverlayActor, CapturedActor, StateDelta> {
    SongLuaRuntimeVisuals {
        overlays,
        overlay_eases,
        overlay_ease_ranges,
        overlay_events,
        background_visual_layers,
        foreground_visual_layers,
        player_actors,
        player_events,
        song_foreground,
        song_foreground_events,
        hidden_players,
        note_hides,
        column_offsets,
        screen_width,
        screen_height,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaPlayerActorDefault {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy)]
pub struct SongLuaRuntimeWindowBuild<'a> {
    pub song_title: &'a str,
    pub timing_players: [&'a TimingData; MAX_PLAYERS],
    pub num_players: usize,
    pub machine_global_offset_seconds: f32,
    pub player_global_offset_shift_seconds: &'a [f32; MAX_PLAYERS],
    pub screen_width: f32,
    pub screen_height: f32,
    pub player_actor_defaults: [SongLuaPlayerActorDefault; MAX_PLAYERS],
}

pub trait SongLuaRuntimeBuilder<OverlayActor, CapturedActor, StateDelta> {
    fn build_song_lua_runtime(
        self,
        params: SongLuaRuntimeWindowBuild<'_>,
    ) -> SongLuaRuntimeBuildOutput<OverlayActor, CapturedActor, StateDelta>;
}

pub fn build_song_lua_runtime_window_build<'a, Profile: GameplayProfileData>(
    song_title: &'a str,
    timing_players: &'a [Arc<TimingData>; MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[Profile; MAX_PLAYERS],
    machine_global_offset_seconds: f32,
    viewport: GameplayViewport,
    session: &GameplaySession,
    center_1player_notefield: bool,
    player_global_offset_shift_seconds: &'a [f32; MAX_PLAYERS],
) -> SongLuaRuntimeWindowBuild<'a> {
    let timing_player_refs = std::array::from_fn(|player| timing_players[player].as_ref());
    let note_field_offsets_x = std::array::from_fn(|player| {
        if player < num_players {
            player_profiles[player].note_field_offset_x()
        } else {
            0.0
        }
    });
    let player_actor_defaults = build_song_lua_player_actor_defaults_like(
        num_players,
        viewport,
        session.play_style,
        gameplay_is_single_p2_side(session.play_style, session.player_side),
        note_field_offsets_x,
        center_1player_notefield,
    );
    SongLuaRuntimeWindowBuild {
        song_title,
        timing_players: timing_player_refs,
        num_players,
        machine_global_offset_seconds,
        player_global_offset_shift_seconds,
        screen_width: viewport.width(),
        screen_height: viewport.height(),
        player_actor_defaults,
    }
}

pub fn build_song_lua_player_actor_defaults_like<PlayStyle>(
    num_players: usize,
    viewport: GameplayViewport,
    play_style: PlayStyle,
    single_player_uses_p2_side: bool,
    note_field_offsets_x: [f32; MAX_PLAYERS],
    center_1player_notefield: bool,
) -> [SongLuaPlayerActorDefault; MAX_PLAYERS]
where
    PlayStyle: SongLuaCompilePlayStyleLike + Copy,
{
    std::array::from_fn(|player_index| SongLuaPlayerActorDefault {
        x: if player_index < num_players {
            song_lua_compile_player_screen_x_like(
                num_players,
                player_index,
                viewport,
                play_style,
                single_player_uses_p2_side,
                note_field_offsets_x[player_index],
                center_1player_notefield,
            )
        } else {
            viewport.center_x()
        },
        y: viewport.center_y(),
    })
}

pub fn build_song_lua_overlay_ease_window_runtime<StateDelta>(
    overlay_index: usize,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    cutoff_second: Option<f32>,
    from: StateDelta,
    to: StateDelta,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> SongLuaOverlayEaseWindowRuntime<StateDelta> {
    SongLuaOverlayEaseWindowRuntime {
        overlay_index,
        start_second,
        end_second,
        sustain_end_second,
        cutoff_second,
        from,
        to,
        easing: easing.map(ToString::to_string),
        opt1,
        opt2,
    }
}

pub fn build_song_lua_visual_layer_runtime<OverlayActor, CapturedActor, StateDelta>(
    start_second: f32,
    screen_width: f32,
    screen_height: f32,
    overlays: Vec<OverlayActor>,
    mut overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    mut overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    song_foreground: CapturedActor,
    mut song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
) -> SongLuaVisualLayerRuntime<OverlayActor, CapturedActor, StateDelta> {
    offset_song_lua_overlay_eases(&mut overlay_eases, start_second);
    let (overlay_eases, overlay_ease_ranges) =
        group_song_lua_overlay_eases(overlays.len(), overlay_eases);
    for events in &mut overlay_events {
        offset_song_lua_message_events(events, start_second);
    }
    offset_song_lua_message_events(&mut song_foreground_events, start_second);
    SongLuaVisualLayerRuntime {
        start_second,
        screen_width,
        screen_height,
        overlays,
        overlay_eases,
        overlay_ease_ranges,
        overlay_events,
        song_foreground,
        song_foreground_events,
    }
}

pub trait SongLuaOverlayDeltaOverlap {
    fn overlaps_song_lua_delta(&self, other: &Self) -> bool;
}

pub type SongLuaOverlayDeltaMask = u128;

#[derive(Clone, Debug, PartialEq)]
pub struct SongLuaRuntimeOverlayStateDelta<Delta> {
    pub overlap_mask: SongLuaOverlayDeltaMask,
    pub delta: Delta,
}

impl SongLuaOverlayDeltaOverlap for SongLuaOverlayDeltaMask {
    #[inline(always)]
    fn overlaps_song_lua_delta(&self, other: &Self) -> bool {
        self & other != 0
    }
}

impl<Delta> SongLuaOverlayDeltaOverlap for SongLuaRuntimeOverlayStateDelta<Delta> {
    #[inline(always)]
    fn overlaps_song_lua_delta(&self, other: &Self) -> bool {
        self.overlap_mask
            .overlaps_song_lua_delta(&other.overlap_mask)
    }
}

pub fn song_lua_overlay_ease_cutoff_second<Delta>(
    start_second: f32,
    from: &Delta,
    to: &Delta,
    blocks: impl IntoIterator<Item = (f32, f32, Delta)>,
) -> Option<f32>
where
    Delta: SongLuaOverlayDeltaOverlap,
{
    const SAME_TICK_CUTOFF_EPSILON: f32 = 0.001;

    let mut cutoff_second: Option<f32> = None;
    for (event_second, block_start, delta) in blocks {
        if !event_second.is_finite() || event_second < start_second {
            continue;
        }
        if !from.overlaps_song_lua_delta(&delta) && !to.overlaps_song_lua_delta(&delta) {
            continue;
        }
        let block_second = event_second + block_start.max(0.0);
        if !block_second.is_finite() || block_second <= start_second + SAME_TICK_CUTOFF_EPSILON {
            continue;
        }
        cutoff_second = Some(match cutoff_second {
            Some(current) => current.min(block_second),
            None => block_second,
        });
    }
    cutoff_second
}

pub trait SongLuaOverlayEaseWindowLike<Delta> {
    fn overlay_index(&self) -> usize;
    fn unit(&self) -> SongLuaRuntimeTimeUnit;
    fn start(&self) -> f32;
    fn limit(&self) -> f32;
    fn span_mode(&self) -> SongLuaRuntimeSpanMode;
    fn sustain(&self) -> Option<f32>;
    fn from(&self) -> &Delta;
    fn to(&self) -> &Delta;
    fn easing(&self) -> Option<&str>;
    fn opt1(&self) -> Option<f32>;
    fn opt2(&self) -> Option<f32>;
}

#[derive(Clone, Debug)]
pub struct SongLuaRuntimeOverlayEaseWindow<Delta> {
    pub overlay_index: usize,
    pub unit: SongLuaRuntimeTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaRuntimeSpanMode,
    pub sustain: Option<f32>,
    pub from: Delta,
    pub to: Delta,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

impl<Delta> SongLuaOverlayEaseWindowLike<Delta> for SongLuaRuntimeOverlayEaseWindow<Delta> {
    fn overlay_index(&self) -> usize {
        self.overlay_index
    }

    fn unit(&self) -> SongLuaRuntimeTimeUnit {
        self.unit
    }

    fn start(&self) -> f32 {
        self.start
    }

    fn limit(&self) -> f32 {
        self.limit
    }

    fn span_mode(&self) -> SongLuaRuntimeSpanMode {
        self.span_mode
    }

    fn sustain(&self) -> Option<f32> {
        self.sustain
    }

    fn from(&self) -> &Delta {
        &self.from
    }

    fn to(&self) -> &Delta {
        &self.to
    }

    fn easing(&self) -> Option<&str> {
        self.easing.as_deref()
    }

    fn opt1(&self) -> Option<f32> {
        self.opt1
    }

    fn opt2(&self) -> Option<f32> {
        self.opt2
    }
}

pub fn build_song_lua_overlay_ease_window_for<Ease, Delta>(
    ease: &Ease,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    cutoff_second: impl FnOnce(f32) -> Option<f32>,
) -> Option<SongLuaOverlayEaseWindowRuntime<Delta>>
where
    Ease: SongLuaOverlayEaseWindowLike<Delta>,
    Delta: Clone,
{
    let (start_second, end_second) = song_lua_window_seconds(
        ease.unit(),
        ease.start(),
        ease.limit(),
        ease.span_mode(),
        timing_player,
        global_offset_seconds,
    )?;
    if end_second < start_second {
        return None;
    }
    let sustain_end_second = song_lua_sustain_end_second(
        ease.unit(),
        ease.start(),
        ease.limit(),
        ease.span_mode(),
        ease.sustain(),
        timing_player,
        global_offset_seconds,
        end_second,
    );
    Some(build_song_lua_overlay_ease_window_runtime(
        ease.overlay_index(),
        start_second,
        end_second,
        sustain_end_second,
        cutoff_second(start_second),
        ease.from().clone(),
        ease.to().clone(),
        ease.easing(),
        ease.opt1(),
        ease.opt2(),
    ))
}

#[inline(always)]
fn song_lua_normalized_value(value: f32) -> f32 {
    value / 100.0
}

fn push_song_lua_ease_target(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    target: SongLuaEaseMaskTarget,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) {
    out.push(SongLuaEaseMaskWindow {
        start_second,
        end_second,
        sustain_end_second,
        target,
        from,
        to,
        easing: easing.map(ToString::to_string),
        opt1,
        opt2,
    });
}

pub fn append_song_lua_ease_targets(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    target_name: &str,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> bool {
    let key = attack_token_key(target_name);
    if key.is_empty() {
        return false;
    }
    let pct_from = song_lua_normalized_value(from);
    let pct_to = song_lua_normalized_value(to);
    let mut push = |target, from, to| {
        push_song_lua_ease_target(
            out,
            target,
            start_second,
            end_second,
            sustain_end_second,
            from,
            to,
            easing,
            opt1,
            opt2,
        );
    };

    if let Some(col) = mod_column_suffix(&key, "bumpy") {
        push(
            SongLuaEaseMaskTarget::VisualBumpyColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "tiny") {
        push(
            SongLuaEaseMaskTarget::VisualTinyColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "movex") {
        push(
            SongLuaEaseMaskTarget::VisualMoveXColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "movey") {
        push(
            SongLuaEaseMaskTarget::VisualMoveYColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "confusionoffset") {
        push(
            SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }

    match key.as_str() {
        "boost" => push(SongLuaEaseMaskTarget::AccelBoost, pct_from, pct_to),
        "brake" => push(SongLuaEaseMaskTarget::AccelBrake, pct_from, pct_to),
        "wave" => push(SongLuaEaseMaskTarget::AccelWave, pct_from, pct_to),
        "expand" => push(SongLuaEaseMaskTarget::AccelExpand, pct_from, pct_to),
        "boomerang" => push(SongLuaEaseMaskTarget::AccelBoomerang, pct_from, pct_to),
        "drunk" => push(SongLuaEaseMaskTarget::VisualDrunk, pct_from, pct_to),
        "dizzy" => push(SongLuaEaseMaskTarget::VisualDizzy, pct_from, pct_to),
        "confusion" => push(SongLuaEaseMaskTarget::VisualConfusion, pct_from, pct_to),
        "confusionoffset" => push(
            SongLuaEaseMaskTarget::VisualConfusionOffset,
            pct_from,
            pct_to,
        ),
        "flip" => push(SongLuaEaseMaskTarget::VisualFlip, pct_from, pct_to),
        "invert" => push(SongLuaEaseMaskTarget::VisualInvert, pct_from, pct_to),
        "tornado" => push(SongLuaEaseMaskTarget::VisualTornado, pct_from, pct_to),
        "tipsy" => push(SongLuaEaseMaskTarget::VisualTipsy, pct_from, pct_to),
        "bumpy" => push(SongLuaEaseMaskTarget::VisualBumpy, pct_from, pct_to),
        "bumpyoffset" => push(SongLuaEaseMaskTarget::VisualBumpyOffset, pct_from, pct_to),
        "bumpyperiod" => push(SongLuaEaseMaskTarget::VisualBumpyPeriod, pct_from, pct_to),
        "pulseinner" => push(SongLuaEaseMaskTarget::VisualPulseInner, pct_from, pct_to),
        "pulseouter" => push(SongLuaEaseMaskTarget::VisualPulseOuter, pct_from, pct_to),
        "pulseperiod" => push(SongLuaEaseMaskTarget::VisualPulsePeriod, pct_from, pct_to),
        "pulseoffset" => push(SongLuaEaseMaskTarget::VisualPulseOffset, pct_from, pct_to),
        "beat" => push(SongLuaEaseMaskTarget::VisualBeat, pct_from, pct_to),
        "hidden" => push(SongLuaEaseMaskTarget::AppearanceHidden, pct_from, pct_to),
        "sudden" => push(SongLuaEaseMaskTarget::AppearanceSudden, pct_from, pct_to),
        "stealth" => push(SongLuaEaseMaskTarget::AppearanceStealth, pct_from, pct_to),
        "blink" => push(SongLuaEaseMaskTarget::AppearanceBlink, pct_from, pct_to),
        "rvanish" | "randomvanish" | "reversevanish" => push(
            SongLuaEaseMaskTarget::AppearanceRandomVanish,
            pct_from,
            pct_to,
        ),
        "dark" => push(SongLuaEaseMaskTarget::VisibilityDark, pct_from, pct_to),
        "blind" => push(SongLuaEaseMaskTarget::VisibilityBlind, pct_from, pct_to),
        "cover" => push(SongLuaEaseMaskTarget::VisibilityCover, pct_from, pct_to),
        "reverse" => push(SongLuaEaseMaskTarget::ScrollReverse, pct_from, pct_to),
        "split" => push(SongLuaEaseMaskTarget::ScrollSplit, pct_from, pct_to),
        "alternate" => push(SongLuaEaseMaskTarget::ScrollAlternate, pct_from, pct_to),
        "cross" => push(SongLuaEaseMaskTarget::ScrollCross, pct_from, pct_to),
        "centered" => push(SongLuaEaseMaskTarget::ScrollCentered, pct_from, pct_to),
        "incoming" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, -pct_from, -pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, pct_from, pct_to);
        }
        "space" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, pct_from, pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, pct_from, pct_to);
        }
        "hallway" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, -pct_from, -pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, 0.0, 0.0);
        }
        "distant" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, pct_from, pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, 0.0, 0.0);
        }
        "overhead" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, 0.0, 0.0);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, 0.0, 0.0);
        }
        "xmod" => push(SongLuaEaseMaskTarget::ScrollSpeedX, from, to),
        "cmod" => push(SongLuaEaseMaskTarget::ScrollSpeedC, from, to),
        "mmod" => push(SongLuaEaseMaskTarget::ScrollSpeedM, from, to),
        "tiny" => push(SongLuaEaseMaskTarget::VisualTiny, pct_from, pct_to),
        "mini" => push(SongLuaEaseMaskTarget::MiniPercent, from, to),
        "skewx" => push(SongLuaEaseMaskTarget::PlayerSkewX, pct_from, pct_to),
        "skewy" => push(SongLuaEaseMaskTarget::PlayerSkewY, pct_from, pct_to),
        "confusionyoffset" => push(
            SongLuaEaseMaskTarget::ConfusionYOffsetY,
            pct_from * (180.0 / std::f32::consts::PI),
            pct_to * (180.0 / std::f32::consts::PI),
        ),
        _ => return false,
    }
    true
}

pub fn append_song_lua_runtime_ease_window(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    target: SongLuaRuntimeEaseTarget<'_>,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> SongLuaRuntimeEaseAppend {
    match target {
        SongLuaRuntimeEaseTarget::Mod(target_name) => {
            if append_song_lua_ease_targets(
                out,
                start_second,
                end_second,
                sustain_end_second,
                target_name,
                from,
                to,
                easing,
                opt1,
                opt2,
            ) {
                SongLuaRuntimeEaseAppend::Appended
            } else {
                SongLuaRuntimeEaseAppend::Unsupported
            }
        }
        SongLuaRuntimeEaseTarget::Player(target) => {
            push_song_lua_ease_target(
                out,
                target,
                start_second,
                end_second,
                sustain_end_second,
                from,
                to,
                easing,
                opt1,
                opt2,
            );
            SongLuaRuntimeEaseAppend::Appended
        }
        SongLuaRuntimeEaseTarget::Function => SongLuaRuntimeEaseAppend::Ignored,
    }
}

pub fn append_song_lua_runtime_ease_window_like<Target>(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    target: &Target,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> SongLuaRuntimeEaseAppend
where
    Target: SongLuaRuntimeEaseTargetLike + ?Sized,
{
    append_song_lua_runtime_ease_window(
        out,
        start_second,
        end_second,
        sustain_end_second,
        target.as_runtime_ease_target(),
        from,
        to,
        easing,
        opt1,
        opt2,
    )
}

pub trait SongLuaEaseWindowLike {
    type Target: SongLuaRuntimeEaseTargetLike + ?Sized;

    fn player(&self) -> Option<u8>;
    fn unit(&self) -> SongLuaRuntimeTimeUnit;
    fn start(&self) -> f32;
    fn limit(&self) -> f32;
    fn span_mode(&self) -> SongLuaRuntimeSpanMode;
    fn target(&self) -> &Self::Target;
    fn from(&self) -> f32;
    fn to(&self) -> f32;
    fn easing(&self) -> Option<&str>;
    fn sustain(&self) -> Option<f32>;
    fn opt1(&self) -> Option<f32>;
    fn opt2(&self) -> Option<f32>;
}

#[derive(Clone, Debug)]
pub struct SongLuaRuntimeEaseWindow {
    pub player: Option<u8>,
    pub unit: SongLuaRuntimeTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaRuntimeSpanMode,
    pub target: SongLuaRuntimeEaseTargetOwned,
    pub from: f32,
    pub to: f32,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

impl SongLuaEaseWindowLike for SongLuaRuntimeEaseWindow {
    type Target = SongLuaRuntimeEaseTargetOwned;

    #[inline(always)]
    fn player(&self) -> Option<u8> {
        self.player
    }

    #[inline(always)]
    fn unit(&self) -> SongLuaRuntimeTimeUnit {
        self.unit
    }

    #[inline(always)]
    fn start(&self) -> f32 {
        self.start
    }

    #[inline(always)]
    fn limit(&self) -> f32 {
        self.limit
    }

    #[inline(always)]
    fn span_mode(&self) -> SongLuaRuntimeSpanMode {
        self.span_mode
    }

    #[inline(always)]
    fn target(&self) -> &Self::Target {
        &self.target
    }

    #[inline(always)]
    fn from(&self) -> f32 {
        self.from
    }

    #[inline(always)]
    fn to(&self) -> f32 {
        self.to
    }

    #[inline(always)]
    fn easing(&self) -> Option<&str> {
        self.easing.as_deref()
    }

    #[inline(always)]
    fn sustain(&self) -> Option<f32> {
        self.sustain
    }

    #[inline(always)]
    fn opt1(&self) -> Option<f32> {
        self.opt1
    }

    #[inline(always)]
    fn opt2(&self) -> Option<f32> {
        self.opt2
    }
}

pub fn append_song_lua_ease_window_for<Window>(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    window: &Window,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> SongLuaRuntimeEaseAppend
where
    Window: SongLuaEaseWindowLike,
{
    let Some((start_second, end_second)) = song_lua_window_seconds(
        window.unit(),
        window.start(),
        window.limit(),
        window.span_mode(),
        timing_player,
        global_offset_seconds,
    ) else {
        return SongLuaRuntimeEaseAppend::Ignored;
    };
    let sustain_end_second = song_lua_sustain_end_second(
        window.unit(),
        window.start(),
        window.limit(),
        window.span_mode(),
        window.sustain(),
        timing_player,
        global_offset_seconds,
        end_second,
    );
    if sustain_end_second <= start_second {
        return SongLuaRuntimeEaseAppend::Ignored;
    }
    append_song_lua_runtime_ease_window_like(
        out,
        start_second,
        end_second,
        sustain_end_second,
        window.target(),
        window.from(),
        window.to(),
        window.easing(),
        window.opt1(),
        window.opt2(),
    )
}

pub fn build_song_lua_ease_windows_for_player<Window>(
    windows: &[Window],
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[AttackMaskWindow],
    mut unsupported_window: impl FnMut(&Window),
) -> (Vec<SongLuaEaseMaskWindow>, usize)
where
    Window: SongLuaEaseWindowLike,
{
    let mut out = Vec::new();
    let mut unsupported_targets = 0usize;
    for window in windows {
        if !song_lua_target_matches_player(window.player(), player) {
            continue;
        }
        if append_song_lua_ease_window_for(&mut out, window, timing_player, global_offset_seconds)
            == SongLuaRuntimeEaseAppend::Unsupported
        {
            unsupported_targets += 1;
            unsupported_window(window);
        }
    }
    song_lua_extend_ease_tails(&mut out, constant_windows);
    (out, unsupported_targets)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SongLuaPlayerTransformValues {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub rotation_x: Option<f32>,
    pub rotation_z: Option<f32>,
    pub rotation_y: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub confusion_y_offset: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaPlayerTransform {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: f32,
    pub rotation_x: f32,
    pub rotation_z: f32,
    pub rotation_y: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
    pub confusion_y_offset: f32,
}

impl Default for SongLuaPlayerTransform {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            z: 0.0,
            rotation_x: 0.0,
            rotation_z: 0.0,
            rotation_y: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
            confusion_y_offset: 0.0,
        }
    }
}

pub type SongLuaPlayerTransforms = [SongLuaPlayerTransform; MAX_PLAYERS];

#[inline(always)]
pub const fn song_lua_player_transforms_default() -> SongLuaPlayerTransforms {
    [SongLuaPlayerTransform {
        x: None,
        y: None,
        z: 0.0,
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
        confusion_y_offset: 0.0,
    }; MAX_PLAYERS]
}

#[inline(always)]
fn finite_transform_option(value: Option<f32>) -> Option<f32> {
    value.filter(|v| v.is_finite())
}

#[inline(always)]
fn finite_transform_or(value: Option<f32>, fallback: f32) -> f32 {
    finite_transform_option(value).unwrap_or(fallback)
}

impl SongLuaPlayerTransformValues {
    pub fn resolve(self) -> SongLuaPlayerTransform {
        SongLuaPlayerTransform {
            x: finite_transform_option(self.x),
            y: finite_transform_option(self.y),
            z: finite_transform_or(self.z, 0.0),
            rotation_x: finite_transform_or(self.rotation_x, 0.0),
            rotation_z: finite_transform_or(self.rotation_z, 0.0),
            rotation_y: finite_transform_or(self.rotation_y, 0.0),
            skew_x: finite_transform_or(self.skew_x, 0.0),
            skew_y: finite_transform_or(self.skew_y, 0.0),
            zoom_x: finite_transform_or(self.zoom_x, 1.0),
            zoom_y: finite_transform_or(self.zoom_y, 1.0),
            zoom_z: finite_transform_or(self.zoom_z, 1.0),
            confusion_y_offset: finite_transform_or(self.confusion_y_offset, 0.0),
        }
    }
}

pub fn song_lua_apply_player_transform_target(
    target: SongLuaEaseMaskTarget,
    value: f32,
    player: &mut SongLuaPlayerTransformValues,
) {
    if !value.is_finite() {
        return;
    }
    match target {
        SongLuaEaseMaskTarget::PlayerX => player.x = Some(value),
        SongLuaEaseMaskTarget::PlayerY => player.y = Some(value),
        SongLuaEaseMaskTarget::PlayerZ => player.z = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationX => player.rotation_x = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationZ => player.rotation_z = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationY => player.rotation_y = Some(value),
        SongLuaEaseMaskTarget::PlayerSkewX => player.skew_x = Some(value),
        SongLuaEaseMaskTarget::PlayerSkewY => player.skew_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZoom => {
            player.zoom_x = Some(value);
            player.zoom_y = Some(value);
            player.zoom_z = Some(value);
        }
        SongLuaEaseMaskTarget::PlayerZoomX => player.zoom_x = Some(value),
        SongLuaEaseMaskTarget::PlayerZoomY => player.zoom_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZoomZ => player.zoom_z = Some(value),
        SongLuaEaseMaskTarget::ConfusionYOffsetY => player.confusion_y_offset = Some(value),
        _ => {}
    }
}

pub fn song_lua_apply_eased_target(
    target: SongLuaEaseMaskTarget,
    value: f32,
    accel: &mut AccelOverrides,
    visual: &mut VisualOverrides,
    appearance: &mut AppearanceEffects,
    visibility: &mut VisibilityOverrides,
    scroll: &mut ScrollOverrides,
    perspective: &mut PerspectiveOverrides,
    scroll_speed: &mut Option<ScrollSpeedSetting>,
    mini_percent: &mut Option<f32>,
    player: &mut SongLuaPlayerTransformValues,
) {
    if !value.is_finite() {
        return;
    }
    match target {
        SongLuaEaseMaskTarget::AccelBoost => accel.boost = Some(value),
        SongLuaEaseMaskTarget::AccelBrake => accel.brake = Some(value),
        SongLuaEaseMaskTarget::AccelWave => accel.wave = Some(value),
        SongLuaEaseMaskTarget::AccelExpand => accel.expand = Some(value),
        SongLuaEaseMaskTarget::AccelBoomerang => accel.boomerang = Some(value),
        SongLuaEaseMaskTarget::VisualDrunk => visual.drunk = Some(value),
        SongLuaEaseMaskTarget::VisualDizzy => visual.dizzy = Some(value),
        SongLuaEaseMaskTarget::VisualConfusion => visual.confusion = Some(value),
        SongLuaEaseMaskTarget::VisualConfusionOffset => visual.confusion_offset = Some(value),
        SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col) => {
            if col < MAX_COLS {
                visual.confusion_offset_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualFlip => visual.flip = Some(value),
        SongLuaEaseMaskTarget::VisualInvert => visual.invert = Some(value),
        SongLuaEaseMaskTarget::VisualTornado => visual.tornado = Some(value),
        SongLuaEaseMaskTarget::VisualTipsy => visual.tipsy = Some(value),
        SongLuaEaseMaskTarget::VisualTiny => visual.tiny = Some(value),
        SongLuaEaseMaskTarget::VisualBumpy => visual.bumpy = Some(value),
        SongLuaEaseMaskTarget::VisualBumpyOffset => visual.bumpy_offset = Some(value),
        SongLuaEaseMaskTarget::VisualBumpyPeriod => visual.bumpy_period = Some(value),
        SongLuaEaseMaskTarget::VisualBumpyColumn(col) => {
            if col < MAX_COLS {
                visual.bumpy_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualTinyColumn(col) => {
            if col < MAX_COLS {
                visual.tiny_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualMoveXColumn(col) => {
            if col < MAX_COLS {
                visual.move_x_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualMoveYColumn(col) => {
            if col < MAX_COLS {
                visual.move_y_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualPulseInner => visual.pulse_inner = Some(value),
        SongLuaEaseMaskTarget::VisualPulseOuter => visual.pulse_outer = Some(value),
        SongLuaEaseMaskTarget::VisualPulsePeriod => visual.pulse_period = Some(value),
        SongLuaEaseMaskTarget::VisualPulseOffset => visual.pulse_offset = Some(value),
        SongLuaEaseMaskTarget::VisualBeat => visual.beat = Some(value),
        SongLuaEaseMaskTarget::AppearanceHidden => appearance.hidden = value,
        SongLuaEaseMaskTarget::AppearanceSudden => appearance.sudden = value,
        SongLuaEaseMaskTarget::AppearanceStealth => appearance.stealth = value,
        SongLuaEaseMaskTarget::AppearanceBlink => appearance.blink = value,
        SongLuaEaseMaskTarget::AppearanceRandomVanish => appearance.random_vanish = value,
        SongLuaEaseMaskTarget::VisibilityDark => visibility.dark = Some(value),
        SongLuaEaseMaskTarget::VisibilityBlind => visibility.blind = Some(value),
        SongLuaEaseMaskTarget::VisibilityCover => visibility.cover = Some(value),
        SongLuaEaseMaskTarget::ScrollReverse => scroll.reverse = Some(value),
        SongLuaEaseMaskTarget::ScrollSplit => scroll.split = Some(value),
        SongLuaEaseMaskTarget::ScrollAlternate => scroll.alternate = Some(value),
        SongLuaEaseMaskTarget::ScrollCross => scroll.cross = Some(value),
        SongLuaEaseMaskTarget::ScrollCentered => scroll.centered = Some(value),
        SongLuaEaseMaskTarget::PerspectiveTilt => perspective.tilt = Some(value),
        SongLuaEaseMaskTarget::PerspectiveSkew => perspective.skew = Some(value),
        SongLuaEaseMaskTarget::ScrollSpeedX => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::XMod(value));
            }
        }
        SongLuaEaseMaskTarget::ScrollSpeedC => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::CMod(value));
            }
        }
        SongLuaEaseMaskTarget::ScrollSpeedM => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::MMod(value));
            }
        }
        SongLuaEaseMaskTarget::MiniPercent => *mini_percent = Some(value),
        SongLuaEaseMaskTarget::PlayerX
        | SongLuaEaseMaskTarget::PlayerY
        | SongLuaEaseMaskTarget::PlayerZ
        | SongLuaEaseMaskTarget::PlayerRotationX
        | SongLuaEaseMaskTarget::PlayerRotationZ
        | SongLuaEaseMaskTarget::PlayerRotationY
        | SongLuaEaseMaskTarget::PlayerSkewX
        | SongLuaEaseMaskTarget::PlayerSkewY
        | SongLuaEaseMaskTarget::PlayerZoom
        | SongLuaEaseMaskTarget::PlayerZoomX
        | SongLuaEaseMaskTarget::PlayerZoomY
        | SongLuaEaseMaskTarget::PlayerZoomZ
        | SongLuaEaseMaskTarget::ConfusionYOffsetY => {
            song_lua_apply_player_transform_target(target, value, player);
        }
    }
}

pub fn attack_mask_window_from_parts(
    attack: &ChartAttackWindow,
    mods: ParsedAttackMods,
) -> Option<AttackMaskWindow> {
    if !mods.has_runtime_mask_effect() && !mods.has_chart_effect() {
        return None;
    }
    let start_second = attack.start_second;
    let end_second = start_second + attack.len_seconds.max(0.0);
    if !start_second.is_finite() || !end_second.is_finite() || end_second <= start_second {
        return None;
    }
    Some(AttackMaskWindow {
        start_second,
        end_second,
        sustain_end_second: end_second,
        persist_after_end: false,
        clear_all: mods.clear_all,
        chart: ChartAttackEffects {
            insert_mask: mods.insert_mask,
            remove_mask: mods.remove_mask,
            holds_mask: mods.holds_mask,
            turn_bits: turn_option_bits(mods.turn_option),
        },
        accel: mods.accel,
        visual: mods.visual,
        visual_speed: mods.visual_speed,
        appearance: mods.appearance,
        appearance_speed: mods.appearance_speed,
        visibility: mods.visibility,
        scroll: mods.scroll,
        scroll_approach_speed: mods.scroll_approach_speed,
        perspective: mods.perspective,
        scroll_speed: mods.scroll_speed,
        mini_percent: mods.mini_percent,
        mini_mode: MiniAttackMode::Absolute,
        mini_speed: mods.mini_speed,
    })
}

pub fn build_attack_mask_windows(attacks: &[ChartAttackWindow]) -> Vec<AttackMaskWindow> {
    if attacks.is_empty() {
        return Vec::new();
    }
    let mut windows = Vec::with_capacity(attacks.len());
    for attack in attacks {
        if let Some(window) = attack_mask_window_from_parts(attack, parse_attack_mods(&attack.mods))
        {
            windows.push(window);
        }
    }
    windows
}

#[inline(always)]
pub const fn song_lua_player_transform_target(target: SongLuaEaseMaskTarget) -> bool {
    matches!(
        target,
        SongLuaEaseMaskTarget::PlayerX
            | SongLuaEaseMaskTarget::PlayerY
            | SongLuaEaseMaskTarget::PlayerZ
            | SongLuaEaseMaskTarget::PlayerRotationX
            | SongLuaEaseMaskTarget::PlayerRotationZ
            | SongLuaEaseMaskTarget::PlayerRotationY
            | SongLuaEaseMaskTarget::PlayerSkewX
            | SongLuaEaseMaskTarget::PlayerSkewY
            | SongLuaEaseMaskTarget::PlayerZoom
            | SongLuaEaseMaskTarget::PlayerZoomX
            | SongLuaEaseMaskTarget::PlayerZoomY
            | SongLuaEaseMaskTarget::PlayerZoomZ
            | SongLuaEaseMaskTarget::ConfusionYOffsetY
    )
}

#[inline(always)]
fn song_lua_constant_sets_target(window: &AttackMaskWindow, target: SongLuaEaseMaskTarget) -> bool {
    if window.clear_all && !song_lua_player_transform_target(target) {
        return true;
    }
    match target {
        SongLuaEaseMaskTarget::AccelBoost => window.accel.boost.is_some(),
        SongLuaEaseMaskTarget::AccelBrake => window.accel.brake.is_some(),
        SongLuaEaseMaskTarget::AccelWave => window.accel.wave.is_some(),
        SongLuaEaseMaskTarget::AccelExpand => window.accel.expand.is_some(),
        SongLuaEaseMaskTarget::AccelBoomerang => window.accel.boomerang.is_some(),
        SongLuaEaseMaskTarget::VisualDrunk => window.visual.drunk.is_some(),
        SongLuaEaseMaskTarget::VisualDizzy => window.visual.dizzy.is_some(),
        SongLuaEaseMaskTarget::VisualConfusion => window.visual.confusion.is_some(),
        SongLuaEaseMaskTarget::VisualConfusionOffset => window.visual.confusion_offset.is_some(),
        SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col) => window
            .visual
            .confusion_offset_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualFlip => window.visual.flip.is_some(),
        SongLuaEaseMaskTarget::VisualInvert => window.visual.invert.is_some(),
        SongLuaEaseMaskTarget::VisualTornado => window.visual.tornado.is_some(),
        SongLuaEaseMaskTarget::VisualTipsy => window.visual.tipsy.is_some(),
        SongLuaEaseMaskTarget::VisualTiny => window.visual.tiny.is_some(),
        SongLuaEaseMaskTarget::VisualBumpy => window.visual.bumpy.is_some(),
        SongLuaEaseMaskTarget::VisualBumpyOffset => window.visual.bumpy_offset.is_some(),
        SongLuaEaseMaskTarget::VisualBumpyPeriod => window.visual.bumpy_period.is_some(),
        SongLuaEaseMaskTarget::VisualBumpyColumn(col) => window
            .visual
            .bumpy_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualTinyColumn(col) => window
            .visual
            .tiny_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualMoveXColumn(col) => window
            .visual
            .move_x_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualMoveYColumn(col) => window
            .visual
            .move_y_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualPulseInner => window.visual.pulse_inner.is_some(),
        SongLuaEaseMaskTarget::VisualPulseOuter => window.visual.pulse_outer.is_some(),
        SongLuaEaseMaskTarget::VisualPulsePeriod => window.visual.pulse_period.is_some(),
        SongLuaEaseMaskTarget::VisualPulseOffset => window.visual.pulse_offset.is_some(),
        SongLuaEaseMaskTarget::VisualBeat => window.visual.beat.is_some(),
        SongLuaEaseMaskTarget::AppearanceHidden => window.appearance.hidden.is_some(),
        SongLuaEaseMaskTarget::AppearanceSudden => window.appearance.sudden.is_some(),
        SongLuaEaseMaskTarget::AppearanceStealth => window.appearance.stealth.is_some(),
        SongLuaEaseMaskTarget::AppearanceBlink => window.appearance.blink.is_some(),
        SongLuaEaseMaskTarget::AppearanceRandomVanish => window.appearance.random_vanish.is_some(),
        SongLuaEaseMaskTarget::VisibilityDark => window.visibility.dark.is_some(),
        SongLuaEaseMaskTarget::VisibilityBlind => window.visibility.blind.is_some(),
        SongLuaEaseMaskTarget::VisibilityCover => window.visibility.cover.is_some(),
        SongLuaEaseMaskTarget::ScrollReverse => window.scroll.reverse.is_some(),
        SongLuaEaseMaskTarget::ScrollSplit => window.scroll.split.is_some(),
        SongLuaEaseMaskTarget::ScrollAlternate => window.scroll.alternate.is_some(),
        SongLuaEaseMaskTarget::ScrollCross => window.scroll.cross.is_some(),
        SongLuaEaseMaskTarget::ScrollCentered => window.scroll.centered.is_some(),
        SongLuaEaseMaskTarget::PerspectiveTilt => window.perspective.tilt.is_some(),
        SongLuaEaseMaskTarget::PerspectiveSkew => window.perspective.skew.is_some(),
        SongLuaEaseMaskTarget::ScrollSpeedX
        | SongLuaEaseMaskTarget::ScrollSpeedC
        | SongLuaEaseMaskTarget::ScrollSpeedM => window.scroll_speed.is_some(),
        SongLuaEaseMaskTarget::MiniPercent => window.mini_percent.is_some(),
        SongLuaEaseMaskTarget::PlayerX
        | SongLuaEaseMaskTarget::PlayerY
        | SongLuaEaseMaskTarget::PlayerZ
        | SongLuaEaseMaskTarget::PlayerRotationX
        | SongLuaEaseMaskTarget::PlayerRotationZ
        | SongLuaEaseMaskTarget::PlayerRotationY
        | SongLuaEaseMaskTarget::PlayerSkewX
        | SongLuaEaseMaskTarget::PlayerSkewY
        | SongLuaEaseMaskTarget::PlayerZoom
        | SongLuaEaseMaskTarget::PlayerZoomX
        | SongLuaEaseMaskTarget::PlayerZoomY
        | SongLuaEaseMaskTarget::PlayerZoomZ
        | SongLuaEaseMaskTarget::ConfusionYOffsetY => false,
    }
}

fn song_lua_constant_cutoff_second(
    constant: &AttackMaskWindow,
    window: &SongLuaEaseMaskWindow,
    epsilon: f32,
) -> Option<f32> {
    if !constant.start_second.is_finite()
        || !constant.end_second.is_finite()
        || !window.end_second.is_finite()
        || !song_lua_constant_sets_target(constant, window.target)
    {
        return None;
    }
    if constant.end_second <= window.end_second + epsilon {
        return None;
    }
    if constant.start_second <= window.end_second + epsilon {
        Some(window.end_second)
    } else {
        Some(constant.start_second)
    }
}

pub fn song_lua_extend_ease_tails(
    out: &mut [SongLuaEaseMaskWindow],
    constants: &[AttackMaskWindow],
) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    for i in 0..out.len() {
        let window = &out[i];
        let default_end = if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
            window.sustain_end_second
        } else {
            f32::MAX
        };
        let cutoff_second = out
            .iter()
            .enumerate()
            .filter_map(|(j, other)| {
                if i == j
                    || other.target != window.target
                    || !other.start_second.is_finite()
                    || other.start_second <= window.start_second + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start_second)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        let constant_cutoff = constants
            .iter()
            .filter_map(|constant| {
                song_lua_constant_cutoff_second(constant, window, SAME_TICK_EPSILON)
            })
            .fold(cutoff_second, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        out[i].sustain_end_second =
            constant_cutoff.map_or(default_end, |cutoff| default_end.min(cutoff));
    }
}

pub fn song_lua_extend_column_offset_tails(out: &mut [SongLuaColumnOffsetWindowRuntime]) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    for i in 0..out.len() {
        let window = &out[i];
        let default_end = if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
            window.sustain_end_second
        } else {
            f32::MAX
        };
        let cutoff_second = out
            .iter()
            .enumerate()
            .filter_map(|(j, other)| {
                if i == j
                    || other.column != window.column
                    || !other.start_second.is_finite()
                    || other.start_second <= window.start_second + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start_second)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        out[i].sustain_end_second =
            cutoff_second.map_or(default_end, |cutoff| default_end.min(cutoff));
    }
}

#[inline(always)]
pub fn song_lua_note_hidden(
    windows: &[SongLuaNoteHideWindowRuntime],
    local_col: usize,
    beat: f32,
) -> bool {
    const EPS: f32 = 1.0e-4;
    windows.iter().any(|window| {
        window.column == local_col
            && beat + EPS >= window.start_beat
            && beat <= window.end_beat + EPS
    })
}

#[inline(always)]
pub fn song_lua_field_note_hidden(
    windows: &[SongLuaNoteHideWindowRuntime],
    cols_per_player: usize,
    column: usize,
    beat: f32,
) -> bool {
    let local_col = local_column_for_field(cols_per_player, column);
    song_lua_note_hidden(windows, local_col, beat)
}

#[inline(always)]
pub fn offset_song_lua_message_events(events: &mut [SongLuaOverlayMessageRuntime], delta: f32) {
    if !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return;
    }
    for event in events {
        event.event_second += delta;
    }
}

pub fn group_song_lua_overlay_eases<StateDelta>(
    overlay_count: usize,
    overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
) -> (
    Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    Vec<std::ops::Range<usize>>,
) {
    let mut buckets = Vec::with_capacity(overlay_count);
    buckets.resize_with(overlay_count, Vec::new);
    for ease in overlay_eases {
        if let Some(bucket) = buckets.get_mut(ease.overlay_index) {
            bucket.push(ease);
        }
    }
    let total_len = buckets.iter().map(Vec::len).sum();
    let mut flat = Vec::with_capacity(total_len);
    let mut ranges = Vec::with_capacity(overlay_count);
    for mut bucket in buckets {
        bucket.sort_by(|left, right| {
            left.start_second
                .total_cmp(&right.start_second)
                .then_with(|| left.end_second.total_cmp(&right.end_second))
                .then_with(|| left.sustain_end_second.total_cmp(&right.sustain_end_second))
        });
        let start = flat.len();
        flat.extend(bucket);
        ranges.push(start..flat.len());
    }
    (flat, ranges)
}

#[inline(always)]
pub fn offset_song_lua_overlay_eases<StateDelta>(
    eases: &mut [SongLuaOverlayEaseWindowRuntime<StateDelta>],
    delta: f32,
) {
    if !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return;
    }
    for ease in eases {
        ease.start_second += delta;
        ease.end_second += delta;
        ease.sustain_end_second += delta;
        ease.cutoff_second = ease.cutoff_second.map(|cutoff| cutoff + delta);
    }
}

#[inline(always)]
fn song_lua_lerp_unclamped(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

pub fn song_lua_ease_window_value(window: &SongLuaEaseMaskWindow, now: f32) -> Option<f32> {
    if !now.is_finite()
        || !window.start_second.is_finite()
        || !window.sustain_end_second.is_finite()
        || !window.from.is_finite()
        || !window.to.is_finite()
        || now < window.start_second
        || now >= window.sustain_end_second
    {
        return None;
    }
    if !window.end_second.is_finite()
        || window.end_second <= window.start_second
        || now >= window.end_second
    {
        return Some(window.to);
    }
    let duration = window.end_second - window.start_second;
    if duration <= f32::EPSILON {
        return Some(window.to);
    }
    let factor = song_lua_ease_factor(
        window.easing.as_deref(),
        (now - window.start_second) / duration,
        window.opt1,
        window.opt2,
    );
    let value = song_lua_lerp_unclamped(window.from, window.to, factor);
    if value.is_finite() {
        Some(value)
    } else {
        Some(window.to)
    }
}

#[inline(always)]
pub fn chart_attack_row_range(
    attack: &ChartAttackWindow,
    timing_player: &TimingData,
) -> Option<(usize, usize)> {
    let start_beat = timing_player.get_beat_for_time(attack.start_second);
    let end_beat = timing_player.get_beat_for_time(attack.start_second + attack.len_seconds);
    let rows_per_beat = ROWS_PER_BEAT.max(1) as f32;
    let start_row = (start_beat.max(0.0) * rows_per_beat).round() as usize;
    let end_row = (end_beat.max(0.0) * rows_per_beat).round() as usize;
    (end_row >= start_row).then_some((start_row, end_row))
}

#[inline(always)]
pub fn chart_attack_turn_seed(base_seed: u64, player: usize, window_index: usize) -> u64 {
    base_seed
        ^ (0x9E37_79B9_u64.wrapping_mul(player as u64 + 1))
        ^ ((window_index as u64).wrapping_mul(0xA5A5_5A5A_u64))
}

pub fn apply_attack_turn_mod(
    notes: &mut [Note],
    col_offset: usize,
    cols: usize,
    turn_option: GameplayTurnOption,
    seed: u64,
    player: usize,
) {
    if notes.is_empty() || turn_option == GameplayTurnOption::None {
        return;
    }
    let note_range = (0usize, notes.len());
    match turn_option {
        GameplayTurnOption::None => {}
        GameplayTurnOption::Blender => {
            apply_turn_permutation(
                notes,
                note_range,
                col_offset,
                cols,
                GameplayTurnOption::Shuffle,
                seed,
            );
            apply_super_shuffle_taps(
                notes,
                note_range,
                col_offset,
                cols,
                seed ^ (0xD00D_F00D_u64.wrapping_mul(player as u64 + 1)),
            );
        }
        GameplayTurnOption::Random => {
            apply_hyper_shuffle(
                notes,
                note_range,
                col_offset,
                cols,
                seed ^ (0xA5A5_5A5A_u64.wrapping_mul(player as u64 + 1)),
            );
        }
        other => {
            apply_turn_permutation(notes, note_range, col_offset, cols, other, seed);
        }
    }
}

pub fn apply_chart_attack_window(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    row_bounds: (usize, usize),
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
    let (start_row, end_row) = row_bounds;
    if notes.is_empty() || end_row < start_row || !mods.has_chart_effect() {
        return;
    }
    let mut in_range = Vec::with_capacity(notes.len());
    let mut out_range = Vec::with_capacity(notes.len());
    for note in notes.drain(..) {
        if note.row_index >= start_row && note.row_index <= end_row {
            in_range.push(note);
        } else {
            out_range.push(note);
        }
    }
    if in_range.is_empty() {
        *notes = out_range;
        return;
    }

    apply_uncommon_masks_with_masks(
        &mut in_range,
        mods.insert_mask,
        mods.remove_mask,
        mods.holds_mask,
        timing_player,
        col_offset,
        cols,
        &out_range,
        Some(row_bounds),
        player,
    );
    apply_attack_turn_mod(
        &mut in_range,
        col_offset,
        cols,
        mods.turn_option,
        turn_seed,
        player,
    );

    out_range.extend(in_range);
    *notes = out_range;
    sort_player_notes(notes);
}

pub fn apply_chart_attack_windows(
    notes: &mut Vec<Note>,
    attacks: &[ChartAttackWindow],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    base_seed: u64,
) {
    for (i, attack) in attacks.iter().enumerate() {
        let mods = parse_attack_mods(&attack.mods);
        if !mods.has_chart_effect() {
            continue;
        }
        let Some(row_bounds) = chart_attack_row_range(attack, timing_player) else {
            continue;
        };
        apply_chart_attack_window(
            notes,
            timing_player,
            col_offset,
            cols,
            player,
            row_bounds,
            mods,
            chart_attack_turn_seed(base_seed, player, i),
        );
    }
}

pub fn apply_chart_attacks_for_mode(
    notes: &mut Vec<Note>,
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) {
    let attacks = build_attack_windows_for_mode(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if !attacks.is_empty() {
        apply_chart_attack_windows(
            notes,
            &attacks,
            timing_player,
            col_offset,
            cols,
            player,
            base_seed,
        );
    }
}

#[derive(Clone, Copy)]
pub struct ChartAttackTransformPlayer<'a> {
    pub chart_attacks: Option<&'a str>,
    pub attack_mode: GameplayAttackMode,
    pub timing_player: &'a TimingData,
}

impl ChartAttackTransformPlayer<'_> {
    #[inline(always)]
    pub fn has_chart_attacks(self) -> bool {
        chart_attacks_enabled_for_mode(self.chart_attacks, self.attack_mode)
    }
}

pub fn apply_chart_attack_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    players: &[ChartAttackTransformPlayer<'_>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
) {
    let active_players = num_players.min(MAX_PLAYERS);
    if active_players == 0
        || !players
            .iter()
            .take(active_players)
            .any(|player| player.has_chart_attacks())
    {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];
    for player in 0..active_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        let attack_player = players[player];
        if !attack_player.has_chart_attacks() {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }

        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_chart_attacks_for_mode(
            &mut player_notes,
            attack_player.chart_attacks,
            attack_player.attack_mode,
            attack_player.timing_player,
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
            base_seed,
            song_length_seconds,
        );
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if active_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }
    *notes = transformed;
    *note_ranges = transformed_ranges;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AttackActiveTargets {
    pub clear_all: bool,
    pub visual: VisualOverrides,
    pub scroll: ScrollOverrides,
    pub mini_percent: bool,
}

#[inline(always)]
fn mark_active_target(targets: &mut Option<f32>, value: Option<f32>) {
    if value.is_some() {
        *targets = Some(0.0);
    }
}

fn mark_visual_targets(targets: &mut VisualOverrides, visual: VisualOverrides) {
    mark_active_target(&mut targets.drunk, visual.drunk);
    mark_active_target(&mut targets.dizzy, visual.dizzy);
    mark_active_target(&mut targets.confusion, visual.confusion);
    mark_active_target(&mut targets.confusion_offset, visual.confusion_offset);
    for (target, value) in targets
        .confusion_offset_cols
        .iter_mut()
        .zip(visual.confusion_offset_cols)
    {
        mark_active_target(target, value);
    }
    mark_active_target(&mut targets.flip, visual.flip);
    mark_active_target(&mut targets.invert, visual.invert);
    mark_active_target(&mut targets.tornado, visual.tornado);
    mark_active_target(&mut targets.tipsy, visual.tipsy);
    mark_active_target(&mut targets.tiny, visual.tiny);
    mark_active_target(&mut targets.bumpy, visual.bumpy);
    mark_active_target(&mut targets.bumpy_offset, visual.bumpy_offset);
    mark_active_target(&mut targets.bumpy_period, visual.bumpy_period);
    for (target, value) in targets.bumpy_cols.iter_mut().zip(visual.bumpy_cols) {
        mark_active_target(target, value);
    }
    for (target, value) in targets.tiny_cols.iter_mut().zip(visual.tiny_cols) {
        mark_active_target(target, value);
    }
    for (target, value) in targets.move_x_cols.iter_mut().zip(visual.move_x_cols) {
        mark_active_target(target, value);
    }
    for (target, value) in targets.move_y_cols.iter_mut().zip(visual.move_y_cols) {
        mark_active_target(target, value);
    }
    mark_active_target(&mut targets.pulse_inner, visual.pulse_inner);
    mark_active_target(&mut targets.pulse_outer, visual.pulse_outer);
    mark_active_target(&mut targets.pulse_period, visual.pulse_period);
    mark_active_target(&mut targets.pulse_offset, visual.pulse_offset);
    mark_active_target(&mut targets.beat, visual.beat);
}

fn mark_scroll_targets(targets: &mut ScrollOverrides, scroll: ScrollOverrides) {
    mark_active_target(&mut targets.reverse, scroll.reverse);
    mark_active_target(&mut targets.split, scroll.split);
    mark_active_target(&mut targets.alternate, scroll.alternate);
    mark_active_target(&mut targets.cross, scroll.cross);
    mark_active_target(&mut targets.centered, scroll.centered);
}

pub fn collect_active_attack_targets(
    windows: &[AttackMaskWindow],
    now: f32,
) -> AttackActiveTargets {
    let mut targets = AttackActiveTargets::default();
    for window in windows {
        if now < window.start_second || now >= window.end_second {
            continue;
        }
        if window.clear_all {
            targets.clear_all = true;
        }
        mark_visual_targets(&mut targets.visual, window.visual);
        mark_scroll_targets(&mut targets.scroll, window.scroll);
        if window.mini_percent.is_some() {
            targets.mini_percent = true;
        }
    }
    targets
}

#[inline(always)]
pub fn persisted_target_allowed(
    persisted: bool,
    active_clear_all: bool,
    active_target: Option<f32>,
) -> bool {
    !persisted || (!active_clear_all && active_target.is_none())
}

#[inline(always)]
pub fn persisted_mini_allowed(persisted: bool, active_targets: AttackActiveTargets) -> bool {
    !persisted || (!active_targets.clear_all && !active_targets.mini_percent)
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackMaskValues {
    pub clear_all: bool,
    pub chart: ChartAttackEffects,
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub visual_speed: VisualOverrides,
    pub appearance_target: AppearanceEffects,
    pub appearance_speed: AppearanceEffects,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub scroll_approach_speed: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
    pub mini_speed: Option<f32>,
}

impl ActiveAttackMaskValues {
    #[inline(always)]
    pub fn new(base_appearance: AppearanceEffects) -> Self {
        Self {
            clear_all: false,
            chart: ChartAttackEffects::default(),
            accel: AccelOverrides::default(),
            visual: VisualOverrides::default(),
            visual_speed: VisualOverrides::default(),
            appearance_target: base_appearance,
            appearance_speed: AppearanceEffects::approach_speeds(),
            visibility: VisibilityOverrides::default(),
            scroll: ScrollOverrides::default(),
            scroll_approach_speed: ScrollOverrides::default(),
            perspective: PerspectiveOverrides::default(),
            scroll_speed: None,
            mini_percent: None,
            mini_speed: None,
        }
    }

    #[inline(always)]
    fn clear_for_window(&mut self) {
        *self = Self::new(AppearanceEffects::default());
        self.clear_all = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackRefreshInput<'a> {
    pub now: f32,
    pub delta_time: f32,
    pub attacks_cleared_for_outro: bool,
    pub base_appearance: AppearanceEffects,
    pub base_visual: VisualEffects,
    pub base_scroll: ScrollEffects,
    pub base_mini_percent: f32,
    pub attack_windows: &'a [AttackMaskWindow],
    pub song_lua_ease_windows: &'a [SongLuaEaseMaskWindow],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AttackBaseEffects {
    pub appearance: AppearanceEffects,
    pub visual: VisualEffects,
    pub scroll: ScrollEffects,
    pub mini_percent: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackRefreshState {
    pub attack_current_appearance: AppearanceEffects,
    pub active_attack_visual: VisualOverrides,
    pub active_attack_visibility: VisibilityOverrides,
    pub active_attack_scroll: ScrollOverrides,
    pub active_attack_mini_percent: Option<f32>,
    pub outro_attack_visual: VisualOverrides,
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackRefreshOutput {
    pub attack_target_appearance: AppearanceEffects,
    pub attack_speed_appearance: AppearanceEffects,
    pub attack_current_appearance: AppearanceEffects,
    pub active_attack_clear_all: bool,
    pub active_attack_chart: ChartAttackEffects,
    pub active_attack_accel: AccelOverrides,
    pub active_attack_visual: VisualOverrides,
    pub active_attack_appearance: AppearanceEffects,
    pub active_attack_visibility: VisibilityOverrides,
    pub active_attack_scroll: ScrollOverrides,
    pub active_attack_perspective: PerspectiveOverrides,
    pub active_attack_scroll_speed: Option<ScrollSpeedSetting>,
    pub active_attack_mini_percent: Option<f32>,
    pub outro_attack_visual: VisualOverrides,
    pub player_transform: SongLuaPlayerTransformValues,
}

#[derive(Clone, Debug)]
pub struct GameplayAttackRuntimeState {
    pub mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS],
    pub song_lua_ease_windows: [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS],
    pub cleared_for_outro: bool,
    pub clear_all: [bool; MAX_PLAYERS],
    pub chart: [ChartAttackEffects; MAX_PLAYERS],
    pub accel: [AccelOverrides; MAX_PLAYERS],
    pub visual: [VisualOverrides; MAX_PLAYERS],
    pub outro_visual: [VisualOverrides; MAX_PLAYERS],
    pub current_appearance: [AppearanceEffects; MAX_PLAYERS],
    pub target_appearance: [AppearanceEffects; MAX_PLAYERS],
    pub speed_appearance: [AppearanceEffects; MAX_PLAYERS],
    pub appearance: [AppearanceEffects; MAX_PLAYERS],
    pub visibility: [VisibilityOverrides; MAX_PLAYERS],
    pub scroll: [ScrollOverrides; MAX_PLAYERS],
    pub perspective: [PerspectiveOverrides; MAX_PLAYERS],
    pub scroll_speed: [Option<ScrollSpeedSetting>; MAX_PLAYERS],
    pub mini_percent: [Option<f32>; MAX_PLAYERS],
}

impl Default for GameplayAttackRuntimeState {
    fn default() -> Self {
        Self {
            mask_windows: std::array::from_fn(|_| Vec::new()),
            song_lua_ease_windows: std::array::from_fn(|_| Vec::new()),
            cleared_for_outro: false,
            clear_all: [false; MAX_PLAYERS],
            chart: [ChartAttackEffects::default(); MAX_PLAYERS],
            accel: [AccelOverrides::default(); MAX_PLAYERS],
            visual: [VisualOverrides::default(); MAX_PLAYERS],
            outro_visual: [VisualOverrides::default(); MAX_PLAYERS],
            current_appearance: [AppearanceEffects::default(); MAX_PLAYERS],
            target_appearance: [AppearanceEffects::default(); MAX_PLAYERS],
            speed_appearance: [AppearanceEffects::default(); MAX_PLAYERS],
            appearance: [AppearanceEffects::default(); MAX_PLAYERS],
            visibility: [VisibilityOverrides::default(); MAX_PLAYERS],
            scroll: [ScrollOverrides::default(); MAX_PLAYERS],
            perspective: [PerspectiveOverrides::default(); MAX_PLAYERS],
            scroll_speed: [None; MAX_PLAYERS],
            mini_percent: [None; MAX_PLAYERS],
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameplayModRuntimeState<OverlayActor, CapturedActor, StateDelta> {
    pub song_lua_visuals: SongLuaRuntimeVisuals<OverlayActor, CapturedActor, StateDelta>,
    pub song_lua_player_transforms: SongLuaPlayerTransforms,
    pub attacks: GameplayAttackRuntimeState,
}

pub fn apply_song_lua_player_eases(
    player: &mut SongLuaPlayerTransformValues,
    windows: &[SongLuaEaseMaskWindow],
    now: f32,
) {
    for window in windows {
        if let Some(value) = song_lua_ease_window_value(window, now) {
            song_lua_apply_player_transform_target(window.target, value, player);
        }
    }
}

pub fn apply_song_lua_attack_eases(
    attack: &mut ActiveAttackMaskValues,
    appearance: &mut AppearanceEffects,
    player: &mut SongLuaPlayerTransformValues,
    windows: &[SongLuaEaseMaskWindow],
    now: f32,
    mini_base_percent: f32,
) {
    for window in windows {
        if let Some(value) = song_lua_ease_window_value(window, now) {
            let value = if matches!(window.target, SongLuaEaseMaskTarget::MiniPercent) {
                mini_base_percent + value
            } else {
                value
            };
            song_lua_apply_eased_target(
                window.target,
                value,
                &mut attack.accel,
                &mut attack.visual,
                appearance,
                &mut attack.visibility,
                &mut attack.scroll,
                &mut attack.perspective,
                &mut attack.scroll_speed,
                &mut attack.mini_percent,
                player,
            );
        }
    }
}

pub fn apply_active_attack_mask_window(
    values: &mut ActiveAttackMaskValues,
    window: &AttackMaskWindow,
    active_targets: AttackActiveTargets,
    persisted: bool,
    profile_mini_percent: f32,
) {
    if window.clear_all {
        values.clear_for_window();
    }
    values.chart.insert_mask |= window.chart.insert_mask;
    values.chart.remove_mask |= window.chart.remove_mask;
    values.chart.holds_mask |= window.chart.holds_mask;
    values.chart.turn_bits |= window.chart.turn_bits;

    if let Some(v) = window.accel.boost {
        values.accel.boost = Some(v);
    }
    if let Some(v) = window.accel.brake {
        values.accel.brake = Some(v);
    }
    if let Some(v) = window.accel.wave {
        values.accel.wave = Some(v);
    }
    if let Some(v) = window.accel.expand {
        values.accel.expand = Some(v);
    }
    if let Some(v) = window.accel.boomerang {
        values.accel.boomerang = Some(v);
    }

    apply_active_visual_window(values, window, active_targets, persisted);
    apply_appearance_target(
        &mut values.appearance_target,
        &mut values.appearance_speed,
        window.appearance,
        window.appearance_speed,
    );

    if let Some(v) = window.visibility.dark {
        values.visibility.dark = Some(v);
    }
    if let Some(v) = window.visibility.blind {
        values.visibility.blind = Some(v);
    }
    if let Some(v) = window.visibility.cover {
        values.visibility.cover = Some(v);
    }

    apply_active_scroll_window(values, window, active_targets, persisted);

    if let Some(v) = window.perspective.tilt {
        values.perspective.tilt = Some(v);
    }
    if let Some(v) = window.perspective.skew {
        values.perspective.skew = Some(v);
    }
    if let Some(speed) = window.scroll_speed {
        values.scroll_speed = Some(speed);
    }
    if let Some(mini) = window.mini_percent.filter(|v| v.is_finite())
        && persisted_mini_allowed(persisted, active_targets)
    {
        let base = if values.clear_all {
            0.0
        } else {
            profile_mini_percent
        };
        values.mini_percent =
            Some(attack_mini_target_percent(mini, window.mini_mode, base).clamp(-100.0, 150.0));
        values.mini_speed = window.mini_speed;
    }
}

pub fn refresh_active_attack_player(
    input: ActiveAttackRefreshInput<'_>,
    mut state: ActiveAttackRefreshState,
) -> ActiveAttackRefreshOutput {
    let active_targets = collect_active_attack_targets(input.attack_windows, input.now);
    let mut attack = ActiveAttackMaskValues::new(input.base_appearance);
    let mut player_transform = SongLuaPlayerTransformValues::default();
    for window in input.attack_windows {
        let persisted = window.persist_after_end && input.now >= window.end_second;
        if !input.attacks_cleared_for_outro
            && input.now >= window.start_second
            && input.now < window.sustain_end_second
            && (input.now < window.end_second || persisted)
        {
            apply_active_attack_mask_window(
                &mut attack,
                window,
                active_targets,
                persisted,
                input.base_mini_percent,
            );
        }
    }

    approach_appearance_effects(
        &mut state.attack_current_appearance,
        attack.appearance_target,
        attack.appearance_speed,
        input.delta_time,
    );
    let mut appearance = state.attack_current_appearance;
    if input.attacks_cleared_for_outro {
        apply_song_lua_player_eases(
            &mut player_transform,
            input.song_lua_ease_windows,
            input.now,
        );
        let mut visual = state.outro_attack_visual;
        approach_visual_overrides_to_base(&mut visual, input.base_visual, input.delta_time);
        return ActiveAttackRefreshOutput {
            attack_target_appearance: attack.appearance_target,
            attack_speed_appearance: attack.appearance_speed,
            attack_current_appearance: appearance,
            active_attack_clear_all: false,
            active_attack_chart: ChartAttackEffects::default(),
            active_attack_accel: AccelOverrides::default(),
            active_attack_visual: visual,
            active_attack_appearance: appearance,
            active_attack_visibility: state.active_attack_visibility,
            active_attack_scroll: ScrollOverrides::default(),
            active_attack_perspective: PerspectiveOverrides::default(),
            active_attack_scroll_speed: None,
            active_attack_mini_percent: None,
            outro_attack_visual: visual,
            player_transform,
        };
    }

    let base_visual = if attack.clear_all {
        VisualEffects::default()
    } else {
        input.base_visual
    };
    approach_visual_overrides_to_target(
        &mut state.active_attack_visual,
        attack.visual,
        attack.visual_speed,
        base_visual,
        input.delta_time,
    );
    attack.visual = state.active_attack_visual;

    let base_scroll = if attack.clear_all {
        ScrollEffects::default()
    } else {
        input.base_scroll
    };
    approach_scroll_overrides_to_target(
        &mut state.active_attack_scroll,
        attack.scroll,
        attack.scroll_approach_speed,
        base_scroll,
        input.delta_time,
    );
    attack.scroll = state.active_attack_scroll;

    let base_mini_percent = if attack.clear_all {
        0.0
    } else {
        input.base_mini_percent
    };
    approach_attack_mini_percent_to_target(
        &mut state.active_attack_mini_percent,
        attack.mini_percent,
        base_mini_percent,
        attack.mini_speed,
        input.delta_time,
    );
    attack.mini_percent = state.active_attack_mini_percent;

    apply_song_lua_attack_eases(
        &mut attack,
        &mut appearance,
        &mut player_transform,
        input.song_lua_ease_windows,
        input.now,
        base_mini_percent,
    );
    if let Some(mini) = attack.mini_percent.filter(|v| v.is_finite()) {
        attack.mini_percent = Some(mini.clamp(-100.0, 150.0));
    }

    ActiveAttackRefreshOutput {
        attack_target_appearance: attack.appearance_target,
        attack_speed_appearance: attack.appearance_speed,
        attack_current_appearance: appearance,
        active_attack_clear_all: attack.clear_all,
        active_attack_chart: attack.chart,
        active_attack_accel: attack.accel,
        active_attack_visual: attack.visual,
        active_attack_appearance: appearance,
        active_attack_visibility: attack.visibility,
        active_attack_scroll: attack.scroll,
        active_attack_perspective: attack.perspective,
        active_attack_scroll_speed: attack.scroll_speed,
        active_attack_mini_percent: attack.mini_percent,
        outro_attack_visual: state.outro_attack_visual,
        player_transform,
    }
}

fn apply_active_visual_target(
    value: &mut Option<f32>,
    speed: &mut Option<f32>,
    incoming: Option<f32>,
    incoming_speed: Option<f32>,
    active_target: Option<f32>,
    active_clear_all: bool,
    persisted: bool,
) {
    if let Some(v) = incoming
        && persisted_target_allowed(persisted, active_clear_all, active_target)
    {
        *value = Some(v);
        *speed = incoming_speed;
    }
}

fn apply_active_visual_cols(
    values: &mut [Option<f32>; MAX_COLS],
    speeds: &mut [Option<f32>; MAX_COLS],
    incoming: [Option<f32>; MAX_COLS],
    incoming_speeds: [Option<f32>; MAX_COLS],
    active: [Option<f32>; MAX_COLS],
    active_clear_all: bool,
    persisted: bool,
) {
    for col in 0..MAX_COLS {
        apply_active_visual_target(
            &mut values[col],
            &mut speeds[col],
            incoming[col],
            incoming_speeds[col],
            active[col],
            active_clear_all,
            persisted,
        );
    }
}

fn apply_active_visual_window(
    values: &mut ActiveAttackMaskValues,
    window: &AttackMaskWindow,
    active_targets: AttackActiveTargets,
    persisted: bool,
) {
    let active_clear_all = active_targets.clear_all;
    apply_active_visual_target(
        &mut values.visual.drunk,
        &mut values.visual_speed.drunk,
        window.visual.drunk,
        window.visual_speed.drunk,
        active_targets.visual.drunk,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.dizzy,
        &mut values.visual_speed.dizzy,
        window.visual.dizzy,
        window.visual_speed.dizzy,
        active_targets.visual.dizzy,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.confusion,
        &mut values.visual_speed.confusion,
        window.visual.confusion,
        window.visual_speed.confusion,
        active_targets.visual.confusion,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.confusion_offset,
        &mut values.visual_speed.confusion_offset,
        window.visual.confusion_offset,
        window.visual_speed.confusion_offset,
        active_targets.visual.confusion_offset,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.confusion_offset_cols,
        &mut values.visual_speed.confusion_offset_cols,
        window.visual.confusion_offset_cols,
        window.visual_speed.confusion_offset_cols,
        active_targets.visual.confusion_offset_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.flip,
        &mut values.visual_speed.flip,
        window.visual.flip,
        window.visual_speed.flip,
        active_targets.visual.flip,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.invert,
        &mut values.visual_speed.invert,
        window.visual.invert,
        window.visual_speed.invert,
        active_targets.visual.invert,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.tornado,
        &mut values.visual_speed.tornado,
        window.visual.tornado,
        window.visual_speed.tornado,
        active_targets.visual.tornado,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.tipsy,
        &mut values.visual_speed.tipsy,
        window.visual.tipsy,
        window.visual_speed.tipsy,
        active_targets.visual.tipsy,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.tiny,
        &mut values.visual_speed.tiny,
        window.visual.tiny,
        window.visual_speed.tiny,
        active_targets.visual.tiny,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.bumpy,
        &mut values.visual_speed.bumpy,
        window.visual.bumpy,
        window.visual_speed.bumpy,
        active_targets.visual.bumpy,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.bumpy_offset,
        &mut values.visual_speed.bumpy_offset,
        window.visual.bumpy_offset,
        window.visual_speed.bumpy_offset,
        active_targets.visual.bumpy_offset,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.bumpy_period,
        &mut values.visual_speed.bumpy_period,
        window.visual.bumpy_period,
        window.visual_speed.bumpy_period,
        active_targets.visual.bumpy_period,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.bumpy_cols,
        &mut values.visual_speed.bumpy_cols,
        window.visual.bumpy_cols,
        window.visual_speed.bumpy_cols,
        active_targets.visual.bumpy_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.tiny_cols,
        &mut values.visual_speed.tiny_cols,
        window.visual.tiny_cols,
        window.visual_speed.tiny_cols,
        active_targets.visual.tiny_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.move_x_cols,
        &mut values.visual_speed.move_x_cols,
        window.visual.move_x_cols,
        window.visual_speed.move_x_cols,
        active_targets.visual.move_x_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.move_y_cols,
        &mut values.visual_speed.move_y_cols,
        window.visual.move_y_cols,
        window.visual_speed.move_y_cols,
        active_targets.visual.move_y_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_inner,
        &mut values.visual_speed.pulse_inner,
        window.visual.pulse_inner,
        window.visual_speed.pulse_inner,
        active_targets.visual.pulse_inner,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_outer,
        &mut values.visual_speed.pulse_outer,
        window.visual.pulse_outer,
        window.visual_speed.pulse_outer,
        active_targets.visual.pulse_outer,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_period,
        &mut values.visual_speed.pulse_period,
        window.visual.pulse_period,
        window.visual_speed.pulse_period,
        active_targets.visual.pulse_period,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_offset,
        &mut values.visual_speed.pulse_offset,
        window.visual.pulse_offset,
        window.visual_speed.pulse_offset,
        active_targets.visual.pulse_offset,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.beat,
        &mut values.visual_speed.beat,
        window.visual.beat,
        window.visual_speed.beat,
        active_targets.visual.beat,
        active_clear_all,
        persisted,
    );
}

fn apply_active_scroll_target(
    value: &mut Option<f32>,
    speed: &mut Option<f32>,
    incoming: Option<f32>,
    incoming_speed: Option<f32>,
    active_target: Option<f32>,
    active_clear_all: bool,
    persisted: bool,
) {
    if let Some(v) = incoming
        && persisted_target_allowed(persisted, active_clear_all, active_target)
    {
        *value = Some(v);
        *speed = incoming_speed;
    }
}

fn apply_active_scroll_window(
    values: &mut ActiveAttackMaskValues,
    window: &AttackMaskWindow,
    active_targets: AttackActiveTargets,
    persisted: bool,
) {
    let active_clear_all = active_targets.clear_all;
    apply_active_scroll_target(
        &mut values.scroll.reverse,
        &mut values.scroll_approach_speed.reverse,
        window.scroll.reverse,
        window.scroll_approach_speed.reverse,
        active_targets.scroll.reverse,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.split,
        &mut values.scroll_approach_speed.split,
        window.scroll.split,
        window.scroll_approach_speed.split,
        active_targets.scroll.split,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.alternate,
        &mut values.scroll_approach_speed.alternate,
        window.scroll.alternate,
        window.scroll_approach_speed.alternate,
        active_targets.scroll.alternate,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.cross,
        &mut values.scroll_approach_speed.cross,
        window.scroll.cross,
        window.scroll_approach_speed.cross,
        active_targets.scroll.cross,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.centered,
        &mut values.scroll_approach_speed.centered,
        window.scroll.centered,
        window.scroll_approach_speed.centered,
        active_targets.scroll.centered,
        active_clear_all,
        persisted,
    );
}

#[inline(always)]
pub const fn turn_option_bits(turn: GameplayTurnOption) -> u16 {
    match turn {
        GameplayTurnOption::None => 0,
        GameplayTurnOption::Mirror => 1 << 0,
        GameplayTurnOption::Left => 1 << 1,
        GameplayTurnOption::Right => 1 << 2,
        GameplayTurnOption::LRMirror => 1 << 3,
        GameplayTurnOption::UDMirror => 1 << 4,
        GameplayTurnOption::Shuffle => 1 << 5,
        GameplayTurnOption::Blender => 1 << 6,
        GameplayTurnOption::Random => 1 << 7,
    }
}

pub fn attack_token_key(token: &str) -> String {
    let mut key = String::with_capacity(token.len());
    for ch in token.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    while key.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        key.remove(0);
    }
    key
}

#[inline(always)]
pub fn mod_column_suffix(key: &str, prefix: &str) -> Option<usize> {
    let suffix = key.strip_prefix(prefix)?;
    if suffix.is_empty() {
        return None;
    }
    let col = suffix.parse::<usize>().ok()?;
    (1..=MAX_COLS).contains(&col).then_some(col - 1)
}

#[inline(always)]
fn parse_attack_scroll_override(token: &str) -> Option<ScrollSpeedSetting> {
    let trimmed = token.trim();
    let value = trimmed
        .strip_suffix('x')
        .or_else(|| trimmed.strip_suffix('X'))
        .and_then(|v| v.trim().parse::<f32>().ok());
    if let Some(v) = value.filter(|v| v.is_finite() && *v > 0.0) {
        return Some(ScrollSpeedSetting::XMod(v));
    }
    ScrollSpeedSetting::from_str(trimmed).ok()
}

#[inline(always)]
fn parse_attack_approach_prefix(token: &str) -> (f32, &str) {
    let token = token.trim();
    let Some(prefix) = token.split_ascii_whitespace().next() else {
        return (1.0, token);
    };
    if prefix.len() <= 1 || !prefix.starts_with('*') {
        return (1.0, token);
    }
    let Some(speed) = prefix[1..]
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
    else {
        return (1.0, token);
    };
    (speed.max(0.0), token[prefix.len()..].trim_start())
}

#[inline(always)]
fn attack_level(percent_value: Option<f32>) -> Option<f32> {
    let raw = percent_value.unwrap_or(100.0);
    raw.is_finite().then_some(raw / 100.0)
}

#[inline(always)]
fn parse_attack_percent_prefix(token: &str) -> (Option<f32>, &str) {
    let Some(idx) = token.find('%') else {
        return (None, token);
    };
    let value = token[..idx].trim().parse::<f32>().ok();
    (value, token[idx + 1..].trim())
}

#[inline(always)]
fn parse_attack_level_token(token: &str) -> (Option<f32>, &str) {
    let token = token.trim();
    if token.len() >= 3 && token[..3].eq_ignore_ascii_case("no ") {
        return (Some(0.0), token[3..].trim());
    }
    parse_attack_percent_prefix(token)
}

#[inline(always)]
fn set_approached_mod(
    value: &mut Option<f32>,
    value_speed: &mut Option<f32>,
    target: Option<f32>,
    approach_speed: f32,
) {
    *value = target;
    if target.is_some() {
        *value_speed = Some(approach_speed.max(0.0));
    }
}

fn apply_runtime_mod(
    out: &mut ParsedAttackMods,
    key: &str,
    percent_value: Option<f32>,
    approach_speed: f32,
) {
    if let Some(col) = mod_column_suffix(key, "bumpy") {
        set_approached_mod(
            &mut out.visual.bumpy_cols[col],
            &mut out.visual_speed.bumpy_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "tiny") {
        set_approached_mod(
            &mut out.visual.tiny_cols[col],
            &mut out.visual_speed.tiny_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "movex") {
        set_approached_mod(
            &mut out.visual.move_x_cols[col],
            &mut out.visual_speed.move_x_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "movey") {
        set_approached_mod(
            &mut out.visual.move_y_cols[col],
            &mut out.visual_speed.move_y_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "confusionoffset") {
        set_approached_mod(
            &mut out.visual.confusion_offset_cols[col],
            &mut out.visual_speed.confusion_offset_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }

    match key {
        "wide" => out.insert_mask |= INSERT_MASK_BIT_WIDE,
        "big" => out.insert_mask |= INSERT_MASK_BIT_BIG,
        "quick" => out.insert_mask |= INSERT_MASK_BIT_QUICK,
        "bmrize" => out.insert_mask |= INSERT_MASK_BIT_BMRIZE,
        "skippy" => out.insert_mask |= INSERT_MASK_BIT_SKIPPY,
        "echo" => out.insert_mask |= INSERT_MASK_BIT_ECHO,
        "stomp" => out.insert_mask |= INSERT_MASK_BIT_STOMP,
        "mines" => out.insert_mask |= INSERT_MASK_BIT_MINES,
        "little" => out.remove_mask |= REMOVE_MASK_BIT_LITTLE,
        "nomines" => out.remove_mask |= REMOVE_MASK_BIT_NO_MINES,
        "noholds" => out.remove_mask |= REMOVE_MASK_BIT_NO_HOLDS,
        "nojumps" => out.remove_mask |= REMOVE_MASK_BIT_NO_JUMPS,
        "nohands" => out.remove_mask |= REMOVE_MASK_BIT_NO_HANDS,
        "noquads" => out.remove_mask |= REMOVE_MASK_BIT_NO_QUADS,
        "nolifts" => out.remove_mask |= REMOVE_MASK_BIT_NO_LIFTS,
        "nofakes" => out.remove_mask |= REMOVE_MASK_BIT_NO_FAKES,
        "planted" => out.holds_mask |= HOLDS_MASK_BIT_PLANTED,
        "floored" => out.holds_mask |= HOLDS_MASK_BIT_FLOORED,
        "twister" => out.holds_mask |= HOLDS_MASK_BIT_TWISTER,
        "norolls" => out.holds_mask |= HOLDS_MASK_BIT_NO_ROLLS,
        "holdrolls" | "holdstorolls" => out.holds_mask |= HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        "mirror" => out.turn_option = GameplayTurnOption::Mirror,
        "left" => out.turn_option = GameplayTurnOption::Left,
        "right" => out.turn_option = GameplayTurnOption::Right,
        "lrmirror" => out.turn_option = GameplayTurnOption::LRMirror,
        "udmirror" => out.turn_option = GameplayTurnOption::UDMirror,
        "shuffle" => out.turn_option = GameplayTurnOption::Shuffle,
        "supershuffle" | "blender" => out.turn_option = GameplayTurnOption::Blender,
        "hypershuffle" => out.turn_option = GameplayTurnOption::Random,
        "reverse" => set_approached_mod(
            &mut out.scroll.reverse,
            &mut out.scroll_approach_speed.reverse,
            attack_level(percent_value),
            approach_speed,
        ),
        "split" => set_approached_mod(
            &mut out.scroll.split,
            &mut out.scroll_approach_speed.split,
            attack_level(percent_value),
            approach_speed,
        ),
        "alternate" => set_approached_mod(
            &mut out.scroll.alternate,
            &mut out.scroll_approach_speed.alternate,
            attack_level(percent_value),
            approach_speed,
        ),
        "cross" => set_approached_mod(
            &mut out.scroll.cross,
            &mut out.scroll_approach_speed.cross,
            attack_level(percent_value),
            approach_speed,
        ),
        "centered" => set_approached_mod(
            &mut out.scroll.centered,
            &mut out.scroll_approach_speed.centered,
            attack_level(percent_value),
            approach_speed,
        ),
        "boost" => out.accel.boost = attack_level(percent_value),
        "brake" => out.accel.brake = attack_level(percent_value),
        "wave" => out.accel.wave = attack_level(percent_value),
        "expand" => out.accel.expand = attack_level(percent_value),
        "boomerang" => out.accel.boomerang = attack_level(percent_value),
        "drunk" => set_approached_mod(
            &mut out.visual.drunk,
            &mut out.visual_speed.drunk,
            attack_level(percent_value),
            approach_speed,
        ),
        "dizzy" => set_approached_mod(
            &mut out.visual.dizzy,
            &mut out.visual_speed.dizzy,
            attack_level(percent_value),
            approach_speed,
        ),
        "confusion" => set_approached_mod(
            &mut out.visual.confusion,
            &mut out.visual_speed.confusion,
            attack_level(percent_value),
            approach_speed,
        ),
        "confusionoffset" => set_approached_mod(
            &mut out.visual.confusion_offset,
            &mut out.visual_speed.confusion_offset,
            attack_level(percent_value),
            approach_speed,
        ),
        "flip" => set_approached_mod(
            &mut out.visual.flip,
            &mut out.visual_speed.flip,
            attack_level(percent_value),
            approach_speed,
        ),
        "invert" => set_approached_mod(
            &mut out.visual.invert,
            &mut out.visual_speed.invert,
            attack_level(percent_value),
            approach_speed,
        ),
        "tornado" => set_approached_mod(
            &mut out.visual.tornado,
            &mut out.visual_speed.tornado,
            attack_level(percent_value),
            approach_speed,
        ),
        "tipsy" => set_approached_mod(
            &mut out.visual.tipsy,
            &mut out.visual_speed.tipsy,
            attack_level(percent_value),
            approach_speed,
        ),
        "bumpy" => set_approached_mod(
            &mut out.visual.bumpy,
            &mut out.visual_speed.bumpy,
            attack_level(percent_value),
            approach_speed,
        ),
        "bumpyoffset" => set_approached_mod(
            &mut out.visual.bumpy_offset,
            &mut out.visual_speed.bumpy_offset,
            attack_level(percent_value),
            approach_speed,
        ),
        "bumpyperiod" => set_approached_mod(
            &mut out.visual.bumpy_period,
            &mut out.visual_speed.bumpy_period,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseinner" => set_approached_mod(
            &mut out.visual.pulse_inner,
            &mut out.visual_speed.pulse_inner,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseouter" => set_approached_mod(
            &mut out.visual.pulse_outer,
            &mut out.visual_speed.pulse_outer,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseperiod" => set_approached_mod(
            &mut out.visual.pulse_period,
            &mut out.visual_speed.pulse_period,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseoffset" => set_approached_mod(
            &mut out.visual.pulse_offset,
            &mut out.visual_speed.pulse_offset,
            attack_level(percent_value),
            approach_speed,
        ),
        "beat" => set_approached_mod(
            &mut out.visual.beat,
            &mut out.visual_speed.beat,
            attack_level(percent_value),
            approach_speed,
        ),
        "tiny" => set_approached_mod(
            &mut out.visual.tiny,
            &mut out.visual_speed.tiny,
            attack_level(percent_value),
            approach_speed,
        ),
        "mini" => {
            let mini = percent_value.unwrap_or(100.0);
            if mini.is_finite() {
                out.mini_percent = Some(mini);
                out.mini_speed = Some(approach_speed.max(0.0));
            }
        }
        "hidden" => {
            out.appearance.hidden = attack_level(percent_value);
            out.appearance_speed.hidden = Some(approach_speed);
        }
        "hiddenoffset" => {
            out.appearance.hidden_offset = attack_level(percent_value);
            out.appearance_speed.hidden_offset = Some(approach_speed);
        }
        "sudden" => {
            out.appearance.sudden = attack_level(percent_value);
            out.appearance_speed.sudden = Some(approach_speed);
        }
        "suddenoffset" => {
            out.appearance.sudden_offset = attack_level(percent_value);
            out.appearance_speed.sudden_offset = Some(approach_speed);
        }
        "stealth" => {
            out.appearance.stealth = attack_level(percent_value);
            out.appearance_speed.stealth = Some(approach_speed);
        }
        "blink" => {
            out.appearance.blink = attack_level(percent_value);
            out.appearance_speed.blink = Some(approach_speed);
        }
        "rvanish" | "randomvanish" | "reversevanish" => {
            out.appearance.random_vanish = attack_level(percent_value);
            out.appearance_speed.random_vanish = Some(approach_speed);
        }
        "dark" => out.visibility.dark = attack_level(percent_value),
        "blind" => out.visibility.blind = attack_level(percent_value),
        "cover" => out.visibility.cover = attack_level(percent_value),
        "overhead" => {
            out.perspective.tilt = Some(0.0);
            out.perspective.skew = Some(0.0);
        }
        "incoming" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(-level);
            out.perspective.skew = Some(level);
        }
        "space" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(level);
            out.perspective.skew = Some(level);
        }
        "hallway" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(-level);
            out.perspective.skew = Some(0.0);
        }
        "distant" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(level);
            out.perspective.skew = Some(0.0);
        }
        _ => {}
    }
}

pub fn parse_attack_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let (approach_speed, token) = parse_attack_approach_prefix(token);
        if token.is_empty() {
            continue;
        }
        if let Some(scroll_speed) = parse_attack_scroll_override(token) {
            out.scroll_speed = Some(scroll_speed);
            continue;
        }
        let (percent_value, token_key) = parse_attack_level_token(token);
        let key = attack_token_key(token_key);
        if key.is_empty() {
            continue;
        }
        match key.as_str() {
            "clearall" => {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
            }
            _ => apply_runtime_mod(&mut out, key.as_str(), percent_value, approach_speed),
        }
    }
    out
}

#[inline(always)]
fn parse_song_lua_mod_amount(word: &str) -> Option<f32> {
    let word = word.trim();
    if word.eq_ignore_ascii_case("no") {
        return Some(0.0);
    }
    if let Some(value) = word.strip_suffix('%') {
        return value.trim().parse::<f32>().ok();
    }
    word.parse::<f32>().ok()
}

pub fn parse_song_lua_runtime_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let mut parts = token.trim().split_ascii_whitespace();
        let Some(first) = parts.next() else {
            continue;
        };
        let Some(second) = parts.next() else {
            if let Some(scroll_speed) = parse_attack_scroll_override(first) {
                out.scroll_speed = Some(scroll_speed);
                continue;
            }
            let key = attack_token_key(first);
            if key.is_empty() {
                continue;
            }
            if key == "clearall" {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
                continue;
            }
            apply_runtime_mod(&mut out, key.as_str(), Some(100.0), 1.0);
            continue;
        };

        if first.starts_with('*') {
            let approach_speed = parse_attack_approach_prefix(first).0;
            let Some(third) = parts.next() else {
                if let Some(scroll_speed) = parse_attack_scroll_override(second) {
                    out.scroll_speed = Some(scroll_speed);
                    continue;
                }
                let key = attack_token_key(second);
                if !key.is_empty() {
                    apply_runtime_mod(&mut out, key.as_str(), Some(100.0), approach_speed);
                }
                continue;
            };
            let key = attack_token_key(third);
            if key.is_empty() {
                continue;
            }
            let amount = parse_song_lua_mod_amount(second).unwrap_or(0.0);
            apply_runtime_mod(&mut out, key.as_str(), Some(amount), approach_speed);
            continue;
        }

        let key = attack_token_key(second);
        if key.is_empty() {
            continue;
        }
        let amount = parse_song_lua_mod_amount(first).unwrap_or(0.0);
        apply_runtime_mod(&mut out, key.as_str(), Some(amount), 1.0);
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollEffects {
    pub reverse: f32,
    pub split: f32,
    pub alternate: f32,
    pub cross: f32,
    pub centered: f32,
}

impl ScrollEffects {
    #[inline(always)]
    pub fn from_flags(
        reverse: bool,
        split: bool,
        alternate: bool,
        cross: bool,
        centered: bool,
    ) -> Self {
        Self {
            reverse: f32::from(reverse),
            split: f32::from(split),
            alternate: f32::from(alternate),
            cross: f32::from(cross),
            centered: f32::from(centered),
        }
    }

    #[inline(always)]
    pub fn reverse_percent_for_column(self, local_col: usize, num_cols: usize) -> f32 {
        scroll_reverse_percent_for_column(
            ScrollReverseOptions {
                reverse: self.reverse,
                split: self.split,
                alternate: self.alternate,
                cross: self.cross,
            },
            local_col,
            num_cols,
        )
    }

    #[inline(always)]
    pub fn reverse_scale_for_column(self, local_col: usize, num_cols: usize) -> f32 {
        scroll_reverse_scale_for_column(
            ScrollReverseOptions {
                reverse: self.reverse,
                split: self.split,
                alternate: self.alternate,
                cross: self.cross,
            },
            local_col,
            num_cols,
        )
    }
}

pub fn approach_scroll_overrides_to_target(
    current: &mut ScrollOverrides,
    target: ScrollOverrides,
    speed: ScrollOverrides,
    base: ScrollEffects,
    delta_time: f32,
) {
    approach_attack_value(
        &mut current.reverse,
        target.reverse,
        base.reverse,
        speed.reverse,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.split,
        target.split,
        base.split,
        speed.split,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.alternate,
        target.alternate,
        base.alternate,
        speed.alternate,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.cross,
        target.cross,
        base.cross,
        speed.cross,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.centered,
        target.centered,
        base.centered,
        speed.centered,
        delta_time,
        1.0,
    );
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerspectiveEffects {
    pub tilt: f32,
    pub skew: f32,
}

#[inline(always)]
pub fn merge_attack_value(base: f32, attack: Option<f32>) -> f32 {
    attack.filter(|v| v.is_finite()).unwrap_or(base)
}

#[inline(always)]
pub fn merge_attack_accel_effects(base: AccelEffects, attack: AccelOverrides) -> AccelEffects {
    AccelEffects {
        boost: merge_attack_value(base.boost, attack.boost),
        brake: merge_attack_value(base.brake, attack.brake),
        wave: merge_attack_value(base.wave, attack.wave),
        expand: merge_attack_value(base.expand, attack.expand),
        boomerang: merge_attack_value(base.boomerang, attack.boomerang),
    }
}

pub fn merge_attack_visual_effects(base: VisualEffects, attack: VisualOverrides) -> VisualEffects {
    let mut confusion_offset_cols = base.confusion_offset_cols;
    let mut bumpy_cols = base.bumpy_cols;
    let mut tiny_cols = base.tiny_cols;
    let mut move_x_cols = base.move_x_cols;
    let mut move_y_cols = base.move_y_cols;
    for i in 0..MAX_COLS {
        if let Some(v) = attack.confusion_offset_cols[i].filter(|v| v.is_finite()) {
            confusion_offset_cols[i] = v;
        }
        if let Some(v) = attack.bumpy_cols[i].filter(|v| v.is_finite()) {
            bumpy_cols[i] = v;
        }
        if let Some(v) = attack.tiny_cols[i].filter(|v| v.is_finite()) {
            tiny_cols[i] = v;
        }
        if let Some(v) = attack.move_x_cols[i].filter(|v| v.is_finite()) {
            move_x_cols[i] = v;
        }
        if let Some(v) = attack.move_y_cols[i].filter(|v| v.is_finite()) {
            move_y_cols[i] = v;
        }
    }
    VisualEffects {
        drunk: merge_attack_value(base.drunk, attack.drunk),
        dizzy: merge_attack_value(base.dizzy, attack.dizzy),
        confusion: merge_attack_value(base.confusion, attack.confusion),
        confusion_offset: merge_attack_value(base.confusion_offset, attack.confusion_offset),
        confusion_offset_cols,
        big: base.big,
        flip: merge_attack_value(base.flip, attack.flip),
        invert: merge_attack_value(base.invert, attack.invert),
        tornado: merge_attack_value(base.tornado, attack.tornado),
        tipsy: merge_attack_value(base.tipsy, attack.tipsy),
        tiny: merge_attack_value(base.tiny, attack.tiny),
        bumpy: merge_attack_value(base.bumpy, attack.bumpy),
        bumpy_offset: merge_attack_value(base.bumpy_offset, attack.bumpy_offset),
        bumpy_period: merge_attack_value(base.bumpy_period, attack.bumpy_period),
        bumpy_cols,
        tiny_cols,
        move_x_cols,
        move_y_cols,
        pulse_inner: merge_attack_value(base.pulse_inner, attack.pulse_inner),
        pulse_outer: merge_attack_value(base.pulse_outer, attack.pulse_outer),
        pulse_period: merge_attack_value(base.pulse_period, attack.pulse_period),
        pulse_offset: merge_attack_value(base.pulse_offset, attack.pulse_offset),
        beat: merge_attack_value(base.beat, attack.beat),
    }
}

#[inline(always)]
pub fn merge_attack_visibility_effects(
    base: VisibilityEffects,
    attack: VisibilityOverrides,
) -> VisibilityEffects {
    VisibilityEffects {
        dark: merge_attack_value(base.dark, attack.dark),
        blind: merge_attack_value(base.blind, attack.blind),
        cover: merge_attack_value(base.cover, attack.cover),
    }
}

#[inline(always)]
pub fn merge_attack_scroll_effects(base: ScrollEffects, attack: ScrollOverrides) -> ScrollEffects {
    ScrollEffects {
        reverse: merge_attack_value(base.reverse, attack.reverse),
        split: merge_attack_value(base.split, attack.split),
        alternate: merge_attack_value(base.alternate, attack.alternate),
        cross: merge_attack_value(base.cross, attack.cross),
        centered: merge_attack_value(base.centered, attack.centered),
    }
}

#[inline(always)]
pub fn merge_attack_perspective_effects(
    base: PerspectiveEffects,
    attack: PerspectiveOverrides,
) -> PerspectiveEffects {
    PerspectiveEffects {
        tilt: merge_attack_value(base.tilt, attack.tilt),
        skew: merge_attack_value(base.skew, attack.skew),
    }
}

#[inline(always)]
pub fn effective_attack_accel_effects(
    base_cleared: bool,
    profile_mask_bits: u8,
    attack: AccelOverrides,
) -> AccelEffects {
    let base = if base_cleared {
        AccelEffects::default()
    } else {
        AccelEffects::from_mask_bits(profile_mask_bits)
    };
    merge_attack_accel_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_visual_effects(
    base_cleared: bool,
    profile_mask_bits: u16,
    attack: VisualOverrides,
) -> VisualEffects {
    let base = if base_cleared {
        VisualEffects::default()
    } else {
        VisualEffects::from_mask_bits(profile_mask_bits)
    };
    merge_attack_visual_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_visibility_effects(attack: VisibilityOverrides) -> VisibilityEffects {
    merge_attack_visibility_effects(VisibilityEffects::default(), attack)
}

#[inline(always)]
pub fn effective_attack_scroll_effects(
    base_cleared: bool,
    base_scroll: ScrollEffects,
    attack: ScrollOverrides,
) -> ScrollEffects {
    let base = if base_cleared {
        ScrollEffects::default()
    } else {
        base_scroll
    };
    merge_attack_scroll_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_perspective_effects(
    base_cleared: bool,
    base_perspective: PerspectiveEffects,
    attack: PerspectiveOverrides,
) -> PerspectiveEffects {
    let base = if base_cleared {
        PerspectiveEffects::default()
    } else {
        base_perspective
    };
    merge_attack_perspective_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_scroll_speed(
    base_cleared: bool,
    active_scroll_speed: Option<ScrollSpeedSetting>,
    base_scroll_speed: ScrollSpeedSetting,
) -> ScrollSpeedSetting {
    active_scroll_speed.unwrap_or_else(|| {
        if base_cleared {
            ScrollSpeedSetting::default()
        } else {
            base_scroll_speed
        }
    })
}

pub const SPACING_PERCENT_MIN: i32 = -100;
pub const SPACING_PERCENT_MAX: i32 = 100;

/// Multiplier applied to noteskin per-column lateral offsets for Spacing.
#[inline(always)]
pub fn spacing_multiplier_for_percent(spacing_percent: i32) -> f32 {
    let clamped = spacing_percent.clamp(SPACING_PERCENT_MIN, SPACING_PERCENT_MAX);
    1.0 + clamped as f32 / 100.0
}

#[inline(always)]
pub fn toggle_flash_alpha(timer_remaining: f32) -> Option<f32> {
    if timer_remaining <= 0.0 {
        return None;
    }
    let age = TOGGLE_FLASH_DURATION - timer_remaining;
    let alpha = if age < TOGGLE_FLASH_FADE_START {
        1.0
    } else {
        let fade_len = TOGGLE_FLASH_DURATION - TOGGLE_FLASH_FADE_START;
        1.0 - ((age - TOGGLE_FLASH_FADE_START) / fade_len).clamp(0.0, 1.0)
    };
    Some(alpha)
}

#[inline(always)]
pub fn tick_positive_timer(timer: &mut f32, delta_time: f32) {
    if *timer > 0.0 {
        *timer = (*timer - delta_time).max(0.0);
    }
}

#[inline(always)]
pub fn approach_f32(current: &mut f32, target: f32, step: f32) {
    if !current.is_finite() || !target.is_finite() {
        *current = target;
        return;
    }
    let step = step.max(0.0);
    if step <= f32::EPSILON || (*current - target).abs() <= f32::EPSILON {
        return;
    }
    let delta = target - *current;
    let step = delta.clamp(-step, step);
    if step.abs() >= delta.abs() {
        *current = target;
    } else {
        *current += step;
    }
}

