use crate::act;
use crate::engine::space::{screen_center_x, widescale};
use crate::engine::present::actors::{Actor, TextContent};

pub fn build_session(text: impl Into<TextContent>) -> Actor {
    build_header_timer(text, screen_center_x())
}

pub fn build_gameplay(text: impl Into<TextContent>) -> Actor {
    build_header_timer(text, screen_center_x() + widescale(150.0, 200.0))
}

fn build_header_timer(text: impl Into<TextContent>, x: f32) -> Actor {
    let text = text.into();
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
