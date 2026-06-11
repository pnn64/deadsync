//! StepManiaX pad GIF decoding and the preloaded animation registry.
//!
//! deadsync owns GIF decode for pad lighting (no SDK involvement): this module
//! turns GIF bytes into per-panel LED frame sequences and preloads every GIF
//! under `assets/smx/` into an immutable registry at startup or options time.
//! The lighting worker only ever holds `Arc` handles into the registry, so no
//! filesystem access or decoding can happen on the gameplay hot path.
//!
//! Formats (shared with the SDK and the stepmaniax-gif-maker tool):
//! - Full-pad: 23x24 (25-LED pads) or 14x15 (16-LED pads). Each panel is a
//!   block in a 3x3 grid with 1px gaps; the extra bottom row carries the loop
//!   marker (bottom-left pixel white-ish marks the frame to loop back to).
//! - Per-panel: 7x8 (25-LED) or 4x5 (16-LED) with the same trailing marker
//!   row, or bare 7x7 / 4x4 which loops the whole sequence. The 7x7 canvas is
//!   a staggered LED grid: an LED sits only where x and y share parity, the
//!   16 even/even LEDs first ("outer 4x4") then the 9 odd/odd ("inner 3x3").
//!
//! Frames are stored in the 25-LED layout (75 bytes) regardless of source
//! size; 16-LED sources leave the inner-ring bytes black, matching how the
//! SDK zero-fills the inner ring when downconverting in `set_lights`.

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbaImage};

use crate::panels::{LEDS_PER_PANEL, PANELS};

/// RGB bytes for one panel's LEDs in the 25-LED layout.
pub const PANEL_RGB_BYTES: usize = LEDS_PER_PANEL * 3;

/// One panel's LEDs for one frame: 25 RGB triples, outer 4x4 then inner 3x3.
pub type PanelFrame = [u8; PANEL_RGB_BYTES];

/// Which pad LED layout a GIF was authored for, from its `_16` / `_25`
/// filename suffix and its pixel dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadSize {
    Leds16,
    Leds25,
}

impl PadSize {
    pub const fn other(self) -> Self {
        match self {
            Self::Leds16 => Self::Leds25,
            Self::Leds25 => Self::Leds16,
        }
    }
}

/// A decoded full-pad background animation: one frame sequence per panel,
/// shared frame timing, and the loop point.
pub struct FullPadAnim {
    pub panels: [Vec<PanelFrame>; PANELS],
    /// Per-frame display time in seconds (realtime playback).
    pub durations: Vec<f32>,
    /// Frame index playback returns to after the last frame.
    pub loop_frame: usize,
    /// Beat-locked playback: one loop spans this many beats (from the `@Nb`
    /// filename suffix). `None` means realtime playback.
    pub beats_per_loop: Option<f32>,
}

/// A decoded per-panel judgement animation.
pub struct PanelAnim {
    pub frames: Vec<PanelFrame>,
    /// Per-frame display time in seconds.
    pub durations: Vec<f32>,
    /// Frame index playback returns to after the last frame (for sustained
    /// freeze/roll loops; one-shots simply stop at the end).
    pub loop_frame: usize,
}

// GIF decoding

struct DecodedGif {
    images: Vec<RgbaImage>,
    durations: Vec<f32>,
}

/// Decode GIF bytes into RGBA frames with per-frame durations. Mirrors the
/// SDK: a delay of 0 or anything in 28..=42ms snaps to exactly 1/30s, else
/// the GIF's own delay is kept.
fn decode_gif(data: &[u8]) -> Result<DecodedGif, &'static str> {
    let decoder = GifDecoder::new(Cursor::new(data)).map_err(|_| "the GIF couldn't be read")?;
    let mut images = Vec::new();
    let mut durations = Vec::new();
    for frame in decoder.into_frames().filter_map(|f| f.ok()) {
        let (numer, denom) = frame.delay().numer_denom_ms();
        let ms = numer as f32 / denom as f32;
        durations.push(if ms <= 0.0 || (28.0..=42.0).contains(&ms) {
            1.0 / 30.0
        } else {
            ms / 1000.0
        });
        images.push(frame.into_buffer());
    }
    if images.is_empty() {
        return Err("the GIF has no frames");
    }
    Ok(DecodedGif { images, durations })
}

