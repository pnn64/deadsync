pub(crate) use deadlib_present::rgba_const;

pub mod color;
mod effects;
pub mod fonts;
pub mod i18n;
mod i18n_runtime;
pub mod notefield_style;
mod resources;
#[doc(inline)]
pub use resources::SFX_PATHS;
pub mod scorebox;
pub mod step_stats;
pub mod step_stats_gifs;
pub mod views;
pub mod visual_styles;

pub use effects::{
    SimplyLoveAdvancedConfigRequest, SimplyLoveCoinConfigRequest, SimplyLoveConfigRequest,
    SimplyLoveContentRequest, SimplyLoveCourseConfigRequest, SimplyLoveDebugRequest,
    SimplyLoveEffect, SimplyLoveEffectRouteContext, SimplyLoveEffectRoutePlan,
    SimplyLoveGameplayConfigRequest, SimplyLoveGameplayPadLights, SimplyLoveGraphOrientation,
    SimplyLoveGraphOrigin, SimplyLoveHardwareRequest, SimplyLoveInputResult,
    SimplyLoveItgImportSummary, SimplyLoveItgProfileCandidate, SimplyLoveLightsConfigRequest,
    SimplyLoveLightsDriver, SimplyLoveLobbyRequest, SimplyLoveLocalProfileEvent,
    SimplyLoveMachineConfigRequest, SimplyLoveMappingsConfigRequest, SimplyLoveMediaRequest,
    SimplyLoveNullOrDieConfigRequest, SimplyLoveNullOrDieGraph, SimplyLoveOnlineConfigRequest,
    SimplyLoveOnlineRequest, SimplyLoveOptionsConfigRequest, SimplyLoveProfileImportEvent,
    SimplyLoveProfileRequest, SimplyLoveQrLoginEvent, SimplyLoveQrLoginPolicy,
    SimplyLoveQrLoginRequest, SimplyLoveQrLoginService, SimplyLoveQrLoginSlot,
    SimplyLoveQrLoginSlotAvailability, SimplyLoveRuntimeRequest, SimplyLoveScoreImportEvent,
    SimplyLoveScoreImportProfile, SimplyLoveScoreImportProgress, SimplyLoveScoreImportRequest,
    SimplyLoveScoreImportSummary, SimplyLoveSelectMusicConfigRequest, SimplyLoveSongSearchRequest,
    SimplyLoveSongSearchResult, SimplyLoveSrpgShopFolder, SimplyLoveSyncEvent,
    SimplyLoveSyncKernel, SimplyLoveSyncKernelTarget, SimplyLoveSyncOwner, SimplyLoveSyncPlotView,
    SimplyLoveSyncRequest, SimplyLoveSyncResult, SimplyLoveSyncSongResult,
    SimplyLoveSyncStreamEvent, SimplyLoveSyncTarget, SimplyLoveTournamentConfigRequest,
    SimplyLoveUpdaterRequest, resolve_effect_route,
};

pub struct SimplyLoveTheme;

impl deadsync_theme::Theme for SimplyLoveTheme {
    type Screen = screens::SimplyLoveScreen;
    type RuntimeRequest = SimplyLoveRuntimeRequest;

    #[inline(always)]
    fn screen_id(screen: Self::Screen) -> deadsync_theme::ThemeScreenId {
        screen.id()
    }
}

mod act_macro {
    macro_rules! act {
        (sprite($tex:literal): $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::static_texture($tex)
            )
        }};
        (sprite($tex:expr): $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::texture($tex)
            )
        }};
        (sprite_static($tex:expr): $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::static_texture($tex)
            )
        }};
        (quad: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::solid()
            )
        }};
        (text: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::TextBuilder::new()
            )
        }};
    }
    pub(crate) use act;
}
pub(crate) use act_macro::act;

#[must_use]
pub fn asset_manifest()
-> deadsync_theme::ThemeAssetManifest<impl Iterator<Item = deadlib_assets::TextureAssetSpec>> {
    deadsync_theme::ThemeAssetManifest {
        fonts: &resources::FONT_ASSETS,
        textures: resources::initial_texture_assets(),
        texture_needs_repeat_sampler: resources::texture_needs_repeat_sampler,
    }
}

pub mod screens;

#[cfg(test)]
mod tests {
    pub(crate) fn init_paths() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let bundle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(std::path::Path::parent)
                .expect("crate is under the workspace crates directory")
                .to_path_buf();
            let data =
                std::env::temp_dir().join(format!("deadsync-theme-paths-{}", std::process::id()));
            let dirs = deadsync_config::dirs::AppDirs {
                cache_dir: data.join("cache"),
                data_dir: data,
                exe_dir: bundle,
                portable: false,
            };
            dirs.ensure_dirs_exist();
            let mut assets = dirs.asset_paths(None);
            if let Some(root) = std::env::var_os("DEADSYNC_WORKSHOP_PACK") {
                let root = std::path::PathBuf::from(root)
                    .canonicalize()
                    .expect("Workshop fixture exists");
                assets.noteskin_pack_roots =
                    vec![root.parent().expect("pack has a parent").to_path_buf()];
            }
            deadsync_assets::init_paths(assets).expect("initialize fixture assets");
            deadsync_config::runtime::init_paths(dirs.config_path(), dirs.judgment_palettes_path())
                .expect("initialize fixture config");
            deadsync_profile::app_runtime::init_paths(
                dirs.profiles_root(),
                dirs.default_player_options_path(),
            )
            .expect("initialize fixture profiles");
            deadsync_simfile::app_runtime::init_paths(deadsync_simfile::app_runtime::ScanPaths {
                song_cache: dirs.song_cache_dir(),
                extra_songs: Vec::new(),
                extra_courses: Vec::new(),
                autogen_courses: dirs.courses_dir(),
                song_movies: Vec::new(),
                random_movies: Vec::new(),
                bg_animations: Vec::new(),
            })
            .expect("initialize fixture scanning");
        });
    }

    #[test]
    fn screen_contract_is_reexported() {
        assert_eq!(
            super::screens::Screen::Menu.current_screen_file_name(),
            "ScreenTitleMenu"
        );
    }

    #[test]
    fn asset_manifest_adapts_current_theme_resources() {
        let manifest = super::asset_manifest();

        assert_eq!(manifest.fonts.len(), super::resources::FONT_ASSETS.len());
        assert!(manifest.textures.into_iter().any(|asset| {
            asset.key == "grades/goldstar (stretch).png"
                && asset.path == "grades/goldstar (stretch).png"
        }));
        assert!((manifest.texture_needs_repeat_sampler)(
            "grades/goldstar (stretch).png"
        ));
        assert!(!(manifest.texture_needs_repeat_sampler)("logo.png"));
    }
}
