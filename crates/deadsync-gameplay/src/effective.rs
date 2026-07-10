#[inline(always)]
pub fn effective_visual_effects_for_player<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    player_idx: usize,
) -> VisualEffects
where
    Profile: GameplayProfileData,
{
    if player_idx >= state.setup.num_players || player_idx >= MAX_PLAYERS {
        return VisualEffects::default();
    }
    state.effective_visual_effects_for_player_with_mask(
        player_idx,
        state.profiles_runtime.profiles[player_idx].visual_mask_bits(),
    )
}

#[inline(always)]
pub fn effective_scroll_effects_for_player<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    player_idx: usize,
) -> ScrollEffects
where
    Profile: GameplayProfileData,
{
    if player_idx >= state.setup.num_players || player_idx >= MAX_PLAYERS {
        return ScrollEffects::default();
    }
    state.effective_scroll_effects_for_player_with_base(
        player_idx,
        state.profiles_runtime.profiles[player_idx].scroll_effects(),
    )
}

#[inline(always)]
pub fn effective_perspective_effects_for_player<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    player_idx: usize,
) -> PerspectiveEffects
where
    Profile: GameplayProfileData,
{
    if player_idx >= state.setup.num_players || player_idx >= MAX_PLAYERS {
        return PerspectiveEffects::default();
    }
    state.effective_perspective_effects_for_player_with_base(
        player_idx,
        state.profiles_runtime.profiles[player_idx].perspective_effects(),
    )
}

#[inline(always)]
pub fn effective_visual_mask_for_player<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    player_idx: usize,
) -> u16
where
    Profile: GameplayProfileData,
{
    if player_idx >= state.setup.num_players || player_idx >= MAX_PLAYERS {
        return 0;
    }
    state
        .effective_visual_effects_for_player_with_mask(
            player_idx,
            state.profiles_runtime.profiles[player_idx].visual_mask_bits(),
        )
        .to_mask_bits()
}

#[inline(always)]
pub fn effective_mini_percent_for_player<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    player_idx: usize,
) -> f32
where
    Profile: GameplayProfileData,
{
    if player_idx >= state.setup.num_players || player_idx >= MAX_PLAYERS {
        return 0.0;
    }
    state.effective_mini_percent_for_player_with_base(
        player_idx,
        state.profiles_runtime.profiles[player_idx].mini_percent(),
    )
}

#[inline(always)]
pub fn effective_mini_value_with_visual_mask<Profile: GameplayProfileData>(
    profile: &Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    profile.effective_mini_value_with_visual_mask(visual_mask, mini_percent)
}

#[inline(always)]
pub fn player_draw_scale_for_tilt_with_visual_mask<Profile: GameplayProfileData>(
    tilt: f32,
    profile: &Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    profile.draw_scale_for_tilt_with_visual_mask(tilt, visual_mask, mini_percent)
}

pub fn refresh_active_attack_masks<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    delta_time: f32,
) where
    Profile: GameplayProfileData,
{
    for player in 0..state.setup.num_players {
        let now = state.visible_music_time_seconds(player);
        let profile = &state.profiles_runtime.profiles[player];
        state.refresh_player_attacks(
            player,
            now,
            delta_time,
            AttackBaseEffects {
                appearance: base_appearance_effects(profile),
                visual: base_visual_effects(profile),
                scroll: profile.scroll_effects(),
                mini_percent: profile.mini_percent(),
            },
        );
    }
}

#[inline(always)]
pub fn song_lua_hides_note_visual<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    player: usize,
    column: usize,
    beat: f32,
) -> bool
where
    Profile: GameplayProfileData,
{
    song_lua_field_note_hidden(
        &state.song_lua_visuals().note_hides[player],
        state.setup.cols_per_player,
        column,
        beat,
    )
}

#[inline(always)]
pub const fn hold_to_exit_seconds(key: HoldToExitKey) -> f32 {
    match key {
        HoldToExitKey::Start => GIVE_UP_HOLD_SECONDS,
        HoldToExitKey::Back => BACK_OUT_HOLD_SECONDS,
    }
}

