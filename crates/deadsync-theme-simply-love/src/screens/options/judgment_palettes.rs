use super::*;
use deadlib_present::color::{Color, JudgmentColorRole};
use deadlib_present::space::{screen_center_x, screen_center_y};

const Z: i16 = 1490;
const PANEL_W: f32 = 620.0;
const PANEL_H: f32 = 452.0;
const HEADER_H: f32 = 48.0;
const ROW_H: f32 = 35.0;
const BROWSER_VISIBLE_ROWS: usize = 8;
const EDITOR_DONE_ROW: usize = 8;
const CURSOR_PERIOD: f32 = 0.8;

#[derive(Clone, Debug)]
pub(super) enum JudgmentPaletteOverlayState {
    Hidden,
    Browser {
        selected: usize,
        blink_t: f32,
        message: Option<String>,
    },
    Editor {
        palette_id: String,
        selected: usize,
        channel_focus: Option<usize>,
        editing_name: bool,
        name_buffer: String,
        blink_t: f32,
        message: Option<String>,
    },
    ConfirmDelete {
        palette_id: String,
        palette_name: String,
        choice: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct JudgmentPalettePresentationKey {
    active_color_index: i32,
    machine_font: crate::config::MachineFont,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

pub(super) struct JudgmentPalettePresentation {
    key: JudgmentPalettePresentationKey,
    overlay: JudgmentPaletteOverlayState,
    catalog: deadsync_config::judgment_palettes::JudgmentPaletteCatalog,
    children: Arc<[Actor]>,
}

fn judgment_palette_overlay_render_eq(
    left: &JudgmentPaletteOverlayState,
    right: &JudgmentPaletteOverlayState,
) -> bool {
    match (left, right) {
        (JudgmentPaletteOverlayState::Hidden, JudgmentPaletteOverlayState::Hidden) => true,
        (
            JudgmentPaletteOverlayState::Browser {
                selected: left_selected,
                message: left_message,
                ..
            },
            JudgmentPaletteOverlayState::Browser {
                selected: right_selected,
                message: right_message,
                ..
            },
        ) => left_selected == right_selected && left_message == right_message,
        (
            JudgmentPaletteOverlayState::Editor {
                palette_id: left_palette_id,
                selected: left_selected,
                channel_focus: left_channel_focus,
                editing_name: left_editing_name,
                name_buffer: left_name_buffer,
                blink_t: left_blink_t,
                message: left_message,
            },
            JudgmentPaletteOverlayState::Editor {
                palette_id: right_palette_id,
                selected: right_selected,
                channel_focus: right_channel_focus,
                editing_name: right_editing_name,
                name_buffer: right_name_buffer,
                blink_t: right_blink_t,
                message: right_message,
            },
        ) => {
            left_palette_id == right_palette_id
                && left_selected == right_selected
                && left_channel_focus == right_channel_focus
                && left_editing_name == right_editing_name
                && left_name_buffer == right_name_buffer
                && left_message == right_message
                && (!left_editing_name
                    || (*left_blink_t < CURSOR_PERIOD * 0.5)
                        == (*right_blink_t < CURSOR_PERIOD * 0.5))
        }
        (
            JudgmentPaletteOverlayState::ConfirmDelete {
                palette_id: left_palette_id,
                palette_name: left_palette_name,
                choice: left_choice,
            },
            JudgmentPaletteOverlayState::ConfirmDelete {
                palette_id: right_palette_id,
                palette_name: right_palette_name,
                choice: right_choice,
            },
        ) => {
            left_palette_id == right_palette_id
                && left_palette_name == right_palette_name
                && left_choice == right_choice
        }
        _ => false,
    }
}

#[inline(always)]
pub(super) const fn judgment_palette_overlay_visible(
    overlay: &JudgmentPaletteOverlayState,
) -> bool {
    !matches!(overlay, JudgmentPaletteOverlayState::Hidden)
}

pub(super) fn show_judgment_palette_overlay(state: &mut State) {
    clear_navigation_holds(state);
    let selected = state
        .judgment_palettes
        .palettes
        .iter()
        .position(|entry| entry.id == state.judgment_palettes.resolved_default_id())
        .map_or(0, |index| index + 1);
    state.judgment_palette_overlay = JudgmentPaletteOverlayState::Browser {
        selected,
        blink_t: 0.0,
        message: None,
    };
}

pub(super) fn update_judgment_palette_overlay(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    match &mut state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::Hidden => return None,
        JudgmentPaletteOverlayState::Browser { blink_t, .. }
        | JudgmentPaletteOverlayState::Editor { blink_t, .. } => {
            *blink_t = (*blink_t + dt.max(0.0)) % CURSOR_PERIOD;
        }
        JudgmentPaletteOverlayState::ConfirmDelete { .. } => {}
    }

    let effect = if palette_channel_adjustment_active(state) {
        let Some(delta) = state.nav_lr_held_direction else {
            return Some(ThemeEffect::None);
        };
        if screen_input::advance_hold_repeat(
            &mut state.nav_lr_held_for,
            &mut state.nav_lr_next_repeat_at,
            NAV_REPEAT_SCROLL_INTERVAL,
            dt,
        ) {
            palette_overlay_horizontal(state, delta)
        } else {
            ThemeEffect::None
        }
    } else {
        if let Some(delta) = state.nav_lr_held_direction {
            on_lr_release(state, delta);
        }
        ThemeEffect::None
    };
    Some(effect)
}

const fn palette_channel_adjustment_active(state: &State) -> bool {
    matches!(
        state.judgment_palette_overlay,
        JudgmentPaletteOverlayState::Editor {
            selected: 1..=7,
            channel_focus: Some(_),
            editing_name: false,
            ..
        }
    )
}

fn palette_horizontal_input(state: &mut State, delta: isize) -> ThemeEffect {
    let adjusting_channel = palette_channel_adjustment_active(state);
    let effect = palette_overlay_horizontal(state, delta);
    if adjusting_channel {
        on_lr_press(state, delta);
    }
    effect
}

pub(super) fn judgment_palettes_effect(state: &State) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::JudgmentPalettes(
        state.judgment_palettes.clone(),
    ))
}

