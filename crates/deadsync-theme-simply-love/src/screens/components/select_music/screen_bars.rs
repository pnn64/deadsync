use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::space::screen_center_x;

#[derive(Clone, Copy)]
pub struct Player<'a> {
    pub joined: bool,
    pub guest: bool,
    pub display_name: &'a str,
    pub avatar_texture_key: Option<&'a str>,
}

pub fn push(
    out: &mut Vec<Actor>,
    top_title: &str,
    players: [Player<'_>; 2],
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let [p1, p2] = players;
    let p1_avatar = p1
        .avatar_texture_key
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2
        .avatar_texture_key
        .map(|texture_key| AvatarParams { texture_key });

    let insert_card = tr("Common", "InsertCard");
    let press_start = tr("Common", "PressStart");
    let event_mode = tr("Common", "EventMode");

    let (footer_left, left_avatar) = if p1.joined {
        (
            Some(if p1.guest {
                insert_card.as_ref()
            } else {
                p1.display_name
            }),
            if p1.guest { None } else { p1_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let (footer_right, right_avatar) = if p2.joined {
        (
            Some(if p2.guest {
                insert_card.as_ref()
            } else {
                p2.display_name
            }),
            if p2.guest { None } else { p2_avatar },
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
        visual_policy,
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
        visual_policy,
    }));
}

pub fn build_stage_display(stage_number: usize, machine_font: MachineFont) -> Actor {
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
                font(machine_font_key(machine_font, FontRole::Normal)):
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
