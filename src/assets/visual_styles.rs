use crate::config::{self, VisualStyle};

pub struct Assets {
    pub select_color: &'static str,
    pub shared_background: &'static str,
    pub menu_music: &'static str,
    pub select_color_size: [u32; 2],
    pub shared_background_size: [u32; 2],
}

pub const ASSETS: [Assets; VisualStyle::ALL.len()] = [
    Assets {
        select_color: "visual_styles/hearts/select_color.png",
        shared_background: "visual_styles/hearts/shared_background.png",
        menu_music: "assets/music/in_two (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/arrows/select_color.png",
        shared_background: "visual_styles/arrows/shared_background.png",
        menu_music: "assets/music/halcyon farms (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/bears/select_color.png",
        shared_background: "visual_styles/bears/shared_background.png",
        menu_music: "assets/music/vrtuous faults (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/ducks/select_color.png",
        shared_background: "visual_styles/ducks/shared_background.png",
        menu_music: "assets/music/Xuxa fami VRC6 (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/cats/select_color.png",
        shared_background: "visual_styles/cats/shared_background.png",
        menu_music: "assets/music/Beanmania IIDX (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/spooky/select_color.png",
        shared_background: "visual_styles/spooky/shared_background.png",
        menu_music: "assets/music/Spooky Scary Chiptunes (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/gay/select_color.png",
        shared_background: "visual_styles/gay/shared_background.png",
        menu_music: "assets/music/Mystical Wheelbarrow Journey (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/stars/select_color.png",
        shared_background: "visual_styles/stars/shared_background.png",
        menu_music: "assets/music/Shooting Star - faux VRC6 remix (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/thonk/select_color.png",
        shared_background: "visual_styles/thonk/shared_background.png",
        menu_music: "assets/music/Da Box of Kardboard Too (feat Naoki vs ZigZag) - TaroNuke Remix (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/technique/select_color.png",
        shared_background: "visual_styles/technique/shared_background.png",
        menu_music: "assets/music/Quaq (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/srpg9/select_color.png",
        shared_background: "visual_styles/srpg9/shared_background.png",
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
pub fn menu_music_asset_path() -> &'static str {
    for_style(current_style()).menu_music
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
