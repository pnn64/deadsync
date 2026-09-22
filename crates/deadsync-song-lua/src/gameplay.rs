//! Translate compiled Lua commands into deterministic gameplay windows.
pub type SongLuaRuntimeOverlayStateDelta =
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<crate::SongLuaOverlayStateDelta>;

#[must_use]
pub const fn song_lua_runtime_time_unit(
    unit: crate::SongLuaTimeUnit,
) -> deadsync_gameplay::SongLuaRuntimeTimeUnit {
    match unit {
        crate::SongLuaTimeUnit::Beat => deadsync_gameplay::SongLuaRuntimeTimeUnit::Beat,
        crate::SongLuaTimeUnit::Second => deadsync_gameplay::SongLuaRuntimeTimeUnit::Second,
    }
}

#[must_use]
pub const fn song_lua_runtime_span_mode(
    span_mode: crate::SongLuaSpanMode,
) -> deadsync_gameplay::SongLuaRuntimeSpanMode {
    match span_mode {
        crate::SongLuaSpanMode::Len => deadsync_gameplay::SongLuaRuntimeSpanMode::Len,
        crate::SongLuaSpanMode::End => deadsync_gameplay::SongLuaRuntimeSpanMode::End,
    }
}

#[must_use]
pub fn song_lua_runtime_ease_target(
    target: &crate::SongLuaEaseTarget,
) -> deadsync_gameplay::SongLuaRuntimeEaseTargetOwned {
    match target {
        crate::SongLuaEaseTarget::Mod(target_name) => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Mod(target_name.clone())
        }
        crate::SongLuaEaseTarget::PlayerX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerX,
            )
        }
        crate::SongLuaEaseTarget::PlayerY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerY,
            )
        }
        crate::SongLuaEaseTarget::PlayerZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZ,
            )
        }
        crate::SongLuaEaseTarget::PlayerRotationX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationX,
            )
        }
        crate::SongLuaEaseTarget::PlayerRotationY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationY,
            )
        }
        crate::SongLuaEaseTarget::PlayerRotationZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationZ,
            )
        }
        crate::SongLuaEaseTarget::PlayerSkewX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewX,
            )
        }
        crate::SongLuaEaseTarget::PlayerSkewY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewY,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoom => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoom,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoomX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomX,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoomY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomY,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoomZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomZ,
            )
        }
        crate::SongLuaEaseTarget::Function => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Function
        }
    }
}

#[inline(always)]
fn song_lua_runtime_ease_target_ref(
    target: &crate::SongLuaEaseTarget,
) -> deadsync_gameplay::SongLuaRuntimeEaseTarget<'_> {
    match target {
        crate::SongLuaEaseTarget::Mod(target_name) => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Mod(target_name)
        }
        crate::SongLuaEaseTarget::PlayerX => deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
            deadsync_gameplay::SongLuaEaseMaskTarget::PlayerX,
        ),
        crate::SongLuaEaseTarget::PlayerY => deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
            deadsync_gameplay::SongLuaEaseMaskTarget::PlayerY,
        ),
        crate::SongLuaEaseTarget::PlayerZ => deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
            deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZ,
        ),
        crate::SongLuaEaseTarget::PlayerRotationX => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationX,
            )
        }
        crate::SongLuaEaseTarget::PlayerRotationY => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationY,
            )
        }
        crate::SongLuaEaseTarget::PlayerRotationZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationZ,
            )
        }
        crate::SongLuaEaseTarget::PlayerSkewX => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewX,
            )
        }
        crate::SongLuaEaseTarget::PlayerSkewY => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewY,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoom => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoom,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoomX => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomX,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoomY => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomY,
            )
        }
        crate::SongLuaEaseTarget::PlayerZoomZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTarget::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomZ,
            )
        }
        crate::SongLuaEaseTarget::Function => deadsync_gameplay::SongLuaRuntimeEaseTarget::Function,
    }
}

