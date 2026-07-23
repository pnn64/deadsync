use super::technique_bg;
use crate::act;
use crate::views::{SimplyLoveVisualPolicyView, VisualBackgroundView};
use deadlib_present::actors::Actor;
#[cfg(test)]
use deadlib_present::actors::SpriteSource;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

// Shared UI elapsed clock advanced by `app` using post-Tab-acceleration dt so
// menu backgrounds stay phase-locked across screens while still honoring
// fast/slow/paused menu animation controls.
static GLOBAL_ELAPSED_BITS: AtomicU64 = AtomicU64::new(0.0_f64.to_bits());
static SRPG_BACKGROUND_KEY: OnceLock<Mutex<Option<Arc<str>>>> = OnceLock::new();
static SRPG9_FALLBACK_KEY: OnceLock<Arc<str>> = OnceLock::new();
static SRPG10_FALLBACK_KEY: OnceLock<Arc<str>> = OnceLock::new();

const COLOR_ADD: [i32; 10] = [-1, 0, 0, -1, -1, -1, 0, 0, 0, 0];
const DIFFUSE_ALPHA: [f32; 10] = [0.05, 0.2, 0.1, 0.1, 0.1, 0.1, 0.1, 0.05, 0.1, 0.1];
const XY: [f32; 10] = [
    0.0, 40.0, 80.0, 120.0, 200.0, 280.0, 360.0, 400.0, 480.0, 560.0,
];
const UV_VEL: [[f32; 2]; 10] = [
    [0.03, 0.01],
    [0.03, 0.02],
    [0.03, 0.01],
    [0.02, 0.02],
    [0.03, 0.03],
    [0.02, 0.02],
    [0.03, 0.01],
    [-0.03, 0.01],
    [0.05, 0.03],
    [0.03, 0.04],
];
const SHARED_BG_ZOOM: f32 = 1.3;
const SHARED_BG_UV_SPAN: f32 = 1.0;

#[derive(Clone, Copy)]
struct TiledStyleState;

#[derive(Clone)]
pub struct State {
    tiled: TiledStyleState,
    technique: technique_bg::State,
}

pub struct Params {
    pub active_color_index: i32,
    pub backdrop_rgba: [f32; 4],
    pub alpha_mul: f32,
    pub visual_policy: SimplyLoveVisualPolicyView,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TiledStyleState {
    fn default() -> Self {
        Self::new()
    }
}

impl TiledStyleState {
    const fn new() -> Self {
        Self
    }

    fn push_at_elapsed(&self, out: &mut Vec<Actor>, params: &Params, elapsed_s: f64) {
        out.reserve(1 + XY.len());
        let w = screen_width();
        let h = screen_height();
        out.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
            z(-100)
        ));

        for i in 0..10 {
            let mut rgba = color::decorative_rgba(params.active_color_index + COLOR_ADD[i]);
            rgba[3] = DIFFUSE_ALPHA[i] * params.alpha_mul;
            let uv = scrolled_uv_rect(UV_VEL[i], elapsed_s);
            push_shared_bg(
                out,
                XY[i],
                XY[i],
                rgba,
                uv,
                params.visual_policy.assets.shared_background,
            );
        }
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            tiled: TiledStyleState::new(),
            technique: technique_bg::State::new(),
        }
    }

    pub fn build(&self, params: Params) -> Vec<Actor> {
        self.build_at_elapsed(params, global_elapsed_s())
    }

    pub fn push(&self, out: &mut Vec<Actor>, params: Params) {
        self.push_at_elapsed(out, params, global_elapsed_s());
    }

    pub fn push_at_elapsed(&self, out: &mut Vec<Actor>, params: Params, elapsed_s: f64) {
        if matches!(
            params.visual_policy.background,
            VisualBackgroundView::Technique
        ) && self.technique.push_at_elapsed(
            out,
            params.active_color_index,
            params.backdrop_rgba,
            params.alpha_mul,
            elapsed_s,
        ) {
            return;
        }
        if matches!(params.visual_policy.background, VisualBackgroundView::Srpg) {
            push_srpg(out, &params);
            return;
        }
        self.tiled.push_at_elapsed(out, &params, elapsed_s);
    }

    pub fn build_at_elapsed(&self, params: Params, elapsed_s: f64) -> Vec<Actor> {
        let mut actors = Vec::new();
        self.push_at_elapsed(&mut actors, params, elapsed_s);
        actors
    }
}

