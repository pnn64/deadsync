use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::space::screen_center_x;
use crate::game::profile;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};

pub fn push(out: &mut Vec<Actor>, top_title: &str) {
    let p1_profile = profile::get_for_side(profile::PlayerSide::P1);
    let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|k| AvatarParams { texture_key: k });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|k| AvatarParams { texture_key: k });

    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let p1_guest = profile::is_session_side_guest(profile::PlayerSide::P1);
    let p2_guest = profile::is_session_side_guest(profile::PlayerSide::P2);

    let insert_card = tr("Common", "InsertCard");
    let press_start = tr("Common", "PressStart");
    let event_mode = tr("Common", "EventMode");

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                insert_card.as_ref()
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                insert_card.as_ref()
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };

    out.push(screen_bar::build_select_music(ScreenBarParams {
        title: top_title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));
    out.push(screen_bar::build_select_music(ScreenBarParams {
        title: event_mode.as_ref(),
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));
}

pub fn build_stage_display(stage_number: usize) -> Actor {
    let text = format!("Stage {stage_number}");
    Actor::Frame {
        align: [0.0, 0.0],
        offset: [screen_center_x(), 40.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 124,
        children: vec![
            act!(quad:
                align(0.5, 0.5):
                xy(0.0, 0.0):
                zoomto(75.0, 22.5):
                diffuse(0.0, 0.0, 0.0, 0.4):
                fadeleft(0.2):
                faderight(0.2):
                z(0)
            ),
            act!(text:
                font(current_machine_font_key(FontRole::Normal)):
                settext(text):
                align(0.5, 0.5):
                xy(0.0, -0.75):
                zoom(0.75):
                shadowlength(0.75):
                strokecolor(0.0, 0.0, 0.0, 1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1):
                horizalign(center)
            ),
        ],
    }
}
