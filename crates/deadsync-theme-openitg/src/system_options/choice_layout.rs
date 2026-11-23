use std::sync::Arc;

use deadlib_present::font::{self, Font, FontMap};

#[derive(Clone, Copy)]
pub(super) struct TextMetrics<'a> {
    fonts: &'a FontMap,
    font: Option<&'a Font>,
}

impl<'a> TextMetrics<'a> {
    pub(super) fn new(fonts: &'a FontMap) -> Self {
        Self {
            fonts,
            font: fonts.get("miso"),
        }
    }

    fn chain_key(self) -> Option<u64> {
        self.font.map(|font| font.chain_key)
    }

    pub(super) fn width(self, text: &str) -> f32 {
        let text = font::replace_markers(text);
        let width = self.font.map_or_else(
            || text.chars().count() as i32 * 8,
            |font| {
                text.split('\n')
                    .map(|line| font::measure_line_width_logical(font, line, self.fonts))
                    .max()
                    .unwrap_or_default()
            },
        );
        (width + (width & 1)) as f32
    }
}

#[derive(Default)]
pub(super) struct ChoiceLayout {
    choices: Option<Arc<[Arc<str>]>>,
    font: Option<u64>,
    widths: Vec<f32>,
    total_width: f32,
}

impl ChoiceLayout {
    pub(super) fn update(&mut self, choices: &Arc<[Arc<str>]>, metrics: TextMetrics<'_>) {
        let font = metrics.chain_key();
        if self.font == font
            && self
                .choices
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, choices))
        {
            return;
        }
        self.font = font;
        self.choices = Some(Arc::clone(choices));
        self.widths.clear();
        self.widths
            .extend(choices.iter().map(|choice| metrics.width(choice)));
        self.total_width = self.widths.iter().sum();
    }

    pub(super) fn widths(&self) -> &[f32] {
        &self.widths
    }

    pub(super) fn total_width(&self) -> f32 {
        self.total_width
    }
}

#[derive(Default)]
pub(super) struct CursorLabel {
    label: Option<Arc<str>>,
    font: Option<u64>,
    width: f32,
}

impl CursorLabel {
    pub(super) fn width(&mut self, label: &Arc<str>, metrics: TextMetrics<'_>) -> f32 {
        let font = metrics.chain_key();
        if self.font != font
            || self
                .label
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, label))
        {
            self.font = font;
            self.width = metrics.width(label);
            self.label = Some(Arc::clone(label));
        }
        self.width
    }
}