pub(super) fn handle_judgment_palette_input(
    state: &mut State,
    event: &InputEvent,
) -> Option<ThemeEffect> {
    if !judgment_palette_overlay_visible(&state.judgment_palette_overlay) {
        return None;
    }
    if !event.pressed {
        match event.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => on_lr_release(state, -1),
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => on_lr_release(state, 1),
            _ => {}
        }
        return Some(ThemeEffect::None);
    }

    let effect = match event.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => palette_overlay_move(state, -1),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => palette_overlay_move(state, 1),
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => palette_horizontal_input(state, -1),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => palette_horizontal_input(state, 1),
        VirtualAction::p1_start | VirtualAction::p2_start => palette_overlay_activate(state),
        VirtualAction::p1_select | VirtualAction::p2_select => palette_overlay_select(state),
        VirtualAction::p1_back | VirtualAction::p2_back => palette_overlay_back(state),
        _ => ThemeEffect::None,
    };
    Some(effect)
}

fn palette_overlay_move(state: &mut State, delta: isize) -> ThemeEffect {
    match &mut state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::Browser { selected, .. } => {
            let count = state.judgment_palettes.palettes.len() + 1;
            *selected = ((*selected as isize + delta).rem_euclid(count as isize)) as usize;
        }
        JudgmentPaletteOverlayState::Editor {
            selected,
            channel_focus,
            editing_name,
            ..
        } => {
            if *editing_name {
                return ThemeEffect::None;
            }
            if let Some(channel) = channel_focus {
                *channel = ((*channel as isize + delta).rem_euclid(3)) as usize;
            } else {
                *selected = ((*selected as isize + delta)
                    .rem_euclid((EDITOR_DONE_ROW + 1) as isize))
                    as usize;
            }
        }
        JudgmentPaletteOverlayState::ConfirmDelete { choice, .. } => {
            *choice ^= 1;
        }
        JudgmentPaletteOverlayState::Hidden => return ThemeEffect::None,
    }
    queue_sfx(state, "assets/sounds/change.ogg");
    ThemeEffect::None
}

