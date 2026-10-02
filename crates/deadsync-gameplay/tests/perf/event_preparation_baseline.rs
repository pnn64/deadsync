// Frozen function bodies from 0410ebce6 (0.5.1685).
pub fn old_group_song_lua_overlay_eases<StateDelta>(
    overlay_count: usize,
    mut overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
) -> (
    Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    Vec<std::ops::Range<usize>>,
) {
    overlay_eases.retain(|ease| ease.overlay_index < overlay_count);
    overlay_eases.sort_by(|left, right| {
        left.overlay_index
            .cmp(&right.overlay_index)
            .then_with(|| left.start_second.total_cmp(&right.start_second))
            .then_with(|| left.end_second.total_cmp(&right.end_second))
            .then_with(|| left.sustain_end_second.total_cmp(&right.sustain_end_second))
    });
    let mut ranges = Vec::with_capacity(overlay_count);
    let mut end = 0;
    for overlay_index in 0..overlay_count {
        let start = end;
        while end < overlay_eases.len() && overlay_eases[end].overlay_index == overlay_index {
            end += 1;
        }
        ranges.push(start..end);
    }
    (overlay_eases, ranges)
}

pub fn old_build_song_lua_message_seconds(
    beats: impl IntoIterator<Item = f32>,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<Option<f32>> {
    beats
        .into_iter()
        .map(|beat| song_lua_message_second(beat, timing_player, global_offset_seconds))
        .collect()
}

fn old_push_song_lua_ease_target(
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
        approach_speed: None,
        start_second,
        end_second,
        sustain_end_second,
        target,
        from,
        to,
        easing: SongLuaEase::from_name(easing),
        opt1,
        opt2,
    });
}

pub fn old_append_song_lua_ease_targets(
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
    let mut key_buffer = [0u8; ATTACK_KEY_STACK_BYTES];
    let key = buffered_attack_token_key(target_name, &mut key_buffer);
    old_append_song_lua_ease_targets_key(
        out,
        start_second,
        end_second,
        sustain_end_second,
        key.as_str(),
        from,
        to,
        easing,
        opt1,
        opt2,
    )
}

