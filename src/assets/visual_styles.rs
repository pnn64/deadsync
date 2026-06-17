use crate::config::{self, VisualStyle};

pub struct Assets {
    pub select_color: &'static str,
    pub shared_background: &'static str,
    pub effects: EffectAssets,
    pub shared_background_video: Option<&'static str>,
    pub menu_music: &'static str,
    pub select_color_size: [u32; 2],
    pub shared_background_size: [u32; 2],
}

pub struct EffectAssets {
    pub titlemenu_flycenter: &'static str,
    pub titlemenu_flytop: &'static str,
    pub titlemenu_flybottom: &'static str,
    pub gameplayin_splode: &'static str,
    pub gameplayin_minisplode: &'static str,
    pub combo_100milestone_splode: &'static str,
    pub combo_100milestone_minisplode: &'static str,
    pub combo_1000milestone_swoosh: &'static str,
}

macro_rules! effect_assets {
    (
        $folder:literal,
        $title_suffix:literal,
        $gameplay_suffix:literal,
        $combo100_splode_suffix:literal,
        $combo100_minisplode_suffix:literal
    ) => {
        EffectAssets {
            titlemenu_flycenter: concat!(
                "visual_styles/",
                $folder,
                "/titlemenu_flycenter",
                $title_suffix,
                ".png"
            ),
            titlemenu_flytop: concat!(
                "visual_styles/",
                $folder,
                "/titlemenu_flytop",
                $title_suffix,
                ".png"
            ),
            titlemenu_flybottom: concat!(
                "visual_styles/",
                $folder,
                "/titlemenu_flybottom",
                $title_suffix,
                ".png"
            ),
            gameplayin_splode: concat!(
                "visual_styles/",
                $folder,
                "/gameplayin_splode",
                $gameplay_suffix,
                ".png"
            ),
            gameplayin_minisplode: concat!(
                "visual_styles/",
                $folder,
                "/gameplayin_minisplode",
                $gameplay_suffix,
                ".png"
            ),
            combo_100milestone_splode: concat!(
                "visual_styles/",
                $folder,
                "/combo_100milestone_splode",
                $combo100_splode_suffix,
                ".png"
            ),
            combo_100milestone_minisplode: concat!(
                "visual_styles/",
                $folder,
                "/combo_100milestone_minisplode",
                $combo100_minisplode_suffix,
                ".png"
            ),
            combo_1000milestone_swoosh: concat!(
                "visual_styles/",
                $folder,
                "/combo_1000milestone_swoosh.png"
            ),
        }
    };
}

pub const ASSETS: [Assets; VisualStyle::ALL.len()] = [
    Assets {
        select_color: "visual_styles/hearts/select_color.png",
        shared_background: "visual_styles/hearts/shared_background.png",
        effects: effect_assets!("hearts", "", "", "", ""),
        shared_background_video: None,
        menu_music: "assets/music/in_two (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/arrows/select_color.png",
        shared_background: "visual_styles/arrows/shared_background.png",
        effects: effect_assets!(
            "arrows",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/halcyon farms (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/bears/select_color.png",
        shared_background: "visual_styles/bears/shared_background.png",
        effects: effect_assets!("bears", "", "", " (doubleres)", " (doubleres)"),
        shared_background_video: None,
        menu_music: "assets/music/vrtuous faults (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/ducks/select_color.png",
        shared_background: "visual_styles/ducks/shared_background.png",
        effects: effect_assets!("ducks", "", " (doubleres)", " (doubleres)", " (doubleres)"),
        shared_background_video: None,
        menu_music: "assets/music/Xuxa fami VRC6 (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/cats/select_color.png",
        shared_background: "visual_styles/cats/shared_background.png",
        effects: effect_assets!("cats", "", "", "", " (doubleres)"),
        shared_background_video: None,
        menu_music: "assets/music/Beanmania IIDX (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/spooky/select_color.png",
        shared_background: "visual_styles/spooky/shared_background.png",
        effects: effect_assets!(
            "spooky",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Spooky Scary Chiptunes (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/gay/select_color.png",
        shared_background: "visual_styles/gay/shared_background.png",
        effects: effect_assets!("gay", "", "", "", ""),
        shared_background_video: None,
        menu_music: "assets/music/Mystical Wheelbarrow Journey (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/stars/select_color.png",
        shared_background: "visual_styles/stars/shared_background.png",
        effects: effect_assets!(
            "stars",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Shooting Star - faux VRC6 remix (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/thonk/select_color.png",
        shared_background: "visual_styles/thonk/shared_background.png",
        effects: effect_assets!(
            "thonk",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Da Box of Kardboard Too (feat Naoki vs ZigZag) - TaroNuke Remix (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/technique/select_color.png",
        shared_background: "visual_styles/technique/shared_background.png",
        effects: effect_assets!(
            "technique",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Quaq (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/srpg9/select_color.png",
        shared_background: "visual_styles/srpg9/shared_background.png",
        effects: effect_assets!("srpg9", "", "", "", ""),
        shared_background_video: Some("assets/graphics/visual_styles/srpg9/background_video.mp4"),
        menu_music: "assets/music/SRPG9 (loop).ogg",
        select_color_size: [244, 244],
        shared_background_size: [1920, 1080],
    },
];

#[inline(always)]
pub fn current_style() -> VisualStyle {
    std::panic::catch_unwind(|| config::get().visual_style).unwrap_or(VisualStyle::Hearts)
}

#[inline(always)]
pub fn for_style(style: VisualStyle) -> &'static Assets {
    &ASSETS[style_index(style)]
}

#[inline(always)]
pub fn select_color_texture_key() -> &'static str {
    for_style(current_style()).select_color
}

#[inline(always)]
pub fn shared_background_texture_key() -> &'static str {
    for_style(current_style()).shared_background
}

#[inline(always)]
pub fn titlemenu_flycenter_texture_key() -> &'static str {
    for_style(current_style()).effects.titlemenu_flycenter
}

#[inline(always)]
pub fn titlemenu_flytop_texture_key() -> &'static str {
    for_style(current_style()).effects.titlemenu_flytop
}

#[inline(always)]
pub fn titlemenu_flybottom_texture_key() -> &'static str {
    for_style(current_style()).effects.titlemenu_flybottom
}

#[inline(always)]
pub fn gameplayin_splode_texture_key() -> &'static str {
    for_style(current_style()).effects.gameplayin_splode
}

#[inline(always)]
pub fn gameplayin_minisplode_texture_key() -> &'static str {
    for_style(current_style()).effects.gameplayin_minisplode
}

#[inline(always)]
pub fn combo_100milestone_splode_texture_key() -> &'static str {
    for_style(current_style()).effects.combo_100milestone_splode
}

#[inline(always)]
pub fn combo_100milestone_minisplode_texture_key() -> &'static str {
    for_style(current_style())
        .effects
        .combo_100milestone_minisplode
}

