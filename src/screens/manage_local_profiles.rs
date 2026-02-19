use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_height, screen_width};
use crate::game::profile;
use crate::screens::components::heart_bg;
use crate::screens::components::screen_bar::{self, ScreenBarPosition, ScreenBarTitlePlacement};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{self, Actor};
use crate::ui::color;
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

/* --------------------------------- layout -------------------------------- */
/// Bars in `screen_bar.rs` use 32.0 px height.
const BAR_H: f32 = 32.0;
/// Screen-space margins (pixels, not scaled)
const LEFT_MARGIN_PX: f32 = 33.0;
const RIGHT_MARGIN_PX: f32 = 25.0;
const FIRST_ROW_TOP_MARGIN_PX: f32 = 18.0;
const BOTTOM_MARGIN_PX: f32 = 0.0;

const VISIBLE_ROWS: usize = 10;
const ROW_H: f32 = 33.0;
const ROW_GAP: f32 = 2.5;
const LIST_W: f32 = 509.0;
const SEP_W: f32 = 2.5;
const DESC_W: f32 = 292.0;
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

const HEART_LEFT_PAD: f32 = 13.0;
const TEXT_LEFT_PAD: f32 = 40.66;
const ITEM_TEXT_ZOOM: f32 = 0.88;
const HEART_ZOOM: f32 = 0.026;

const DESC_TITLE_TOP_PAD_PX: f32 = 9.75;
const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_TOP_PAD_PX: f32 = 23.25;
const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_INDENT_PX: f32 = 10.0;
const DESC_TITLE_ZOOM: f32 = 1.0;
const DESC_BODY_ZOOM: f32 = 1.0;

const NAME_MAX_LEN: usize = 32;

#[derive(Clone, Debug)]
enum RowKind {
    CreateNew,
    Profile { id: String, display_name: String },
    Exit,
}

#[derive(Clone, Debug)]
struct Row {
    kind: RowKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavDirection {
    Up,
    Down,
}

#[derive(Clone, Debug)]
struct NameEntryState {
    value: String,
    error: Option<&'static str>,
    blink_t: f32,
}

#[derive(Clone, Debug)]
struct DeleteConfirmState {
    id: String,
    display_name: String,
    error: Option<&'static str>,
}

pub struct State {
    pub selected: usize,
    prev_selected: usize,
    pub active_color_index: i32,
    bg: heart_bg::State,
    rows: Vec<Row>,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    name_entry: Option<NameEntryState>,
    delete_confirm: Option<DeleteConfirmState>,
}

pub fn init() -> State {
    let rows = build_rows();
    State {
        selected: 0,
        prev_selected: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        rows,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        name_entry: None,
        delete_confirm: None,
    }
}

fn build_rows() -> Vec<Row> {
    let profiles = profile::scan_local_profiles();
    let mut out = Vec::with_capacity(profiles.len() + 2);
    out.push(Row {
        kind: RowKind::CreateNew,
    });
    for p in profiles {
        out.push(Row {
            kind: RowKind::Profile {
                id: p.id,
                display_name: p.display_name,
            },
        });
    }
    out.push(Row {
        kind: RowKind::Exit,
    });
    out
}

fn refresh_rows(state: &mut State) {
    state.rows = build_rows();
    if state.rows.is_empty() {
        state.selected = 0;
        state.prev_selected = 0;
        return;
    }
    state.selected = state.selected.min(state.rows.len() - 1);
    state.prev_selected = state.prev_selected.min(state.rows.len() - 1);
}

fn move_selected(state: &mut State, dir: NavDirection) {
    let total = state.rows.len();
    if total == 0 {
        state.selected = 0;
        return;
    }
    state.prev_selected = state.selected;
    state.selected = match dir {
        NavDirection::Up => {
            if state.selected == 0 {
                total - 1
            } else {
                state.selected - 1
            }
        }
        NavDirection::Down => (state.selected + 1) % total,
    };
}

fn on_nav_press(state: &mut State, dir: NavDirection) {
    let now = Instant::now();
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(now);
    state.nav_key_last_scrolled_at = Some(now);
}

fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

fn reset_nav_hold(state: &mut State) {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.nav_key_last_scrolled_at = None;
}

fn scroll_offset(selected: usize, total_rows: usize) -> usize {
    let anchor_row: usize = 4;
    let max_offset = total_rows.saturating_sub(VISIBLE_ROWS);
    if total_rows <= VISIBLE_ROWS {
        0
    } else {
        selected.saturating_sub(anchor_row).min(max_offset)
    }
}

fn update_hold_scroll(state: &mut State) {
    if state.name_entry.is_some() || state.delete_confirm.is_some() {
        return;
    }
    let Some(dir) = state.nav_key_held_direction else {
        return;
    };
    let Some(held_since) = state.nav_key_held_since else {
        return;
    };
    let Some(last_at) = state.nav_key_last_scrolled_at else {
        return;
    };

    let now = Instant::now();
    if now.duration_since(held_since) < NAV_INITIAL_HOLD_DELAY {
        return;
    }
    if now.duration_since(last_at) < NAV_REPEAT_SCROLL_INTERVAL {
        return;
    }

    move_selected(state, dir);
    state.nav_key_last_scrolled_at = Some(now);
}

fn update_name_entry_blink(state: &mut State, dt: f32) {
    let Some(entry) = state.name_entry.as_mut() else {
        return;
    };
    entry.blink_t = (entry.blink_t + dt) % 1.0;
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    update_hold_scroll(state);
    update_name_entry_blink(state, dt);
    None
}

fn name_conflicts(state: &State, name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    for row in &state.rows {
        if let RowKind::Profile { display_name, .. } = &row.kind {
            if display_name.trim() == trimmed {
                return true;
            }
        }
    }
    false
}

fn default_new_profile_name(state: &State) -> String {
    for i in 1..1000 {
        let candidate = format!("New{i:04}");
        if !name_conflicts(state, &candidate) {
            return candidate;
        }
    }
    "New0001".to_string()
}

fn validate_new_profile_name(state: &State, name: &str) -> Result<(), &'static str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Profile name cannot be blank.");
    }

    if name_conflicts(state, trimmed) {
        return Err(
            "The name you chose conflicts with another profile. Please use a different name.",
        );
    }
    Ok(())
}

