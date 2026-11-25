use crate::color;
use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::actors::SpriteSource;
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_notefield::{
    BrokenRunLookup, ComboMilestoneAssets, ComboMilestoneSprite,
    NotefieldComposeRequest, NotefieldHudComposeResult, NotefieldHudFrameView,
    PreparedNotefield, StreamProgressLookup,
};
use deadsync_profile as profile_data;
use deadsync_song_lua::gameplay_presentation::FieldHudBuffers;
use deadsync_song_lua::playback::FieldFrame;
use deadsync_assets::gameplay_presentation::field;
use std::cell::Cell;

use deadsync_assets::gameplay_presentation::display_mods::{self, DisplayModsFrame};

pub use field::preferred_mods_text;
pub(crate) use field::{GameplayNotefieldPlan, ResolvedJudgmentAssets, prewarm_hud_actor_resources};

pub(crate) fn gameplay_notefield_plan(
    profile: &profile_data::Profile,
    judgment_assets: &ResolvedJudgmentAssets,
    blue_fantastic_window_s: f32,
    judgment_palette: deadsync_theme::color::JudgmentPalette,
) -> GameplayNotefieldPlan {
    field::gameplay_notefield_plan(
        profile, judgment_assets, blue_fantastic_window_s,
        crate::notefield_style::notefield_style_with_palette(judgment_palette),
    )
}

const COMBO_BURST_TEXTURE: &str = "combo_explosion.png";

#[derive(Clone, Copy)]
struct CachedComboMilestoneSizes {
    keys: [&'static str; 4],
    sizes: [[f32; 2]; 4],
}

/// Visual-style-stable combo milestone art plus screen-lifetime native sizes.
///
/// The gameplay screen owns this single-threaded four-entry set and warms it
/// during the incoming transition. A steady frame performs one fixed key
/// comparison with no lookup, allocation, lock, growth, or eviction. A visual
/// style change replaces all four sizes through bounded texture-dimension
/// lookups; skipped transition setup pays the same bounded work on first use.
/// The set is dropped with the screen, and focused cache tests cover hits and
/// replacement. Worst steady-frame cost is four static-string comparisons;
/// worst miss cost is four registry dimension lookups outside normal play.
pub(crate) struct ResolvedComboMilestoneAssets {
    cached: Cell<Option<CachedComboMilestoneSizes>>,
}

impl ResolvedComboMilestoneAssets {
    pub(crate) const fn new() -> Self {
        Self {
            cached: Cell::new(None),
        }
    }

    #[inline(always)]
    pub(crate) fn resolve(
        &self,
        effects: &'static crate::visual_styles::EffectAssets,
    ) -> ComboMilestoneAssets {
        let sizes = self.sizes_for_keys(combo_milestone_keys(effects), |key| {
            deadlib_assets::texture_dims(key)
                .map_or([0.0; 2], |meta| [meta.w as f32, meta.h as f32])
        });
        combo_milestone_assets(effects, sizes)
    }

    fn sizes_for_keys(
        &self,
        keys: [&'static str; 4],
        resolve: impl FnMut(&str) -> [f32; 2],
    ) -> [[f32; 2]; 4] {
        if let Some(cached) = self.cached.get()
            && cached.keys == keys
        {
            return cached.sizes;
        }
        let sizes = keys.map(resolve);
        self.cached
            .set(Some(CachedComboMilestoneSizes { keys, sizes }));
        sizes
    }

    pub(crate) fn prewarm(&self, effects: &'static crate::visual_styles::EffectAssets) {
        let _ = self.resolve(effects);
    }
}

#[inline(always)]
const fn combo_milestone_keys(
    effects: &'static crate::visual_styles::EffectAssets,
) -> [&'static str; 4] {
    [
        COMBO_BURST_TEXTURE,
        effects.combo_100milestone_splode,
        effects.combo_100milestone_minisplode,
        effects.combo_1000milestone_swoosh,
    ]
}

#[inline(always)]
fn combo_milestone_assets(
    effects: &'static crate::visual_styles::EffectAssets,
    sizes: [[f32; 2]; 4],
) -> ComboMilestoneAssets {
    let keys = combo_milestone_keys(effects);
    ComboMilestoneAssets {
        burst: ComboMilestoneSprite {
            source: SpriteSource::static_texture(keys[0]),
            native_size: sizes[0],
            zoom_scale: 1.0,
        },
        hundred: ComboMilestoneSprite {
            source: SpriteSource::static_texture(keys[1]),
            native_size: sizes[1],
            zoom_scale: crate::visual_styles::effect_zoom_scale(keys[1]),
        },
        hundred_mini: ComboMilestoneSprite {
            source: SpriteSource::static_texture(keys[2]),
            native_size: sizes[2],
            zoom_scale: crate::visual_styles::effect_zoom_scale(keys[2]),
        },
        thousand: ComboMilestoneSprite {
            source: SpriteSource::static_texture(keys[3]),
            native_size: sizes[3],
            zoom_scale: crate::visual_styles::effect_zoom_scale(keys[3]),
        },
    }
}

pub(crate) fn compose_hud(
    state: &State,
    judgment_assets: &ResolvedJudgmentAssets,
    resolved_combo_assets: &ResolvedComboMilestoneAssets,
    notefield_plan: &GameplayNotefieldPlan,
    frame: &FieldFrame<'_>,
    request: &NotefieldComposeRequest<'_, SpriteSlot>,
    prepared: &PreparedNotefield<'_, SpriteSlot>,
    visual_effects: &'static crate::visual_styles::EffectAssets,
    textures: &(impl deadlib_present::texture::TextureContext + ?Sized),
    broken_run_lookup: &BrokenRunLookup,
    stream_progress_lookup: &StreamProgressLookup,
    warn_cmod_for_itl_chart: bool,
    display_mods_text: &std::sync::Arc<str>,
    buffers: FieldHudBuffers<'_>,
) -> NotefieldHudComposeResult {
    let player = &state.players()[frame.player];
    let elapsed_screen = request.visual.elapsed_screen_s;
    let playfield_center_x = prepared.field.playfield_center_x;
    let notefield_offset_y = prepared.field.notefield_offset_y;

    if let Some(alpha) = display_mods::active_alpha(request.view.hide_display_mods, elapsed_screen)
    {
        display_mods::compose(
            buffers.actors,
            DisplayModsFrame {
                mods_text: std::sync::Arc::clone(display_mods_text),
                alpha,
                playfield_center_x,
                notefield_offset_y,
            },
            warn_cmod_for_itl_chart.then(|| crate::i18n::tr("Gameplay", "CModOn").into()),
        );
    }

    let combo_active = frame.combo_visible && NotefieldHudFrameView::combo_active(
        request.view.hide_combo, prepared.blind_active, request.options.combo_visible,
    );
    let milestone_assets = (combo_active && !request.options.hide_combo_explosions
        && !player.combo_milestones.is_empty())
        .then(|| resolved_combo_assets.resolve(visual_effects));
    let player_color = milestone_assets.as_ref()
        .map_or([1.0; 4], |_| color::decorative_rgba(state.player_color_index()));
    field::compose_hud(
        state, judgment_assets, milestone_assets.as_ref(), notefield_plan, frame, request,
        prepared, player_color, textures, broken_run_lookup, stream_progress_lookup, buffers,
    )
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
