use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::*;
use crate::game::parsing::noteskin::{self, Noteskin, Quantization, NUM_QUANTIZATIONS};
use crate::game::profile::{self, ActiveProfile};
use crate::game::scroll::ScrollSpeedSetting;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{self, Actor};
use crate::ui::color;
use crate::ui::components::screen_bar::{ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::components::{heart_bg, screen_bar};
use std::path::Path;
use std::str::FromStr;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ------------------------------ layout ------------------------------- */
const ROW_H: f32 = 35.0;
const ROWS_VISIBLE: i32 = 9;
const FRAME_BASE_W: f32 = 200.0;
const FRAME_W_SCROLLER: f32 = FRAME_BASE_W * 1.1;
const FRAME_W_JOIN: f32 = FRAME_BASE_W * 0.9;
const FRAME_H: f32 = 214.0;
const FRAME_BORDER: f32 = 2.0;
const FRAME_CX_OFF: f32 = 150.0;

const INFO_W: f32 = FRAME_BASE_W * 0.475;
const INFO_X0_OFF: f32 = 15.5;
const INFO_PAD: f32 = 4.0;

const SCROLLER_W: f32 = FRAME_W_SCROLLER - INFO_W;
const SCROLLER_CX_OFF: f32 = -47.0;
const SCROLLER_TEXT_PAD_X: f32 = 6.0;

const AVATAR_BG_HEX: &str = "#283239aa";
const AVATAR_X_OFF: f32 = INFO_PAD * 1.125;
const AVATAR_Y_OFF: f32 = -103.5;
const AVATAR_HEART_X: f32 = 13.0;
const AVATAR_HEART_Y: f32 = 8.0;
const AVATAR_HEART_ZOOM: f32 = 0.09;
const AVATAR_TEXT_Y: f32 = 67.0;

const INFO_LINE_Y_OFF: f32 = 18.0;
// Unified Y offset for side-by-side previews
const PREVIEW_Y_OFF: f32 = 42.0;

const PREVIEW_LABEL_H: f32 = 12.0;
const PREVIEW_VALUE_H: f32 = 16.0;

#[derive(Clone)]
struct Choice {
    kind: ActiveProfile,
    display_name: String,
    speed_mod: String,
    avatar_key: Option<String>,
    noteskin: profile::NoteSkin,
    judgment: profile::JudgmentGraphic,
}

pub struct State {
    pub active_color_index: i32,
    selected_index: usize,
    choices: Vec<Choice>,
    bg: heart_bg::State,
    preview_noteskin: Option<Noteskin>,
}

fn load_noteskin(kind: profile::NoteSkin) -> Option<Noteskin> {
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };

    let path_str = match kind {
        profile::NoteSkin::Cel => "assets/noteskins/cel/dance-single.txt",
        profile::NoteSkin::Metal => "assets/noteskins/metal/dance-single.txt",
        profile::NoteSkin::EnchantmentV2 => "assets/noteskins/enchantment-v2/dance-single.txt",
        profile::NoteSkin::DevCel2024V3 => "assets/noteskins/devcel-2024-v3/dance-single.txt",
    };

    noteskin::load(Path::new(path_str), &style)
        .ok()
        .or_else(|| noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style).ok())
        .or_else(|| noteskin::load(Path::new("assets/noteskins/fallback.txt"), &style).ok())
}

fn rebuild_preview(state: &mut State) {
    let Some(choice) = state.choices.get(state.selected_index) else {
        state.preview_noteskin = None;
        return;
    };

    state.preview_noteskin = match choice.kind {
        ActiveProfile::Guest => None,
        ActiveProfile::Local { .. } => load_noteskin(choice.noteskin),
    };
}

