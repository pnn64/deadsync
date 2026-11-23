use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextAlign};
use deadsync_theme::views::{SystemOptionsPage, SystemOptionsPhase};

use crate::option_explanation::{Explanation, ExplanationPlacement, fit_zoom};
use crate::presentation::Layout;

pub(super) const DEFAULT_PANEL_BOTTOM: f32 = 426.0;
const MAX_PANEL_BOTTOM: f32 = 470.0;
const PLACEMENT: [f32; 2] = [54.0, 381.0];
const HELP_WIDTH: f32 = 548.0;
const BASE_ZOOM: f32 = 0.86;
const WRAP_WIDTH: i32 = (HELP_WIDTH / BASE_ZOOM) as i32;
const PADDING: f32 = 7.0;
const DETAIL_GAP: f32 = 6.0;

#[derive(Default)]
pub(super) struct OptionsHelp {
    description: Option<Explanation>,
    detail: Option<Explanation>,
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
            self.description = page.description.as_ref().map(Explanation::new);
        }
        if detail_changed {
            self.detail = page.detail.as_ref().map(Explanation::new);
        }
        if new_page || description_changed || detail_changed {
            self.fit(assets);
        }
    }

    fn fit(&mut self, assets: &AssetManager) {
        let measure = |text: &Option<Explanation>| {
            text.as_ref()
                .map_or([0; 2], |text| text.measure(assets.fonts(), WRAP_WIDTH))
        };
        let [description, detail] = [measure(&self.description), measure(&self.detail)];
        let gap = if self.description.is_some() && self.detail.is_some() {
            DETAIL_GAP
        } else {
            0.0
        };
        let available_height = MAX_PANEL_BOTTOM - PLACEMENT[1] - PADDING - gap;
        self.zoom = fit_zoom(
            [description[0].max(detail[0]), description[1] + detail[1]],
            [HELP_WIDTH, available_height],
            BASE_ZOOM,
        );
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
            actors.push(description.actor(
                layout,
                self.placement(PLACEMENT[1]),
                colors[0],
                now - self.started,
            ));
        }
        if let Some(detail) = &self.detail {
            actors.push(detail.actor(
                layout,
                self.placement(self.detail_y),
                colors[1],
                f64::INFINITY,
            ));
        }
    }

    fn placement(&self, y: f32) -> ExplanationPlacement {
        ExplanationPlacement {
            position: [PLACEMENT[0], y],
            align: TextAlign::Left,
            zoom: self.zoom,
            wrap_width: WRAP_WIDTH,
        }
    }
}