fn palette_overlay_horizontal(state: &mut State, delta: isize) -> ThemeEffect {
    match &state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::ConfirmDelete { .. } => {
            if let JudgmentPaletteOverlayState::ConfirmDelete { choice, .. } =
                &mut state.judgment_palette_overlay
            {
                *choice ^= 1;
            }
            queue_sfx(state, "assets/sounds/change.ogg");
            ThemeEffect::None
        }
        JudgmentPaletteOverlayState::Editor {
            palette_id,
            selected,
            channel_focus: Some(channel),
            editing_name: false,
            ..
        } if (1..=7).contains(selected) => {
            let palette_id = palette_id.clone();
            let role = JudgmentColorRole::ALL[*selected - 1];
            let channel = *channel;
            let Some(definition) = state.judgment_palettes.palette(&palette_id) else {
                return ThemeEffect::None;
            };
            let mut color = Color::from_rgba(definition.palette.color(role));
            let value = match channel {
                0 => &mut color.r,
                1 => &mut color.g,
                _ => &mut color.b,
            };
            let byte = (*value * 255.0).round() as i16;
            let next_byte = (byte + delta as i16).clamp(0, 255);
            if next_byte == byte {
                return ThemeEffect::None;
            }
            *value = f32::from(next_byte) / 255.0;
            match state.judgment_palettes.set_color(&palette_id, role, color) {
                Ok(()) => {
                    set_overlay_message(state, None);
                    queue_sfx(state, "assets/sounds/change_value.ogg");
                    judgment_palettes_effect(state)
                }
                Err(error) => {
                    set_overlay_message(state, Some(error));
                    ThemeEffect::None
                }
            }
        }
        _ => ThemeEffect::None,
    }
}

fn palette_overlay_activate(state: &mut State) -> ThemeEffect {
    match &state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::Browser { selected: 0, .. } => create_palette(state, None),
        JudgmentPaletteOverlayState::Browser { selected, .. } => {
            let Some(entry) = state.judgment_palettes.palettes.get(selected - 1) else {
                return ThemeEffect::None;
            };
            if entry.built_in {
                create_palette(state, Some(entry.id.clone()))
            } else {
                open_editor(state, entry.id.clone());
                queue_sfx(state, "assets/sounds/start.ogg");
                ThemeEffect::None
            }
        }
        JudgmentPaletteOverlayState::Editor {
            selected: 0,
            editing_name: false,
            ..
        } => {
            if let JudgmentPaletteOverlayState::Editor {
                editing_name,
                name_buffer,
                palette_id,
                message,
                ..
            } = &mut state.judgment_palette_overlay
            {
                *editing_name = true;
                *name_buffer = state
                    .judgment_palettes
                    .palette(palette_id)
                    .map_or_else(String::new, |entry| entry.name.clone());
                *message = None;
            }
            queue_sfx(state, "assets/sounds/start.ogg");
            ThemeEffect::None
        }
        JudgmentPaletteOverlayState::Editor {
            selected,
            channel_focus: None,
            editing_name: false,
            ..
        } if (1..=7).contains(selected) => {
            if let JudgmentPaletteOverlayState::Editor { channel_focus, .. } =
                &mut state.judgment_palette_overlay
            {
                *channel_focus = Some(0);
            }
            queue_sfx(state, "assets/sounds/start.ogg");
            ThemeEffect::None
        }
        JudgmentPaletteOverlayState::Editor {
            channel_focus: Some(_),
            ..
        } => {
            if let JudgmentPaletteOverlayState::Editor { channel_focus, .. } =
                &mut state.judgment_palette_overlay
            {
                *channel_focus = None;
            }
            queue_sfx(state, "assets/sounds/start.ogg");
            ThemeEffect::None
        }
        JudgmentPaletteOverlayState::Editor {
            selected: EDITOR_DONE_ROW,
            editing_name: false,
            ..
        } => {
            return_to_browser(state, None);
            queue_sfx(state, "assets/sounds/start.ogg");
            ThemeEffect::None
        }
        JudgmentPaletteOverlayState::ConfirmDelete {
            palette_id, choice, ..
        } => {
            if *choice == 0 {
                return_to_browser(state, None);
                queue_sfx(state, "assets/sounds/change.ogg");
                return ThemeEffect::None;
            }
            let palette_id = palette_id.clone();
            match state.judgment_palettes.delete_palette(&palette_id) {
                Ok(()) => {
                    sync_default_choice(state);
                    return_to_browser(state, Some(tr("JudgmentPalettes", "Deleted").to_string()));
                    queue_sfx(state, "assets/sounds/start.ogg");
                    judgment_palettes_effect(state)
                }
                Err(error) => {
                    return_to_browser(state, Some(error));
                    ThemeEffect::None
                }
            }
        }
        _ => ThemeEffect::None,
    }
}