/// First frame whose loop marker is set: the bottom-left pixel of the marker
/// row is white-ish (alpha 255, R >= 128). Defaults to 0 when none is marked.
fn marked_loop_frame(images: &[RgbaImage], marker_y: u32) -> usize {
    images
        .iter()
        .position(|img| {
            let px = img.get_pixel(0, marker_y);
            px[3] == 255 && px[0] >= 128
        })
        .unwrap_or(0)
}

// LED sampling

/// Sample one panel's LEDs from a 7x7-footprint block at (bx, by): the 16
/// even/even pixels (outer 4x4) then the 9 odd/odd pixels (inner 3x3).
fn sample_block_25(img: &RgbaImage, bx: u32, by: u32) -> PanelFrame {
    let mut out = [0u8; PANEL_RGB_BYTES];
    for dy in 0..4u32 {
        for dx in 0..4u32 {
            let led = (dy * 4 + dx) as usize;
            let px = img.get_pixel(bx + dx * 2, by + dy * 2);
            out[led * 3..led * 3 + 3].copy_from_slice(&px.0[..3]);
        }
    }
    for dy in 0..3u32 {
        for dx in 0..3u32 {
            let led = 16 + (dy * 3 + dx) as usize;
            let px = img.get_pixel(bx + dx * 2 + 1, by + dy * 2 + 1);
            out[led * 3..led * 3 + 3].copy_from_slice(&px.0[..3]);
        }
    }
    out
}

/// Sample one panel's LEDs from a 4x4 block at (bx, by). The inner-ring bytes
/// (LEDs 16..25) stay black; the pad's inner LEDs simply stay off.
fn sample_block_16(img: &RgbaImage, bx: u32, by: u32) -> PanelFrame {
    let mut out = [0u8; PANEL_RGB_BYTES];
    for dy in 0..4u32 {
        for dx in 0..4u32 {
            let led = (dy * 4 + dx) as usize;
            let px = img.get_pixel(bx + dx, by + dy);
            out[led * 3..led * 3 + 3].copy_from_slice(&px.0[..3]);
        }
    }
    out
}

/// Sample panel `p` (0..9, row-major 3x3) from a full-pad image.
fn sample_full_pad_panel(img: &RgbaImage, panel: usize, size: PadSize) -> PanelFrame {
    let col = (panel % 3) as u32;
    let row = (panel / 3) as u32;
    match size {
        // 14x15: 4x4 panel blocks at a 5px stride (1px gaps).
        PadSize::Leds16 => sample_block_16(img, col * 5, row * 5),
        // 23x24: 7x7 panel footprints at an 8px stride.
        PadSize::Leds25 => sample_block_25(img, col * 8, row * 8),
    }
}

const fn full_pad_size(w: u32, h: u32) -> Option<PadSize> {
    match (w, h) {
        (14, 15) => Some(PadSize::Leds16),
        (23, 24) => Some(PadSize::Leds25),
        _ => None,
    }
}

/// Per-panel canvas: size plus whether a trailing marker row is present.
const fn panel_canvas(w: u32, h: u32) -> Option<(PadSize, bool)> {
    match (w, h) {
        (4, 4) => Some((PadSize::Leds16, false)),
        (4, 5) => Some((PadSize::Leds16, true)),
        (7, 7) => Some((PadSize::Leds25, false)),
        (7, 8) => Some((PadSize::Leds25, true)),
        _ => None,
    }
}

/// Decode a full-pad background GIF (23x24 or 14x15). Returns the animation
/// and the size implied by its dimensions. `beats_per_loop` is left `None`;
/// the registry fills it from the filename.
pub fn decode_full_pad(data: &[u8]) -> Result<(FullPadAnim, PadSize), &'static str> {
    let gif = decode_gif(data)?;
    let first = &gif.images[0];
    let (w, h) = (first.width(), first.height());
    let size = full_pad_size(w, h).ok_or("a full-pad GIF must be 23x24 or 14x15")?;
    let loop_frame = marked_loop_frame(&gif.images, h - 1);
    let mut panels: [Vec<PanelFrame>; PANELS] =
        std::array::from_fn(|_| Vec::with_capacity(gif.images.len()));
    for img in &gif.images {
        for (panel, frames) in panels.iter_mut().enumerate() {
            frames.push(sample_full_pad_panel(img, panel, size));
        }
    }
    Ok((
        FullPadAnim {
            panels,
            durations: gif.durations,
            loop_frame,
            beats_per_loop: None,
        },
        size,
    ))
}

