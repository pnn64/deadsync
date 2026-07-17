use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_center_x, widescale};

pub fn build_session(text: impl Into<TextContent>, machine_font: MachineFont) -> Actor {
    build_header_timer(text, screen_center_x(), machine_font)
}

pub fn build_gameplay(text: impl Into<TextContent>, machine_font: MachineFont) -> Actor {
    build_header_timer(
        text,
        screen_center_x() + widescale(150.0, 200.0),
        machine_font,
    )
}

fn build_header_timer(text: impl Into<TextContent>, x: f32, machine_font: MachineFont) -> Actor {
    let text = text.into();
    act!(text:
        font(machine_font_key(machine_font, FontRole::Numbers)):
        settext(text):
        align(0.5, 0.5):
        xy(x, 10.0):
        zoom(widescale(0.3, 0.36)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    )
}
