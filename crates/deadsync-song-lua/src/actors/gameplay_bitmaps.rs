//! Bundled gameplay bitmaps, decoded with the existing texture policy.
use std::sync::OnceLock;

struct NativeGameplayInBitmap {
    key: &'static str,
    filename: &'static str,
    bytes: &'static [u8],
    prepared: OnceLock<Result<String, String>>,
}

static NATIVE_GAMEPLAY_IN_BITMAPS: [NativeGameplayInBitmap; 57] = [
    NativeGameplayInBitmap {
        key: "Combo 100milestone/explosion",
        filename: "Combo 100milestone/explosion.png",
        bytes: include_bytes!("../../assets/simply-love/Graphics/Combo 100milestone/explosion.png"),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Arrows/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Arrows/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/Combo 100milestone minisplode",
        filename: "_VisualStyles/Arrows/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Arrows/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/Combo 100milestone splode",
        filename: "_VisualStyles/Arrows/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Arrows/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Bears/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Bears/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/Combo 100milestone minisplode",
        filename: "_VisualStyles/Bears/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Bears/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/Combo 100milestone splode",
        filename: "_VisualStyles/Bears/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Bears/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Cats/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Cats/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/Combo 100milestone minisplode",
        filename: "_VisualStyles/Cats/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Cats/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/Combo 100milestone splode",
        filename: "_VisualStyles/Cats/Combo 100milestone splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Cats/Combo 100milestone splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Ducks/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Ducks/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/Combo 100milestone minisplode",
        filename: "_VisualStyles/Ducks/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Ducks/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/Combo 100milestone splode",
        filename: "_VisualStyles/Ducks/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Ducks/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Gay/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Gay/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/Combo 100milestone minisplode",
        filename: "_VisualStyles/Gay/Combo 100milestone minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Gay/Combo 100milestone minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/Combo 100milestone splode",
        filename: "_VisualStyles/Gay/Combo 100milestone splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Gay/Combo 100milestone splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Hearts/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Hearts/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/Combo 100milestone minisplode",
        filename: "_VisualStyles/Hearts/Combo 100milestone minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Hearts/Combo 100milestone minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/Combo 100milestone splode",
        filename: "_VisualStyles/Hearts/Combo 100milestone splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Hearts/Combo 100milestone splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Spooky/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Spooky/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/Combo 100milestone minisplode",
        filename: "_VisualStyles/Spooky/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Spooky/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/Combo 100milestone splode",
        filename: "_VisualStyles/Spooky/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Spooky/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/Combo 1000milestone swoosh",
        filename: "_VisualStyles/SRPG10/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/SRPG10/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/Combo 100milestone minisplode",
        filename: "_VisualStyles/SRPG10/Combo 100milestone minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/SRPG10/Combo 100milestone minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/Combo 100milestone splode",
        filename: "_VisualStyles/SRPG10/Combo 100milestone splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/SRPG10/Combo 100milestone splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Stars/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Stars/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/Combo 100milestone minisplode",
        filename: "_VisualStyles/Stars/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Stars/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/Combo 100milestone splode",
        filename: "_VisualStyles/Stars/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Stars/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Technique/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Technique/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/Combo 100milestone minisplode",
        filename: "_VisualStyles/Technique/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Technique/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/Combo 100milestone splode",
        filename: "_VisualStyles/Technique/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Technique/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Thonk/Combo 1000milestone swoosh.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Thonk/Combo 1000milestone swoosh.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/Combo 100milestone minisplode",
        filename: "_VisualStyles/Thonk/Combo 100milestone minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Thonk/Combo 100milestone minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/Combo 100milestone splode",
        filename: "_VisualStyles/Thonk/Combo 100milestone splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Thonk/Combo 100milestone splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/GameplayIn minisplode",
        filename: "_VisualStyles/Arrows/GameplayIn minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Arrows/GameplayIn minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/GameplayIn splode",
        filename: "_VisualStyles/Arrows/GameplayIn splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Arrows/GameplayIn splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/GameplayIn minisplode",
        filename: "_VisualStyles/Bears/GameplayIn minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Bears/GameplayIn minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/GameplayIn splode",
        filename: "_VisualStyles/Bears/GameplayIn splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Bears/GameplayIn splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/GameplayIn minisplode",
        filename: "_VisualStyles/Cats/GameplayIn minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Cats/GameplayIn minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/GameplayIn splode",
        filename: "_VisualStyles/Cats/GameplayIn splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Cats/GameplayIn splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/GameplayIn minisplode",
        filename: "_VisualStyles/Ducks/GameplayIn minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Ducks/GameplayIn minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/GameplayIn splode",
        filename: "_VisualStyles/Ducks/GameplayIn splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Ducks/GameplayIn splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/GameplayIn minisplode",
        filename: "_VisualStyles/Gay/GameplayIn minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Gay/GameplayIn minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/GameplayIn splode",
        filename: "_VisualStyles/Gay/GameplayIn splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Gay/GameplayIn splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/GameplayIn minisplode",
        filename: "_VisualStyles/Hearts/GameplayIn minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Hearts/GameplayIn minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/GameplayIn splode",
        filename: "_VisualStyles/Hearts/GameplayIn splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Hearts/GameplayIn splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/ExtraSpooky/Bats",
        filename: "_VisualStyles/Spooky/ExtraSpooky/Bats (doubleres) 2x1.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Spooky/ExtraSpooky/Bats (doubleres) 2x1.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/GameplayIn minisplode",
        filename: "_VisualStyles/Spooky/GameplayIn minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Spooky/GameplayIn minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/GameplayIn splode",
        filename: "_VisualStyles/Spooky/GameplayIn splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Spooky/GameplayIn splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/GameplayIn minisplode",
        filename: "_VisualStyles/SRPG10/GameplayIn minisplode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/SRPG10/GameplayIn minisplode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/GameplayIn splode",
        filename: "_VisualStyles/SRPG10/GameplayIn splode.png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/SRPG10/GameplayIn splode.png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/GameplayIn minisplode",
        filename: "_VisualStyles/Stars/GameplayIn minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Stars/GameplayIn minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/GameplayIn splode",
        filename: "_VisualStyles/Stars/GameplayIn splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Stars/GameplayIn splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/GameplayIn minisplode",
        filename: "_VisualStyles/Technique/GameplayIn minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Technique/GameplayIn minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/GameplayIn splode",
        filename: "_VisualStyles/Technique/GameplayIn splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Technique/GameplayIn splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/GameplayIn minisplode",
        filename: "_VisualStyles/Thonk/GameplayIn minisplode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Thonk/GameplayIn minisplode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/GameplayIn splode",
        filename: "_VisualStyles/Thonk/GameplayIn splode (doubleres).png",
        bytes: include_bytes!(
            "../../assets/simply-love/Graphics/_VisualStyles/Thonk/GameplayIn splode (doubleres).png"
        ),
        prepared: OnceLock::new(),
    },
];