fn build_choices() -> Vec<Choice> {
    let mut out = Vec::new();

    let guest_speed_mod = format!("{}", crate::game::profile::Profile::default().scroll_speed);
    out.push(Choice {
        kind: ActiveProfile::Guest,
        display_name: "[ GUEST ]".to_string(),
        speed_mod: guest_speed_mod,
        avatar_key: None,
        noteskin: profile::NoteSkin::default(),
        judgment: profile::JudgmentGraphic::default(),
    });
    for p in profile::scan_local_profiles() {
        let mut speed_mod = String::new();
        let mut noteskin = profile::NoteSkin::default();
        let mut judgment = profile::JudgmentGraphic::default();
        let ini_path = std::path::Path::new("save/profiles")
            .join(&p.id)
            .join("profile.ini");
        let mut ini = crate::config::SimpleIni::new();
        if ini.load(&ini_path).is_ok()
            && let Some(raw) = ini.get("PlayerOptions", "ScrollSpeed")
        {
            let trimmed = raw.trim();
            speed_mod = if let Ok(setting) = ScrollSpeedSetting::from_str(trimmed) {
                format!("{}", setting)
            } else {
                trimmed.to_string()
            };
        }
        if let Ok(value) = ini
            .get("PlayerOptions", "NoteSkin")
            .unwrap_or_default()
            .parse::<profile::NoteSkin>()
        {
            noteskin = value;
        }
        if let Ok(value) = ini
            .get("PlayerOptions", "JudgmentGraphic")
            .unwrap_or_default()
            .parse::<profile::JudgmentGraphic>()
        {
            judgment = value;
        }

        out.push(Choice {
            kind: ActiveProfile::Local { id: p.id },
            display_name: p.display_name,
            speed_mod,
            avatar_key: p.avatar_path.map(|path| path.to_string_lossy().into_owned()),
            noteskin,
            judgment,
        });
    }
    out
}

