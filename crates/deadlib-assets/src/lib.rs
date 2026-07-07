pub mod builtin;
pub mod context;
pub mod decode;
pub mod discover;
pub mod dynamic;
pub mod font;
pub mod registry;
pub mod texture_store;
pub mod upload;

pub use builtin::{
    BLACK_TEXTURE_KEY, BuiltinTextureImage, WHITE_TEXTURE_KEY, black_texture_image,
    fallback_texture_image, solid_texture_image, white_texture_image,
};
pub use context::{ASSET_TEXTURE_CONTEXT, AssetTextureContext};
pub use decode::{
    GraphicTextureDiscovery, TextureDecodeJob, TextureDecodeResult, decode_texture_image,
    decode_texture_jobs_parallel, initial_texture_decode_jobs,
};
pub use discover::{
    DiscoveredTexture, NONE_TEXTURE_CHOICE_KEY, TextureChoice, TextureChoiceLike,
    canonical_texture_key_with_asset_roots, discover_graphic_textures_in_roots,
    graphic_texture_roots, initial_texture_source_path, noteskin_png_texture_entries,
    resolve_texture_choice_entry, resolve_texture_choice_key, texture_choices_from_discovered,
    texture_key_source_path,
};
pub use font::{
    AssetFontTextureContext, PreparedFontTexture, font_texture_asset_roots, font_texture_key,
    parse_font_with_asset_context, prepare_font_texture, set_font_fallback,
};
pub use registry::{
    GeneratedTexture, TexMeta, clear_texture_handles, generated_texture,
    register_generated_texture, register_texture_dims, register_texture_handle,
    remove_texture_handle, sprite_sheet_dims, take_pending_generated_texture_keys, texture_dims,
    texture_handle, texture_registry_generation,
};
pub use texture_store::TextureStore;

use deadlib_render::{SamplerDesc, SamplerFilter, SamplerWrap};
use image::{ImageFormat, ImageReader, RgbaImage};
use log::warn;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Default)]
pub struct TextureHints {
    pub raw: String,
    pub mipmaps: Option<bool>,
    pub grayscale: bool,
    pub alphamap: bool,
    pub doubleres: bool,
    pub stretch: bool,
    pub dither: bool,
    pub color_depth: Option<u32>,
    pub sampler_filter: Option<SamplerFilter>,
    pub sampler_wrap: Option<SamplerWrap>,
}

impl TextureHints {
    #[inline(always)]
    pub fn is_default(&self) -> bool {
        self.raw.is_empty() || self.raw.eq_ignore_ascii_case("default")
    }

    #[inline(always)]
    pub fn sampler_desc(&self) -> SamplerDesc {
        SamplerDesc {
            filter: self.sampler_filter.unwrap_or(SamplerFilter::Linear),
            wrap: self.sampler_wrap.unwrap_or(SamplerWrap::Clamp),
            mipmaps: self.mipmaps.unwrap_or(false),
        }
    }
}