#[inline(always)]
const fn song_lua_runtime_column_transform_target(
    target: crate::SongLuaColumnTransformTarget,
) -> deadsync_gameplay::SongLuaColumnTransformTarget {
    match target {
        crate::SongLuaColumnTransformTarget::OffsetX => {
            deadsync_gameplay::SongLuaColumnTransformTarget::OffsetX
        }
        crate::SongLuaColumnTransformTarget::OffsetY => {
            deadsync_gameplay::SongLuaColumnTransformTarget::OffsetY
        }
        crate::SongLuaColumnTransformTarget::Zoom => {
            deadsync_gameplay::SongLuaColumnTransformTarget::Zoom
        }
        crate::SongLuaColumnTransformTarget::RotationZ => {
            deadsync_gameplay::SongLuaColumnTransformTarget::RotationZ
        }
    }
}

struct SongLuaModWindowRef<'a>(&'a crate::SongLuaModWindow);

impl deadsync_gameplay::SongLuaModWindowLike for SongLuaModWindowRef<'_> {
    #[inline(always)]
    fn player(&self) -> Option<u8> {
        self.0.player
    }

    #[inline(always)]
    fn unit(&self) -> deadsync_gameplay::SongLuaRuntimeTimeUnit {
        song_lua_runtime_time_unit(self.0.unit)
    }

    #[inline(always)]
    fn start(&self) -> f32 {
        self.0.start
    }

    #[inline(always)]
    fn limit(&self) -> f32 {
        self.0.limit
    }

    #[inline(always)]
    fn span_mode(&self) -> deadsync_gameplay::SongLuaRuntimeSpanMode {
        song_lua_runtime_span_mode(self.0.span_mode)
    }

    #[inline(always)]
    fn mods(&self) -> &str {
        &self.0.mods
    }
}

struct SongLuaEaseWindowRef<'a> {
    source: &'a crate::SongLuaEaseWindow,
    target: deadsync_gameplay::SongLuaRuntimeEaseTarget<'a>,
}

impl<'a> SongLuaEaseWindowRef<'a> {
    #[inline(always)]
    fn new(source: &'a crate::SongLuaEaseWindow) -> Self {
        Self {
            source,
            target: song_lua_runtime_ease_target_ref(&source.target),
        }
    }
}

impl<'a> deadsync_gameplay::SongLuaEaseWindowLike for SongLuaEaseWindowRef<'a> {
    fn approach_speed(&self) -> Option<f32> {
        self.source.approach_speed
    }
    type Target = deadsync_gameplay::SongLuaRuntimeEaseTarget<'a>;

    #[inline(always)]
    fn player(&self) -> Option<u8> {
        self.source.player
    }

    #[inline(always)]
    fn unit(&self) -> deadsync_gameplay::SongLuaRuntimeTimeUnit {
        song_lua_runtime_time_unit(self.source.unit)
    }

    #[inline(always)]
    fn start(&self) -> f32 {
        self.source.start
    }

    #[inline(always)]
    fn limit(&self) -> f32 {
        self.source.limit
    }

    #[inline(always)]
    fn span_mode(&self) -> deadsync_gameplay::SongLuaRuntimeSpanMode {
        song_lua_runtime_span_mode(self.source.span_mode)
    }

    #[inline(always)]
    fn target(&self) -> &Self::Target {
        &self.target
    }

    #[inline(always)]
    fn from(&self) -> f32 {
        self.source.from
    }

    #[inline(always)]
    fn to(&self) -> f32 {
        self.source.to
    }

    #[inline(always)]
    fn easing(&self) -> Option<&str> {
        self.source.easing.as_deref()
    }

    #[inline(always)]
    fn sustain(&self) -> Option<f32> {
        self.source.sustain
    }

    #[inline(always)]
    fn opt1(&self) -> Option<f32> {
        self.source.opt1
    }

    #[inline(always)]
    fn opt2(&self) -> Option<f32> {
        self.source.opt2
    }
}

struct SongLuaColumnOffsetWindowRef<'a>(&'a crate::SongLuaColumnOffsetWindow);