/// Decode a per-panel judgement GIF (7x8, 7x7, 4x5, or 4x4). Bare 7x7 / 4x4
/// canvases have no marker row, so they loop from frame 0.
pub fn decode_panel(data: &[u8]) -> Result<(PanelAnim, PadSize), &'static str> {
    let gif = decode_gif(data)?;
    let first = &gif.images[0];
    let (w, h) = (first.width(), first.height());
    let (size, has_marker_row) =
        panel_canvas(w, h).ok_or("a per-panel GIF must be 7x8, 7x7, 4x5, or 4x4")?;
    let loop_frame = if has_marker_row {
        marked_loop_frame(&gif.images, h - 1)
    } else {
        0
    };
    let frames = gif
        .images
        .iter()
        .map(|img| match size {
            PadSize::Leds16 => sample_block_16(img, 0, 0),
            PadSize::Leds25 => sample_block_25(img, 0, 0),
        })
        .collect();
    Ok((
        PanelAnim {
            frames,
            durations: gif.durations,
            loop_frame,
        },
        size,
    ))
}

// Filename parsing

/// Parse a GIF file stem like `song_select_25@2b` into its base name, pad
/// size, and optional beats-per-loop. Returns `None` for stems that don't
/// follow the `<name>_<16|25>[@<N>b]` convention.
fn parse_stem(stem: &str) -> Option<(&str, PadSize, Option<f32>)> {
    let (base, beats) = match stem.split_once('@') {
        Some((base, tail)) => (base, Some(parse_beats(tail)?)),
        None => (stem, None),
    };
    let (name, size) = base.rsplit_once('_')?;
    let size = match size {
        "16" => PadSize::Leds16,
        "25" => PadSize::Leds25,
        _ => return None,
    };
    (!name.is_empty()).then_some((name, size, beats))
}

/// Parse the `<N>b` tail of a beat-lock suffix into beats per loop.
fn parse_beats(tail: &str) -> Option<f32> {
    let n: f32 = tail.strip_suffix('b')?.parse().ok()?;
    (n.is_finite() && n > 0.0).then_some(n)
}

// Registry

/// Identity of one GIF in the registry. `pack: None` is the built-in
/// `common/` set; `Some` is a `user/<pack>/` directory.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    pack: Option<String>,
    name: String,
    size: PadSize,
}

/// Immutable preloaded SMX GIF registry: every full-pad background and
/// per-panel judgement under the asset root, decoded once. Lookups resolve
/// the pack and size fallback chain and hand out `Arc` clones, so the
/// lighting worker never touches the filesystem.
#[derive(Default)]
pub struct GifRegistry {
    backgrounds: HashMap<Key, Arc<FullPadAnim>>,
    judgements: HashMap<Key, Arc<PanelAnim>>,
}

impl GifRegistry {
    /// Scan and decode everything under `root` (the `assets/smx` directory).
    /// Unreadable or malformed files are logged and skipped; missing
    /// directories just yield an empty category.
    pub fn load(root: &Path) -> Self {
        let mut reg = Self::default();
        for_each_gif(&root.join("pad_animations"), |pack, path| {
            reg.load_background(pack, path);
        });
        for_each_gif(&root.join("judgements"), |pack, path| {
            reg.load_judgement(pack, path);
        });
        log::info!(
            "SMX gifs: loaded {} backgrounds, {} judgements from {}",
            reg.backgrounds.len(),
            reg.judgements.len(),
            root.display()
        );
        reg
    }

    /// Resolve a full-pad background by role (`default`, `song_select`,
    /// `gameplay`, ...). Falls back from the selected pack to `common` and
    /// from the requested size to the other one.
    pub fn background(
        &self,
        pack: Option<&str>,
        role: &str,
        size: PadSize,
    ) -> Option<Arc<FullPadAnim>> {
        resolve(&self.backgrounds, pack, role, size)
    }

    /// Resolve a per-panel judgement by name (`miss`, `freeze`, ...), with
    /// the same pack and size fallback as `background`.
    pub fn judgement(
        &self,
        pack: Option<&str>,
        name: &str,
        size: PadSize,
    ) -> Option<Arc<PanelAnim>> {
        resolve(&self.judgements, pack, name, size)
    }