#[inline(always)]
fn has_ascii_case_insensitive_substr(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[inline(always)]
fn parse_ascii_digits(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(value)
}

#[inline(always)]
fn skip_parenthetical(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut idx = start;
    while idx < bytes.len() {
        match bytes[idx] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return idx + 1;
                }
                depth -= 1;
                if depth == 0 {
                    return idx + 1;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    bytes.len()
}

#[inline(always)]
fn trim_ascii_ws(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |idx| idx + 1);
    &bytes[start..end]
}

fn parse_res_dims(section: &[u8]) -> Option<(u32, u32)> {
    let mut scan = 0usize;
    while scan + 4 <= section.len() {
        if !section[scan..scan + 4].eq_ignore_ascii_case(b"res ") {
            scan += 1;
            continue;
        }

        let mut width_start = scan + 4;
        while width_start < section.len() && section[width_start].is_ascii_whitespace() {
            width_start += 1;
        }

        let Some(x_rel) = section[width_start..]
            .iter()
            .position(|b| matches!(*b, b'x' | b'X'))
        else {
            break;
        };
        let x_idx = width_start + x_rel;
        let width = parse_ascii_digits(trim_ascii_ws(&section[width_start..x_idx]));

        let mut height_end = x_idx + 1;
        while height_end < section.len() && section[height_end].is_ascii_digit() {
            height_end += 1;
        }
        let height = parse_ascii_digits(&section[x_idx + 1..height_end]);

        if let (Some(width), Some(height)) = (width, height)
            && width > 0
            && height > 0
        {
            return Some((width, height));
        }

        scan = height_end.max(width_start + 1);
    }
    None
}

pub fn parse_texture_resolution_hint(raw: &str) -> Option<(u32, u32)> {
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'(' {
            i += 1;
            continue;
        }
        let end = skip_parenthetical(bytes, i);
        if end <= i + 2 {
            i = end.max(i + 1);
            continue;
        }
        if let Some(dims) = parse_res_dims(&bytes[i + 1..end - 1]) {
            return Some(dims);
        }
        i = end.max(i + 1);
    }
    None
}

pub fn texture_source_dims_from_real(texture_key: &str, real_w: u32, real_h: u32) -> (u32, u32) {
    let (mut source_w, mut source_h) =
        parse_texture_resolution_hint(texture_key).unwrap_or((real_w, real_h));
    if parse_texture_hints(texture_key).doubleres {
        source_w /= 2;
        source_h /= 2;
    }
    (source_w, source_h)
}

#[inline(always)]
fn is_res_tag(bytes: &[u8], idx: usize) -> bool {
    idx + 4 <= bytes.len()
        && bytes[idx] == b'('
        && bytes[idx + 1].eq_ignore_ascii_case(&b'r')
        && bytes[idx + 2].eq_ignore_ascii_case(&b'e')
        && bytes[idx + 3].eq_ignore_ascii_case(&b's')
}

pub fn parse_sprite_sheet_dims(filename: &str) -> (u32, u32) {
    let bytes = filename.as_bytes();
    let mut dims: Option<(u32, u32)> = None;
    let mut i = 0usize;

    while i < bytes.len() {
        if is_res_tag(bytes, i) {
            i = skip_parenthetical(bytes, i);
            continue;
        }

        let b = bytes[i];
        if (b == b'x' || b == b'X') && i > 0 && bytes[i - 1].is_ascii_digit() {
            let mut left = i;
            while left > 0 && bytes[left - 1].is_ascii_digit() {
                left -= 1;
            }

            let mut right = i + 1;
            while right < bytes.len() && bytes[right].is_ascii_digit() {
                right += 1;
            }

            if left < i
                && i + 1 < right
                && is_sprite_sheet_left_boundary(bytes, left)
                && is_sprite_sheet_right_boundary(bytes, right)
                && let (Some(w), Some(h)) = (
                    parse_ascii_digits(&bytes[left..i]),
                    parse_ascii_digits(&bytes[i + 1..right]),
                )
                && w > 0
                && h > 0
            {
                dims = Some((w, h));
            }

            i = right;
            continue;
        }

        i += 1;
    }

    dims.unwrap_or((1, 1))
}

#[inline(always)]
fn is_sprite_sheet_left_boundary(bytes: &[u8], left: usize) -> bool {
    left > 0 && matches!(bytes[left - 1], b' ' | b'\t' | b'\r' | b'\n' | b'_')
}

#[inline(always)]
fn is_sprite_sheet_right_boundary(bytes: &[u8], right: usize) -> bool {
    right == bytes.len()
        || matches!(
            bytes[right],
            b'.' | b' ' | b'\t' | b'\r' | b'\n' | b'(' | b'_'
        )
}

