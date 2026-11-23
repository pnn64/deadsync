use std::borrow::Cow;
use std::sync::Arc;

use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextAttributes, TextContent};
use deadlib_present::{compose::measure_wrapped_text, font};
use deadsync_theme::views::{SystemOptionsPage, SystemOptionsPhase};

use crate::presentation::Layout;

pub(super) const DEFAULT_PANEL_BOTTOM: f32 = 426.0;
const MAX_PANEL_BOTTOM: f32 = 470.0;
const PLACEMENT: [f32; 2] = [54.0, 381.0];
const HELP_WIDTH: f32 = 548.0;
const BASE_ZOOM: f32 = 0.86;
const WRAP_WIDTH: i32 = (HELP_WIDTH / BASE_ZOOM) as i32;
const PADDING: f32 = 7.0;
const DETAIL_GAP: f32 = 6.0;
const REVEAL_SECONDS: f64 = 0.5;

struct Paragraph {
    source: Arc<str>,
    text: Arc<str>,
}

impl Paragraph {
    fn new(source: &Arc<str>) -> Self {
        let text = match font::replace_markers(source) {
            Cow::Borrowed(_) => Arc::clone(source),
            Cow::Owned(text) => Arc::from(text),
        };
        Self {
            source: Arc::clone(source),
            text,
        }
    }
}

#[derive(Default)]
pub(super) struct OptionsHelp {
    description: Option<Paragraph>,
    detail: Option<Paragraph>,
    started: f64,
    zoom: f32,
    detail_y: f32,
    panel_bottom: f32,
}

impl OptionsHelp {
    pub(super) fn update(
        &mut self,
        assets: &AssetManager,
        page: &SystemOptionsPage<'_>,
        new_page: bool,
        now: f64,
    ) {
        let description_changed =
            self.description.as_ref().map(|text| &text.source) != page.description.as_ref();
        let detail_changed = self.detail.as_ref().map(|text| &text.source) != page.detail.as_ref();
        if new_page || description_changed {
            self.started = match page.phase {
                SystemOptionsPhase::Entering(elapsed) if new_page => now - f64::from(elapsed),
                _ => now,
            };
        }
        if description_changed {
            self.description = page.description.as_ref().map(Paragraph::new);
        }
        if detail_changed {
            self.detail = page.detail.as_ref().map(Paragraph::new);
        }
        if new_page || description_changed || detail_changed {
            self.fit(assets);
        }
    }

    fn fit(&mut self, assets: &AssetManager) {
        let [description, detail] = assets.with_fonts(|fonts| {
            let measure = |text: &Option<Paragraph>| match (fonts.get("miso"), text) {
                (Some(font), Some(text)) => {
                    measure_wrapped_text(font, fonts, &text.text, WRAP_WIDTH)
                }
                _ => [0; 2],
            };
            [measure(&self.description), measure(&self.detail)]
        });
        let gap = if self.description.is_some() && self.detail.is_some() {
            DETAIL_GAP
        } else {
            0.0
        };
        let width = description[0].max(detail[0]).max(1) as f32;
        let height = (description[1] + detail[1]).max(1) as f32;
        let available_height = MAX_PANEL_BOTTOM - PLACEMENT[1] - PADDING - gap;
        self.zoom = BASE_ZOOM
            .min(HELP_WIDTH / width)
            .min(available_height / height);
        self.detail_y = PLACEMENT[1] + description[1] as f32 * self.zoom + gap;
        self.panel_bottom = (self.detail_y + detail[1] as f32 * self.zoom + PADDING)
            .clamp(DEFAULT_PANEL_BOTTOM, MAX_PANEL_BOTTOM);
    }

    pub(super) fn panel_bottom(&self) -> f32 {
        self.panel_bottom.max(DEFAULT_PANEL_BOTTOM)
    }

    pub(super) fn push(
        &self,
        actors: &mut Vec<Actor>,
        layout: Layout,
        colors: [[f32; 4]; 2],
        now: f64,
    ) {
        if let Some(description) = &self.description {
            let fraction = ((now - self.started) / REVEAL_SECONDS).clamp(0.0, 1.0) as f32;
            let mut paragraph = self.paragraph(layout, PLACEMENT[1], description, colors[0]);
            if fraction < 1.0
                && let Actor::Text { attributes, .. } = &mut paragraph
            {
                *attributes = TextAttributes::GlyphReveal(fraction);
            }
            actors.push(paragraph);
        }
        if let Some(detail) = &self.detail {
            actors.push(self.paragraph(layout, self.detail_y, detail, colors[1]));
        }
    }

    fn paragraph(&self, layout: Layout, y: f32, text: &Paragraph, color: [f32; 4]) -> Actor {
        let content = TextContent::Shared(Arc::clone(&text.text));
        let mut paragraph = layout.text([PLACEMENT[0], y], 0.0, content, self.zoom, color);
        paragraph.valign(0.0);
        paragraph.wrapwidthpixels(WRAP_WIDTH as f32);
        paragraph.build(0)
    }
}