fn try_create_profile(state: &mut State, name: &str) -> Result<String, &'static str> {
    validate_new_profile_name(state, name)?;
    let trimmed = name.trim();
    profile::create_local_profile(trimmed).map_err(|_| "Failed to create profile.")
}

fn confirm_name_entry(state: &mut State) {
    let Some(entry) = state.name_entry.take() else {
        return;
    };

    match try_create_profile(state, &entry.value) {
        Ok(id) => {
            audio::play_sfx("assets/sounds/start.ogg");
            refresh_rows(state);
            reset_nav_hold(state);
            if let Some(pos) = state.rows.iter().position(|r| match &r.kind {
                RowKind::Profile { id: row_id, .. } => row_id == &id,
                _ => false,
            }) {
                state.selected = pos;
                state.prev_selected = pos;
            }
        }
        Err(e) => {
            state.name_entry = Some(NameEntryState {
                value: entry.value,
                error: Some(e),
                blink_t: entry.blink_t,
            });
        }
    }
}

fn cancel_name_entry(state: &mut State) {
    state.name_entry = None;
    reset_nav_hold(state);
}

fn begin_delete_confirm(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.delete_confirm = Some(DeleteConfirmState {
        id: id.to_string(),
        display_name: display_name.to_string(),
        error: None,
    });
}

#[inline(always)]
fn selected_after_delete(selected_before: usize, total_after: usize) -> usize {
    if total_after == 0 {
        return 0;
    }
    let mut selected = selected_before.min(total_after - 1);
    if selected + 1 == total_after && selected > 0 {
        selected -= 1;
    }
    selected
}

fn confirm_delete(state: &mut State) {
    let Some(confirm) = state.delete_confirm.take() else {
        return;
    };

    let selected_before = state.selected;
    match profile::delete_local_profile(&confirm.id) {
        Ok(()) => {
            audio::play_sfx("assets/sounds/start.ogg");
            refresh_rows(state);
            reset_nav_hold(state);
            let selected = selected_after_delete(selected_before, state.rows.len());
            state.selected = selected;
            state.prev_selected = selected;
        }
        Err(_) => {
            state.delete_confirm = Some(DeleteConfirmState {
                id: confirm.id,
                display_name: confirm.display_name,
                error: Some("Failed to delete profile."),
            });
        }
    }
}