pub fn texture_source_frame_dims_from_real(
    texture_key: &str,
    real_w: u32,
    real_h: u32,
) -> (u32, u32) {
    let (source_w, source_h) = texture_source_dims_from_real(texture_key, real_w, real_h);
    let (frames_wide, frames_high) = parse_sprite_sheet_dims(texture_key);
    (source_w / frames_wide.max(1), source_h / frames_high.max(1))
}

pub fn texture_filename_has_multiframe_hint(filename: &str) -> bool {
    let bytes = filename.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b' ' {
            i += 1;
            continue;
        }
        let mut left = i + 1;
        while left < bytes.len() && bytes[left].is_ascii_digit() {
            left += 1;
        }
        if left == i + 1 || left >= bytes.len() || !matches!(bytes[left], b'x' | b'X') {
            i += 1;
            continue;
        }
        let mut right = left + 1;
        while right < bytes.len() && bytes[right].is_ascii_digit() {
            right += 1;
        }
        if right > left + 1 {
            return Path::new(filename)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.as_bytes().iter().all(u8::is_ascii_alphabetic));
        }
        i = right;
    }
    false
}

pub fn strip_sprite_hints(name: &str) -> String {
    let file_name = Path::new(name)
        .file_name()
        .and_then(|file| file.to_str())
        .unwrap_or(name);
    let without_ext = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let bytes = without_ext.as_bytes();
    let mut out = String::with_capacity(without_ext.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let mut left = i + 1;
            while left < bytes.len() && bytes[left].is_ascii_digit() {
                left += 1;
            }
            if left > i + 1 && left < bytes.len() && matches!(bytes[left], b'x' | b'X') {
                let mut right = left + 1;
                while right < bytes.len() && bytes[right].is_ascii_digit() {
                    right += 1;
                }
                if right > left + 1 {
                    i = right;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out.replace(" (doubleres)", "").trim().to_string()
}

pub fn direct_texture_key_path(raw: &str, key: &str) -> Option<PathBuf> {
    for candidate in [Path::new(raw), Path::new(key)] {
        if candidate.is_absolute() && candidate.is_file() {
            return Some(candidate.to_path_buf());
        }
    }

    #[cfg(unix)]
    for candidate in [raw, key] {
        if candidate.starts_with('/') {
            continue;
        }
        let absolute = PathBuf::from(format!("/{candidate}"));
        if absolute.is_file() {
            return Some(absolute);
        }
    }

    None
}

fn open_image_fallback_mode(
    path: &Path,
    warn_mismatch: bool,
) -> image::ImageResult<image::DynamicImage> {
    let hint = ImageFormat::from_path(path).ok();
    if let Some(fmt) = hint {
        let mut reader = ImageReader::open(path).map_err(image::ImageError::IoError)?;
        reader.set_format(fmt);
        if let Ok(img) = reader.decode() {
            return Ok(img);
        }
    }

    let guessed = ImageReader::open(path)
        .map_err(image::ImageError::IoError)?
        .with_guessed_format()?;
    let guessed_fmt = guessed.format();
    if let (Some(hint_fmt), Some(real_fmt)) = (hint, guessed_fmt)
        && hint_fmt != real_fmt
        && warn_mismatch
    {
        warn!(
            "Graphic file '{}' is really {:?}",
            path.to_string_lossy(),
            real_fmt
        );
    }
    guessed.decode()
}

pub fn open_image_fallback(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, true)
}

pub fn open_image_fallback_quiet(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, false)
}

pub fn ascii_ci_hash(input: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037u64;
    for &b in input.as_bytes() {
        hash ^= u64::from(b.to_ascii_lowercase());
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

pub fn media_path_key(path: &Path) -> Arc<str> {
    match path.to_string_lossy() {
        std::borrow::Cow::Borrowed(key) => Arc::from(key),
        std::borrow::Cow::Owned(key) => Arc::from(key),
    }
}

pub fn parse_texture_hints(raw: &str) -> TextureHints {
    let mut hints = TextureHints::default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return hints;
    }
    hints.raw = trimmed.to_string();
    if trimmed.eq_ignore_ascii_case("default") {
        return hints;
    }

    let has = |sub: &[u8]| has_ascii_case_insensitive_substr(trimmed.as_bytes(), sub);

    if has(b"32bpp") {
        hints.color_depth = Some(32);
    } else if has(b"16bpp") {
        hints.color_depth = Some(16);
    }
    if has(b"dither") {
        hints.dither = true;
    }
    if has(b"stretch") {
        hints.stretch = true;
    }
    if has(b"mipmaps") {
        hints.mipmaps = Some(true);
    }
    if has(b"nomipmaps") {
        hints.mipmaps = Some(false);
    }
    if has(b"grayscale") {
        hints.grayscale = true;
    }
    if has(b"alphamap") {
        hints.alphamap = true;
    }
    if has(b"doubleres") {
        hints.doubleres = true;
    }
    if has(b"nearest") || has(b"point") {
        hints.sampler_filter = Some(SamplerFilter::Nearest);
    }
    if has(b"linear") {
        hints.sampler_filter = Some(SamplerFilter::Linear);
    }
    if has(b"wrap") || has(b"repeat") {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }
    if has(b"clamp") {
        hints.sampler_wrap = Some(SamplerWrap::Clamp);
    }
    if hints.mipmaps == Some(true) && hints.sampler_wrap.is_none() {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }

    hints
}

#[inline(always)]
pub fn is_noteskin_texture_key(key: &str) -> bool {
    key.starts_with("noteskins/")
}

#[inline(always)]
pub fn initial_texture_sampler(key: &str, needs_repeat: bool) -> SamplerDesc {
    if needs_repeat {
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..SamplerDesc::default()
        }
    } else if is_noteskin_texture_key(key) {
        parse_texture_hints(key).sampler_desc()
    } else {
        SamplerDesc::default()
    }
}

#[inline(always)]
pub fn texture_key_sampler(hints: &TextureHints, needs_repeat: bool) -> SamplerDesc {
    if needs_repeat {
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..hints.sampler_desc()
        }
    } else {
        hints.sampler_desc()
    }
}

