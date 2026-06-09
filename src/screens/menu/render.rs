//! Pure render path for the title menu — the hot-reload target.
//!
//! Every value this code needs is **pre-resolved host-side** and handed in
//! through `HostContext` (see `super::build_host_context`). This module performs
//! **no** engine-global read of its own and keeps no persistent state. Because
//! the hot boundary runs under one shared allocator (`-C prefer-dynamic`), the
//! render path freely **clones** the host's `Arc<str>` text into the actors it
//! emits — the resulting `Vec<Actor>` is allocated here and dropped host-side,
//! which is sound only under that shared allocator.
//!
//! KNOWN IMPURITY (intentionally deferred): the component builders called below
//! (`visual_style_bg`, `logo`, `menu_list`, `screen_bar`) and the `act!` text
//! actors still resolve textures/fonts through engine globals
//! (`assets::texture_registry_generation()`) and bake `&'static str`
//! texture/font keys. In-process this is identical to today. Before the runtime
//! can ever *unload* an old cdylib it must be addressed — favored approach:
//! render emits actors carrying texture *keys* only and the host re-resolves
//! handles after return (keeps asset lifetime, registry generation and
//! `&'static` keys host-owned).

use deadsync::act;
use deadsync::engine::present::actors::{Actor, TextAlign};
use deadsync::engine::present::color;
use deadsync::screens::components::menu::logo::{self, LogoParams};
use deadsync::screens::components::menu::menu_list::{self};
use deadsync::screens::components::shared::{screen_bar, visual_style_bg};
use deadsync::screens::menu::state::{HostContext, State};
use std::sync::Arc;

const NORMAL_COLOR_HEX: &str = "#888888";

const MENU_BELOW_LOGO: f32 = 29.0;
const MENU_ROW_SPACING: f32 = 28.0;

const INFO_PX: f32 = 15.0;
const INFO_GAP: f32 = 5.0;
const INFO_MARGIN_ABOVE: f32 = 20.0;
const STATUS_BASE_X: f32 = 10.0;
const STATUS_BASE_Y: f32 = 15.0;
const STATUS_ZOOM: f32 = 0.8;
const STATUS_LINE_HEIGHT: f32 = 18.0;
const STATUS_BLOCK_GAP: f32 = 6.0;

#[inline(always)]
fn status_text_actor(
    text: Arc<str>,
    align_x: f32,
    x: f32,
    y: f32,
    zoom: f32,
    alpha: f32,
    align_text: TextAlign,
) -> Actor {
    // TODO: `font("miso")` bakes a hot-owned `&'static str`; route through a
    // lib-owned font-key value on `HostContext` before old cdylibs are unloaded.
    let mut actor = act!(text:
        font("miso"):
        settext(text):
        align(align_x, 0.0):
        xy(x, y):
        zoom(zoom):
        z(200)
    );
    if let Actor::Text {
        color,
        align_text: text_align,
        ..
    } = &mut actor
    {
        color[3] = alpha;
        *text_align = align_text;
    }
    actor
}