fn push_shared_bg(
    out: &mut Vec<Actor>,
    x: f32,
    y: f32,
    rgba: [f32; 4],
    uv: [f32; 4],
    texture_key: &'static str,
) {
    out.push(act!(sprite_static(texture_key):
        xy(x, y):
        zoom(SHARED_BG_ZOOM):
        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(-99)
    ));
}

fn push_srpg(out: &mut Vec<Actor>, params: &Params) {
    out.reserve(3);
    let w = screen_width();
    let h = screen_height();
    let background_key = srpg_background_key(params.visual_policy.assets.shared_background);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
        z(-100)
    ));

    let mut tint =
        srpg_background_tint(params.active_color_index, params.visual_policy.srpg10_tint);
    tint[0] = (tint[0] * 3.0).min(1.0);
    tint[1] = (tint[1] * 3.0).min(1.0);
    tint[2] = (tint[2] * 3.0).min(1.0);
    tint[3] = params.alpha_mul;
    out.push(act!(sprite(background_key):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_center_y()):
        setsize((h * 16.0 / 9.0).max(w), h):
        diffuse(tint[0], tint[1], tint[2], tint[3]):
        z(-99)
    ));
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.5 * params.alpha_mul):
        z(-98)
    ));
}

fn srpg_background_key(fallback_key: &'static str) -> Arc<str> {
    let fallback = || srpg_fallback_key(fallback_key);
    match SRPG_BACKGROUND_KEY.get_or_init(|| Mutex::new(None)).lock() {
        Ok(key) => key.clone().unwrap_or_else(fallback),
        Err(_) => fallback(),
    }
}

fn srpg_fallback_key(fallback_key: &'static str) -> Arc<str> {
    let srpg10_key = crate::visual_styles::for_style_and_variant(
        deadsync_config::prelude::VisualStyle::Srpg9,
        deadsync_config::prelude::SrpgVariant::Srpg10,
    )
    .shared_background;
    let cache = if fallback_key == srpg10_key {
        &SRPG10_FALLBACK_KEY
    } else {
        &SRPG9_FALLBACK_KEY
    };
    Arc::clone(cache.get_or_init(|| Arc::<str>::from(fallback_key)))
}

pub fn set_srpg_background_key(key: Option<String>) {
    if let Ok(mut slot) = SRPG_BACKGROUND_KEY.get_or_init(|| Mutex::new(None)).lock() {
        if slot.as_deref() == key.as_deref() {
            return;
        }
        *slot = key.map(Arc::<str>::from);
    }
}

#[cfg(feature = "bench-support")]
fn set_srpg_background_key_legacy(key: Option<String>) {
    if let Ok(mut slot) = SRPG_BACKGROUND_KEY.get_or_init(|| Mutex::new(None)).lock() {
        *slot = key.map(Arc::<str>::from);
    }
}

#[cfg(feature = "bench-support")]
pub struct TechniqueBackgroundBench {
    inner: technique_bg::BenchState,
}

#[cfg(feature = "bench-support")]
impl TechniqueBackgroundBench {
    pub fn new() -> Self {
        Self {
            inner: technique_bg::BenchState::new(),
        }
    }

    pub fn build(&self, elapsed_s: f64) -> Vec<Actor> {
        self.inner.build(elapsed_s)
    }

    pub fn build_legacy(&self, elapsed_s: f64) -> Vec<Actor> {
        self.inner.build_legacy(elapsed_s)
    }

    pub fn projection(&self, width: f32, height: f32) -> [f32; 16] {
        self.inner.projection(width, height)
    }
}

#[cfg(feature = "bench-support")]
impl Default for TechniqueBackgroundBench {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "bench-support")]
pub fn technique_projection_legacy_for_bench(width: f32, height: f32) -> [f32; 16] {
    technique_bg::projection_legacy_for_bench(width, height)
}

