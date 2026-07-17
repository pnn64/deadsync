use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::pad_display;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_width, widescale};
use deadsync_profile::PlayStyle;

pub fn build_label(text: String, machine_font: MachineFont) -> Actor {
    act!(text:
        font(machine_font_key(machine_font, FontRole::Header)):
        settext(text):
        align(1.0, 0.5):
        xy(screen_width() - widescale(55.0, 62.0), 15.0):
        zoom(widescale(0.5, 0.6)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0)
    )
}

fn states(play_style: PlayStyle, joined: [bool; 2]) -> [bool; 2] {
    if play_style == PlayStyle::Double {
        return [true, true];
    }
    joined
}

pub fn build(play_style: PlayStyle, joined: [bool; 2]) -> [Actor; 2] {
    let [p1_active, p2_active] = states(play_style, joined);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_activates_both_pads_and_other_styles_follow_joined_sides() {
        assert_eq!(states(PlayStyle::Double, [true, false]), [true, true]);
        assert_eq!(states(PlayStyle::Single, [false, true]), [false, true]);
        assert_eq!(states(PlayStyle::Versus, [true, true]), [true, true]);
    }
}
