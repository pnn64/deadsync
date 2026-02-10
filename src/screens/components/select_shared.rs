use crate::act;
use crate::core::space::{screen_center_x, screen_width, widescale};
use crate::game::profile;
use crate::screens::components::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::ui::actors::Actor;

pub fn build_screen_bars(top_title: &'static str) -> Vec<Actor> {
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

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };

    vec![
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
            title: "EVENT MODE",
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

pub fn build_session_timer(text: String) -> Actor {
    build_header_timer(text, screen_center_x())
}

pub fn build_gameplay_timer(text: String) -> Actor {
    build_header_timer(text, screen_center_x() + widescale(150.0, 200.0))
}

fn build_header_timer(text: String, x: f32) -> Actor {
    act!(text:
        font("wendy_monospace_numbers"):
        settext(text):
        align(0.5, 0.5):
        xy(x, 10.0):
        zoom(widescale(0.3, 0.36)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    )
}

pub fn build_mode_pad_text(text: &str) -> Actor {
    act!(text:
        font("wendy"):
        settext(text):
        align(1.0, 0.5):
        xy(screen_width() - widescale(55.0, 62.0), 15.0):
        zoom(widescale(0.5, 0.6)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0)
    )
}