fn cancel_delete_confirm(state: &mut State) {
    state.delete_confirm = None;
    reset_nav_hold(state);
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if state.name_entry.is_some() {
        match ev.action {
            VirtualAction::p1_start if ev.pressed => confirm_name_entry(state),
            VirtualAction::p1_back if ev.pressed => cancel_name_entry(state),
            _ => {}
        }
        return ScreenAction::None;
    }

    if state.delete_confirm.is_some() {
        match ev.action {
            VirtualAction::p1_start if ev.pressed => confirm_delete(state),
            VirtualAction::p1_back if ev.pressed => cancel_delete_confirm(state),
            _ => {}
        }
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back if ev.pressed => return ScreenAction::Navigate(Screen::Options),
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if ev.pressed {
                move_selected(state, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if ev.pressed {
                move_selected(state, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_start if ev.pressed => {
            let total = state.rows.len();
            if total == 0 {
                return ScreenAction::None;
            }
            let sel = state.selected.min(total - 1);
            let start_row = state.rows[sel].kind.clone();
            match start_row {
                RowKind::CreateNew => {
                    reset_nav_hold(state);
                    state.name_entry = Some(NameEntryState {
                        value: default_new_profile_name(state),
                        error: None,
                        blink_t: 0.0,
                    });
                }
                RowKind::Exit => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::Navigate(Screen::Options);
                }
                RowKind::Profile { id, display_name } => {
                    begin_delete_confirm(state, &id, &display_name);
                }
            }
        }
        _ => {}
    }

    ScreenAction::None
}

pub fn handle_raw_key_event(state: &mut State, key_event: &KeyEvent) -> ScreenAction {
    let Some(entry) = state.name_entry.as_mut() else {
        return ScreenAction::None;
    };
    if key_event.state != ElementState::Pressed {
        return ScreenAction::None;
    }

    if let PhysicalKey::Code(code) = key_event.physical_key {
        match code {
            KeyCode::Backspace => {
                let _ = entry.value.pop();
                entry.error = None;
                return ScreenAction::None;
            }
            KeyCode::Escape => return ScreenAction::None,
            _ => {}
        }
    }

    let Some(text) = key_event.text.as_ref() else {
        return ScreenAction::None;
    };

    let mut len = entry.value.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= NAME_MAX_LEN {
            break;
        }
        entry.value.push(ch);
        len += 1;
    }
    entry.error = None;
    ScreenAction::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

fn scaled_block_origin_with_margins() -> (f32, f32, f32) {
    let total_w = LIST_W + SEP_W + DESC_W;
    let total_h = DESC_H;

    let sw = screen_width();
    let sh = screen_height();

    let content_top = BAR_H;
    let content_bottom = sh - BAR_H;
    let content_h = (content_bottom - content_top).max(0.0);

    let avail_w = (sw - LEFT_MARGIN_PX - RIGHT_MARGIN_PX).max(0.0);
    let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

    let s_w = if total_w > 0.0 {
        avail_w / total_w
    } else {
        1.0
    };
    let s_h = if total_h > 0.0 {
        avail_h / total_h
    } else {
        1.0
    };
    let s = s_w.min(s_h).max(0.0);

    let ox = LEFT_MARGIN_PX + total_w.mul_add(-s, avail_w).max(0.0);
    let oy = content_top + FIRST_ROW_TOP_MARGIN_PX;
    (s, ox, oy)
}

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Mesh { vertices, .. } => {
            let mut out: Vec<crate::core::gfx::MeshVertex> = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                let mut c = v.color;
                c[3] *= alpha;
                out.push(crate::core::gfx::MeshVertex {
                    pos: v.pos,
                    color: c,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::TexturedMesh { vertices, .. } => {
            let mut out: Vec<crate::core::gfx::TexturedMeshVertex> =
                Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                let mut c = v.color;
                c[3] *= alpha;
                out.push(crate::core::gfx::TexturedMeshVertex {
                    pos: v.pos,
                    uv: v.uv,
                    color: c,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::Frame {
            background,
            children,
            ..
        } => {
            if let Some(actors::Background::Color(c)) = background {
                c[3] *= alpha;
            }
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Camera { children, .. } => {
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Shadow { color, child, .. } => {
            color[3] *= alpha;
            apply_alpha_to_actor(child, alpha);
        }
    }
}

fn indicator_text(id: &str, p1_id: Option<&str>, p2_id: Option<&str>) -> Option<&'static str> {
    let is_p1 = p1_id.is_some_and(|p1| p1 == id);
    let is_p2 = p2_id.is_some_and(|p2| p2 == id);
    match (is_p1, is_p2) {
        (true, true) => Some("P1+P2"),
        (true, false) => Some("P1"),
        (false, true) => Some("P2"),
        (false, false) => None,
    }
}

fn help_for_selected(state: &State, p1_id: Option<&str>, p2_id: Option<&str>) -> (String, String) {
    let Some(row) = state.rows.get(state.selected) else {
        return (String::new(), String::new());
    };

    match &row.kind {
        RowKind::CreateNew => {
            let title = "Create a new local profile.";
            let bullets = make_bullets(&[
                "Enter a name for the profile.",
                "Press Start to confirm.",
                "Press Back to cancel.",
            ]);
            (title.to_string(), bullets)
        }
        RowKind::Exit => ("Return to Options.".to_string(), String::new()),
        RowKind::Profile { id, display_name } => {
            let mut title = String::new();
            title.push_str("Local profile: ");
            title.push_str(display_name);

            let assigned = match indicator_text(id, p1_id, p2_id) {
                Some(tag) => format!("Assigned: {tag}"),
                None => "Assigned: (none)".to_string(),
            };
            let bullets = make_bullets(&[
                &format!("ID: {id}"),
                &assigned,
                "Press Start to delete this profile.",
            ]);
            (title, bullets)
        }
    }
}

fn make_bullets(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut first = true;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        out.push('•');
        out.push(' ');
        out.push_str(trimmed);
        first = false;
    }
    out
}

fn push_desc(ui: &mut Vec<Actor>, state: &State, s: f32, desc_x: f32, list_y: f32) {
    let p1 = profile::active_local_profile_id_for_side(profile::PlayerSide::P1);
    let p2 = profile::active_local_profile_id_for_side(profile::PlayerSide::P2);
    let (title, bullets) = help_for_selected(state, p1.as_deref(), p2.as_deref());

    let mut cursor_y = DESC_TITLE_TOP_PAD_PX.mul_add(s, list_y);
    let title_x = desc_x + DESC_TITLE_SIDE_PAD_PX * s;
    let max_title_w = (DESC_W - 2.0 * DESC_TITLE_SIDE_PAD_PX)
        .mul_add(s, 0.0)
        .max(0.0);
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(title_x, cursor_y):
        zoom(DESC_TITLE_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        maxwidth(max_title_w):
        settext(title):
        horizalign(left)
    ));

    cursor_y += DESC_BULLET_TOP_PAD_PX * s;
    if bullets.is_empty() {
        return;
    }

    let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
    let bullet_x = DESC_BULLET_INDENT_PX.mul_add(s, desc_x + bullet_side_pad);
    let max_bullet_w = (DESC_W - 2.0 * DESC_BULLET_SIDE_PAD_PX)
        .mul_add(s, 0.0)
        .max(0.0);
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(bullet_x, cursor_y):
        zoom(DESC_BODY_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        maxwidth(max_bullet_w):
        settext(bullets):
        horizalign(left)
    ));
}

fn push_name_entry_overlay(ui: &mut Vec<Actor>, state: &State) {
    let Some(entry) = &state.name_entry else {
        return;
    };

    let w = screen_width();
    let h = screen_height();

    let box_w = 560.0_f32.min(w * 0.9);
    let box_h = 170.0_f32;
    let cx = w * 0.5;
    let cy = h * 0.5;

    push_overlay_backdrop(ui, w, h);
    push_overlay_box(ui, cx, cy, box_w, box_h);
    push_overlay_prompt(ui, cx, cy, box_h);
    push_overlay_value(ui, entry, cx, cy, box_w);
    push_overlay_footer(ui, cx, cy, box_h);
    push_overlay_error(ui, entry.error, cx, cy, box_w, box_h);
}

fn push_delete_confirm_overlay(ui: &mut Vec<Actor>, state: &State) {
    let Some(confirm) = &state.delete_confirm else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let box_w = 700.0_f32.min(w * 0.92);
    let box_h = 190.0_f32;
    let cx = w * 0.5;
    let cy = h * 0.5;

    push_overlay_backdrop(ui, w, h);
    push_overlay_box(ui, cx, cy, box_w, box_h);

    let prompt = format!(
        "Are you sure you want to delete the profile '{}'?",
        confirm.display_name
    );
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 16.0):
        font("miso"):
        zoom(1.0):
        maxwidth(box_w - 40.0):
        settext(prompt):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 58.0):
        font("miso"):
        zoom(0.9):
        settext("This cannot be undone."):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
    ui.push(act!(text:
        align(0.5, 1.0):
        xy(cx, cy + box_h * 0.5 - 10.0):
        font("miso"):
        zoom(0.9):
        settext("Start: Yes    Back: No"):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));

    push_overlay_error(ui, confirm.error, cx, cy, box_w, box_h);
}

fn push_overlay_backdrop(ui: &mut Vec<Actor>, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(1000)
    ));
}

fn push_overlay_box(ui: &mut Vec<Actor>, cx: f32, cy: f32, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(w, h):
        diffuse(0.2, 0.2, 0.2, 1.0):
        z(1001)
    ));
}