fn palette_overlay_select(state: &mut State) -> ThemeEffect {
    let JudgmentPaletteOverlayState::Browser { selected, .. } = &state.judgment_palette_overlay
    else {
        return ThemeEffect::None;
    };
    if *selected == 0 {
        return ThemeEffect::None;
    }
    let Some(entry) = state.judgment_palettes.palettes.get(*selected - 1) else {
        return ThemeEffect::None;
    };
    if entry.built_in {
        set_overlay_message(
            state,
            Some(tr("JudgmentPalettes", "BuiltInReadOnly").to_string()),
        );
        queue_sfx(state, "assets/sounds/change.ogg");
        return ThemeEffect::None;
    }
    state.judgment_palette_overlay = JudgmentPaletteOverlayState::ConfirmDelete {
        palette_id: entry.id.clone(),
        palette_name: entry.name.clone(),
        choice: 0,
    };
    queue_sfx(state, "assets/sounds/start.ogg");
    ThemeEffect::None
}

fn palette_overlay_back(state: &mut State) -> ThemeEffect {
    match &mut state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::Hidden => {}
        JudgmentPaletteOverlayState::Browser { .. } => {
            state.judgment_palette_overlay = JudgmentPaletteOverlayState::Hidden;
        }
        JudgmentPaletteOverlayState::Editor {
            channel_focus,
            editing_name,
            ..
        } if channel_focus.is_some() || *editing_name => {
            *channel_focus = None;
            *editing_name = false;
        }
        JudgmentPaletteOverlayState::Editor { .. }
        | JudgmentPaletteOverlayState::ConfirmDelete { .. } => return_to_browser(state, None),
    }
    clear_navigation_holds(state);
    queue_sfx(state, "assets/sounds/change.ogg");
    ThemeEffect::None
}

fn create_palette(state: &mut State, source_id: Option<String>) -> ThemeEffect {
    let source_id =
        source_id.unwrap_or_else(|| state.judgment_palettes.resolved_default_id().to_owned());
    let mut suffix = 1;
    let name = loop {
        let candidate = format!("Custom Palette {suffix}");
        if !state
            .judgment_palettes
            .palettes
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(&candidate))
        {
            break candidate;
        }
        suffix += 1;
    };
    match state.judgment_palettes.create_palette(&name, &source_id) {
        Ok(id) => {
            open_editor(state, id);
            queue_sfx(state, "assets/sounds/start.ogg");
            judgment_palettes_effect(state)
        }
        Err(error) => {
            set_overlay_message(state, Some(error));
            ThemeEffect::None
        }
    }
}

fn open_editor(state: &mut State, palette_id: String) {
    state.judgment_palette_overlay = JudgmentPaletteOverlayState::Editor {
        palette_id,
        selected: 0,
        channel_focus: None,
        editing_name: false,
        name_buffer: String::new(),
        blink_t: 0.0,
        message: None,
    };
}

