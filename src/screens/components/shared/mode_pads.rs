use crate::act;
use crate::assets::{FontRole, current_theme_font_key};
use crate::engine::present::actors::Actor;
use crate::engine::space::{screen_width, widescale};
use crate::game::profile;
use crate::screens::components::shared::pad_display;

pub fn build_label(text: String) -> Actor {
    act!(text:
        font(current_theme_font_key(FontRole::Header)):
        settext(text):
        align(1.0, 0.5):
        xy(screen_width() - widescale(55.0, 62.0), 15.0):
        zoom(widescale(0.5, 0.6)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0)
    )
}

fn states() -> [bool; 2] {
    if profile::get_session_play_style() == profile::PlayStyle::Double {
        return [true, true];
    }
    [
        profile::is_session_side_joined(profile::PlayerSide::P1),
        profile::is_session_side_joined(profile::PlayerSide::P2),
    ]
}

pub fn build() -> [Actor; 2] {
    let [p1_active, p2_active] = states();
    let pad_zoom = 0.24 * widescale(0.435, 0.525);
    [
        pad_display::build(pad_display::PadDisplayParams {
            center_x: screen_width() - widescale(35.0, 41.0),
            center_y: widescale(22.0, 23.5),
            zoom: pad_zoom,
            z: 121,
            is_active: p1_active,
        }),
        pad_display::build(pad_display::PadDisplayParams {
            center_x: screen_width() - widescale(15.0, 17.0),
            center_y: widescale(22.0, 23.5),
            zoom: pad_zoom,
            z: 121,
            is_active: p2_active,
        }),
    ]
}