#[cfg(feature = "bench-support")]
pub fn technique_layout_checksum_for_bench(elapsed_s: f64) -> u64 {
    technique_bg::layout_checksum_for_bench(elapsed_s)
}

#[cfg(feature = "bench-support")]
pub fn technique_layout_legacy_checksum_for_bench(elapsed_s: f64) -> u64 {
    technique_bg::layout_legacy_checksum_for_bench(elapsed_s)
}

#[cfg(feature = "bench-support")]
pub fn technique_layer_checksum_for_bench(elapsed_s: f64) -> u64 {
    technique_bg::layer_checksum_for_bench(elapsed_s)
}

#[cfg(feature = "bench-support")]
pub fn technique_layer_legacy_checksum_for_bench(elapsed_s: f64) -> u64 {
    technique_bg::layer_legacy_checksum_for_bench(elapsed_s)
}

#[cfg(feature = "bench-support")]
pub struct OtherVisualBackgroundBench {}

#[cfg(feature = "bench-support")]
impl OtherVisualBackgroundBench {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build_srpg(&self) -> Vec<Actor> {
        let mut actors = Vec::new();
        push_srpg(&mut actors, &srpg_bench_params());
        actors
    }

    pub fn build_srpg_legacy(&self) -> Vec<Actor> {
        let mut actors = Vec::new();
        push_srpg_legacy(&mut actors, &srpg_bench_params());
        actors
    }

    pub fn set_srpg_key(&self, key: Option<&str>) {
        set_srpg_background_key(key.map(str::to_owned));
    }

    pub fn set_srpg_key_legacy(&self, key: Option<&str>) {
        set_srpg_background_key_legacy(key.map(str::to_owned));
    }
}

#[cfg(feature = "bench-support")]
impl Default for OtherVisualBackgroundBench {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "bench-support")]
fn background_bench_params() -> Params {
    Params {
        active_color_index: 3,
        backdrop_rgba: [0.05, 0.1, 0.15, 1.0],
        alpha_mul: 0.65,
        visual_policy: SimplyLoveVisualPolicyView::default(),
    }
}

#[cfg(feature = "bench-support")]
fn srpg_bench_params() -> Params {
    let mut params = background_bench_params();
    params.visual_policy.background = VisualBackgroundView::Srpg;
    params.visual_policy.assets =
        crate::visual_styles::for_style(deadsync_config::prelude::VisualStyle::Srpg9);
    params
}

#[cfg(any(test, feature = "bench-support"))]
fn srpg_background_key_legacy(fallback_key: &'static str) -> Arc<str> {
    let fallback = || Arc::<str>::from(fallback_key);
    match SRPG_BACKGROUND_KEY.get_or_init(|| Mutex::new(None)).lock() {
        Ok(slot) => slot.clone().unwrap_or_else(fallback),
        Err(_) => fallback(),
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn push_srpg_legacy(out: &mut Vec<Actor>, params: &Params) {
    out.reserve(3);
    let w = screen_width();
    let h = screen_height();
    let background_key = srpg_background_key_legacy(params.visual_policy.assets.shared_background);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
        z(-100)
    ));

    let mut tint =
        srpg_background_tint(params.active_color_index, params.visual_policy.srpg10_tint);
    tint[0] = (tint[0] * 3.0).min(1.0);
    tint[1] = (tint[1] * 3.0).min(1.0);
    tint[2] = (tint[2] * 3.0).min(1.0);
    tint[3] = params.alpha_mul;
    out.push(act!(sprite(background_key):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_center_y()):
        setsize((h * 16.0 / 9.0).max(w), h):
        diffuse(tint[0], tint[1], tint[2], tint[3]):
        z(-99)
    ));
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.5 * params.alpha_mul):
        z(-98)
    ));
}

fn srpg_background_tint(active_color_index: i32, srpg10: bool) -> [f32; 4] {
    if srpg10 {
        color::srpg10_rgba(active_color_index)
    } else {
        color::decorative_rgba(active_color_index)
    }
}