fn return_to_browser(state: &mut State, message: Option<String>) {
    state.judgment_palette_overlay = JudgmentPaletteOverlayState::Browser {
        selected: 0,
        blink_t: 0.0,
        message,
    };
}

fn set_overlay_message(state: &mut State, next: Option<String>) {
    match &mut state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::Browser { message, .. }
        | JudgmentPaletteOverlayState::Editor { message, .. } => *message = next,
        _ => {}
    }
}

fn sync_default_choice(state: &mut State) {
    let index = state
        .judgment_palettes
        .palettes
        .iter()
        .position(|entry| entry.id == state.judgment_palettes.resolved_default_id())
        .unwrap_or(0);
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::DefaultJudgmentPalette,
        index,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].cursor_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::DefaultJudgmentPalette,
        index,
    );
}

pub fn handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    if !judgment_palette_overlay_visible(&state.judgment_palette_overlay) {
        return false;
    }
    let editing_name = matches!(
        state.judgment_palette_overlay,
        JudgmentPaletteOverlayState::Editor {
            editing_name: true,
            ..
        }
    );
    if editing_name {
        let mut effect = ThemeEffect::None;
        if let Some(text) = text {
            if let JudgmentPaletteOverlayState::Editor {
                name_buffer,
                blink_t,
                ..
            } = &mut state.judgment_palette_overlay
            {
                for ch in text
                    .chars()
                    .filter(|ch| !matches!(ch, '\n' | '\r' | '[' | ']' | '='))
                {
                    if name_buffer.chars().count() >= 32 {
                        break;
                    }
                    if !ch.is_control() {
                        name_buffer.push(ch);
                    }
                }
                *blink_t = 0.0;
            }
        } else if let Some(key) = key.filter(|key| key.pressed) {
            match key.code {
                KeyCode::Backspace => {
                    if let JudgmentPaletteOverlayState::Editor { name_buffer, .. } =
                        &mut state.judgment_palette_overlay
                    {
                        name_buffer.pop();
                    }
                }
                KeyCode::Escape => {
                    if let JudgmentPaletteOverlayState::Editor { editing_name, .. } =
                        &mut state.judgment_palette_overlay
                    {
                        *editing_name = false;
                    }
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
                KeyCode::Enter | KeyCode::NumpadEnter if !key.repeat => {
                    effect = commit_name(state);
                }
                _ => {}
            }
        }
        append_pending_effects(state, effect, effects);
        return true;
    }

    if let Some(key) = key.filter(|key| key.pressed && !key.repeat) {
        let effect = match key.code {
            KeyCode::Escape => palette_overlay_back(state),
            KeyCode::Enter | KeyCode::NumpadEnter
                if matches!(
                    state.judgment_palette_overlay,
                    JudgmentPaletteOverlayState::ConfirmDelete { .. }
                ) =>
            {
                palette_overlay_activate(state)
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight
                if matches!(
                    state.judgment_palette_overlay,
                    JudgmentPaletteOverlayState::ConfirmDelete { .. }
                ) =>
            {
                palette_overlay_horizontal(state, 1)
            }
            _ => return text.is_some(),
        };
        append_pending_effects(state, effect, effects);
        return true;
    }
    text.is_some()
}

fn commit_name(state: &mut State) -> ThemeEffect {
    let JudgmentPaletteOverlayState::Editor {
        palette_id,
        name_buffer,
        ..
    } = &state.judgment_palette_overlay
    else {
        return ThemeEffect::None;
    };
    let (palette_id, name) = (palette_id.clone(), name_buffer.clone());
    match state.judgment_palettes.rename_palette(&palette_id, &name) {
        Ok(()) => {
            if let JudgmentPaletteOverlayState::Editor {
                editing_name,
                message,
                ..
            } = &mut state.judgment_palette_overlay
            {
                *editing_name = false;
                *message = None;
            }
            queue_sfx(state, "assets/sounds/start.ogg");
            judgment_palettes_effect(state)
        }
        Err(error) => {
            set_overlay_message(state, Some(error));
            queue_sfx(state, "assets/sounds/change.ogg");
            ThemeEffect::None
        }
    }
}

pub(super) fn push_judgment_palette_overlay(
    out: &mut Vec<Actor>,
    state: &State,
    active_color_index: i32,
    machine_font: crate::config::MachineFont,
) -> bool {
    if !judgment_palette_overlay_visible(&state.judgment_palette_overlay) {
        return false;
    }
    let key = JudgmentPalettePresentationKey {
        active_color_index,
        machine_font,
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = state
        .judgment_palette_presentation
        .borrow()
        .as_ref()
        .filter(|presentation| {
            presentation.key == key
                && judgment_palette_overlay_render_eq(
                    &presentation.overlay,
                    &state.judgment_palette_overlay,
                )
                && presentation.catalog == state.judgment_palettes
        })
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(96);
        push_judgment_palette_overlay_unreserved(
            &mut children,
            state,
            active_color_index,
            machine_font,
        );
        let children = Arc::<[Actor]>::from(children);
        *state.judgment_palette_presentation.borrow_mut() = Some(JudgmentPalettePresentation {
            key,
            overlay: state.judgment_palette_overlay.clone(),
            catalog: state.judgment_palettes.clone(),
            children: Arc::clone(&children),
        });
        children
    });
    crate::screens::components::select_music::push_retained_overlay(out, children);
    true
}