#[inline(always)]
pub const fn exit_total_seconds(kind: ExitTransitionKind) -> f32 {
    match kind {
        ExitTransitionKind::Out => GIVE_UP_OUT_FADE_DELAY_SECONDS + GIVE_UP_OUT_FADE_SECONDS,
        ExitTransitionKind::Cancel => BACK_OUT_FADE_DELAY_SECONDS + BACK_OUT_FADE_SECONDS,
    }
}

#[inline(always)]
pub fn exit_transition_alpha_elapsed(kind: ExitTransitionKind, elapsed_s: f32) -> f32 {
    let (delay, fade) = match kind {
        ExitTransitionKind::Out => (GIVE_UP_OUT_FADE_DELAY_SECONDS, GIVE_UP_OUT_FADE_SECONDS),
        ExitTransitionKind::Cancel => (BACK_OUT_FADE_DELAY_SECONDS, BACK_OUT_FADE_SECONDS),
    };
    if fade <= 0.0 {
        return 1.0;
    }
    let alpha = if elapsed_s <= delay {
        0.0
    } else {
        (elapsed_s - delay) / fade
    };
    alpha.clamp(0.0, 1.0)
}

#[inline(always)]
pub fn exit_transition_alpha(exit: &ExitTransition) -> f32 {
    exit_transition_alpha_elapsed(exit.kind, exit.started_at.elapsed().as_secs_f32())
}

#[inline(always)]
pub const fn gameplay_exit_for_kind(kind: ExitTransitionKind) -> GameplayExit {
    match kind {
        ExitTransitionKind::Out => GameplayExit::Complete,
        ExitTransitionKind::Cancel => GameplayExit::Cancel,
    }
}

#[inline(always)]
pub const fn gameplay_menu_input_plan(
    input: GameplayMenuInput,
    pressed: bool,
    p1_menu_active: bool,
    p2_menu_active: bool,
    delayed_back: bool,
    hold_to_exit_key: Option<HoldToExitKey>,
) -> GameplayMenuInputPlan {
    match input {
        GameplayMenuInput::P1Start if p1_menu_active => {
            hold_to_exit_input_plan(HoldToExitKey::Start, pressed, true, hold_to_exit_key)
        }
        GameplayMenuInput::P2Start if p2_menu_active => {
            hold_to_exit_input_plan(HoldToExitKey::Start, pressed, true, hold_to_exit_key)
        }
        GameplayMenuInput::P1Back if p1_menu_active => {
            hold_to_exit_input_plan(HoldToExitKey::Back, pressed, delayed_back, hold_to_exit_key)
        }
        GameplayMenuInput::P2Back if p2_menu_active => {
            hold_to_exit_input_plan(HoldToExitKey::Back, pressed, delayed_back, hold_to_exit_key)
        }
        _ => GameplayMenuInputPlan::None,
    }
}

#[inline(always)]
pub const fn gameplay_offset_prompt_choice_delta(
    action: VirtualAction,
    dedicated_menu_only: bool,
) -> Option<i8> {
    if dedicated_menu_only && action.is_gameplay_arrow() {
        return None;
    }
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => Some(-1),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(1),
        _ => None,
    }
}

#[inline(always)]
const fn hold_to_exit_input_plan(
    key: HoldToExitKey,
    pressed: bool,
    delayed_hold: bool,
    hold_to_exit_key: Option<HoldToExitKey>,
) -> GameplayMenuInputPlan {
    if pressed {
        if delayed_hold {
            GameplayMenuInputPlan::ArmHold(key)
        } else {
            GameplayMenuInputPlan::BeginExit(ExitTransitionKind::Cancel)
        }
    } else if matches!(
        (hold_to_exit_key, key),
        (Some(HoldToExitKey::Start), HoldToExitKey::Start)
            | (Some(HoldToExitKey::Back), HoldToExitKey::Back)
    ) {
        GameplayMenuInputPlan::AbortHold(key)
    } else {
        GameplayMenuInputPlan::None
    }
}
