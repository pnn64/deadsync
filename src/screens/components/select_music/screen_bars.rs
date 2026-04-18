use crate::assets::i18n::tr;
use crate::engine::present::actors::Actor;
use crate::game::profile;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};

pub fn build(top_title: &str) -> [Actor; 2] {
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

    [
        screen_bar::build(ScreenBarParams {
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
        }),
        screen_bar::build(ScreenBarParams {
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
        }),
    ]
}
