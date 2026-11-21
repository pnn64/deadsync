use crate::act;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::screen_height;

const ARTIST_TEXT_TOP_INSET: f32 = 3.0;
const STEP_LABEL_X_OFFSET: f32 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepArtistBarLayoutType {
    Legacy,   // one line, standard layout
    Expanded, // for arrow-cloud, multi-line
}

impl From<bool> for StepArtistBarLayoutType {
    fn from(expanded: bool) -> Self {
        Self::from_bool(expanded)
    }
}

impl StepArtistBarLayoutType {
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
            Self::Expanded => 9.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepArtistBarLayout {
    pub background_width: f32,
    pub background_y: f32,
    pub background_height: f32,
    pub artist_text_y: f32,
    pub artist_x_offset: f32,
    pub artist_max_width: f32,
    pub max_lines: usize,
}

impl StepArtistBarLayout {
    pub(crate) fn new(
        background_width: f32,
        background_y: f32,
        background_height: f32,
        artist_x_offset: f32,
        artist_max_width: f32,
        max_lines: usize,
    ) -> Self {
        Self {
            background_width,
            background_y,
            background_height,
            artist_text_y: background_y + ARTIST_TEXT_TOP_INSET,
            artist_x_offset,
            artist_max_width,
            max_lines,
        }
    }

    // Used by select course - we can change the select_course code
    // to provide a layout instead of relying on this helper
    pub(crate) fn legacy(
        step_header_y: f32,
        background_width: f32,
        artist_x_offset: f32,
        artist_max_width: f32,
    ) -> Self {
        let background_height =
            screen_height() / StepArtistBarLayoutType::Legacy.height_screen_divisor();
        Self::new(
            background_width,
            step_header_y - background_height * 0.5,
            background_height,
            artist_x_offset,
            artist_max_width,
            1,
        )
    }

