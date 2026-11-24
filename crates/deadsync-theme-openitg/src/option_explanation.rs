use std::borrow::Cow;
use std::sync::Arc;

use deadlib_present::actors::{Actor, TextAlign, TextAttributes, TextContent};
use deadlib_present::{compose::measure_wrapped_text, font};

use crate::presentation::Layout;

const REVEAL_SECONDS: f64 = 0.5;

pub(crate) struct Explanation {
    pub(crate) source: Arc<str>,
    text: Arc<str>,
}

pub(crate) struct ExplanationPlacement {
    pub(crate) position: [f32; 2],
    pub(crate) align: TextAlign,
    pub(crate) zoom: f32,
    pub(crate) wrap_width: i32,
    pub(crate) font: &'static str,
}

impl Explanation {
    pub(crate) fn new(source: &Arc<str>) -> Self {
        let text = match font::replace_markers(source) {
            Cow::Borrowed(_) => Arc::clone(source),
            Cow::Owned(text) => Arc::from(text),
        };
        Self {
            source: Arc::clone(source),
            text,
        }
    }

    pub(crate) fn measure(
        &self,
        fonts: &font::FontMap,
        face: &'static str,
        wrap_width: i32,
    ) -> [i32; 2] {
        fonts.get(face).map_or([0; 2], |font| {
            measure_wrapped_text(font, fonts, &self.text, wrap_width)
        })
    }

    pub(crate) fn actor(
        &self,
        layout: Layout,
        placement: ExplanationPlacement,
        color: [f32; 4],
        elapsed: f64,
    ) -> Actor {
        let alignment = match placement.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => 0.5,
            TextAlign::Right => 1.0,
        };
        let mut text = layout.text(
            placement.position,
            alignment,
            TextContent::Shared(Arc::clone(&self.text)),
            placement.zoom,
            color,
        );
        text.font(placement.font);
        text.valign(0.0);
        text.horizalign(placement.align);
        text.wrapwidthpixels(placement.wrap_width as f32);
        let mut actor = text.build(0);
        let fraction = (elapsed / REVEAL_SECONDS).clamp(0.0, 1.0) as f32;
        if fraction < 1.0
            && let Actor::Text { attributes, .. } = &mut actor
        {
            *attributes = TextAttributes::GlyphReveal(fraction);
        }
        actor
    }
}

pub(crate) fn fit_zoom(
    [width, height]: [i32; 2],
    [max_width, max_height]: [f32; 2],
    zoom: f32,
) -> f32 {
    zoom.min(max_width / width.max(1) as f32)
        .min(max_height / height.max(1) as f32)
}