    /// Sorted user pack names that supply at least one background.
    pub fn background_packs(&self) -> Vec<String> {
        user_packs(self.backgrounds.keys())
    }

    /// Sorted user pack names that supply at least one judgement.
    pub fn judgement_packs(&self) -> Vec<String> {
        user_packs(self.judgements.keys())
    }

    pub fn is_empty(&self) -> bool {
        self.backgrounds.is_empty() && self.judgements.is_empty()
    }

    fn load_background(&mut self, pack: Option<&str>, path: &Path) {
        let Some((bytes, name, size, beats)) = read_named_gif(path) else {
            return;
        };
        match decode_full_pad(&bytes) {
            Ok((mut anim, decoded_size)) if decoded_size == size => {
                anim.beats_per_loop = beats;
                self.backgrounds
                    .insert(key(pack, &name, size), Arc::new(anim));
            }
            Ok(_) => log::warn!(
                "SMX gifs: {} dimensions don't match its _16/_25 suffix; skipped",
                path.display()
            ),
            Err(e) => log::warn!("SMX gifs: {}: {e}; skipped", path.display()),
        }
    }

    fn load_judgement(&mut self, pack: Option<&str>, path: &Path) {
        let Some((bytes, name, size, beats)) = read_named_gif(path) else {
            return;
        };
        if beats.is_some() {
            log::warn!(
                "SMX gifs: {}: beat suffix is only used on backgrounds; ignored",
                path.display()
            );
        }
        match decode_panel(&bytes) {
            Ok((anim, decoded_size)) if decoded_size == size => {
                self.judgements
                    .insert(key(pack, &name, size), Arc::new(anim));
            }
            Ok(_) => log::warn!(
                "SMX gifs: {} dimensions don't match its _16/_25 suffix; skipped",
                path.display()
            ),
            Err(e) => log::warn!("SMX gifs: {}: {e}; skipped", path.display()),
        }
    }
}

fn key(pack: Option<&str>, name: &str, size: PadSize) -> Key {
    Key {
        pack: pack.map(str::to_owned),
        name: name.to_owned(),
        size,
    }
}

/// Pack and size fallback: selected pack at the requested then other size,
/// then `common` at the requested then other size.
fn resolve<T>(
    map: &HashMap<Key, Arc<T>>,
    pack: Option<&str>,
    name: &str,
    size: PadSize,
) -> Option<Arc<T>> {
    let lookup = |p: Option<&str>, s: PadSize| map.get(&key(p, name, s)).cloned();
    if let Some(p) = pack
        && let Some(anim) = lookup(Some(p), size).or_else(|| lookup(Some(p), size.other()))
    {
        return Some(anim);
    }
    lookup(None, size).or_else(|| lookup(None, size.other()))
}

fn user_packs<'a>(keys: impl Iterator<Item = &'a Key>) -> Vec<String> {
    let mut packs: Vec<String> = keys.filter_map(|k| k.pack.clone()).collect();
    packs.sort();
    packs.dedup();
    packs
}

/// Read a GIF file and parse its stem; logs and yields `None` on unreadable
/// files or stems that don't follow the naming convention.
fn read_named_gif(path: &Path) -> Option<(Vec<u8>, String, PadSize, Option<f32>)> {
    let stem = path.file_stem()?.to_str()?;
    let Some((name, size, beats)) = parse_stem(stem) else {
        log::warn!(
            "SMX gifs: {} doesn't match <name>_<16|25>[@<N>b].gif; skipped",
            path.display()
        );
        return None;
    };
    match fs::read(path) {
        Ok(bytes) => Some((bytes, name.to_owned(), size, beats)),
        Err(e) => {
            log::warn!("SMX gifs: failed to read {}: {e}; skipped", path.display());
            None
        }
    }
}

/// Visit every `.gif` in `<dir>/common/` and `<dir>/user/<pack>/`.
fn for_each_gif(dir: &Path, mut visit: impl FnMut(Option<&str>, &Path)) {
    visit_pack(&dir.join("common"), None, &mut visit);
    let Ok(entries) = fs::read_dir(dir.join("user")) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(pack) = path.file_name().and_then(|n| n.to_str())
        {
            let pack = pack.to_owned();
            visit_pack(&path, Some(&pack), &mut visit);
        }
    }
}