pub(super) fn load(name: &str) -> Result<String, String> {
    let bitmap = NATIVE_GAMEPLAY_IN_BITMAPS
        .iter()
        .find(|bitmap| bitmap.key == name || bitmap.filename == name)
        .ok_or_else(|| format!("missing native gameplay-in bitmap: {name}"))?;
    bitmap
        .prepared
        .get_or_init(|| {
            let decoded =
                image::load_from_memory_with_format(bitmap.bytes, image::ImageFormat::Png)
                    .map_err(|error| error.to_string())?;
            let mut image = decoded.into_rgba8();
            let source_size = [image.width(), image.height()];
            let mut hints = deadlib_assets::parse_texture_hints(bitmap.filename);
            hints.min_size = Some(8);
            hints.max_size = Some(2048);
            let size = deadlib_assets::texture_image_size(source_size, &hints)
                .map_err(|error| error.to_string())?;
            if size != source_size {
                image = image::imageops::resize(
                    &image,
                    size[0],
                    size[1],
                    image::imageops::FilterType::Triangle,
                );
            }
            deadlib_assets::apply_texture_hints(&mut image, &hints);
            deadlib_assets::fix_hidden_alpha(&mut image);
            let path = format!("{}G/{}", crate::SONG_LUA_THEME_PATH_PREFIX, bitmap.filename);
            deadlib_assets::register_prepared_generated_texture(
                &path,
                image,
                source_size,
                hints.sampler_desc(),
            );
            Ok(path)
        })
        .as_ref()
        .cloned()
        .map_err(Clone::clone)
}
