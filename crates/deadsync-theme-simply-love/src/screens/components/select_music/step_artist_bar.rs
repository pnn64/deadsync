use crate::act;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::screen_height;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepArtistBarLayout {
    Legacy,   // one line, standard layout
    Expanded, // for arrow-cloud, multi-line
}

impl From<bool> for StepArtistBarLayout {
    fn from(expanded: bool) -> Self {
        Self::from_bool(expanded)
    }
}

impl StepArtistBarLayout {
    pub(crate) const fn from_bool(expanded: bool) -> Self {
        if expanded {
            Self::Expanded
        } else {
            Self::Legacy
        }
    }

    pub(crate) const fn height_screen_divisor(self) -> f32 {
        match self {
            Self::Legacy => 28.0,
            Self::Expanded => 8.0,
        }
    }
}

pub struct StepArtistBarParams {
    pub left_x: f32,
    pub center_y: f32,
    pub layout: StepArtistBarLayout,
    pub expanded_line_count: usize,
    pub accent_color: [f32; 4],
    pub z_base: i16,
    pub background_width: f32,
    pub label_text: TextContent,
    pub label_max_width: f32,
    pub artist_text: TextContent,
    pub artist_x_offset: f32,
    pub artist_max_width: f32,
    pub artist_color: [f32; 4],
}

pub struct StepArtistBarGeometry {
    pub background_y: f32,
    pub background_height: f32,
    pub artist_text_y: f32,
}

pub fn push(out: &mut Vec<Actor>, p: StepArtistBarParams) {
    let background_height = screen_height()
        / match p.layout {
            StepArtistBarLayout::Legacy => 28.0,
            StepArtistBarLayout::Expanded => 8.0,
        };
    let background_center_y = p.center_y
        + match p.layout {
            StepArtistBarLayout::Legacy => 0.0,
            StepArtistBarLayout::Expanded => 18.0,
        };
    let artist_text_y = p.center_y
        - match p.layout {
            StepArtistBarLayout::Legacy => 0.0,
            StepArtistBarLayout::Expanded => 6.0,
        };
    push_with_geometry(
        out,
        p,
        StepArtistBarGeometry {
            background_y: background_center_y - background_height * 0.5,
            background_height,
            artist_text_y,
        },
    );
}