fn visit_pack(dir: &Path, pack: Option<&str>, visit: &mut impl FnMut(Option<&str>, &Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_gif = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
        if path.is_file() && is_gif {
            visit(pack, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    // Pixel sampling (pure, on raw images)

    #[test]
    fn full_pad_23x24_maps_outer_and_inner_leds() {
        let mut img = RgbaImage::new(23, 24);
        // Panel 0: outer LED 0 at (0,0), inner LED 0 at (1,1).
        img.put_pixel(0, 0, Rgba([128, 64, 32, 255]));
        img.put_pixel(1, 1, Rgba([10, 20, 30, 255]));
        // Panel 4 (col 1, row 1, base 8,8): outer LED 5 (dx 1, dy 1) at (10,10).
        img.put_pixel(10, 10, Rgba([1, 2, 3, 255]));

        let p0 = sample_full_pad_panel(&img, 0, PadSize::Leds25);
        assert_eq!(&p0[0..3], &[128, 64, 32]);
        assert_eq!(&p0[16 * 3..16 * 3 + 3], &[10, 20, 30]);

        let p4 = sample_full_pad_panel(&img, 4, PadSize::Leds25);
        assert_eq!(&p4[5 * 3..5 * 3 + 3], &[1, 2, 3]);
    }

    #[test]
    fn full_pad_14x15_maps_outer_leds_and_leaves_inner_black() {
        let mut img = RgbaImage::new(14, 15);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        // Panel 4 (col 1, row 1, base 5,5): LED 0 at (5,5).
        img.put_pixel(5, 5, Rgba([0, 255, 0, 255]));

        let p0 = sample_full_pad_panel(&img, 0, PadSize::Leds16);
        assert_eq!(&p0[0..3], &[255, 0, 0]);
        assert!(p0[16 * 3..].iter().all(|&b| b == 0), "inner ring stays off");

        let p4 = sample_full_pad_panel(&img, 4, PadSize::Leds16);
        assert_eq!(&p4[0..3], &[0, 255, 0]);
    }

    #[test]
    fn panel_7x7_samples_only_same_parity_cells() {
        let mut img = RgbaImage::new(7, 7);
        img.put_pixel(0, 0, Rgba([1, 1, 1, 255])); // outer LED 0
        img.put_pixel(1, 1, Rgba([2, 2, 2, 255])); // inner LED 16
        img.put_pixel(6, 6, Rgba([3, 3, 3, 255])); // outer LED 15 (dx 3, dy 3)
        img.put_pixel(5, 5, Rgba([4, 4, 4, 255])); // inner LED 24 (dx 2, dy 2)
        img.put_pixel(1, 0, Rgba([9, 9, 9, 255])); // opposite parity: no LED

        let f = sample_block_25(&img, 0, 0);
        assert_eq!(&f[0..3], &[1, 1, 1]);
        assert_eq!(&f[16 * 3..16 * 3 + 3], &[2, 2, 2]);
        assert_eq!(&f[15 * 3..15 * 3 + 3], &[3, 3, 3]);
        assert_eq!(&f[24 * 3..24 * 3 + 3], &[4, 4, 4]);
        // The off-parity pixel's colour must not land on any LED.
        assert!(f.chunks_exact(3).all(|led| led != [9, 9, 9]));
    }

    #[test]
    fn panel_4x4_maps_directly_and_leaves_inner_black() {
        let mut img = RgbaImage::new(4, 4);
        img.put_pixel(0, 0, Rgba([5, 6, 7, 255]));
        img.put_pixel(3, 3, Rgba([8, 9, 10, 255]));

        let f = sample_block_16(&img, 0, 0);
        assert_eq!(&f[0..3], &[5, 6, 7]);
        assert_eq!(&f[15 * 3..15 * 3 + 3], &[8, 9, 10]);
        assert!(f[16 * 3..].iter().all(|&b| b == 0));
    }

    // Filename parsing

    #[test]
    fn stem_parsing_extracts_name_size_and_beats() {
        assert_eq!(
            parse_stem("song_select_25@2b"),
            Some(("song_select", PadSize::Leds25, Some(2.0)))
        );
        assert_eq!(
            parse_stem("song_select_25@0.5b"),
            Some(("song_select", PadSize::Leds25, Some(0.5)))
        );
        assert_eq!(parse_stem("miss_16"), Some(("miss", PadSize::Leds16, None)));
        assert_eq!(
            parse_stem("fantastic_blue_25"),
            Some(("fantastic_blue", PadSize::Leds25, None))
        );
    }

    #[test]
    fn stem_parsing_rejects_malformed_stems() {
        for bad in [
            "default",       // no size suffix
            "default_24",    // unknown size
            "_25",           // empty name
            "default_25@b",  // empty beat count
            "default_25@2",  // missing 'b'
            "default_25@0b", // beats must be positive
            "default_25@-1b",
        ] {
            assert_eq!(parse_stem(bad), None, "{bad:?} should be rejected");
        }
    }

    // GIF decoding (round-trip through a real encoder)

    /// Encode solid-colour frames with explicit delays into GIF bytes.
    fn encode_gif(size: (u32, u32), frames: &[([u8; 3], u32)]) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};

        let mut buf = Cursor::new(Vec::new());
        {
            let mut enc = GifEncoder::new(&mut buf);
            for &(rgb, delay_ms) in frames {
                let img =
                    RgbaImage::from_pixel(size.0, size.1, Rgba([rgb[0], rgb[1], rgb[2], 255]));
                let frame = Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(delay_ms, 1));
                enc.encode_frames(std::iter::once(frame)).unwrap();
            }
        }
        buf.into_inner()
    }

    #[test]
    fn durations_keep_real_delays_and_snap_near_30hz() {
        let gif = encode_gif((23, 24), &[([0, 0, 0], 100), ([0, 0, 0], 33)]);
        let (anim, size) = decode_full_pad(&gif).unwrap();
        assert_eq!(size, PadSize::Leds25);
        assert_eq!(anim.durations.len(), 2);
        assert!((anim.durations[0] - 0.1).abs() < 1e-4);
        assert!((anim.durations[1] - 1.0 / 30.0).abs() < 1e-4);
    }

    #[test]
    fn loop_marker_picks_the_first_marked_frame() {
        // Frame 0 black (no marker), frame 1 solid red: its bottom-left
        // marker pixel has R >= 128, so the loop returns to frame 1.
        let gif = encode_gif((23, 24), &[([0, 0, 0], 100), ([255, 0, 0], 100)]);
        let (anim, _) = decode_full_pad(&gif).unwrap();
        assert_eq!(anim.loop_frame, 1);
        assert_eq!(anim.panels[0].len(), 2);
    }

    #[test]
    fn bare_panel_canvas_loops_from_frame_zero() {
        // A 7x7 canvas has no marker row, so even all-red frames loop at 0.
        let gif = encode_gif((7, 7), &[([255, 0, 0], 100), ([255, 0, 0], 100)]);
        let (anim, size) = decode_panel(&gif).unwrap();
        assert_eq!(size, PadSize::Leds25);
        assert_eq!(anim.loop_frame, 0);
    }

    #[test]
    fn panel_canvas_with_marker_row_honours_the_marker() {
        let gif = encode_gif((7, 8), &[([0, 0, 0], 100), ([255, 255, 255], 100)]);
        let (anim, _) = decode_panel(&gif).unwrap();
        assert_eq!(anim.loop_frame, 1);
    }

    #[test]
    fn decode_accepts_all_panel_canvases() {
        for size in [(7, 8), (7, 7), (4, 5), (4, 4)] {
            let gif = encode_gif(size, &[([0, 255, 0], 100)]);
            assert!(decode_panel(&gif).is_ok(), "{size:?} should decode");
        }
    }

    #[test]
    fn decode_rejects_wrong_sizes_and_corrupt_data() {
        let wrong = encode_gif((10, 10), &[([0, 0, 0], 100)]);
        assert!(decode_full_pad(&wrong).is_err());
        assert!(decode_panel(&wrong).is_err());
        assert!(decode_full_pad(b"not a gif").is_err());
        assert!(decode_panel(&[]).is_err());
    }

    // Registry resolution (on hand-built registries, no filesystem)

    fn dummy_full_pad() -> Arc<FullPadAnim> {
        Arc::new(FullPadAnim {
            panels: std::array::from_fn(|_| vec![[0u8; PANEL_RGB_BYTES]]),
            durations: vec![1.0 / 30.0],
            loop_frame: 0,
            beats_per_loop: None,
        })
    }

    #[test]
    fn resolution_prefers_pack_then_size_then_common() {
        let mut reg = GifRegistry::default();
        let entries = [
            (Some("foo"), PadSize::Leds25),
            (Some("foo"), PadSize::Leds16),
            (None, PadSize::Leds25),
            (None, PadSize::Leds16),
        ];
        for (pack, size) in entries {
            reg.backgrounds
                .insert(key(pack, "default", size), dummy_full_pad());
        }

        // Full registry: the selected pack at the requested size wins.
        let hit = |reg: &GifRegistry, pack, size| reg.background(pack, "default", size);
        assert!(hit(&reg, Some("foo"), PadSize::Leds16).is_some());

        // Drop the pack's _16: falls back to the pack's _25.
        reg.backgrounds
            .remove(&key(Some("foo"), "default", PadSize::Leds16));
        let got = hit(&reg, Some("foo"), PadSize::Leds16).unwrap();
        let pack_25 = reg.backgrounds[&key(Some("foo"), "default", PadSize::Leds25)].clone();
        assert!(Arc::ptr_eq(&got, &pack_25));

        // Drop the pack entirely: falls back to common at the requested size.
        reg.backgrounds
            .remove(&key(Some("foo"), "default", PadSize::Leds25));
        let got = hit(&reg, Some("foo"), PadSize::Leds16).unwrap();
        let common_16 = reg.backgrounds[&key(None, "default", PadSize::Leds16)].clone();
        assert!(Arc::ptr_eq(&got, &common_16));

        // Drop common _16 too: common at the other size.
        reg.backgrounds
            .remove(&key(None, "default", PadSize::Leds16));
        assert!(hit(&reg, Some("foo"), PadSize::Leds16).is_some());

        // Unknown role: nothing.
        assert!(
            reg.background(Some("foo"), "results", PadSize::Leds25)
                .is_none()
        );
    }

    #[test]
    fn unknown_pack_falls_back_to_common() {
        let mut reg = GifRegistry::default();
        reg.backgrounds
            .insert(key(None, "default", PadSize::Leds25), dummy_full_pad());
        assert!(
            reg.background(Some("nope"), "default", PadSize::Leds25)
                .is_some()
        );
        assert!(reg.background(None, "default", PadSize::Leds25).is_some());
    }

    // Registry load (end to end through the filesystem)

    #[test]
    fn load_scans_common_and_user_packs() {
        let root =
            std::env::temp_dir().join(format!("deadsync-smx-gifs-test-{}", std::process::id()));
        let bg_common = root.join("pad_animations/common");
        let bg_user = root.join("pad_animations/user/mypack");
        let j_common = root.join("judgements/common");
        fs::create_dir_all(&bg_common).unwrap();
        fs::create_dir_all(&bg_user).unwrap();
        fs::create_dir_all(&j_common).unwrap();

        let full_pad = encode_gif((23, 24), &[([0, 0, 255], 100)]);
        let panel = encode_gif((7, 8), &[([255, 0, 0], 100)]);
        fs::write(bg_common.join("default_25.gif"), &full_pad).unwrap();
        fs::write(bg_user.join("song_select_25@2b.gif"), &full_pad).unwrap();
        fs::write(j_common.join("miss_25.gif"), &panel).unwrap();
        // Junk that must be skipped without failing the load.
        fs::write(bg_common.join("notes.txt"), b"not a gif").unwrap();
        fs::write(bg_common.join("broken_25.gif"), b"garbage").unwrap();
        fs::write(j_common.join("badname.gif"), &panel).unwrap();

        let reg = GifRegistry::load(&root);
        fs::remove_dir_all(&root).unwrap();

        assert!(reg.background(None, "default", PadSize::Leds25).is_some());
        let song = reg
            .background(Some("mypack"), "song_select", PadSize::Leds25)
            .unwrap();
        assert_eq!(song.beats_per_loop, Some(2.0));
        assert!(reg.judgement(None, "miss", PadSize::Leds25).is_some());
        // The 16-LED request falls back to the only (_25) asset.
        assert!(reg.judgement(None, "miss", PadSize::Leds16).is_some());

        assert!(reg.background(None, "broken", PadSize::Leds25).is_none());
        assert!(reg.judgement(None, "badname", PadSize::Leds25).is_none());
        assert_eq!(reg.background_packs(), vec!["mypack".to_owned()]);
        assert!(reg.judgement_packs().is_empty());
    }
}