fn push_overlay_prompt(ui: &mut Vec<Actor>, cx: f32, cy: f32, box_h: f32) {
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 14.0):
        font("miso"):
        zoom(1.0):
        settext("Enter a name for the profile."):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
}

fn push_overlay_value(ui: &mut Vec<Actor>, entry: &NameEntryState, cx: f32, cy: f32, box_w: f32) {
    let cursor = if entry.blink_t < 0.5 { "▮" } else { " " };
    let mut value = entry.value.clone();
    if value.chars().count() < NAME_MAX_LEN {
        value.push_str(cursor);
    }

    ui.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy):
        font("miso"):
        zoom(1.2):
        maxwidth(box_w - 40.0):
        settext(value):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
}

fn push_overlay_footer(ui: &mut Vec<Actor>, cx: f32, cy: f32, box_h: f32) {
    ui.push(act!(text:
        align(0.5, 1.0):
        xy(cx, cy + box_h * 0.5 - 10.0):
        font("miso"):
        zoom(0.9):
        settext("Start: Confirm    Back: Cancel"):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
}

fn push_overlay_error(
    ui: &mut Vec<Actor>,
    err: Option<&'static str>,
    cx: f32,
    cy: f32,
    box_w: f32,
    box_h: f32,
) {
    let Some(err) = err else {
        return;
    };
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy + box_h * 0.5 - 46.0):
        font("miso"):
        zoom(0.9):
        maxwidth(box_w - 40.0):
        settext(err):
        diffuse(1.0, 0.2, 0.2, 1.0):
        z(1002):
        horizalign(center)
    ));
}

