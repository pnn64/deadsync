use crate::act;
use deadlib_present::actors::Actor;
use std::sync::Arc;

pub(crate) struct Params<'a> {
    pub layout: super::layout::CourseSummaryLayout,
    pub songs_label: &'a Arc<str>,
    pub songs_value: &'a Arc<str>,
    pub bpm_label: &'a Arc<str>,
    pub bpm_value: &'a Arc<str>,
    pub length_label: &'a Arc<str>,
    pub length_value: &'a Arc<str>,
    pub background_color: [f32; 4],
}

pub(crate) fn push(actors: &mut Vec<Actor>, params: Params<'_>) {
    let Params {
        layout,
        songs_label,
        songs_value,
        bpm_label,
        bpm_value,
        length_label,
        length_value,
        background_color,
    } = params;
    let scale = layout.scale;
    let left = layout.center_x - layout.width * 0.5;
    let right = layout.center_x + layout.width * 0.5;

    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(layout.center_x, layout.center_y):
        setsize(layout.width, layout.height):
        z(51):
        diffuse(
            background_color[0],
            background_color[1],
            background_color[2],
            background_color[3]
        )
    ));

    // magic numbers galore, these are used for positioning the text elements within the course summary box
    for (text, x, y, align, max_width, diffuse) in [
        (
            songs_label,
            left + 58.0 * scale,
            layout.center_y - 10.0 * scale,
            [1.0, 0.5],
            56.0 * scale,
            [0.5, 0.5, 0.5, 1.0],
        ),
        (
            songs_value,
            left + 63.0 * scale,
            layout.center_y - 10.0 * scale,
            [0.0, 0.5],
            40.0 * scale,
            [1.0, 1.0, 1.0, 1.0],
        ),
        (
            bpm_label,
            left + 43.0 * scale,
            layout.center_y + 10.0 * scale,
            [1.0, 0.5],
            40.0 * scale,
            [0.5, 0.5, 0.5, 1.0],
        ),
        (
            bpm_value,
            left + 48.0 * scale,
            layout.center_y + 10.0 * scale,
            [0.0, 0.5],
            84.0 * scale,
            [1.0, 1.0, 1.0, 1.0],
        ),
        (
            length_label,
            right - 91.0 * scale,
            layout.center_y + 10.0 * scale,
            [1.0, 0.5],
            64.0 * scale,
            [0.5, 0.5, 0.5, 1.0],
        ),
        (
            length_value,
            right - 86.0 * scale,
            layout.center_y + 10.0 * scale,
            [0.0, 0.5],
            78.0 * scale,
            [1.0, 1.0, 1.0, 1.0],
        ),
    ] {
        actors.push(act!(text:
            font("miso"):
            settext(Arc::clone(text)):
            align(align[0], align[1]):
            xy(x, y):
            zoom(scale):
            maxwidth(max_width):
            z(52):
            diffuse(diffuse[0], diffuse[1], diffuse[2], diffuse[3])
        ));
    }
}
