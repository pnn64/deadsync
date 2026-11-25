use deadlib_assets::AssetView;

use crate::views::{
    SystemOptionsPhase, SystemOptionsRenderer, SystemOptionsServiceRow, SystemOptionsTiming,
};

#[derive(Clone, Copy, Default)]
pub struct SystemOptionsLayout {
    pub split_graphics: bool,
    pub service_order: &'static [SystemOptionsServiceRow],
    pub timing: Option<SystemOptionsTiming>,
    pub return_overscan_to_graphics: bool,
}

pub trait SystemOptionsPresentation<A> {
    fn layout(&self) -> SystemOptionsLayout;
    fn reset(&mut self);
    fn with_renderer(
        &mut self,
        phase: SystemOptionsPhase,
        now_seconds: f64,
        assets: &dyn AssetView,
        draw: &mut dyn FnMut(SystemOptionsRenderer<'_, A>),
    );
}