pub fn push_with_geometry(
    out: &mut Vec<Actor>,
    p: StepArtistBarParams,
    geometry: StepArtistBarGeometry,
) {
    let z_text = p.z_base.saturating_add(1);
    match p.layout {
        StepArtistBarLayout::Legacy => {
            out.push(act!(quad:
                align(0.5, 0.5):
                xy(p.left_x + p.background_width * 0.5, geometry.background_y + geometry.background_height * 0.5):
                setsize(p.background_width, geometry.background_height):
                z(p.z_base):
                diffuse(p.accent_color[0], p.accent_color[1], p.accent_color[2], 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext(p.label_text):
                align(0.0, 0.5):
                xy(p.left_x + 4.5, p.center_y):
                maxwidth(p.label_max_width):
                zoom(0.8):
                z(z_text):
                diffuse(0.0, 0.0, 0.0, 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext(p.artist_text):
                align(0.0, 0.5):
                xy(p.left_x + p.artist_x_offset, geometry.artist_text_y):
                maxwidth(p.artist_max_width):
                zoom(0.8):
                z(z_text):
                diffuse(
                    p.artist_color[0],
                    p.artist_color[1],
                    p.artist_color[2],
                    p.artist_color[3]
                )
            ));
        }
        StepArtistBarLayout::Expanded => {
            out.push(act!(quad:
                align(0.5, 0.5):
                xy(p.left_x + p.background_width * 0.5, geometry.background_y + geometry.background_height * 0.5):
                setsize(p.background_width, geometry.background_height):
                z(p.z_base):
                diffuse(p.accent_color[0], p.accent_color[1], p.accent_color[2], 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext(p.label_text):
                align(0.0, 0.5):
                xy(p.left_x + 5.0, p.center_y):
                maxwidth(p.label_max_width):
                zoom(0.8):
                z(z_text):
                diffuse(0.0, 0.0, 0.0, 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext(p.artist_text):
                align(0.0, 0.0):
                xy(p.left_x + p.artist_x_offset, geometry.artist_text_y):
                maxwidth(p.artist_max_width):
                zoom(0.8):
                z(z_text):
                diffuse(0.0, 0.0, 0.0, 1.0)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::TextContent;

    fn params(layout: StepArtistBarLayout) -> StepArtistBarParams {
        StepArtistBarParams {
            left_x: 32.0,
            center_y: 0.0,
            layout,
            expanded_line_count: 3,
            accent_color: [1.0, 0.0, 0.0, 1.0],
            z_base: 10,
            background_width: match layout {
                StepArtistBarLayout::Legacy => 175.0,
                StepArtistBarLayout::Expanded => 190.0,
            },
            label_text: TextContent::Static("STEPS"),
            label_max_width: 40.0,
            artist_text: TextContent::Static("a very long chart description"),
            artist_x_offset: 49.5,
            artist_max_width: match layout {
                StepArtistBarLayout::Legacy => 124.0,
                StepArtistBarLayout::Expanded => 175.0,
            },
            artist_color: [0.0, 0.0, 0.0, 1.0],
        }
    }

    fn text_width_flags(actors: &[Actor]) -> Vec<(Option<f32>, bool)> {
        actors
            .iter()
            .filter_map(|actor| {
                if let Actor::Text {
                    max_width,
                    max_w_pre_zoom,
                    ..
                } = actor
                {
                    Some((*max_width, *max_w_pre_zoom))
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn legacy_step_artist_maxwidth_is_unzoomed_like_itgmania() {
        let mut actors = Vec::new();
        push(&mut actors, params(StepArtistBarLayout::Legacy));

        assert_eq!(
            text_width_flags(&actors),
            vec![(Some(40.0), true), (Some(124.0), true)]
        );
    }

    #[test]
    fn expanded_step_artist_maxwidth_is_unzoomed_like_arrow_cloud() {
        let mut actors = Vec::new();
        push(&mut actors, params(StepArtistBarLayout::Expanded));

        assert_eq!(
            text_width_flags(&actors),
            vec![(Some(40.0), true), (Some(175.0), true)]
        );
    }

    #[test]
    fn step_header_background_starts_at_its_layout_left_edge() {
        for (layout, expected_width, expected_center_offset) in [
            (StepArtistBarLayout::Legacy, 175.0, 87.5),
            (StepArtistBarLayout::Expanded, 190.0, 95.0),
        ] {
            let mut actors = Vec::new();
            push(&mut actors, params(layout));
            let Actor::Sprite { offset, size, .. } = &actors[0] else {
                panic!("step header background should be a sprite");
            };
            let deadlib_present::actors::SizeSpec::Px(width) = size[0] else {
                panic!("step header background should have a fixed width");
            };

            assert_eq!(width, expected_width);
            assert_eq!(offset[0], 32.0 + expected_center_offset);
            assert_eq!(offset[0] - width * 0.5, 32.0);
        }
    }

    #[test]
    fn legacy_bar_uses_the_width_allocated_beside_peak_info() {
        let mut p = params(StepArtistBarLayout::Legacy);
        p.background_width = 220.0;
        p.artist_x_offset = 52.0;
        p.artist_max_width = 150.0;
        let mut actors = Vec::new();

        push(&mut actors, p);

        let Actor::Sprite { offset, size, .. } = &actors[0] else {
            panic!("step header background should be a sprite");
        };
        let deadlib_present::actors::SizeSpec::Px(width) = size[0] else {
            panic!("step header background should have a fixed width");
        };
        assert_eq!(width, 220.0);
        assert_eq!(offset[0], 32.0 + 110.0);
        assert_eq!(
            text_width_flags(&actors),
            vec![(Some(40.0), true), (Some(150.0), true)]
        );
    }
}