pub fn init() -> State {
    let choices = build_choices();
    let active = profile::get_active_profile();
    let active_color_index = crate::config::get().simply_love_color;

    let mut selected_index = 0usize;
    if let ActiveProfile::Local { id } = active {
        if let Some(i) = choices.iter().position(|c| match &c.kind {
            ActiveProfile::Local { id: cid } => cid == &id,
            ActiveProfile::Guest => false,
        }) {
            selected_index = i;
        }
    }

    let mut state = State {
        active_color_index,
        selected_index,
        choices,
        bg: heart_bg::State::new(),
        preview_noteskin: None,
    };
    rebuild_preview(&mut state);
    state
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

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if state.selected_index > 0 {
                state.selected_index -= 1;
                rebuild_preview(state);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if state.selected_index + 1 < state.choices.len() {
                state.selected_index += 1;
                rebuild_preview(state);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_start => {
            audio::play_sfx("assets/sounds/start.ogg");
            let choice = state
                .choices
                .get(state.selected_index)
                .map(|c| c.kind.clone())
                .unwrap_or(ActiveProfile::Guest);
            ScreenAction::SelectProfile(choice)
        }
        VirtualAction::p1_back | VirtualAction::p1_select => ScreenAction::Navigate(Screen::Menu),
        _ => ScreenAction::None,
    }
}

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
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
        Actor::Shadow { color, child, .. } => {
            color[3] *= alpha;
            apply_alpha_to_actor(child, alpha);
        }
    }
}

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(128);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    let fg = [1.0, 1.0, 1.0, 1.0];

    actors.push(screen_bar::build(ScreenBarParams {
        title: "SELECT PROFILE",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: fg,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
    }));
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: fg,
        left_text: None,
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: None,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui: Vec<Actor> = Vec::new();

    let frame_h = FRAME_H;
    let cx = screen_center_x();
    let cy = screen_center_y();

    let frame_y0 = cy - frame_h * 0.5;

    let p1_cx = cx - FRAME_CX_OFF;
    let p2_cx = cx + FRAME_CX_OFF;

    // Use dim profile palette for the info pane (Left Column).
    let col_dim = color::select_profile_dim_rgba(state.active_color_index);
    // Use bright profile palette for the main frame (Right Column).
    let col_bright = color::select_profile_rgba(state.active_color_index);

    let border_rgba = [1.0, 1.0, 1.0, 1.0];

    // P1 frame background (Right Column / Scroller) - Bright Color
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p1_cx, cy):
        zoomto(FRAME_W_SCROLLER + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(border_rgba[0], border_rgba[1], border_rgba[2], border_rgba[3]):
        z(100)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p1_cx, cy):
        zoomto(FRAME_W_SCROLLER, frame_h):
        diffuse(col_bright[0], col_bright[1], col_bright[2], col_bright[3]):
        z(101)
    ));

    // P1 info pane background (Left Column / Stats) - Dim Color
    let info_x0 = p1_cx + INFO_X0_OFF;
    let info_text_x = info_x0 + INFO_PAD * 1.25;
    let info_max_w = INFO_W - INFO_PAD * 2.5;

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(info_x0, frame_y0):
        zoomto(INFO_W, frame_h):
        diffuse(col_dim[0], col_dim[1], col_dim[2], col_dim[3]):
        z(102)
    ));

    // P2 join prompt (template only; not functional yet)
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p2_cx, cy):
        zoomto(FRAME_W_JOIN + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(border_rgba[0], border_rgba[1], border_rgba[2], border_rgba[3]):
        z(100)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p2_cx, cy):
        zoomto(FRAME_W_JOIN, frame_h):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(101)
    ));
    ui.push(act!(text:
        align(0.5, 0.5):
        xy(p2_cx, cy):
        font("miso"):
        zoomtoheight(18.0):
        maxwidth(FRAME_W_JOIN - 20.0):
        settext("Press START to join!"):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(103)
    ));

    // P1 scroller (Selection Bar) - Dim Color (contrast against bright bg)
    let scroller_cx = p1_cx + SCROLLER_CX_OFF;
    let highlight_h = ROW_H;

    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(scroller_cx, cy):
        zoomto(SCROLLER_W, highlight_h):
        diffuse(col_dim[0], col_dim[1], col_dim[2], col_dim[3]):
        z(102)
    ));

    let rows_half = ROWS_VISIBLE / 2;
    for d in -rows_half..=rows_half {
        let idx_i = state.selected_index as i32 + d;
        if idx_i < 0 || idx_i >= state.choices.len() as i32 {
            continue;
        }
        let choice = &state.choices[idx_i as usize];
        let y = cy + d as f32 * ROW_H;

        let a = 1.0 - (d.abs() as f32 / (rows_half as f32 + 1.0));
        let mut text_color = [1.0, 1.0, 1.0, 0.35 + 0.65 * a];
        let mut shadow = 0.0;
        if d == 0 {
            // Selected row: pure white with shadow
            text_color = [1.0, 1.0, 1.0, 1.0];
            shadow = 1.0;
        }

        ui.push(act!(text:
            align(0.5, 0.5):
            xy(scroller_cx, y):
            font("miso"):
            maxwidth(SCROLLER_W - SCROLLER_TEXT_PAD_X * 2.0):
            zoom(0.92):
            settext(choice.display_name.clone()):
            diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
            shadowlength(shadow):
            z(103):
            horizalign(center)
        ));
    }

    let selected = state.choices.get(state.selected_index);
    let selected_speed = selected
        .map(|c| c.speed_mod.as_str())
        .unwrap_or_default();

    // Avatar slot (SL-style): show profile.png if present, else heart + text.
    let avatar_dim = INFO_W - INFO_PAD * 2.25;
    let avatar_x = info_x0 + AVATAR_X_OFF;
    let avatar_y = cy + AVATAR_Y_OFF;

    if let Some(choice) = selected {
        let is_guest = matches!(choice.kind, ActiveProfile::Guest);

        let show_fallback = is_guest || choice.avatar_key.is_none();
        if show_fallback {
            let bg = color::rgba_hex(AVATAR_BG_HEX);
            ui.push(act!(quad:
                align(0.0, 0.0):
                xy(avatar_x, avatar_y):
                zoomto(avatar_dim, avatar_dim):
                diffuse(bg[0], bg[1], bg[2], bg[3]):
                z(103)
            ));
            ui.push(act!(sprite("heart.png"):
                align(0.0, 0.0):
                xy(avatar_x + AVATAR_HEART_X, avatar_y + AVATAR_HEART_Y):
                zoom(AVATAR_HEART_ZOOM):
                diffuse(1.0, 1.0, 1.0, 0.9):
                z(104)
            ));

            let label = if is_guest { "[ GUEST ]" } else { "No Avatar" };
            ui.push(act!(text:
                align(0.5, 0.0):
                xy(avatar_x + avatar_dim * 0.5, avatar_y + AVATAR_TEXT_Y):
                font("miso"):
                maxwidth(avatar_dim - 8.0):
                zoomtoheight(14.0):
                settext(label):
                diffuse(1.0, 1.0, 1.0, 0.9):
                z(105):
                horizalign(center)
            ));
        } else if let Some(key) = &choice.avatar_key {
            ui.push(act!(sprite(key.clone()):
                align(0.0, 0.0):
                xy(avatar_x, avatar_y):
                zoomto(avatar_dim, avatar_dim):
                z(104)
            ));
        }
    }

    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_text_x, avatar_y + avatar_dim + 20.0):
        font("miso"):
        maxwidth(info_max_w):
        zoomtoheight(PREVIEW_VALUE_H):
        settext(selected_speed.to_string()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(103)
    ));

    // Thin white line separating stats from mods (SL-style).
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(info_x0 + INFO_PAD * 1.25, cy + INFO_LINE_Y_OFF):
        zoomto(info_max_w, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.5):
        z(103)
    ));

    // NoteSkin + JudgmentGraphic previews (like PlayerOptions; SL-style placement).
    // Now positioned side-by-side within the info pane.
    if selected.is_some_and(|c| matches!(c.kind, ActiveProfile::Local { .. })) {
        let preview_y = cy + PREVIEW_Y_OFF;

        if let Some(ns) = &state.preview_noteskin {
            let note_idx = 2 * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
            if let Some(note_slot) = ns.notes.get(note_idx) {
                let frame = note_slot.frame_index(0.0, 0.0);
                let uv = note_slot.uv_for_frame(frame);
                let size = note_slot.size();
                let width = size[0].max(1) as f32;
                let height = size[1].max(1) as f32;

                const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;
                const PREVIEW_SCALE: f32 = 0.4;
                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                let scale = if height > 0.0 {
                    target_height / height
                } else {
                    PREVIEW_SCALE
                };

                let ns_x = info_x0 + INFO_W * 0.28;

                ui.push(act!(sprite(note_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(ns_x, preview_y):
                    zoomto(width * scale, target_height):
                    rotationz(-note_slot.def.rotation_deg as f32):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    z(104)
                ));
            }
        }

        let judgment_texture = selected
            .map(|c| match c.judgment {
                profile::JudgmentGraphic::Love => Some("judgements/Love 2x7 (doubleres).png"),
                profile::JudgmentGraphic::LoveChroma => {
                    Some("judgements/Love Chroma 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Rainbowmatic => {
                    Some("judgements/Rainbowmatic 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::GrooveNights => {
                    Some("judgements/GrooveNights 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Emoticon => {
                    Some("judgements/Emoticon 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Censored => {
                    Some("judgements/Censored 1x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Chromatic => {
                    Some("judgements/Chromatic 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::ITG2 => Some("judgements/ITG2 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Bebas => Some("judgements/Bebas 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Code => Some("judgements/Code 2x7 (doubleres).png"),
                profile::JudgmentGraphic::ComicSans => {
                    Some("judgements/Comic Sans 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Focus => Some("judgements/Focus 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Grammar => {
                    Some("judgements/Grammar 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Miso => Some("judgements/Miso 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Papyrus => {
                    Some("judgements/Papyrus 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Roboto => Some("judgements/Roboto 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Shift => Some("judgements/Shift 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Tactics => {
                    Some("judgements/Tactics 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Wendy => Some("judgements/Wendy 2x7 (doubleres).png"),
                profile::JudgmentGraphic::WendyChroma => {
                    Some("judgements/Wendy Chroma 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::None => None,
            })
            .unwrap_or(None);

        if let Some(texture) = judgment_texture {
            let jd_x = info_x0 + INFO_W * 0.72;
            ui.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(jd_x, preview_y):
                setstate(0):
                zoom(0.225):
                z(104)
            ));
        }
    }

    for mut a in ui {
        apply_alpha_to_actor(&mut a, alpha_multiplier);
        actors.push(a);
    }

    actors
}