fn push_list_chrome(
    ui: &mut Vec<Actor>,
    col_active_bg: [f32; 4],
    s: f32,
    list_x: f32,
    list_y: f32,
) {
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let desc_h = DESC_H * s;

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x + list_w, list_y):
        zoomto(sep_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));

    let desc_x = list_x + list_w + sep_w;
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, list_y):
        zoomto(DESC_W * s, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));
}

struct RowColors {
    active_bg: [f32; 4],
    inactive_bg: [f32; 4],
    brand_bg: [f32; 4],
    active_text: [f32; 4],
    white: [f32; 4],
    black: [f32; 4],
}

fn row_label(kind: &RowKind) -> &str {
    match kind {
        RowKind::CreateNew => "Create New Profile",
        RowKind::Exit => "Exit",
        RowKind::Profile { display_name, .. } => display_name.as_str(),
    }
}

fn row_is_exit(kind: &RowKind) -> bool {
    matches!(kind, RowKind::Exit)
}

fn row_width(list_w: f32, sep_w: f32, is_active: bool, is_exit: bool) -> f32 {
    if is_exit {
        list_w - sep_w
    } else if is_active {
        list_w
    } else {
        list_w - sep_w
    }
}

fn row_bg_color(colors: &RowColors, is_active: bool, is_exit: bool) -> [f32; 4] {
    if is_active {
        if is_exit {
            colors.brand_bg
        } else {
            colors.active_bg
        }
    } else {
        colors.inactive_bg
    }
}

fn row_text_color(colors: &RowColors, is_active: bool, is_exit: bool) -> [f32; 4] {
    if is_exit {
        if is_active {
            colors.black
        } else {
            colors.white
        }
    } else if is_active {
        colors.active_text
    } else {
        colors.white
    }
}