pub(super) fn push_judgment_palette_overlay_unreserved(
    out: &mut Vec<Actor>,
    state: &State,
    active_color_index: i32,
    machine_font: crate::config::MachineFont,
) {
    let accent = color::simply_love_rgba(active_color_index);
    let cx = screen_center_x();
    let cy = screen_center_y();
    let header_font = machine_font_key(machine_font, FontRole::Header);
    let bold_font = machine_font_key(machine_font, FontRole::Bold);
    push_panel(out, accent, cx, cy);
    out.push(act!(text:
        font(header_font): settext(tr("JudgmentPalettes", "Title")):
        align(0.0, 0.5): xy(PANEL_W.mul_add(-0.5, cx) + 18.0, cy - 188.0):
        zoom(0.40): diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));

    match &state.judgment_palette_overlay {
        JudgmentPaletteOverlayState::Browser {
            selected, message, ..
        } => push_browser(
            out,
            state,
            *selected,
            message.as_deref(),
            accent,
            cx,
            cy,
            bold_font,
        ),
        JudgmentPaletteOverlayState::Editor {
            palette_id,
            selected,
            channel_focus,
            editing_name,
            name_buffer,
            blink_t,
            message,
        } => push_editor(
            out,
            state,
            palette_id,
            *selected,
            *channel_focus,
            *editing_name,
            name_buffer,
            *blink_t,
            message.as_deref(),
            accent,
            cx,
            cy,
            bold_font,
        ),
        JudgmentPaletteOverlayState::ConfirmDelete {
            palette_name,
            choice,
            ..
        } => {
            push_browser(out, state, 0, None, accent, cx, cy, bold_font);
            push_delete_confirm(
                out,
                palette_name,
                *choice,
                accent,
                cx,
                cy,
                header_font,
                bold_font,
            );
        }
        JudgmentPaletteOverlayState::Hidden => {}
    }
}

fn push_panel(out: &mut Vec<Actor>, accent: [f32; 4], cx: f32, cy: f32) {
    out.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.90): z(Z)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W + 4.0, PANEL_H + 4.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 1)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W, PANEL_H):
        diffuse(0.025, 0.025, 0.035, 0.99): z(Z + 2)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, HEADER_H.mul_add(0.5, PANEL_H.mul_add(-0.5, cy))):
        zoomto(PANEL_W, HEADER_H): diffuse(0.0, 0.0, 0.0, 0.92): z(Z + 4)
    ));
}