    // Fade the bottom of the box depending
    // on how many lines of text are present/fit
    fn fade_bottom(self, line_count: usize) -> f32 {
        if line_count == 0 {
            return 0.0;
        }

        match self.max_lines.saturating_sub(line_count) {
            2 => 0.8,
            1 => 0.5,
            _ => 0.0,
        }
    }
}

pub struct StepArtistBarParams {
    pub left_x: f32,
    pub layout: StepArtistBarLayout,
    pub accent_color: [f32; 4],
    pub z_base: i16,
    pub label_text: TextContent,
    pub label_max_width: f32,
    pub artist_text: TextContent,
    pub artist_color: [f32; 4],
}

pub fn push(out: &mut Vec<Actor>, p: StepArtistBarParams) {
    let z_text = p.z_base.saturating_add(1);
    let fade_bottom = p.layout.fade_bottom(p.artist_text.as_str().lines().count());
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(p.left_x + p.layout.background_width * 0.5, p.layout.background_y + p.layout.background_height * 0.5):
        setsize(p.layout.background_width, p.layout.background_height):
        fadebottom(fade_bottom):
        z(p.z_base):
        diffuse(p.accent_color[0], p.accent_color[1], p.accent_color[2], 1.0)
    ));
    out.push(act!(text:
        font("miso"):
        settext(p.label_text):
        align(0.0, 0.0):
        xy(
            p.left_x + STEP_LABEL_X_OFFSET,
            p.layout.background_y + ARTIST_TEXT_TOP_INSET
        ):
        maxwidth(p.label_max_width):
        zoom(0.8):
        z(z_text):
        diffuse(0.0, 0.0, 0.0, 1.0)
    ));
    out.push(act!(text:
        font("miso"):
        settext(p.artist_text):
        align(0.0, 0.0):
        xy(p.left_x + p.layout.artist_x_offset, p.layout.artist_text_y):
        maxwidth(p.layout.artist_max_width):
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::TextContent;

    fn params(layout_type: StepArtistBarLayoutType) -> StepArtistBarParams {
        let background_width = match layout_type {
            StepArtistBarLayoutType::Legacy => 175.0,
            StepArtistBarLayoutType::Expanded => 190.0,
        };
        let artist_x_offset = 49.5;
        let artist_max_width = match layout_type {
            StepArtistBarLayoutType::Legacy => 124.0,
            StepArtistBarLayoutType::Expanded => 175.0,
        };
        let layout = StepArtistBarLayout::new(
            background_width,
            0.0,
            80.0,
            artist_x_offset,
            artist_max_width,
            match layout_type {
                StepArtistBarLayoutType::Legacy => 1,
                StepArtistBarLayoutType::Expanded => 3,
            },
        );
        StepArtistBarParams {
            left_x: 32.0,
            layout,
            accent_color: [1.0, 0.0, 0.0, 1.0],
            z_base: 10,
            label_text: TextContent::Static("STEPS"),
            label_max_width: 40.0,
            artist_text: TextContent::Static("a very long chart description"),
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
        push(&mut actors, params(StepArtistBarLayoutType::Legacy));

        assert_eq!(
            text_width_flags(&actors),
            vec![(Some(40.0), true), (Some(124.0), true)]
        );
    }

    #[test]
    fn expanded_step_artist_maxwidth_is_unzoomed_like_arrow_cloud() {
        let mut actors = Vec::new();
        push(&mut actors, params(StepArtistBarLayoutType::Expanded));

        assert_eq!(
            text_width_flags(&actors),
            vec![(Some(40.0), true), (Some(175.0), true)]
        );
    }

    #[test]
    fn background_fade_depends_on_rendered_text_lines() {
        for (layout_type, text, expected_fade) in [
            (StepArtistBarLayoutType::Legacy, "one line", 0.0),
            (StepArtistBarLayoutType::Expanded, "one\n", 0.8),
            (StepArtistBarLayoutType::Expanded, "one\ntwo\n", 0.5),
            (StepArtistBarLayoutType::Expanded, "one\ntwo\nthree\n", 0.0),
            (StepArtistBarLayoutType::Expanded, "", 0.0),
        ] {
            let mut p = params(layout_type);
            p.artist_text = TextContent::Static(text);
            let mut actors = Vec::new();

            push(&mut actors, p);

            let Actor::Sprite { fadebottom, .. } = actors[0] else {
                panic!("expanded step artist background should be a sprite");
            };
            assert_eq!(fadebottom, expected_fade);
        }
    }

    #[test]
    fn artist_text_is_offset_from_the_background_top() {
        let mut p = params(StepArtistBarLayoutType::Expanded);
        p.layout.background_y = 100.0;
        p.layout.artist_text_y = 104.0;
        let mut actors = Vec::new();

        push(&mut actors, p);

        let Actor::Text { align, offset, .. } = &actors[1] else {
            panic!("expanded step artist label should be text");
        };
        assert_eq!(*align, [0.0, 0.0]);
        assert_eq!(offset[0], 32.0 + STEP_LABEL_X_OFFSET);
        assert_eq!(offset[1], 100.0 + ARTIST_TEXT_TOP_INSET);

        let Actor::Text { offset, .. } = &actors[2] else {
            panic!("expanded artist metadata should be text");
        };
        assert_eq!(offset[1], 104.0);
    }

    #[test]
    fn label_is_top_left_and_pinned_to_background_top() {
        for layout_type in [
            StepArtistBarLayoutType::Legacy,
            StepArtistBarLayoutType::Expanded,
        ] {
            let mut p = params(layout_type);
            p.layout.background_y = 100.0;
            let mut actors = Vec::new();
            push(&mut actors, p);

            let Actor::Text { align, offset, .. } = &actors[1] else {
                panic!("step artist label should be text");
            };
            assert_eq!(*align, [0.0, 0.0]);
            assert_eq!(offset[0], 32.0 + STEP_LABEL_X_OFFSET);
            assert_eq!(offset[1], 100.0 + ARTIST_TEXT_TOP_INSET);
        }
    }

    #[test]
    fn step_header_background_starts_at_its_layout_left_edge() {
        for (layout, expected_width, expected_center_offset) in [
            (StepArtistBarLayoutType::Legacy, 175.0, 87.5),
            (StepArtistBarLayoutType::Expanded, 190.0, 95.0),
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
        let mut p = params(StepArtistBarLayoutType::Legacy);
        p.layout.background_width = 220.0;
        p.layout.artist_x_offset = 52.0;
        p.layout.artist_max_width = 150.0;
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