pub fn get_actors(state: &State, ctx: &HostContext, alpha_multiplier: f32) -> Vec<Actor> {
    let lp = LogoParams::default();
    let mut actors: Vec<Actor> = Vec::with_capacity(96);

    // 1) background component (never fades)
    let backdrop = if state.rainbow_mode {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    actors.extend(state.bg.build_at_elapsed(
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: backdrop,
            alpha_mul: 1.0,
        },
        ctx.bg_elapsed_s,
    ));

    // If fully faded, don't create the other actors
    if alpha_multiplier <= 0.0 {
        return actors;
    }

    // 2) logo + info
    let info2_y_tl = lp.top_margin - INFO_MARGIN_ABOVE - INFO_PX;
    let info1_y_tl = info2_y_tl - INFO_PX - INFO_GAP;

    let logo_actors = logo::build_logo_default();
    for mut actor in logo_actors {
        if let Actor::Sprite { tint, .. } = &mut actor {
            tint[3] *= alpha_multiplier;
        }
        actors.push(actor);
    }

    let mut info_color = [1.0, 1.0, 1.0, 1.0];
    info_color[3] *= alpha_multiplier;

    actors.push(act!(text:
        align(0.5, 0.0): xy(ctx.screen_center_x, info1_y_tl): zoom(0.8):
        font("miso"): settext(ctx.info_text.clone()): horizalign(center):
        diffuse(info_color[0], info_color[1], info_color[2], info_color[3])
    ));

    // 3) menu list
    let base_y = lp.top_margin + lp.target_h + MENU_BELOW_LOGO;
    let mut selected = color::menu_selected_rgba(state.active_color_index);
    let mut normal = color::rgba_hex(NORMAL_COLOR_HEX);
    selected[3] *= alpha_multiplier;
    normal[3] *= alpha_multiplier;

    let params = menu_list::MenuParams {
        options: &ctx.menu_labels,
        selected_index: state.selected_index,
        start_center_y: base_y,
        row_spacing: MENU_ROW_SPACING,
        selected_color: selected,
        normal_color: normal,
        font: ctx.menu_font,
    };
    actors.extend(menu_list::build_vertical_menu(params));

    // --- footer bar ---
    let mut footer_fg = [1.0, 1.0, 1.0, 1.0];
    footer_fg[3] *= alpha_multiplier;

    actors.push(screen_bar::build_title_menu(screen_bar::ScreenBarParams {
        title: ctx.footer_title.as_ref(),
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some(ctx.footer_side.as_ref()),
        center_text: None,
        right_text: Some(ctx.footer_side.as_ref()),
        left_avatar: None,
        right_avatar: None,
        fg_color: footer_fg,
    }));

    // --- GrooveStats Info Pane (top-left) ---
    let gs_text = &ctx.gs;
    actors.push(status_text_actor(
        gs_text.main.clone(),
        0.0,
        STATUS_BASE_X,
        STATUS_BASE_Y,
        STATUS_ZOOM,
        alpha_multiplier,
        TextAlign::Left,
    ));
    for line_idx in 0..gs_text.line_count {
        if let Some(text) = gs_text.lines[line_idx].as_ref() {
            actors.push(status_text_actor(
                text.clone(),
                0.0,
                STATUS_BASE_X,
                (STATUS_LINE_HEIGHT * (line_idx as f32 + 1.0)).mul_add(STATUS_ZOOM, STATUS_BASE_Y),
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            ));
        }
    }

    // --- Arrow Cloud Info Pane (below GrooveStats/BoogieStats) ---
    let ac_base_y = (STATUS_LINE_HEIGHT * (gs_text.line_count as f32 + 1.0))
        .mul_add(STATUS_ZOOM, STATUS_BASE_Y + STATUS_BLOCK_GAP);
    let ac_text = &ctx.ac;
    actors.push(status_text_actor(
        ac_text.main.clone(),
        0.0,
        STATUS_BASE_X,
        ac_base_y,
        STATUS_ZOOM,
        alpha_multiplier,
        TextAlign::Left,
    ));
    for line_idx in 0..ac_text.line_count {
        if let Some(text) = ac_text.lines[line_idx].as_ref() {
            actors.push(status_text_actor(
                text.clone(),
                0.0,
                STATUS_BASE_X,
                (STATUS_LINE_HEIGHT * (line_idx as f32 + 1.0)).mul_add(STATUS_ZOOM, ac_base_y),
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            ));
        }
    }

    // --- StepManiaX pad-conflict warning (host-resolved; only when active). ---
    if let Some(lines) = &ctx.smx_warning {
        let smx_base_y = (STATUS_LINE_HEIGHT * (ac_text.line_count as f32 + 1.0))
            .mul_add(STATUS_ZOOM, ac_base_y + STATUS_BLOCK_GAP);
        for (i, text) in lines.iter().enumerate() {
            let y = (STATUS_LINE_HEIGHT * i as f32).mul_add(STATUS_ZOOM, smx_base_y);
            let mut actor = status_text_actor(
                text.clone(),
                0.0,
                STATUS_BASE_X,
                y,
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            );
            if let Actor::Text { color, .. } = &mut actor {
                // Amber warning (alpha already applied by status_text_actor).
                color[..3].copy_from_slice(&deadsync::engine::smx::CONFLICT_WARNING_RGB);
            }
            actors.push(actor);
        }
    }

    actors
}