#[inline(always)]
pub fn combo_1000milestone_swoosh_texture_key() -> &'static str {
    for_style(current_style())
        .effects
        .combo_1000milestone_swoosh
}

#[inline(always)]
pub fn effect_zoom_scale(texture_key: &str) -> f32 {
    if texture_key.contains("doubleres") {
        0.5
    } else {
        1.0
    }
}

#[inline(always)]
pub fn shared_background_video_asset_path() -> Option<&'static str> {
    for_style(current_style()).shared_background_video
}

#[inline(always)]
pub fn menu_music_asset_path() -> &'static str {
    for_style(current_style()).menu_music
}

/// Returns the absolute path to the menu music file that should play for the
/// current visual style. If the user has dropped one or more `.ogg` files
/// into `{data_dir}/assets/music/menu/{style}/` (lowercase style name) a
/// random one of those is returned; otherwise the bundled per-style file
/// from [`menu_music_asset_path`] is used. Folder override + bundled file
/// satisfy issue #375 without requiring users to overwrite anything inside
/// the bundle.
pub fn menu_music_resolved_path() -> std::path::PathBuf {
    let style = current_style();
    let folder_rel = format!("assets/music/menu/{}", style.as_str().to_ascii_lowercase());
    if let Some(p) = crate::assets::audio_folder::random_music_path(&folder_rel) {
        return p;
    }
    deadlib_platform::dirs::app_dirs().resolve_asset_path(menu_music_asset_path())
}

/// Background-music tracks bundled with the game: the per-style menu loops plus
/// the course-select and credits loops. Each relative asset key is resolved
/// through the normal overlay so the returned paths match what actually plays.
/// Used to pre-warm the ReplayGain cache at startup so a fresh install (or a
/// cleared cache) doesn't audibly adjust loudness the first time a menu track
/// plays.
pub fn bundled_music_paths() -> Vec<std::path::PathBuf> {
    use std::collections::BTreeSet;
    let mut rels: BTreeSet<&'static str> = ASSETS.iter().map(|assets| assets.menu_music).collect();
    rels.insert("assets/music/select_course (loop).ogg");
    rels.insert("assets/music/credits.ogg");

    let dirs = deadlib_platform::dirs::app_dirs();
    rels.into_iter()
        .map(|rel| dirs.resolve_asset_path(rel))
        .collect()
}

#[inline(always)]
pub fn select_color_aspect(style: VisualStyle) -> f32 {
    let size = for_style(style).select_color_size;
    size[0] as f32 / size[1] as f32
}

#[inline(always)]
pub fn select_color_zoom_scale(style: VisualStyle) -> f32 {
    566.0 / for_style(style).select_color_size[1] as f32
}

#[inline(always)]
pub fn is_shared_background_texture(key: &str) -> bool {
    ASSETS.iter().any(|asset| asset.shared_background == key)
}

const fn style_index(style: VisualStyle) -> usize {
    match style {
        VisualStyle::Hearts => 0,
        VisualStyle::Arrows => 1,
        VisualStyle::Bears => 2,
        VisualStyle::Ducks => 3,
        VisualStyle::Cats => 4,
        VisualStyle::Spooky => 5,
        VisualStyle::Gay => 6,
        VisualStyle::Stars => 7,
        VisualStyle::Thonk => 8,
        VisualStyle::Technique => 9,
        VisualStyle::Srpg9 => 10,
    }
}
