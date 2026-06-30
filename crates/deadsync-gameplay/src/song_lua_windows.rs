#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SongLuaCompilePlayStyle {
    #[default]
    Single,
    Double,
    Versus,
}

pub trait SongLuaCompilePlayStyleLike {
    fn as_song_lua_compile_play_style(self) -> SongLuaCompilePlayStyle;
}

impl SongLuaCompilePlayStyleLike for SongLuaCompilePlayStyle {
    #[inline(always)]
    fn as_song_lua_compile_play_style(self) -> SongLuaCompilePlayStyle {
        self
    }
}

impl SongLuaCompilePlayStyleLike for GameplayInputPlayStyle {
    #[inline(always)]
    fn as_song_lua_compile_play_style(self) -> SongLuaCompilePlayStyle {
        match self {
            GameplayInputPlayStyle::Single => SongLuaCompilePlayStyle::Single,
            GameplayInputPlayStyle::Versus => SongLuaCompilePlayStyle::Versus,
            GameplayInputPlayStyle::Double => SongLuaCompilePlayStyle::Double,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaRuntimeTimeUnit {
    Beat,
    Second,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaRuntimeSpanMode {
    Len,
    End,
}

pub trait SongLuaRuntimeTimeUnitLike {
    fn as_runtime_time_unit(self) -> SongLuaRuntimeTimeUnit;
}

impl SongLuaRuntimeTimeUnitLike for SongLuaRuntimeTimeUnit {
    #[inline(always)]
    fn as_runtime_time_unit(self) -> SongLuaRuntimeTimeUnit {
        self
    }
}

pub trait SongLuaRuntimeSpanModeLike {
    fn as_runtime_span_mode(self) -> SongLuaRuntimeSpanMode;
}

impl SongLuaRuntimeSpanModeLike for SongLuaRuntimeSpanMode {
    #[inline(always)]
    fn as_runtime_span_mode(self) -> SongLuaRuntimeSpanMode {
        self
    }
}

#[inline(always)]
pub fn song_lua_time_to_second_like<Unit>(
    unit: Unit,
    value: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> f32
where
    Unit: SongLuaRuntimeTimeUnitLike,
{
    song_lua_time_to_second(
        unit.as_runtime_time_unit(),
        value,
        timing_player,
        global_offset_seconds,
    )
}

#[inline(always)]
pub fn song_lua_window_seconds_like<Unit, Span>(
    unit: Unit,
    start: f32,
    limit: f32,
    span_mode: Span,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<(f32, f32)>
where
    Unit: SongLuaRuntimeTimeUnitLike,
    Span: SongLuaRuntimeSpanModeLike,
{
    song_lua_window_seconds(
        unit.as_runtime_time_unit(),
        start,
        limit,
        span_mode.as_runtime_span_mode(),
        timing_player,
        global_offset_seconds,
    )
}

#[inline(always)]
pub fn song_lua_sustain_end_second_like<Unit, Span>(
    unit: Unit,
    start: f32,
    limit: f32,
    span_mode: Span,
    sustain: Option<f32>,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    end_second: f32,
) -> f32
where
    Unit: SongLuaRuntimeTimeUnitLike,
    Span: SongLuaRuntimeSpanModeLike,
{
    song_lua_sustain_end_second(
        unit.as_runtime_time_unit(),
        start,
        limit,
        span_mode.as_runtime_span_mode(),
        sustain,
        timing_player,
        global_offset_seconds,
        end_second,
    )
}

#[inline(always)]
pub fn song_lua_target_matches_player(target_player: Option<u8>, player: usize) -> bool {
    match target_player {
        Some(target) => usize::from(target) == player + 1,
        None => true,
    }
}

#[inline(always)]
pub fn song_lua_end_value(start: f32, limit: f32, span_mode: SongLuaRuntimeSpanMode) -> f32 {
    match span_mode {
        SongLuaRuntimeSpanMode::Len => start + limit.max(0.0),
        SongLuaRuntimeSpanMode::End => limit,
    }
}

#[inline(always)]
pub fn song_lua_time_to_second(
    unit: SongLuaRuntimeTimeUnit,
    value: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> f32 {
    match unit {
        SongLuaRuntimeTimeUnit::Beat => timing_player.get_time_for_beat(value),
        SongLuaRuntimeTimeUnit::Second => value - global_offset_seconds,
    }
}

#[inline(always)]
pub fn song_lua_message_second(
    beat: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<f32> {
    let event_second = song_lua_time_to_second(
        SongLuaRuntimeTimeUnit::Beat,
        beat,
        timing_player,
        global_offset_seconds,
    );
    event_second.is_finite().then_some(event_second)
}

pub fn song_lua_window_seconds(
    unit: SongLuaRuntimeTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaRuntimeSpanMode,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<(f32, f32)> {
    let end = song_lua_end_value(start, limit, span_mode);
    let start_second = song_lua_time_to_second(unit, start, timing_player, global_offset_seconds);
    let end_second = song_lua_time_to_second(unit, end, timing_player, global_offset_seconds);
    if !start_second.is_finite() || !end_second.is_finite() || end_second < start_second {
        return None;
    }
    Some((start_second, end_second))
}

pub fn song_lua_sustain_end_second(
    unit: SongLuaRuntimeTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaRuntimeSpanMode,
    sustain: Option<f32>,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    end_second: f32,
) -> f32 {
    let Some(sustain) = sustain else {
        return end_second;
    };
    let sustain_value = match span_mode {
        SongLuaRuntimeSpanMode::Len => song_lua_end_value(start, limit, span_mode) + sustain,
        SongLuaRuntimeSpanMode::End => sustain,
    };
    let sustain_end_second =
        song_lua_time_to_second(unit, sustain_value, timing_player, global_offset_seconds);
    if sustain_end_second.is_finite() && sustain_end_second > end_second {
        sustain_end_second
    } else {
        end_second
    }
}

#[allow(clippy::too_many_arguments)]
pub trait SongLuaModWindowLike {
    fn player(&self) -> Option<u8>;
    fn unit(&self) -> SongLuaRuntimeTimeUnit;
    fn start(&self) -> f32;
    fn limit(&self) -> f32;
    fn span_mode(&self) -> SongLuaRuntimeSpanMode;
    fn mods(&self) -> &str;
}

#[derive(Clone, Debug)]
pub struct SongLuaRuntimeModWindow {
    pub player: Option<u8>,
    pub unit: SongLuaRuntimeTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaRuntimeSpanMode,
    pub mods: String,
}

impl SongLuaModWindowLike for SongLuaRuntimeModWindow {
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
    fn mods(&self) -> &str {
        &self.mods
    }
}

#[inline(always)]
fn build_song_lua_constant_window_from_mod<Window: SongLuaModWindowLike>(
    window: &Window,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<AttackMaskWindow> {
    let (start_second, end_second) = song_lua_window_seconds(
        window.unit(),
        window.start(),
        window.limit(),
        window.span_mode(),
        timing_player,
        global_offset_seconds,
    )?;

    build_song_lua_constant_attack_mask_window(start_second, end_second, window.mods())
}

pub fn build_song_lua_constant_windows_for_player<Window: SongLuaModWindowLike>(
    time_mods: &[Window],
    beat_mods: &[Window],
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let mut out = Vec::new();
    for window in time_mods {
        if song_lua_target_matches_player(window.player(), player)
            && let Some(window) = build_song_lua_constant_window_from_mod(
                window,
                timing_player,
                global_offset_seconds,
            )
        {
            out.push(window);
        }
    }
    for window in beat_mods {
        if song_lua_target_matches_player(window.player(), player)
            && let Some(window) = build_song_lua_constant_window_from_mod(
                window,
                timing_player,
                global_offset_seconds,
            )
        {
            out.push(window);
        }
    }
    out
}

pub trait SongLuaColumnOffsetWindowLike {
    fn player(&self) -> usize;
    fn unit(&self) -> SongLuaRuntimeTimeUnit;
    fn start(&self) -> f32;
    fn limit(&self) -> f32;
    fn span_mode(&self) -> SongLuaRuntimeSpanMode;
    fn column(&self) -> usize;
    fn from_y(&self) -> f32;
    fn to_y(&self) -> f32;
    fn easing(&self) -> Option<&str>;
    fn sustain(&self) -> Option<f32>;
    fn opt1(&self) -> Option<f32>;
    fn opt2(&self) -> Option<f32>;
}

#[derive(Clone, Debug)]
pub struct SongLuaRuntimeColumnOffsetWindow {
    pub player: usize,
    pub unit: SongLuaRuntimeTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaRuntimeSpanMode,
    pub column: usize,
    pub from_y: f32,
    pub to_y: f32,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

impl SongLuaColumnOffsetWindowLike for SongLuaRuntimeColumnOffsetWindow {
    #[inline(always)]
    fn player(&self) -> usize {
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
    fn column(&self) -> usize {
        self.column
    }

    #[inline(always)]
    fn from_y(&self) -> f32 {
        self.from_y
    }

    #[inline(always)]
    fn to_y(&self) -> f32 {
        self.to_y
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

pub fn build_song_lua_column_offset_windows_for_player<Window: SongLuaColumnOffsetWindowLike>(
    windows: &[Window],
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<SongLuaColumnOffsetWindowRuntime> {
    let mut out = Vec::new();
    for window in windows {
        if window.player() != player {
            continue;
        }
        let Some((start_second, end_second)) = song_lua_window_seconds(
            window.unit(),
            window.start(),
            window.limit(),
            window.span_mode(),
            timing_player,
            global_offset_seconds,
        ) else {
            continue;
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
        out.push(build_song_lua_column_offset_window_runtime(
            window.column(),
            start_second,
            end_second,
            sustain_end_second,
            window.from_y(),
            window.to_y(),
            window.easing(),
            window.opt1(),
            window.opt2(),
        ));
    }
    song_lua_extend_column_offset_tails(&mut out);
    out
}

pub struct SongLuaPlayerRuntimeWindows {
    pub constant_windows: Vec<AttackMaskWindow>,
    pub ease_windows: Vec<SongLuaEaseMaskWindow>,
    pub column_offsets: Vec<SongLuaColumnOffsetWindowRuntime>,
    pub unsupported_targets: usize,
}

pub fn build_song_lua_player_runtime_windows<ModWindow, EaseWindow, ColumnWindow>(
    time_mods: &[ModWindow],
    beat_mods: &[ModWindow],
    eases: &[EaseWindow],
    column_offset_windows: &[ColumnWindow],
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
    unsupported_ease: impl FnMut(&EaseWindow),
) -> SongLuaPlayerRuntimeWindows
where
    ModWindow: SongLuaModWindowLike,
    EaseWindow: SongLuaEaseWindowLike,
    ColumnWindow: SongLuaColumnOffsetWindowLike,
{
    let constant_windows = build_song_lua_constant_windows_for_player(
        time_mods,
        beat_mods,
        timing_player,
        player,
        global_offset_seconds,
    );
    let (ease_windows, unsupported_targets) = build_song_lua_ease_windows_for_player(
        eases,
        timing_player,
        player,
        global_offset_seconds,
        &constant_windows,
        unsupported_ease,
    );
    let column_offsets = build_song_lua_column_offset_windows_for_player(
        column_offset_windows,
        timing_player,
        player,
        global_offset_seconds,
    );
    SongLuaPlayerRuntimeWindows {
        constant_windows,
        ease_windows,
        column_offsets,
        unsupported_targets,
    }
}

pub fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    viewport: GameplayViewport,
    play_style: SongLuaCompilePlayStyle,
    single_player_uses_p2_side: bool,
    note_field_offset_x: f32,
    center_1player_notefield: bool,
) -> f32 {
    let clamped_width = viewport.width().clamp(640.0, 854.0);
    let centered_one_side = num_players == 1
        && play_style == SongLuaCompilePlayStyle::Single
        && center_1player_notefield;
    let centered_both_sides = num_players == 1 && play_style == SongLuaCompilePlayStyle::Double;
    let p2_side = if num_players == 1 {
        single_player_uses_p2_side
    } else {
        player_index == 1
    };
    let base_center_x = if num_players == 2 {
        if p2_side {
            viewport.center_x() + (clamped_width * 0.25)
        } else {
            viewport.center_x() - (clamped_width * 0.25)
        }
    } else if centered_both_sides || centered_one_side {
        viewport.center_x()
    } else if p2_side {
        viewport.center_x() + (clamped_width * 0.25)
    } else {
        viewport.center_x() - (clamped_width * 0.25)
    };
    if num_players == 1 && (centered_both_sides || centered_one_side) {
        viewport.center_x()
    } else {
        let offset_sign = if p2_side { 1.0 } else { -1.0 };
        base_center_x + offset_sign * note_field_offset_x.clamp(0.0, 50.0)
    }
}

pub fn song_lua_compile_player_screen_x_like<PlayStyle>(
    num_players: usize,
    player_index: usize,
    viewport: GameplayViewport,
    play_style: PlayStyle,
    single_player_uses_p2_side: bool,
    note_field_offset_x: f32,
    center_1player_notefield: bool,
) -> f32
where
    PlayStyle: SongLuaCompilePlayStyleLike,
{
    song_lua_compile_player_screen_x(
        num_players,
        player_index,
        viewport,
        play_style.as_song_lua_compile_play_style(),
        single_player_uses_p2_side,
        note_field_offset_x,
        center_1player_notefield,
    )
}