fn push_browser(
    out: &mut Vec<Actor>,
    state: &State,
    selected: usize,
    message: Option<&str>,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let total = state.judgment_palettes.palettes.len() + 1;
    let start = selected
        .saturating_sub(BROWSER_VISIBLE_ROWS / 2)
        .min(total.saturating_sub(BROWSER_VISIBLE_ROWS));
    for index in start..(start + BROWSER_VISIBLE_ROWS).min(total) {
        let y = ((index - start) as f32).mul_add(ROW_H, cy - 132.0);
        let active = index == selected;
        if active {
            out.push(act!(quad:
                align(0.5, 0.5): xy(cx, y): zoomto(PANEL_W - 30.0, ROW_H - 3.0):
                diffuse(accent[0], accent[1], accent[2], 0.72): z(Z + 4)
            ));
        }
        if index == 0 {
            out.push(act!(text:
                font(bold_font): settext(tr("JudgmentPalettes", "Create")):
                align(0.0, 0.5): xy(PANEL_W.mul_add(-0.5, cx) + 28.0, y): zoom(0.30):
                diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
            ));
            continue;
        }
        let entry = &state.judgment_palettes.palettes[index - 1];
        let marker = if entry.id == state.judgment_palettes.resolved_default_id() {
            format!("{}  [{}]", entry.name, tr("JudgmentPalettes", "Default"))
        } else if entry.built_in {
            format!("{}  [{}]", entry.name, tr("JudgmentPalettes", "BuiltIn"))
        } else {
            entry.name.clone()
        };
        out.push(act!(text:
            font(bold_font): settext(marker): align(0.0, 0.5):
            xy(PANEL_W.mul_add(-0.5, cx) + 28.0, y): zoom(0.28): maxwidth(305.0):
            diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
        ));
        for role in JudgmentColorRole::ALL {
            let rgba = entry.palette.color(role);
            let x = (role.index() as f32).mul_add(28.0, cx + 96.0);
            out.push(act!(quad:
                align(0.5, 0.5): xy(x, y): zoomto(22.0, 22.0):
                diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z + 6)
            ));
        }
    }
    push_footer(
        out,
        message
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| tr("JudgmentPalettes", "BrowserHelp").to_string()),
        accent,
        cx,
        cy,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_editor(
    out: &mut Vec<Actor>,
    state: &State,
    palette_id: &str,
    selected: usize,
    channel_focus: Option<usize>,
    editing_name: bool,
    name_buffer: &str,
    blink_t: f32,
    message: Option<&str>,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let Some(entry) = state.judgment_palettes.palette(palette_id) else {
        return;
    };
    for row in 0..=EDITOR_DONE_ROW {
        let y = (row as f32).mul_add(34.0, cy - 140.0);
        if row == selected {
            out.push(act!(quad:
                align(0.5, 0.5): xy(cx, y): zoomto(PANEL_W - 30.0, 31.0):
                diffuse(accent[0], accent[1], accent[2], 0.72): z(Z + 4)
            ));
        }
        if row == 0 {
            let cursor = if editing_name && blink_t < CURSOR_PERIOD * 0.5 {
                "_"
            } else {
                ""
            };
            let value = if editing_name {
                format!("{name_buffer}{cursor}")
            } else {
                entry.name.clone()
            };
            push_editor_text(
                out,
                tr("JudgmentPalettes", "Name").to_string(),
                value,
                y,
                cx,
                bold_font,
            );
        } else if row <= 7 {
            let role = JudgmentColorRole::ALL[row - 1];
            let rgba = entry.palette.color(role);
            let color = Color::from_rgba(rgba);
            push_editor_text(
                out,
                tr("JudgmentPalettes", role.config_key()).to_string(),
                color.to_hex(),
                y,
                cx,
                bold_font,
            );
            out.push(act!(quad:
                align(0.5, 0.5): xy(cx + 95.0, y): zoomto(54.0, 23.0):
                diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z + 6)
            ));
            let channels = [color.r, color.g, color.b].map(|v| (v * 255.0).round() as u8);
            let channel_text = ["R", "G", "B"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    if row == selected && channel_focus == Some(index) {
                        format!("[{name} {:03}]", channels[index])
                    } else {
                        format!("{name} {:03}", channels[index])
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            out.push(act!(text:
                font("miso"): settext(channel_text): align(1.0, 0.5):
                xy(PANEL_W.mul_add(0.5, cx) - 26.0, y): zoom(0.62):
                diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(right)
            ));
        } else {
            push_editor_text(
                out,
                tr("JudgmentPalettes", "Done").to_string(),
                String::new(),
                y,
                cx,
                bold_font,
            );
        }
    }
    let help = message.map(ToOwned::to_owned).unwrap_or_else(|| {
        if editing_name {
            tr("JudgmentPalettes", "NameHelp").to_string()
        } else if channel_focus.is_some() {
            tr("JudgmentPalettes", "ColorHelp").to_string()
        } else {
            tr("JudgmentPalettes", "EditorHelp").to_string()
        }
    });
    push_footer(out, help, accent, cx, cy);
}

fn push_editor_text(
    out: &mut Vec<Actor>,
    label: String,
    value: String,
    y: f32,
    cx: f32,
    bold_font: &'static str,
) {
    out.push(act!(text:
        font(bold_font): settext(label): align(0.0, 0.5):
        xy(PANEL_W.mul_add(-0.5, cx) + 28.0, y): zoom(0.28):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    if !value.is_empty() {
        out.push(act!(text:
            font("miso"): settext(value): align(0.0, 0.5):
            xy(cx - 72.0, y): zoom(0.70): maxwidth(245.0):
            diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
        ));
    }
}

fn push_footer(out: &mut Vec<Actor>, text: String, accent: [f32; 4], cx: f32, cy: f32) {
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, PANEL_H.mul_add(0.5, cy) - 25.0):
        zoomto(PANEL_W - 20.0, 38.0): diffuse(0.0, 0.0, 0.0, 0.72): z(Z + 4)
    ));
    out.push(act!(text:
        font("miso"): settext(text): align(0.5, 0.5):
        xy(cx, PANEL_H.mul_add(0.5, cy) - 25.0): zoom(0.62): maxwidth(PANEL_W - 40.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 6): horizalign(center)
    ));
}