fn push_row(
    ui: &mut Vec<Actor>,
    kind: &RowKind,
    is_active: bool,
    row_y: f32,
    list_x: f32,
    list_w: f32,
    sep_w: f32,
    s: f32,
    colors: &RowColors,
    p1_id: Option<&str>,
    p2_id: Option<&str>,
) {
    let is_exit = row_is_exit(kind);
    let row_mid_y = (0.5 * ROW_H).mul_add(s, row_y);
    let row_w = row_width(list_w, sep_w, is_active, is_exit);
    let bg = row_bg_color(colors, is_active, is_exit);
    let text_col = row_text_color(colors, is_active, is_exit);

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x, row_y):
        zoomto(row_w, ROW_H * s):
        diffuse(bg[0], bg[1], bg[2], bg[3])
    ));

    if !is_exit {
        let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
        let heart_tint = if is_active {
            colors.active_text
        } else {
            colors.white
        };
        ui.push(act!(sprite("heart.png"):
            align(0.0, 0.5):
            xy(heart_x, row_mid_y):
            zoom(HEART_ZOOM):
            diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
        ));
    }

    let text_x = TEXT_LEFT_PAD.mul_add(s, list_x);
    ui.push(act!(text:
        align(0.0, 0.5):
        xy(text_x, row_mid_y):
        zoom(ITEM_TEXT_ZOOM):
        diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
        font("miso"):
        settext(row_label(kind)):
        horizalign(left)
    ));

    if let RowKind::Profile { id, .. } = kind
        && let Some(tag) = indicator_text(id, p1_id, p2_id)
    {
        ui.push(act!(text:
            align(1.0, 0.5):
            xy(list_x + list_w - 12.0 * s, row_mid_y):
            zoom(0.75):
            diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
            font("miso"):
            settext(tag):
            horizalign(right)
        ));
    }
}

fn push_rows(
    ui: &mut Vec<Actor>,
    state: &State,
    s: f32,
    list_x: f32,
    list_y: f32,
    col_active_bg: [f32; 4],
    col_inactive_bg: [f32; 4],
) {
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let total_rows = state.rows.len();
    let offset = scroll_offset(state.selected, total_rows);
    let colors = RowColors {
        active_bg: col_active_bg,
        inactive_bg: col_inactive_bg,
        brand_bg: color::simply_love_rgba(state.active_color_index),
        active_text: color::simply_love_rgba(state.active_color_index + state.selected as i32),
        white: [1.0, 1.0, 1.0, 1.0],
        black: [0.0, 0.0, 0.0, 1.0],
    };

    let p1 = profile::active_local_profile_id_for_side(profile::PlayerSide::P1);
    let p2 = profile::active_local_profile_id_for_side(profile::PlayerSide::P2);
    let p1_id = p1.as_deref();
    let p2_id = p2.as_deref();

    for i_vis in 0..VISIBLE_ROWS {
        let row_idx = offset + i_vis;
        if row_idx >= total_rows {
            break;
        }
        let row_y = ((i_vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, list_y);
        let is_active = row_idx == state.selected;
        push_row(
            ui,
            &state.rows[row_idx].kind,
            is_active,
            row_y,
            list_x,
            list_w,
            sep_w,
            s,
            &colors,
            p1_id,
            p2_id,
        );
    }
}

pub fn get_actors(
    state: &State,
    _asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(220);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui = Vec::new();
    ui.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "MANAGE PROFILES",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        fg_color: [1.0, 1.0, 1.0, 1.0],
    }));

    let col_active_bg = color::rgba_hex("#333333");
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];

    let (s, list_x, list_y) = scaled_block_origin_with_margins();
    push_list_chrome(&mut ui, col_active_bg, s, list_x, list_y);
    push_rows(
        &mut ui,
        state,
        s,
        list_x,
        list_y,
        col_active_bg,
        col_inactive_bg,
    );

    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let desc_x = list_x + list_w + sep_w;
    push_desc(&mut ui, state, s, desc_x, list_y);
    push_name_entry_overlay(&mut ui, state);
    push_delete_confirm_overlay(&mut ui, state);

    for actor in &mut ui {
        apply_alpha_to_actor(actor, alpha_multiplier);
    }
    actors.extend(ui);
    actors
}
