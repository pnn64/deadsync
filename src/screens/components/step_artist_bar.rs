use crate::act;
use crate::core::space::screen_height;
use crate::ui::actors::Actor;

pub struct StepArtistBarParams<'a> {
    pub x0: f32,
    pub center_y: f32,
    pub accent_color: [f32; 4],
    pub z_base: i16,
    pub label_text: &'a str,
    pub label_max_width: f32,
    pub artist_text: &'a str,
    pub artist_x_offset: f32,
    pub artist_max_width: f32,
    pub artist_color: [f32; 4],
}

pub fn build(p: StepArtistBarParams<'_>) -> Vec<Actor> {
    let comp_h = screen_height() / 28.0;
    let z_text = p.z_base.saturating_add(1);
    vec![
        act!(quad:
            align(0.5, 0.5):
            xy(p.x0 + 113.0, p.center_y):
            setsize(175.0, comp_h):
            z(p.z_base):
            diffuse(p.accent_color[0], p.accent_color[1], p.accent_color[2], 1.0)
        ),
        act!(text:
            font("miso"):
            settext(p.label_text):
            align(0.0, 0.5):
            xy(p.x0 + 30.0, p.center_y):
            zoom(0.8):
            maxwidth(p.label_max_width):
            z(z_text):
            diffuse(0.0, 0.0, 0.0, 1.0)
        ),
        act!(text:
            font("miso"):
            settext(p.artist_text):
            align(0.0, 0.5):
            xy(p.x0 + p.artist_x_offset, p.center_y):
            zoom(0.8):
            maxwidth(p.artist_max_width):
            z(z_text):
            diffuse(
                p.artist_color[0],
                p.artist_color[1],
                p.artist_color[2],
                p.artist_color[3]
            )
        ),
    ]
}