fn push_delete_confirm(
    out: &mut Vec<Actor>,
    name: &str,
    choice: usize,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
    header_font: &'static str,
    bold_font: &'static str,
) {
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(440.0, 180.0):
        diffuse(0.01, 0.01, 0.02, 0.98): z(Z + 20)
    ));
    out.push(act!(text:
        font(header_font): settext(tr("JudgmentPalettes", "DeleteTitle")):
        align(0.5, 0.5): xy(cx, cy - 52.0): zoom(0.32):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 22): horizalign(center)
    ));
    out.push(act!(text:
        font(bold_font): settext(name.to_owned()): align(0.5, 0.5): xy(cx, cy - 13.0):
        zoom(0.30): maxwidth(380.0): diffuse(1.0, 1.0, 1.0, 1.0):
        z(Z + 22): horizalign(center)
    ));
    for (index, key) in ["Cancel", "Delete"].into_iter().enumerate() {
        let x = (index as f32 - 0.5).mul_add(150.0, cx);
        out.push(act!(quad:
            align(0.5, 0.5): xy(x, cy + 42.0): zoomto(130.0, 38.0):
            diffuse(
                if choice == index { accent[0] } else { 0.12 },
                if choice == index { accent[1] } else { 0.12 },
                if choice == index { accent[2] } else { 0.14 },
                1.0
            ): z(Z + 21)
        ));
        out.push(act!(text:
            font(bold_font): settext(tr("JudgmentPalettes", key)):
            align(0.5, 0.5): xy(x, cy + 42.0): zoom(0.28):
            diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 22): horizalign(center)
        ));
    }
}