pub fn apply_texture_hints(image: &mut RgbaImage, hints: &TextureHints) {
    if !(hints.grayscale || hints.alphamap) {
        return;
    }

    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let lum = ((u16::from(r) * 30 + u16::from(g) * 59 + u16::from(b) * 11) / 100) as u8;
        if hints.alphamap {
            pixel.0 = [255, 255, 255, lum];
        } else {
            pixel.0 = [lum, lum, lum, a];
        }
    }
}

fn edge_alpha_rgb(image: &RgbaImage, reverse: bool) -> Option<[u8; 3]> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return None;
    }

    if reverse {
        for y in (0..height).rev() {
            for x in (0..width).rev() {
                let [r, g, b, a] = image.get_pixel(x, y).0;
                if a != 0 {
                    return Some([r, g, b]);
                }
            }
        }
    } else {
        for y in 0..height {
            for x in 0..width {
                let [r, g, b, a] = image.get_pixel(x, y).0;
                if a != 0 {
                    return Some([r, g, b]);
                }
            }
        }
    }
    None
}

pub fn fix_hidden_alpha(image: &mut RgbaImage) {
    let Some(first) = edge_alpha_rgb(image, false) else {
        return;
    };
    let Some(last) = edge_alpha_rgb(image, true) else {
        return;
    };
    let [r, g, b] = if first == last { first } else { [0, 0, 0] };
    for pixel in image.pixels_mut() {
        if pixel.0[3] == 0 {
            pixel.0[0] = r;
            pixel.0[1] = g;
            pixel.0[2] = b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_texture_resolution_hint_from_parenthetical_res_tag() {
        assert_eq!(
            parse_texture_resolution_hint("_miso light 15x15 (res 360x360).png"),
            Some((360, 360))
        );
    }

    #[test]
    fn parses_texture_resolution_hint_case_insensitively() {
        assert_eq!(
            parse_texture_resolution_hint("banner (ReS 512x160).png"),
            Some((512, 160))
        );
    }

    #[test]
    fn ignores_invalid_res_tags_until_a_valid_one() {
        assert_eq!(
            parse_texture_resolution_hint("sheet (res nope) (res 384 x170).png"),
            Some((384, 170))
        );
    }

    #[test]
    fn ignores_zero_sized_res_tags() {
        assert_eq!(parse_texture_resolution_hint("sheet (res 0x170).png"), None);
    }

    #[test]
    fn ignores_non_parenthetical_sheet_dims() {
        assert_eq!(
            parse_texture_resolution_hint("_miso light 16x7 doubleres.png"),
            None
        );
    }

    #[test]
    fn parses_texture_hints_case_insensitively() {
        let hints = parse_texture_hints("example (32BPP DITHER DOUBLEres MIPMAPS nearest wrap)");
        assert_eq!(hints.color_depth, Some(32));
        assert!(hints.dither);
        assert!(hints.doubleres);
        assert_eq!(hints.mipmaps, Some(true));
        assert_eq!(hints.sampler_filter, Some(SamplerFilter::Nearest));
        assert_eq!(hints.sampler_wrap, Some(SamplerWrap::Repeat));
    }

    #[test]
    fn apply_texture_hints_converts_grayscale() {
        let mut image = RgbaImage::from_raw(1, 1, vec![100, 150, 200, 77]).expect("test image");
        let hints = TextureHints {
            grayscale: true,
            ..TextureHints::default()
        };

        apply_texture_hints(&mut image, &hints);

        assert_eq!(image.get_pixel(0, 0).0, [140, 140, 140, 77]);
    }

    #[test]
    fn apply_texture_hints_converts_alphamap() {
        let mut image = RgbaImage::from_raw(1, 1, vec![100, 150, 200, 77]).expect("test image");
        let hints = TextureHints {
            alphamap: true,
            ..TextureHints::default()
        };

        apply_texture_hints(&mut image, &hints);

        assert_eq!(image.get_pixel(0, 0).0, [255, 255, 255, 140]);
    }

    #[test]
    fn texture_source_dims_honors_res_and_doubleres() {
        assert_eq!(
            texture_source_dims_from_real("tex (res 400x200) (doubleres).png", 800, 600),
            (200, 100)
        );
        assert_eq!(
            texture_source_dims_from_real("tex.png", 800, 600),
            (800, 600)
        );
    }

    #[test]
    fn texture_source_frame_dims_honors_sheet_and_source_dims() {
        assert_eq!(
            texture_source_frame_dims_from_real("sheet 4x2 (res 800x400) (doubleres).png", 1, 1),
            (100, 100)
        );
    }

    #[test]
    fn parses_itg_style_sprite_sheet_dims() {
        assert_eq!(parse_sprite_sheet_dims("grades/grades 1x19.png"), (1, 19));
        assert_eq!(
            parse_sprite_sheet_dims("_miso light 16x7 doubleres.png"),
            (16, 7)
        );
    }

    #[test]
    fn preserves_local_underscore_sprite_sheet_dims() {
        assert_eq!(
            parse_sprite_sheet_dims("submit/LoadingSpinner_10x3.png"),
            (10, 3)
        );
        assert_eq!(
            parse_sprite_sheet_dims("practice/note_field_bars_1x4_wrap.png"),
            (1, 4)
        );
    }

    #[test]
    fn ignores_resolution_labels_in_banner_names() {
        assert_eq!(
            parse_sprite_sheet_dims("1024x480-song-banner-background.png"),
            (1, 1)
        );
        assert_eq!(
            parse_sprite_sheet_dims("song-banner-1024x480-dimensions.png"),
            (1, 1)
        );
    }

    #[test]
    fn detects_space_delimited_multiframe_hints() {
        assert!(texture_filename_has_multiframe_hint("grades 1x19.png"));
        assert!(texture_filename_has_multiframe_hint(
            "_miso light 16x7 doubleres.png"
        ));
        assert!(!texture_filename_has_multiframe_hint(
            "LoadingSpinner_10x3.png"
        ));
        assert!(!texture_filename_has_multiframe_hint("banner 1024x480"));
    }

    #[test]
    fn strips_sprite_hints_for_display_labels() {
        assert_eq!(strip_sprite_hints("grades/grades 1x19.png"), "grades");
        assert_eq!(
            strip_sprite_hints("_miso light 16x7 doubleres.png"),
            "_miso light doubleres"
        );
        assert_eq!(
            strip_sprite_hints("practice/snap_display_icon_9x1 (doubleres).png"),
            "snap_display_icon_9x1"
        );
        assert_eq!(strip_sprite_hints("mine.png"), "mine");
    }

    #[test]
    fn direct_texture_key_path_accepts_absolute_keys() {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-texture-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Tap Note parts (mipmaps).png");
        std::fs::write(&path, [0u8]).unwrap();

        let key = path.to_string_lossy().replace('\\', "/");
        let resolved = direct_texture_key_path(&key, &key).unwrap();
        assert!(resolved.is_file());
        assert_eq!(resolved.file_name(), path.file_name());

        #[cfg(unix)]
        {
            let stripped = key.trim_start_matches('/');
            assert_eq!(
                direct_texture_key_path(stripped, stripped).as_deref(),
                Some(path.as_path())
            );
        }

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn fix_hidden_alpha_uses_matching_edge_rgb() {
        let mut image =
            RgbaImage::from_raw(3, 1, vec![255, 255, 255, 0, 12, 34, 56, 255, 9, 9, 9, 0])
                .expect("test image");

        fix_hidden_alpha(&mut image);

        assert_eq!(image.get_pixel(0, 0).0, [12, 34, 56, 0]);
        assert_eq!(image.get_pixel(2, 0).0, [12, 34, 56, 0]);
    }

    #[test]
    fn fix_hidden_alpha_uses_black_for_mixed_edges() {
        let mut image = RgbaImage::from_raw(
            4,
            1,
            vec![
                255, 255, 255, 0, 12, 34, 56, 255, 78, 90, 12, 255, 9, 9, 9, 0,
            ],
        )
        .expect("test image");

        fix_hidden_alpha(&mut image);

        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(image.get_pixel(3, 0).0, [0, 0, 0, 0]);
    }

    #[test]
    fn ascii_hash_is_case_insensitive() {
        assert_eq!(ascii_ci_hash("Texture.PNG"), ascii_ci_hash("texture.png"));
        assert_ne!(ascii_ci_hash("Texture.PNG"), ascii_ci_hash("texture2.png"));
    }

    #[test]
    fn initial_texture_sampler_uses_noteskin_hints_only_for_noteskins() {
        assert_eq!(
            initial_texture_sampler("noteskins/foo (nearest).png", false).filter,
            SamplerFilter::Nearest
        );
        assert_eq!(
            initial_texture_sampler("graphics/foo (nearest).png", false).filter,
            SamplerFilter::Linear
        );
    }

    #[test]
    fn texture_key_sampler_preserves_hints_with_repeat_override() {
        let hints = parse_texture_hints("foo (nearest).png");
        let sampler = texture_key_sampler(&hints, true);

        assert_eq!(sampler.filter, SamplerFilter::Nearest);
        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
    }

    #[test]
    fn media_path_key_uses_lossless_path_text() {
        assert_eq!(
            media_path_key(Path::new("assets/music/loop.ogg")).as_ref(),
            "assets/music/loop.ogg"
        );
    }
}