fn old_append_song_lua_ease_targets_key(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    key: &str,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> bool {
    if key.is_empty() {
        return false;
    }
    let pct_from = song_lua_normalized_value(from);
    let pct_to = song_lua_normalized_value(to);
    let mut push = |target, from, to| {
        old_push_song_lua_ease_target(
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

    if let Some(col) = mod_column_suffix(key, "stealth") {
        push(
            SongLuaEaseMaskTarget::AppearanceStealthColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(key, "dark") {
        push(
            SongLuaEaseMaskTarget::VisibilityDarkColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(key, "bumpy") {
        push(
            SongLuaEaseMaskTarget::VisualBumpyColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(key, "tiny") {
        push(
            SongLuaEaseMaskTarget::VisualTinyColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(key, "movex") {
        push(
            SongLuaEaseMaskTarget::VisualMoveXColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(key, "movey") {
        push(
            SongLuaEaseMaskTarget::VisualMoveYColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(key, "confusionoffset") {
        push(
            SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }

    match key {
        "boost" => push(SongLuaEaseMaskTarget::AccelBoost, pct_from, pct_to),
        "brake" => push(SongLuaEaseMaskTarget::AccelBrake, pct_from, pct_to),
        "wave" => push(SongLuaEaseMaskTarget::AccelWave, pct_from, pct_to),
        "expand" => push(SongLuaEaseMaskTarget::AccelExpand, pct_from, pct_to),
        "boomerang" => push(SongLuaEaseMaskTarget::AccelBoomerang, pct_from, pct_to),
        "modtimersetting" => push(SongLuaEaseMaskTarget::VisualModTimerType, pct_from, pct_to),
        "modtimergame" => push(SongLuaEaseMaskTarget::VisualModTimerType, 0.0, 0.0),
        "modtimerbeat" => push(SongLuaEaseMaskTarget::VisualModTimerType, 1.0, 1.0),
        "modtimersong" => push(SongLuaEaseMaskTarget::VisualModTimerType, 2.0, 2.0),
        "modtimerdefault" => push(SongLuaEaseMaskTarget::VisualModTimerType, 3.0, 3.0),
        "dizzyholds" => push(SongLuaEaseMaskTarget::VisualDizzyHolds, pct_from, pct_to),
        "zbuffer" => push(SongLuaEaseMaskTarget::VisualZBuffer, pct_from, pct_to),
        "cosecant" => push(SongLuaEaseMaskTarget::VisualCosecant, pct_from, pct_to),
        "drunk" => push(SongLuaEaseMaskTarget::VisualDrunk, pct_from, pct_to),
        "drunkperiod" => push(SongLuaEaseMaskTarget::VisualDrunkPeriod, pct_from, pct_to),
        "drunkspeed" => push(SongLuaEaseMaskTarget::VisualDrunkSpeed, pct_from, pct_to),
        "drunkoffset" => push(SongLuaEaseMaskTarget::VisualDrunkOffset, pct_from, pct_to),
        "dizzy" => push(SongLuaEaseMaskTarget::VisualDizzy, pct_from, pct_to),
        "twirl" => push(SongLuaEaseMaskTarget::VisualTwirl, pct_from, pct_to),
        "roll" => push(SongLuaEaseMaskTarget::VisualRoll, pct_from, pct_to),
        "parabolax" => push(SongLuaEaseMaskTarget::VisualParabolaX, pct_from, pct_to),
        "modtimermult" => push(SongLuaEaseMaskTarget::VisualModTimerMult, pct_from, pct_to),
        "modtimeroffset" => push(
            SongLuaEaseMaskTarget::VisualModTimerOffset,
            pct_from,
            pct_to,
        ),
        "bumpyx" => push(SongLuaEaseMaskTarget::VisualBumpyX, pct_from, pct_to),
        "bumpyxoffset" => push(SongLuaEaseMaskTarget::VisualBumpyXOffset, pct_from, pct_to),
        "bumpyxperiod" => push(SongLuaEaseMaskTarget::VisualBumpyXPeriod, pct_from, pct_to),
        "tanbumpy" => push(SongLuaEaseMaskTarget::VisualTanBumpy, pct_from, pct_to),
        "tanbumpyoffset" => push(
            SongLuaEaseMaskTarget::VisualTanBumpyOffset,
            pct_from,
            pct_to,
        ),
        "tanbumpyperiod" => push(
            SongLuaEaseMaskTarget::VisualTanBumpyPeriod,
            pct_from,
            pct_to,
        ),
        "tanbumpyx" => push(SongLuaEaseMaskTarget::VisualTanBumpyX, pct_from, pct_to),
        "tanbumpyxoffset" => push(
            SongLuaEaseMaskTarget::VisualTanBumpyXOffset,
            pct_from,
            pct_to,
        ),
        "tanbumpyxperiod" => push(
            SongLuaEaseMaskTarget::VisualTanBumpyXPeriod,
            pct_from,
            pct_to,
        ),
        "drunkz" => push(SongLuaEaseMaskTarget::VisualDrunkZ, pct_from, pct_to),
        "drunkzoffset" => push(SongLuaEaseMaskTarget::VisualDrunkZOffset, pct_from, pct_to),
        "drunkzspeed" => push(SongLuaEaseMaskTarget::VisualDrunkZSpeed, pct_from, pct_to),
        "drunkzperiod" => push(SongLuaEaseMaskTarget::VisualDrunkZPeriod, pct_from, pct_to),
        "tandrunk" => push(SongLuaEaseMaskTarget::VisualTanDrunk, pct_from, pct_to),
        "tandrunkoffset" => push(
            SongLuaEaseMaskTarget::VisualTanDrunkOffset,
            pct_from,
            pct_to,
        ),
        "tandrunkspeed" => push(SongLuaEaseMaskTarget::VisualTanDrunkSpeed, pct_from, pct_to),
        "tandrunkperiod" => push(
            SongLuaEaseMaskTarget::VisualTanDrunkPeriod,
            pct_from,
            pct_to,
        ),
        "tandrunkz" => push(SongLuaEaseMaskTarget::VisualTanDrunkZ, pct_from, pct_to),
        "tandrunkzoffset" => push(
            SongLuaEaseMaskTarget::VisualTanDrunkZOffset,
            pct_from,
            pct_to,
        ),
        "tandrunkzspeed" => push(
            SongLuaEaseMaskTarget::VisualTanDrunkZSpeed,
            pct_from,
            pct_to,
        ),
        "tandrunkzperiod" => push(
            SongLuaEaseMaskTarget::VisualTanDrunkZPeriod,
            pct_from,
            pct_to,
        ),
        "drawsize" => push(SongLuaEaseMaskTarget::VisualDrawSize, pct_from, pct_to),
        "drawsizeback" => push(SongLuaEaseMaskTarget::VisualDrawSizeBack, pct_from, pct_to),
        "square" => push(SongLuaEaseMaskTarget::VisualSquare, pct_from, pct_to),
        "squareoffset" => push(SongLuaEaseMaskTarget::VisualSquareOffset, pct_from, pct_to),
        "squareperiod" => push(SongLuaEaseMaskTarget::VisualSquarePeriod, pct_from, pct_to),
        "squarez" => push(SongLuaEaseMaskTarget::VisualSquareZ, pct_from, pct_to),
        "squarezoffset" => push(SongLuaEaseMaskTarget::VisualSquareZOffset, pct_from, pct_to),
        "squarezperiod" => push(SongLuaEaseMaskTarget::VisualSquareZPeriod, pct_from, pct_to),
        "xmode" => push(SongLuaEaseMaskTarget::VisualXmode, pct_from, pct_to),
        "parabolaz" => push(SongLuaEaseMaskTarget::VisualParabolaZ, pct_from, pct_to),
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
        "tipsyspeed" => push(SongLuaEaseMaskTarget::VisualTipsySpeed, pct_from, pct_to),
        "tipsyoffset" => push(SongLuaEaseMaskTarget::VisualTipsyOffset, pct_from, pct_to),
        "bumpy" => push(SongLuaEaseMaskTarget::VisualBumpy, pct_from, pct_to),
        "bumpyoffset" => push(SongLuaEaseMaskTarget::VisualBumpyOffset, pct_from, pct_to),
        "bumpyperiod" => push(SongLuaEaseMaskTarget::VisualBumpyPeriod, pct_from, pct_to),
        "pulseinner" => push(SongLuaEaseMaskTarget::VisualPulseInner, pct_from, pct_to),
        "pulseouter" => push(SongLuaEaseMaskTarget::VisualPulseOuter, pct_from, pct_to),
        "pulseperiod" => push(SongLuaEaseMaskTarget::VisualPulsePeriod, pct_from, pct_to),
        "pulseoffset" => push(SongLuaEaseMaskTarget::VisualPulseOffset, pct_from, pct_to),
        "beat" => push(SongLuaEaseMaskTarget::VisualBeat, pct_from, pct_to),
        "randomspeed" => push(SongLuaEaseMaskTarget::VisualRandomSpeed, pct_from, pct_to),
        "hidden" => push(SongLuaEaseMaskTarget::AppearanceHidden, pct_from, pct_to),
        "hiddenoffset" => push(
            SongLuaEaseMaskTarget::AppearanceHiddenOffset,
            pct_from,
            pct_to,
        ),
        "sudden" => push(SongLuaEaseMaskTarget::AppearanceSudden, pct_from, pct_to),
        "suddenoffset" => push(
            SongLuaEaseMaskTarget::AppearanceSuddenOffset,
            pct_from,
            pct_to,
        ),
        "stealth" => push(SongLuaEaseMaskTarget::AppearanceStealth, pct_from, pct_to),
        "stealthtype" => push(
            SongLuaEaseMaskTarget::AppearanceStealthType,
            pct_from,
            pct_to,
        ),
        "stealthpastreceptors" => push(
            SongLuaEaseMaskTarget::AppearanceStealthPastReceptors,
            pct_from,
            pct_to,
        ),
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
        "tilt" => push(SongLuaEaseMaskTarget::PerspectiveTilt, pct_from, pct_to),
        "skew" => push(SongLuaEaseMaskTarget::PerspectiveSkew, pct_from, pct_to),
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

pub fn old_append_song_lua_runtime_ease_window(
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
            if old_append_song_lua_ease_targets(
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
            old_push_song_lua_ease_target(
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

pub fn old_append_song_lua_runtime_ease_window_like<Target>(
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
    old_append_song_lua_runtime_ease_window(
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

pub fn old_append_song_lua_ease_window_for<Window>(
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
    let first = out.len();
    let result = old_append_song_lua_runtime_ease_window_like(
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
    );
    for compiled in &mut out[first..] {
        compiled.approach_speed = window.approach_speed();
    }
    result
}

pub fn old_build_song_lua_ease_windows_for_player<Window>(
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
    let mut out = Vec::with_capacity(windows.len().saturating_mul(2));
    let mut unsupported_targets = 0usize;
    for window in windows {
        if !song_lua_target_matches_player(window.player(), player) {
            continue;
        }
        if old_append_song_lua_ease_window_for(
            &mut out,
            window,
            timing_player,
            global_offset_seconds,
        ) == SongLuaRuntimeEaseAppend::Unsupported
        {
            unsupported_targets += 1;
            unsupported_window(window);
        }
    }
    song_lua_extend_ease_tails(&mut out, constant_windows);
    (out, unsupported_targets)
}

pub fn old_build_song_lua_ease_windows_for_player_iter<Window, Windows>(
    windows: Windows,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[AttackMaskWindow],
    mut unsupported_window: impl FnMut(&Window),
) -> (Vec<SongLuaEaseMaskWindow>, usize)
where
    Window: SongLuaEaseWindowLike,
    Windows: IntoIterator<Item = Window>,
{
    let windows = windows.into_iter();
    let (lower, upper) = windows.size_hint();
    let capacity = upper.unwrap_or(lower).saturating_mul(2);
    let mut out = Vec::with_capacity(capacity);
    let mut unsupported_targets = 0usize;
    for window in windows {
        if !song_lua_target_matches_player(window.player(), player) {
            continue;
        }
        if old_append_song_lua_ease_window_for(
            &mut out,
            &window,
            timing_player,
            global_offset_seconds,
        ) == SongLuaRuntimeEaseAppend::Unsupported
        {
            unsupported_targets += 1;
            unsupported_window(&window);
        }
    }
    song_lua_extend_ease_tails(&mut out, constant_windows);
    (out, unsupported_targets)
}