impl deadsync_gameplay::SongLuaColumnOffsetWindowLike for SongLuaColumnOffsetWindowRef<'_> {
    #[inline(always)]
    fn player(&self) -> usize {
        self.0.player
    }

    #[inline(always)]
    fn unit(&self) -> deadsync_gameplay::SongLuaRuntimeTimeUnit {
        song_lua_runtime_time_unit(self.0.unit)
    }

    #[inline(always)]
    fn start(&self) -> f32 {
        self.0.start
    }

    #[inline(always)]
    fn limit(&self) -> f32 {
        self.0.limit
    }

    #[inline(always)]
    fn span_mode(&self) -> deadsync_gameplay::SongLuaRuntimeSpanMode {
        song_lua_runtime_span_mode(self.0.span_mode)
    }

    #[inline(always)]
    fn column(&self) -> usize {
        self.0.column
    }

    #[inline(always)]
    fn target(&self) -> deadsync_gameplay::SongLuaColumnTransformTarget {
        song_lua_runtime_column_transform_target(self.0.target)
    }

    #[inline(always)]
    fn from_y(&self) -> f32 {
        self.0.from_y
    }

    #[inline(always)]
    fn to_y(&self) -> f32 {
        self.0.to_y
    }

    #[inline(always)]
    fn easing(&self) -> Option<&str> {
        self.0.easing.as_deref()
    }

    #[inline(always)]
    fn sustain(&self) -> Option<f32> {
        self.0.sustain
    }

    #[inline(always)]
    fn opt1(&self) -> Option<f32> {
        self.0.opt1
    }

    #[inline(always)]
    fn opt2(&self) -> Option<f32> {
        self.0.opt2
    }
}