#[inline(always)]
fn scrolled_uv_rect(velocity: [f32; 2], elapsed_s: f64) -> [f32; 4] {
    let u0 = (f64::from(velocity[0]) * elapsed_s).rem_euclid(1.0) as f32;
    let v0 = (f64::from(velocity[1]) * elapsed_s).rem_euclid(1.0) as f32;
    [u0, v0, u0 + SHARED_BG_UV_SPAN, v0 + SHARED_BG_UV_SPAN]
}

#[inline]
pub fn tick_global(dt: f32) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let dt = f64::from(dt);
    let mut bits = GLOBAL_ELAPSED_BITS.load(Ordering::Relaxed);
    loop {
        let elapsed = f64::from_bits(bits);
        let next = elapsed + dt;
        let next_bits = if next.is_finite() {
            next.max(0.0).to_bits()
        } else {
            bits
        };
        match GLOBAL_ELAPSED_BITS.compare_exchange_weak(
            bits,
            next_bits,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => bits = observed,
        }
    }
}

#[inline]
fn global_elapsed_s() -> f64 {
    f64::from_bits(GLOBAL_ELAPSED_BITS.load(Ordering::Relaxed))
}

#[cfg(test)]
fn set_global_elapsed_for_test(elapsed_s: f64) {
    GLOBAL_ELAPSED_BITS.store(elapsed_s.max(0.0).to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_config::prelude::{SrpgVariant, VisualStyle};

    const EPS: f64 = 1e-3;
    static SRPG_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn params() -> Params {
        Params {
            active_color_index: 3,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy: SimplyLoveVisualPolicyView::default(),
        }
    }

    fn first_bg_sprite(actors: &[Actor]) -> ([f32; 2], [f32; 4]) {
        let Some(Actor::Sprite {
            offset,
            source,
            uv_rect,
            ..
        }) = actors.get(1)
        else {
            panic!("missing first background sprite");
        };
        assert_eq!(
            source.texture_key(),
            Some(
                SimplyLoveVisualPolicyView::default()
                    .assets
                    .shared_background
            )
        );
        (
            *offset,
            uv_rect.expect("shared background should scroll UVs"),
        )
    }

    fn params_for(
        background: VisualBackgroundView,
        style: VisualStyle,
        variant: SrpgVariant,
    ) -> Params {
        Params {
            active_color_index: 3,
            backdrop_rgba: [0.05, 0.1, 0.15, 1.0],
            alpha_mul: 0.65,
            visual_policy: SimplyLoveVisualPolicyView {
                background,
                assets: crate::visual_styles::for_style_and_variant(style, variant),
                srpg10_tint: style.is_srpg() && variant == SrpgVariant::Srpg10,
                ..SimplyLoveVisualPolicyView::default()
            },
        }
    }

    fn normalized_actor_debug(mut actors: Vec<Actor>) -> Vec<(Option<String>, String)> {
        actors
            .iter_mut()
            .map(|actor| {
                let texture = match actor {
                    Actor::Sprite { source, .. } => {
                        let texture = source.texture_key().map(str::to_owned);
                        *source = SpriteSource::Solid;
                        texture
                    }
                    _ => None,
                };
                (texture, format!("{actor:?}"))
            })
            .collect()
    }

    #[test]
    fn build_reads_shared_elapsed_clock() {
        set_global_elapsed_for_test(2.5);
        let state = TiledStyleState::new();
        let mut shared_actors = Vec::new();
        state.push_at_elapsed(&mut shared_actors, &params(), global_elapsed_s());
        let mut explicit_actors = Vec::new();
        state.push_at_elapsed(&mut explicit_actors, &params(), 2.5);
        let shared = first_bg_sprite(&shared_actors);
        let explicit = first_bg_sprite(&explicit_actors);
        assert!(
            f64::from((shared.0[0] - explicit.0[0]).abs()) < EPS
                && f64::from((shared.0[1] - explicit.0[1]).abs()) < EPS
                && shared
                    .1
                    .iter()
                    .zip(explicit.1)
                    .all(|(a, b)| f64::from((*a - b).abs()) < EPS),
            "shared={shared:?} explicit={explicit:?}"
        );
    }

    #[test]
    fn every_tiled_variant_builds_expected_layers_and_asset() {
        let styles = [
            VisualStyle::Hearts,
            VisualStyle::Arrows,
            VisualStyle::Bears,
            VisualStyle::Ducks,
            VisualStyle::Cats,
            VisualStyle::Spooky,
            VisualStyle::Gay,
            VisualStyle::Stars,
            VisualStyle::Thonk,
        ];
        let state = TiledStyleState::new();
        for style in styles {
            for elapsed_s in [0.0, 12.375, 1_000_000.0] {
                let params = params_for(VisualBackgroundView::Tiled, style, SrpgVariant::Srpg9);
                let expected_texture = params.visual_policy.assets.shared_background;
                let mut actors = Vec::new();
                state.push_at_elapsed(&mut actors, &params, elapsed_s);
                assert_eq!(actors.len(), 11, "style={style:?} elapsed={elapsed_s}");
                for (index, actor) in actors.iter().enumerate().skip(1) {
                    let Actor::Sprite {
                        source,
                        offset,
                        tint,
                        uv_rect: Some(uv),
                        ..
                    } = actor
                    else {
                        panic!("style={style:?} elapsed={elapsed_s} layer={index}");
                    };
                    assert_eq!(source.texture_key(), Some(expected_texture));
                    assert_eq!(*offset, [XY[index - 1], XY[index - 1]]);
                    assert_eq!(tint[3], DIFFUSE_ALPHA[index - 1] * params.alpha_mul);
                    assert!(uv.iter().all(|value| value.is_finite()));
                }
            }
        }
    }

    #[test]
    fn srpg_variants_and_dynamic_key_match_legacy_output() {
        let _guard = SRPG_TEST_LOCK.lock().expect("SRPG test lock poisoned");
        for variant in SrpgVariant::ALL {
            let params = params_for(VisualBackgroundView::Srpg, VisualStyle::Srpg9, variant);
            for key in [None, Some("dynamic/srpg-video".to_string())] {
                set_srpg_background_key(key);
                let mut optimized = Vec::new();
                let mut legacy = Vec::new();
                push_srpg(&mut optimized, &params);
                push_srpg_legacy(&mut legacy, &params);
                assert_eq!(
                    normalized_actor_debug(optimized),
                    normalized_actor_debug(legacy),
                    "variant={variant:?}"
                );
            }
        }
        set_srpg_background_key(None);
    }

    #[test]
    fn publishing_the_same_srpg_key_preserves_its_allocation() {
        let _guard = SRPG_TEST_LOCK.lock().expect("SRPG test lock poisoned");
        set_srpg_background_key(Some("dynamic/same-video".to_string()));
        let first = SRPG_BACKGROUND_KEY
            .get()
            .and_then(|key| key.lock().ok()?.clone())
            .expect("published key");
        set_srpg_background_key(Some("dynamic/same-video".to_string()));
        let second = SRPG_BACKGROUND_KEY
            .get()
            .and_then(|key| key.lock().ok()?.clone())
            .expect("published key");
        assert!(Arc::ptr_eq(&first, &second));
        set_srpg_background_key(None);
    }

    #[test]
    fn tick_global_accumulates_positive_dt() {
        set_global_elapsed_for_test(1.0);
        tick_global(0.5);
        assert!(
            (global_elapsed_s() - 1.5).abs() < EPS,
            "got {}",
            global_elapsed_s()
        );
        tick_global(0.0);
        assert!(
            (global_elapsed_s() - 1.5).abs() < EPS,
            "got {}",
            global_elapsed_s()
        );
        tick_global(-0.25);
        assert!(
            (global_elapsed_s() - 1.5).abs() < EPS,
            "got {}",
            global_elapsed_s()
        );
    }

    #[test]
    fn tick_global_keeps_subframe_precision_after_long_uptime() {
        set_global_elapsed_for_test(1_000_000.0);
        tick_global(1.0 / 240.0);
        assert!(
            global_elapsed_s() > 1_000_000.0,
            "got {}",
            global_elapsed_s()
        );
    }
}