#[must_use]
pub fn song_lua_runtime_mod_windows(
    windows: &[crate::SongLuaModWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeModWindow> {
    windows
        .iter()
        .map(|window| deadsync_gameplay::SongLuaRuntimeModWindow {
            player: window.player,
            unit: song_lua_runtime_time_unit(window.unit),
            start: window.start,
            limit: window.limit,
            span_mode: song_lua_runtime_span_mode(window.span_mode),
            mods: window.mods.clone(),
        })
        .collect()
}

#[must_use]
pub fn song_lua_runtime_ease_windows(
    windows: &[crate::SongLuaEaseWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeEaseWindow> {
    windows
        .iter()
        .map(|window| deadsync_gameplay::SongLuaRuntimeEaseWindow {
            approach_speed: window.approach_speed,
            player: window.player,
            unit: song_lua_runtime_time_unit(window.unit),
            start: window.start,
            limit: window.limit,
            span_mode: song_lua_runtime_span_mode(window.span_mode),
            target: song_lua_runtime_ease_target(&window.target),
            from: window.from,
            to: window.to,
            easing: window.easing.clone(),
            sustain: window.sustain,
            opt1: window.opt1,
            opt2: window.opt2,
        })
        .collect()
}

#[must_use]
pub fn song_lua_runtime_column_offset_windows(
    windows: &[crate::SongLuaColumnOffsetWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeColumnOffsetWindow> {
    windows
        .iter()
        .map(
            |window| deadsync_gameplay::SongLuaRuntimeColumnOffsetWindow {
                player: window.player,
                unit: song_lua_runtime_time_unit(window.unit),
                start: window.start,
                limit: window.limit,
                span_mode: song_lua_runtime_span_mode(window.span_mode),
                column: window.column,
                target: song_lua_runtime_column_transform_target(window.target),
                from_y: window.from_y,
                to_y: window.to_y,
                easing: window.easing.clone(),
                sustain: window.sustain,
                opt1: window.opt1,
                opt2: window.opt2,
            },
        )
        .collect()
}

#[must_use]
pub const fn song_lua_overlay_delta_mask(
    delta: &crate::SongLuaOverlayStateDelta,
) -> deadsync_gameplay::SongLuaOverlayDeltaMask {
    let mut mask = 0u128;
    let mut bit = 0u32;
    macro_rules! field {
        ($field:ident) => {{
            if delta.$field.is_some() {
                mask |= 1u128 << bit;
            }
            bit += 1;
        }};
    }

    field!(x);
    field!(y);
    field!(z);
    field!(z_bias);
    field!(draw_order);
    field!(draw_by_z_position);
    field!(halign);
    field!(valign);
    field!(text_align);
    field!(uppercase);
    field!(shadow_len);
    field!(shadow_color);
    field!(glow);
    field!(fov);
    field!(vanishpoint);
    field!(diffuse);
    field!(vertex_colors);
    field!(visible);
    field!(cropleft);
    field!(cropright);
    field!(croptop);
    field!(cropbottom);
    field!(fadeleft);
    field!(faderight);
    field!(fadetop);
    field!(fadebottom);
    field!(mask_source);
    field!(mask_dest);
    field!(depth_test);
    field!(zoom);
    field!(zoom_x);
    field!(zoom_y);
    field!(zoom_z);
    field!(basezoom);
    field!(basezoom_x);
    field!(basezoom_y);
    field!(basezoom_z);
    field!(rot_x_deg);
    field!(rot_y_deg);
    field!(rot_z_deg);
    field!(skew_x);
    field!(skew_y);
    field!(blend);
    field!(vibrate);
    field!(effect_magnitude);
    field!(effect_clock);
    field!(effect_mode);
    field!(effect_color1);
    field!(effect_color2);
    field!(effect_period);
    field!(effect_offset);
    field!(effect_timing);
    field!(rainbow);
    field!(rainbow_scroll);
    field!(text_jitter);
    field!(text_distortion);
    field!(text_glow_mode);
    field!(mult_attrs_with_diffuse);
    field!(sprite_animate);
    field!(sprite_loop);
    field!(sprite_playback_rate);
    field!(sprite_state_delay);
    field!(sprite_state_index);
    field!(vert_spacing);
    field!(wrap_width_pixels);
    field!(max_width);
    field!(max_height);
    field!(max_w_pre_zoom);
    field!(max_h_pre_zoom);
    field!(max_dimension_uses_zoom);
    field!(texture_filtering);
    field!(texture_wrapping);
    field!(texcoord_offset);
    field!(custom_texture_rect);
    field!(texcoord_velocity);
    field!(size);
    field!(stretch_rect);
    field!(sound_play);

    let _ = bit;
    mask
}

#[must_use]
pub const fn song_lua_runtime_overlay_state_delta(
    delta: crate::SongLuaOverlayStateDelta,
) -> SongLuaRuntimeOverlayStateDelta {
    SongLuaRuntimeOverlayStateDelta {
        overlap_mask: song_lua_overlay_delta_mask(&delta),
        delta,
    }
}

#[must_use]
pub fn song_lua_runtime_overlay_ease_window(
    ease: &crate::SongLuaOverlayEase,
) -> deadsync_gameplay::SongLuaRuntimeOverlayEaseWindow<SongLuaRuntimeOverlayStateDelta> {
    deadsync_gameplay::SongLuaRuntimeOverlayEaseWindow {
        overlay_index: ease.overlay_index,
        unit: song_lua_runtime_time_unit(ease.unit),
        start: ease.start,
        limit: ease.limit,
        span_mode: song_lua_runtime_span_mode(ease.span_mode),
        sustain: ease.sustain,
        from: song_lua_runtime_overlay_state_delta(ease.from),
        to: song_lua_runtime_overlay_state_delta(ease.to),
        easing: ease.easing.clone(),
        opt1: ease.opt1,
        opt2: ease.opt2,
    }
}

#[must_use]
pub fn build_song_lua_constant_windows_for_player<OverlayActor>(
    compiled: &crate::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<deadsync_gameplay::AttackMaskWindow> {
    deadsync_gameplay::build_song_lua_constant_windows_for_player_iter(
        compiled.time_mods.iter().map(SongLuaModWindowRef),
        compiled.beat_mods.iter().map(SongLuaModWindowRef),
        timing_player,
        player,
        global_offset_seconds,
    )
}

#[must_use]
pub fn build_song_lua_ease_windows_for_player<OverlayActor>(
    compiled: &crate::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[deadsync_gameplay::AttackMaskWindow],
) -> (Vec<deadsync_gameplay::SongLuaEaseMaskWindow>, usize) {
    deadsync_gameplay::build_song_lua_ease_windows_for_player_iter(
        compiled.eases.iter().map(SongLuaEaseWindowRef::new),
        timing_player,
        player,
        global_offset_seconds,
        constant_windows,
        |_| {},
    )
}

#[must_use]
pub fn build_song_lua_column_offset_windows_for_player<OverlayActor>(
    compiled: &crate::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<deadsync_gameplay::SongLuaColumnOffsetWindowRuntime> {
    deadsync_gameplay::build_song_lua_column_offset_windows_for_player_iter(
        compiled
            .column_offsets
            .iter()
            .map(SongLuaColumnOffsetWindowRef),
        timing_player,
        player,
        global_offset_seconds,
    )
}

#[must_use]
pub fn build_song_lua_actor_message_events_for_commands(
    messages: &[crate::SongLuaMessageEvent],
    message_seconds: &[Option<f32>],
    commands: &[crate::SongLuaOverlayMessageCommand],
) -> Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime> {
    deadsync_gameplay::build_song_lua_actor_message_events_with_seconds(
        messages
            .iter()
            .enumerate()
            .map(|(idx, message)| (idx, message.message.as_str())),
        message_seconds,
        commands
            .iter()
            .enumerate()
            .map(|(idx, command)| (idx, command.message.as_str())),
    )
}

#[must_use]
pub fn build_song_lua_overlay_message_events_with_seconds<Kind>(
    compiled: &crate::CompiledSongLua<crate::SongLuaOverlayActor<Kind>>,
    message_seconds: &[Option<f32>],
) -> Vec<Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime>> {
    compiled
        .overlays
        .iter()
        .map(|overlay| {
            build_song_lua_actor_message_events_for_commands(
                &compiled.messages,
                message_seconds,
                &overlay.message_commands,
            )
        })
        .collect()
}

fn song_lua_compiled_overlay_ease_cutoff_second<Kind>(
    compiled: &crate::CompiledSongLua<crate::SongLuaOverlayActor<Kind>>,
    ease: &crate::SongLuaOverlayEase,
    overlay_events: &[Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime>],
    start_second: f32,
) -> Option<f32> {
    let overlay = compiled.overlays.get(ease.overlay_index)?;
    let events = overlay_events.get(ease.overlay_index)?;
    let from_mask = song_lua_overlay_delta_mask(&ease.from);
    let to_mask = song_lua_overlay_delta_mask(&ease.to);
    let blocks = events
        .iter()
        .filter_map(|event| {
            let command = overlay.message_commands.get(event.command_index)?;
            Some((event.event_second, command))
        })
        .flat_map(|(event_second, command)| {
            command.blocks.iter().map(move |block| {
                (
                    event_second,
                    block.start,
                    song_lua_overlay_delta_mask(&block.delta),
                )
            })
        });
    deadsync_gameplay::song_lua_overlay_ease_cutoff_second(
        start_second,
        &from_mask,
        &to_mask,
        blocks,
    )
}

#[must_use]
pub fn build_song_lua_overlay_ease_windows_with_events<Kind>(
    compiled: &crate::CompiledSongLua<crate::SongLuaOverlayActor<Kind>>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
    overlay_events: &[Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime>],
) -> Vec<deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>> {
    let mut out = Vec::new();
    for ease in &compiled.overlay_eases {
        let runtime_ease = song_lua_runtime_overlay_ease_window(ease);
        if let Some(window) = deadsync_gameplay::build_song_lua_overlay_ease_window_for(
            &runtime_ease,
            timing_player,
            global_offset_seconds,
            |start_second| {
                song_lua_compiled_overlay_ease_cutoff_second(
                    compiled,
                    ease,
                    overlay_events,
                    start_second,
                )
            },
        ) {
            out.push(window);
        }
    }
    out
}

#[must_use]
pub fn build_song_lua_overlay_ease_windows<Kind>(
    compiled: &crate::CompiledSongLua<crate::SongLuaOverlayActor<Kind>>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Vec<deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>> {
    let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        timing_player,
        global_offset_seconds,
    );
    let overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    )
}

#[must_use]
pub fn build_song_lua_overlay_update_tracks<OverlayActor>(
    compiled: &crate::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Vec<crate::SongLuaOverlayRuntimeUpdateTrack> {
    let mut out = Vec::with_capacity(compiled.overlay_updates.len());
    for track in &compiled.overlay_updates {
        let samples = track
            .samples
            .iter()
            .filter_map(|sample| {
                deadsync_gameplay::song_lua_message_second(
                    sample.beat,
                    timing_player,
                    global_offset_seconds,
                )
                .map(|second| crate::SongLuaOverlayRuntimeUpdateSample {
                    second,
                    value: sample.value.clone(),
                })
            })
            .collect::<Vec<_>>();
        if samples.is_empty() {
            continue;
        }
        out.push(crate::SongLuaOverlayRuntimeUpdateTrack {
            overlay_index: track.overlay_index,
            target: track.target,
            samples,
        });
    }
    out.sort_by_key(|track| track.overlay_index);
    out
}
