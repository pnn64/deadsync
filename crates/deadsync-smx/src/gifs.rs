//! StepManiaX pad GIF decoding and the preloaded animation registry.
//!
//! deadsync owns GIF decode for pad lighting (no SDK involvement): this module
//! turns GIF bytes into per-panel LED frame sequences and preloads every GIF
//! under `assets/smx-pad-lights/` (full-pad backgrounds) and
//! `assets/smx-judge-lights/` (per-panel judgements) into an immutable
//! registry at startup or options time. Each root holds pack directories
//! under `common/` (shipped; `common/basic` is the default set) and `dance/`
//! (user packs). The lighting worker only ever holds `Arc` handles into the
//! registry, so no filesystem access or decoding can happen on the gameplay
//! hot path.
//!
//! Each pack directory may contain an optional `gifpack.ini` that declares
//! pack-level metadata. Currently only one key is recognised:
//!
//! ```toml
//! fallback = "basic"   # fall back to this pack when a gif is missing
//! ```
//!
//! When a pack has no `gifpack.ini`, or the file omits `fallback`, missing
//! gifs resolve to nothing rather than silently pulling from another pack.
//! `fallback = "none"` is accepted as an explicit opt-out.
//!
//! Formats (shared with the SDK and the stepmaniax-gif-maker tool):
//! - Full-pad: 23x24 (25-LED pads) or 14x15 (16-LED pads). Each panel is a
//!   block in a 3x3 grid with 1px gaps; the extra bottom row carries the
//!   markers.
//! - Per-panel: 7x8 (25-LED) or 4x5 (16-LED) with the same trailing marker
//!   row, or bare 7x7 / 4x4 which loops the whole sequence. The 7x7 canvas is
//!   a staggered LED grid: an LED sits only where x and y share parity, the
//!   16 even/even LEDs first ("outer 4x4") then the 9 odd/odd ("inner 3x3").
//!
//! The marker row carries one flag pixel per column (white-ish: alpha 255,
//! R >= 128). x 0 marks the frame playback loops back to. On per-panel GIFs,
//! x 1 marks the last frame of the loop region: frames after it form an outro
//! that plays on panel release (see `panels::OverlayDrive`). Backgrounds
//! ignore x 1.
//!
//! Frames are stored in the 25-LED layout (75 bytes) regardless of source
//! size; 16-LED sources leave the inner-ring bytes black, matching how the
//! SDK zero-fills the inner ring when downconverting in `set_lights`.

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
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
    /// Beat-locked playback: one loop spans this many beats (from the
    /// `@<beats>b<bpm>` filename suffix). Playback follows the live beat;
    /// `None` means realtime playback.
    pub beats_per_loop: Option<f32>,
}

/// One background gif for a (pack, role, size), plus the reference BPM from its
/// `@<beats>b<bpm>` suffix. A role can have several variants authored at
/// different reference tempos; `select_variant` picks the best fit for a song's
/// BPM (more frames at low tempo, fewer at high, to stay under the pad's 30fps
/// without dropping frames). `None` reference BPM is an untagged/single gif.
#[derive(Clone)]
pub struct BackgroundVariant {
    pub anim: Arc<FullPadAnim>,
    pub ref_bpm: Option<f32>,
}

/// Pick the background variant that best fits `song_bpm`: the smallest
/// reference BPM at or above the song's tempo (the densest gif that still plays
/// at or under the pad's 30fps cap), falling back to the highest reference BPM
/// when the song is faster than every variant (those half-time via the playback
/// cap). With no song BPM or no tagged variants, the lowest-reference (or only)
/// variant is used, deterministically regardless of load order.
pub fn select_variant(variants: &[BackgroundVariant], song_bpm: Option<f32>) -> Option<Arc<FullPadAnim>> {
    if let Some(bpm) = song_bpm.filter(|b| b.is_finite() && *b > 0.0) {
        let at_or_above = variants
            .iter()
            .filter_map(|v| v.ref_bpm.map(|y| (y, v)))
            .filter(|(y, _)| *y >= bpm)
            .min_by(|(a, _), (b, _)| a.total_cmp(b));
        let pick = at_or_above.or_else(|| {
            variants
                .iter()
                .filter_map(|v| v.ref_bpm.map(|y| (y, v)))
                .max_by(|(a, _), (b, _)| a.total_cmp(b))
        });
        if let Some((_, v)) = pick {
            return Some(v.anim.clone());
        }
    }
    // No BPM selection: the lowest-reference variant, or any untagged one.
    variants
        .iter()
        .min_by(|a, b| ref_bpm_sort_key(a).total_cmp(&ref_bpm_sort_key(b)))
        .map(|v| v.anim.clone())
}

/// Sort key making untagged variants (`ref_bpm` None) order after tagged ones,
/// so the no-selection fallback prefers a real tempo when both are present.
fn ref_bpm_sort_key(v: &BackgroundVariant) -> f32 {
    v.ref_bpm.unwrap_or(f32::INFINITY)
}

/// A decoded per-panel judgement animation.
pub struct PanelAnim {
    pub frames: Vec<PanelFrame>,
    /// Per-frame display time in seconds.
    pub durations: Vec<f32>,
    /// Frame index playback returns to after the last frame (for sustained
    /// freeze/roll loops; one-shots simply stop at the end).
    pub loop_frame: usize,
    /// Last frame of the loop region. Frames after it form an outro played on
    /// panel release; equals the last frame when the GIF has no outro marker.
    pub loop_end: usize,
}

impl PanelAnim {
    /// Whether frames exist after `loop_end`: an outro to play on release.
    pub fn has_outro(&self) -> bool {
        self.loop_end + 1 < self.frames.len()
    }
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

/// First frame whose marker pixel at `(x, marker_y)` is white-ish (alpha 255,
/// R >= 128). The marker row carries one flag pixel per column: x 0 is the
/// loop start, x 1 the loop end.
fn marked_frame(images: &[RgbaImage], x: u32, marker_y: u32) -> Option<usize> {
    images.iter().position(|img| {
        let px = img.get_pixel(x, marker_y);
        px[3] == 255 && px[0] >= 128
    })
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
    let loop_frame = marked_frame(&gif.images, 0, h - 1).unwrap_or(0);
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
/// canvases have no marker row, so they loop from frame 0 with no outro.
pub fn decode_panel(data: &[u8]) -> Result<(PanelAnim, PadSize), &'static str> {
    let gif = decode_gif(data)?;
    let first = &gif.images[0];
    let (w, h) = (first.width(), first.height());
    let (size, has_marker_row) =
        panel_canvas(w, h).ok_or("a per-panel GIF must be 7x8, 7x7, 4x5, or 4x4")?;
    let (loop_frame, loop_end) = if has_marker_row {
        let loop_frame = marked_frame(&gif.images, 0, h - 1).unwrap_or(0);
        // A loop end marked before the loop start is author error; ignore it.
        let loop_end = marked_frame(&gif.images, 1, h - 1)
            .filter(|&end| end >= loop_frame)
            .unwrap_or(gif.images.len() - 1);
        (loop_frame, loop_end)
    } else {
        (0, gif.images.len() - 1)
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
            loop_end,
        },
        size,
    ))
}

// Filename parsing

/// A parsed `@<beats>b<bpm>` beat-lock suffix: how many beats one loop spans,
/// and the optional authored reference tempo used to pick among variants.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BeatSpec {
    beats: f32,
    ref_bpm: Option<f32>,
}

/// Parse a GIF file stem like `song_select_25@1b120` into its base name, pad
/// size, and optional beat-lock spec. Returns `None` for stems that don't
/// follow the `<name>_<16|25>[@<beats>b<bpm>]` convention.
fn parse_stem(stem: &str) -> Option<(String, PadSize, Option<BeatSpec>)> {
    // Format: <role>_<16|25>[@<tag>]
    //
    // We locate the size marker first (rightmost _16/_25 that is followed by
    // nothing or an @-tag), then classify the tag:
    //
    //   - tag starts with a digit, '-', or '.': BPM spec (<N>b[<bpm>]); parse
    //     or reject if malformed.
    //   - tag starts with anything else (letter, '*', …): role modifier (e.g. a
    //     grade suffix: @S+, @B+, @*****); appended to the role name.
    //
    // This order is necessary because grade letters include 'B', which is also
    // the beat/bpm separator, so splitting on '@' first and checking for 'b'/'B'
    // would misclassify @B+ as a malformed BPM spec.
    fn find_size_pos(stem: &str, suffix: &str) -> Option<usize> {
        let mut end = stem.len();
        loop {
            let pos = stem[..end].rfind(suffix)?;
            let after = &stem[pos + suffix.len()..];
            if after.is_empty() || after.starts_with('@') {
                return Some(pos);
            }
            if pos == 0 {
                return None;
            }
            end = pos;
        }
    }
    let (size_pos, size) = match (find_size_pos(stem, "_25"), find_size_pos(stem, "_16")) {
        (Some(p25), Some(p16)) if p25 >= p16 => (p25, PadSize::Leds25),
        (Some(_), Some(p16)) => (p16, PadSize::Leds16),
        (Some(p25), None) => (p25, PadSize::Leds25),
        (None, Some(p16)) => (p16, PadSize::Leds16),
        (None, None) => return None,
    };
    let name_base = &stem[..size_pos];
    if name_base.is_empty() {
        return None;
    }
    let rest = &stem[size_pos + 3..]; // characters after "_25" or "_16"
    if rest.is_empty() {
        return Some((name_base.to_owned(), size, None));
    }
    let tag = rest.strip_prefix('@')?;
    let looks_like_bpm = matches!(tag.chars().next(), Some(c) if c.is_ascii_digit() || c == '-' || c == '.');
    if looks_like_bpm {
        Some((name_base.to_owned(), size, Some(parse_beats(tag)?)))
    } else {
        Some((format!("{name_base}@{tag}"), size, None))
    }
}

/// Parse the `<beats>b<bpm>` tail of a beat-lock suffix (e.g. `1b120` = one
/// beat at a 120bpm reference). The bpm half is the authored reference tempo,
/// used to select among several variants of the same role; playback still
/// paces itself from the live beat. A bare `<N>b` (no bpm) is also accepted.
fn parse_beats(tail: &str) -> Option<BeatSpec> {
    let (beats, bpm) = match tail.split_once(['B', 'b']) {
        Some((beats, "")) => (beats, None),
        Some((beats, bpm)) => (beats, Some(bpm)),
        None => return None,
    };
    let ref_bpm = match bpm {
        Some(bpm) => {
            let bpm: f32 = bpm.parse().ok()?;
            if !bpm.is_finite() || bpm <= 0.0 {
                return None;
            }
            Some(bpm)
        }
        None => None,
    };
    let beats: f32 = beats.parse().ok()?;
    (beats.is_finite() && beats > 0.0).then_some(BeatSpec { beats, ref_bpm })
}

// Registry

/// The shipped default pack (`common/basic`), the end of every fallback chain.
pub const DEFAULT_PACK: &str = "basic";

/// Identity of one GIF in the registry. Every GIF lives in a named pack
/// directory under `common/` (shipped) or `dance/` (user); a user pack
/// shadows a shipped pack of the same name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    pack: String,
    name: String,
    size: PadSize,
}

/// Immutable preloaded SMX GIF registry: every full-pad background and
/// per-panel judgement under the asset root, decoded once. Lookups resolve
/// the size fallback and optional pack fallback declared in `gifpack.ini`,
/// then hand out `Arc` clones so the lighting worker never touches the
/// filesystem.
#[derive(Default)]
pub struct GifRegistry {
    /// A role can hold several BPM variants (see `BackgroundVariant`), so the
    /// value is a list resolved per song; most roles have just one.
    backgrounds: HashMap<Key, Vec<BackgroundVariant>>,
    judgements: HashMap<Key, Arc<PanelAnim>>,
    /// Pack-level fallback declared in `gifpack.ini`: when a gif is missing
    /// from the selected pack, try this pack before returning `None`. Packs
    /// with no `gifpack.ini` (or no `fallback` key) have no entry here, and
    /// missing gifs resolve to nothing.
    pack_fallbacks: HashMap<String, String>,
}

impl GifRegistry {
    /// Scan and decode everything under `root` (the `assets` directory):
    /// full-pad backgrounds from `smx-pad-lights/` and per-panel judgements
    /// from `smx-judge-lights/`. Unreadable or malformed files are logged and
    /// skipped; missing directories just yield an empty category.
    ///
    /// `gifpack.ini` files are also read from each pack directory. `dance/`
    /// packs shadow `common/` packs of the same name for both gifs and metadata.
    pub fn load(root: &Path) -> Self {
        let mut reg = Self::default();
        for dir in [root.join("smx-pad-lights"), root.join("smx-judge-lights")] {
            for (pack, fallback) in load_pack_fallbacks(&dir) {
                reg.pack_fallbacks.entry(pack).or_insert(fallback);
            }
        }
        for_each_gif(&root.join("smx-pad-lights"), |pack, path| {
            reg.load_background(pack, path);
        });
        for_each_gif(&root.join("smx-judge-lights"), |pack, path| {
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
    /// `gameplay`, ...). Tries the selected pack first (both sizes), then the
    /// pack's declared `gifpack.ini` fallback if any, then nothing. Picks the
    /// variant best fitting `song_bpm` among those found (see `select_variant`).
    pub fn background(
        &self,
        pack: Option<&str>,
        role: &str,
        size: PadSize,
        song_bpm: Option<f32>,
    ) -> Option<Arc<FullPadAnim>> {
        lookup(&self.backgrounds, &self.pack_fallbacks, pack, role, size)
            .and_then(|v| select_variant(v, song_bpm))
    }

    /// Resolve a per-panel judgement by name (`bad`, `freeze`, ...), with the
    /// same pack and size lookup as `background`.
    pub fn judgement(
        &self,
        pack: Option<&str>,
        name: &str,
        size: PadSize,
    ) -> Option<Arc<PanelAnim>> {
        lookup(&self.judgements, &self.pack_fallbacks, pack, name, size).cloned()
    }

    /// Sorted pack names (other than the default) that supply at least one
    /// background.
    pub fn background_packs(&self) -> Vec<String> {
        named_packs(self.backgrounds.keys())
    }

    /// Sorted pack names (other than the default) that supply at least one
    /// judgement.
    pub fn judgement_packs(&self) -> Vec<String> {
        named_packs(self.judgements.keys())
    }

    pub fn is_empty(&self) -> bool {
        self.backgrounds.is_empty() && self.judgements.is_empty()
    }

    fn load_background(&mut self, pack: &str, path: &Path) {
        let Some((bytes, name, size, beats)) = read_named_gif(path) else {
            return;
        };
        match decode_full_pad(&bytes) {
            Ok((mut anim, decoded_size)) if decoded_size == size => {
                anim.beats_per_loop = beats.map(|b| b.beats);
                self.backgrounds
                    .entry(key(pack, &name, size))
                    .or_default()
                    .push(BackgroundVariant {
                        anim: Arc::new(anim),
                        ref_bpm: beats.and_then(|b| b.ref_bpm),
                    });
            }
            Ok(_) => log::warn!(
                "SMX gifs: {} dimensions don't match its _16/_25 suffix; skipped",
                path.display()
            ),
            Err(e) => log::warn!("SMX gifs: {}: {e}; skipped", path.display()),
        }
    }

    fn load_judgement(&mut self, pack: &str, path: &Path) {
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

fn key(pack: &str, name: &str, size: PadSize) -> Key {
    Key {
        pack: pack.to_owned(),
        name: name.to_owned(),
        size,
    }
}

/// Look up a gif in a map, respecting the size fallback and the pack's
/// `gifpack.ini` fallback if declared.
///
/// Resolution order:
/// 1. Selected pack at the requested size, then the other size.
/// 2. If still not found and the pack has a declared fallback pack (from
///    `gifpack.ini`), try that pack at both sizes.
/// 3. Return `None`.
///
/// When no pack is selected (or the selected pack is the default), only the
/// default pack is tried. Returns a reference to the stored value so callers
/// clone or select from it.
fn lookup<'a, V>(
    map: &'a HashMap<Key, V>,
    fallbacks: &HashMap<String, String>,
    pack: Option<&str>,
    name: &str,
    size: PadSize,
) -> Option<&'a V> {
    let get = |p: &str, s: PadSize| map.get(&key(p, name, s));
    if let Some(p) = pack.filter(|p| *p != DEFAULT_PACK) {
        if let Some(v) = get(p, size).or_else(|| get(p, size.other())) {
            return Some(v);
        }
        // Not in the selected pack; use its declared fallback if any.
        if let Some(fb) = fallbacks.get(p) {
            return get(fb, size).or_else(|| get(fb, size.other()));
        }
        return None;
    }
    // No pack selected (or selected pack is the default): use the default pack.
    get(DEFAULT_PACK, size).or_else(|| get(DEFAULT_PACK, size.other()))
}

fn named_packs<'a>(keys: impl Iterator<Item = &'a Key>) -> Vec<String> {
    let mut packs: Vec<String> = keys
        .filter(|k| k.pack != DEFAULT_PACK)
        .map(|k| k.pack.clone())
        .collect();
    packs.sort();
    packs.dedup();
    packs
}

/// Load all per-song / per-pack background variants for `role` from `dir` (a
/// `smx-pad-lights/` subfolder of a song or pack folder). Scans `dir` for
/// `<role>_<size>[@<beats>b<bpm>].gif` files, preferring the requested `size`
/// and falling back to the other only if no requested-size file exists, and
/// decodes each into a `BackgroundVariant` (so several BPM variants resolve the
/// same as the global packs). Returns an empty vec when the folder is absent or
/// has no matching, decodable gif; malformed matches are logged and skipped.
/// The app caches the result per (dir, role), so this touches the filesystem
/// only the first time a folder is seen.
pub fn load_scoped_background(dir: &Path, role: &str, size: PadSize) -> Vec<BackgroundVariant> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    // Group matching files by their pad size; use the requested size if any
    // exist, else the other size. (A folder usually has just the one size.)
    let mut requested: Vec<PathBuf> = Vec::new();
    let mut other: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some((name, stem_size, _)) = parse_stem(stem) else {
            continue;
        };
        if name != role {
            continue;
        }
        if stem_size == size {
            requested.push(path);
        } else {
            other.push(path);
        }
    }
    let paths = if requested.is_empty() {
        other
    } else {
        requested
    };
    let mut variants = Vec::new();
    for path in paths {
        let Some((bytes, _, parsed_size, beats)) = read_named_gif(&path) else {
            continue;
        };
        match decode_full_pad(&bytes) {
            Ok((mut anim, decoded_size)) if decoded_size == parsed_size => {
                anim.beats_per_loop = beats.map(|b| b.beats);
                variants.push(BackgroundVariant {
                    anim: Arc::new(anim),
                    ref_bpm: beats.and_then(|b| b.ref_bpm),
                });
            }
            Ok(_) => log::warn!(
                "SMX gifs: {} dimensions don't match its _16/_25 suffix; skipped",
                path.display()
            ),
            Err(e) => log::warn!("SMX gifs: {}: {e}; skipped", path.display()),
        }
    }
    variants
}

/// Read a GIF file and parse its stem; logs and yields `None` on unreadable
/// files or stems that don't follow the naming convention.
fn read_named_gif(path: &Path) -> Option<(Vec<u8>, String, PadSize, Option<BeatSpec>)> {
    let stem = path.file_stem()?.to_str()?;
    let Some((name, size, beats)) = parse_stem(stem) else {
        log::warn!(
            "SMX gifs: {} doesn't match <name>_<16|25>[@<beats>b<bpm>].gif; skipped",
            path.display()
        );
        return None;
    };
    match fs::read(path) {
        Ok(bytes) => Some((bytes, name, size, beats)),
        Err(e) => {
            log::warn!("SMX gifs: failed to read {}: {e}; skipped", path.display());
            None
        }
    }
}

/// Parse the contents of a `gifpack.ini` file and return the declared
/// fallback pack name. Returns `None` when no `fallback` key is present or
/// its value is `"none"`. Unrecognised keys and comment lines are ignored.
fn parse_gifpack_ini(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != "fallback" {
            continue;
        }
        let v = v.trim().trim_matches('"');
        return (!v.is_empty() && !v.eq_ignore_ascii_case("none")).then(|| v.to_owned());
    }
    None
}

/// Scan `<dir>/common/*/gifpack.ini` and `<dir>/dance/*/gifpack.ini` and
/// return the per-pack fallback map. `dance/` packs override `common/` packs
/// of the same name, matching the gif shadowing rule.
fn load_pack_fallbacks(dir: &Path) -> HashMap<String, String> {
    let mut fallbacks = HashMap::new();
    for group in ["common", "dance"] {
        let Ok(entries) = fs::read_dir(dir.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(pack) = path.file_name().and_then(|n| n.to_str())
            {
                let toml_path = path.join("gifpack.ini");
                if let Ok(content) = fs::read_to_string(&toml_path) {
                    if let Some(fallback) = parse_gifpack_ini(&content) {
                        fallbacks.insert(pack.to_owned(), fallback);
                    }
                }
            }
        }
    }
    fallbacks
}

/// Visit every `.gif` in `<dir>/common/<pack>/` and `<dir>/dance/<pack>/`.
/// `common/` holds shipped packs (`basic` at minimum), `dance/` user packs;
/// scanning `common` first lets a user pack shadow a shipped one of the same
/// name.
fn for_each_gif(dir: &Path, mut visit: impl FnMut(&str, &Path)) {
    for group in ["common", "dance"] {
        let Ok(entries) = fs::read_dir(dir.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(pack) = path.file_name().and_then(|n| n.to_str())
            {
                let pack = pack.to_owned();
                visit_pack(&path, &pack, &mut visit);
            }
        }
    }
}

fn visit_pack(dir: &Path, pack: &str, visit: &mut impl FnMut(&str, &Path)) {
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
        let spec = |beats, ref_bpm| BeatSpec { beats, ref_bpm };
        let s = |name: &str| name.to_owned();
        // The full form: beats at an authored reference bpm.
        assert_eq!(
            parse_stem("song_select_25@1B120"),
            Some((s("song_select"), PadSize::Leds25, Some(spec(1.0, Some(120.0)))))
        );
        assert_eq!(
            parse_stem("song_select_25@0.5B90"),
            Some((s("song_select"), PadSize::Leds25, Some(spec(0.5, Some(90.0)))))
        );
        // Bare beat counts (no bpm) are accepted too.
        assert_eq!(
            parse_stem("song_select_25@2b"),
            Some((s("song_select"), PadSize::Leds25, Some(spec(2.0, None))))
        );
        assert_eq!(parse_stem("bad_16"), Some((s("bad"), PadSize::Leds16, None)));
        assert_eq!(
            parse_stem("fantastic_blue_25"),
            Some((s("fantastic_blue"), PadSize::Leds25, None))
        );
        // Grade-modifier role tags: @<grade> after the size becomes part of the name.
        assert_eq!(
            parse_stem("results_25@S+"),
            Some((s("results@S+"), PadSize::Leds25, None))
        );
        assert_eq!(
            parse_stem("results_25@B+"),
            Some((s("results@B+"), PadSize::Leds25, None))
        );
        assert_eq!(
            parse_stem("results_25@*****"),
            Some((s("results@*****"), PadSize::Leds25, None))
        );
    }

    #[test]
    fn stem_parsing_rejects_malformed_stems() {
        for bad in [
            "default",           // no size suffix
            "default_24",        // unknown size
            "_25",               // empty name
            "default_25@2",      // digit-starting tag with no 'b' separator
            "default_25@0B120",  // beats must be positive
            "default_25@-1B120", // negative beats
            "default_25@1B0",    // bpm, when given, must be positive
            "default_25@1B-120", // negative bpm
            "default_25@1Bfast", // bpm must be numeric
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

    /// Encode black 7x8 frames with white marker pixels written into the
    /// bottom row: `markers[f]` lists the marked x positions of frame `f`.
    fn encode_marked_panel_gif(markers: &[&[u32]]) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};

        let mut buf = Cursor::new(Vec::new());
        {
            let mut enc = GifEncoder::new(&mut buf);
            for xs in markers {
                let mut img = RgbaImage::from_pixel(7, 8, Rgba([0, 0, 0, 255]));
                for &x in *xs {
                    img.put_pixel(x, 7, Rgba([255, 255, 255, 255]));
                }
                let frame = Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(100, 1));
                enc.encode_frames(std::iter::once(frame)).unwrap();
            }
        }
        buf.into_inner()
    }

    #[test]
    fn outro_marker_sets_the_loop_end() {
        // Loop start on frame 1 (x 0), loop end on frame 2 (x 1): frame 3 is
        // the outro.
        let gif = encode_marked_panel_gif(&[&[], &[0], &[1], &[]]);
        let (anim, _) = decode_panel(&gif).unwrap();
        assert_eq!(anim.loop_frame, 1);
        assert_eq!(anim.loop_end, 2);
        assert!(anim.has_outro());
    }

    #[test]
    fn both_markers_on_one_frame_make_a_single_frame_loop() {
        let gif = encode_marked_panel_gif(&[&[], &[0, 1], &[]]);
        let (anim, _) = decode_panel(&gif).unwrap();
        assert_eq!(anim.loop_frame, 1);
        assert_eq!(anim.loop_end, 1);
        assert!(anim.has_outro());
    }

    #[test]
    fn loop_end_before_the_loop_start_is_ignored() {
        let gif = encode_marked_panel_gif(&[&[1], &[], &[0]]);
        let (anim, _) = decode_panel(&gif).unwrap();
        assert_eq!(anim.loop_frame, 2);
        assert_eq!(anim.loop_end, 2, "invalid marker falls back to the end");
        assert!(!anim.has_outro());
    }

    #[test]
    fn unmarked_gifs_have_no_outro() {
        for gif in [
            encode_marked_panel_gif(&[&[0], &[]]),
            encode_gif((7, 7), &[([255, 0, 0], 100), ([255, 0, 0], 100)]),
        ] {
            let (anim, _) = decode_panel(&gif).unwrap();
            assert_eq!(anim.loop_end, anim.frames.len() - 1);
            assert!(!anim.has_outro());
        }
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

    /// A single untagged background variant (one gif, no BPM reference).
    fn dummy_variants() -> Vec<BackgroundVariant> {
        vec![BackgroundVariant {
            anim: dummy_full_pad(),
            ref_bpm: None,
        }]
    }

    #[test]
    fn resolution_prefers_pack_size_then_other_size() {
        let mut reg = GifRegistry::default();
        for (pack, size) in [("foo", PadSize::Leds25), ("foo", PadSize::Leds16)] {
            reg.backgrounds
                .insert(key(pack, "default", size), dummy_variants());
        }
        let hit = |reg: &GifRegistry, pack, size| reg.background(pack, "default", size, None);

        // Pack has both sizes: the requested size wins.
        assert!(hit(&reg, Some("foo"), PadSize::Leds16).is_some());

        // Drop _16: pack falls back to its own _25.
        reg.backgrounds
            .remove(&key("foo", "default", PadSize::Leds16));
        let got = hit(&reg, Some("foo"), PadSize::Leds16).unwrap();
        let pack_25 = reg.backgrounds[&key("foo", "default", PadSize::Leds25)][0]
            .anim
            .clone();
        assert!(Arc::ptr_eq(&got, &pack_25));
    }

    #[test]
    fn pack_without_gifpack_toml_does_not_fall_back_to_basic() {
        let mut reg = GifRegistry::default();
        reg.backgrounds
            .insert(key(DEFAULT_PACK, "default", PadSize::Leds25), dummy_variants());
        // "foo" pack exists but has no fallback declared.
        assert!(
            reg.background(Some("foo"), "default", PadSize::Leds25, None)
                .is_none(),
            "no fallback => None, not basic"
        );
    }

    #[test]
    fn pack_with_gifpack_toml_fallback_reaches_declared_pack() {
        let mut reg = GifRegistry::default();
        reg.backgrounds
            .insert(key(DEFAULT_PACK, "default", PadSize::Leds25), dummy_variants());
        // "foo" pack declares fallback = "basic".
        reg.pack_fallbacks
            .insert("foo".to_owned(), DEFAULT_PACK.to_owned());

        // "foo" has no "default" gif, so it falls through to basic.
        let got = reg
            .background(Some("foo"), "default", PadSize::Leds25, None)
            .unwrap();
        let basic = reg.backgrounds[&key(DEFAULT_PACK, "default", PadSize::Leds25)][0]
            .anim
            .clone();
        assert!(Arc::ptr_eq(&got, &basic));

        // "foo" does have a gif: own gif wins over the fallback.
        reg.backgrounds
            .insert(key("foo", "default", PadSize::Leds25), dummy_variants());
        let got2 = reg
            .background(Some("foo"), "default", PadSize::Leds25, None)
            .unwrap();
        let foo = reg.backgrounds[&key("foo", "default", PadSize::Leds25)][0]
            .anim
            .clone();
        assert!(Arc::ptr_eq(&got2, &foo));
    }

    #[test]
    fn no_pack_and_basic_by_name_both_use_the_default_pack() {
        let mut reg = GifRegistry::default();
        reg.backgrounds
            .insert(key(DEFAULT_PACK, "default", PadSize::Leds25), dummy_variants());
        assert!(reg.background(None, "default", PadSize::Leds25, None).is_some());
        // Selecting basic by name is the same as selecting nothing.
        assert!(
            reg.background(Some(DEFAULT_PACK), "default", PadSize::Leds25, None)
                .is_some()
        );
        // Unknown pack with no fallback: nothing.
        assert!(
            reg.background(Some("nope"), "default", PadSize::Leds25, None)
                .is_none()
        );
    }

    // gifpack.ini parsing

    #[test]
    fn parse_gifpack_ini_extracts_fallback() {
        assert_eq!(
            parse_gifpack_ini("fallback = \"basic\""),
            Some("basic".to_owned())
        );
        // Whitespace and comment lines are tolerated.
        assert_eq!(
            parse_gifpack_ini("# my pack\nfallback = \"basic\"\n"),
            Some("basic".to_owned())
        );
        // "none" and absent key both yield None.
        assert_eq!(parse_gifpack_ini("fallback = \"none\""), None);
        assert_eq!(parse_gifpack_ini(""), None);
        assert_eq!(parse_gifpack_ini("# just a comment"), None);
        // Unknown keys are ignored.
        assert_eq!(
            parse_gifpack_ini("name = \"cool pack\"\nfallback = \"basic\""),
            Some("basic".to_owned())
        );
    }

    #[test]
    fn variant_selection_picks_best_fit_by_bpm() {
        // Two variants of one role: reference 113bpm and 225bpm.
        let variants = vec![
            BackgroundVariant {
                anim: dummy_full_pad(),
                ref_bpm: Some(113.0),
            },
            BackgroundVariant {
                anim: dummy_full_pad(),
                ref_bpm: Some(225.0),
            },
        ];
        let v113 = variants[0].anim.clone();
        let v225 = variants[1].anim.clone();
        let pick = |bpm| select_variant(&variants, Some(bpm)).unwrap();
        // At/under 113: the 113 variant (densest that stays under the cap).
        assert!(Arc::ptr_eq(&pick(60.0), &v113));
        assert!(Arc::ptr_eq(&pick(113.0), &v113));
        // Between them: the smallest reference at or above the tempo (225).
        assert!(Arc::ptr_eq(&pick(150.0), &v225));
        assert!(Arc::ptr_eq(&pick(225.0), &v225));
        // Faster than all variants: the highest reference (it half-times).
        assert!(Arc::ptr_eq(&pick(300.0), &v225));
        // No BPM given: the lowest reference, deterministically.
        assert!(Arc::ptr_eq(
            &select_variant(&variants, None).unwrap(),
            &v113
        ));
        // Empty list resolves to nothing.
        assert!(select_variant(&[], Some(120.0)).is_none());
    }

    // Registry load (end to end through the filesystem)

    #[test]
    fn load_scans_common_and_dance_packs() {
        let root =
            std::env::temp_dir().join(format!("deadsync-smx-gifs-test-{}", std::process::id()));
        let bg_basic = root.join("smx-pad-lights/common/basic");
        let bg_user = root.join("smx-pad-lights/dance/mypack");
        let j_basic = root.join("smx-judge-lights/common/basic");
        fs::create_dir_all(&bg_basic).unwrap();
        fs::create_dir_all(&bg_user).unwrap();
        fs::create_dir_all(&j_basic).unwrap();

        let full_pad = encode_gif((23, 24), &[([0, 0, 255], 100)]);
        let panel = encode_gif((7, 8), &[([255, 0, 0], 100)]);
        fs::write(bg_basic.join("default_25.gif"), &full_pad).unwrap();
        fs::write(bg_user.join("song_select_25@1B120.gif"), &full_pad).unwrap();
        fs::write(j_basic.join("bad_25.gif"), &panel).unwrap();
        // Junk that must be skipped without failing the load.
        fs::write(bg_basic.join("notes.txt"), b"not a gif").unwrap();
        fs::write(bg_basic.join("broken_25.gif"), b"garbage").unwrap();
        fs::write(j_basic.join("badname.gif"), &panel).unwrap();
        // gifpack.ini: mypack declares basic as its fallback.
        fs::write(bg_user.join("gifpack.ini"), b"fallback = \"basic\"\n").unwrap();

        let reg = GifRegistry::load(&root);
        fs::remove_dir_all(&root).unwrap();

        assert!(reg.background(None, "default", PadSize::Leds25, None).is_some());
        let song = reg
            .background(Some("mypack"), "song_select", PadSize::Leds25, None)
            .unwrap();
        assert_eq!(song.beats_per_loop, Some(1.0));
        assert!(reg.judgement(None, "bad", PadSize::Leds25).is_some());
        // The 16-LED request falls back to the only (_25) asset.
        assert!(reg.judgement(None, "bad", PadSize::Leds16).is_some());

        assert!(reg.background(None, "broken", PadSize::Leds25, None).is_none());
        assert!(reg.judgement(None, "badname", PadSize::Leds25).is_none());
        assert_eq!(reg.background_packs(), vec!["mypack".to_owned()]);
        assert!(reg.judgement_packs().is_empty());

        // mypack declares fallback = basic, so a missing role falls through.
        assert!(
            reg.background(Some("mypack"), "default", PadSize::Leds25, None)
                .is_some(),
            "mypack fallback to basic should resolve 'default'"
        );
        // A pack with no gifpack.ini does not fall back.
        assert!(
            reg.background(Some("nofallback"), "default", PadSize::Leds25, None)
                .is_none()
        );
    }

    #[test]
    fn scoped_background_loads_role_variants_from_a_folder() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-smx-scoped-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let full_pad = encode_gif((23, 24), &[([0, 0, 255], 100)]);
        fs::write(dir.join("gameplay_25.gif"), &full_pad).unwrap();
        // Two BPM variants of the same role.
        fs::write(dir.join("song_select_25@1b113.gif"), &full_pad).unwrap();
        fs::write(dir.join("song_select_25@1b225.gif"), &full_pad).unwrap();
        // Junk is skipped.
        fs::write(dir.join("notes.txt"), b"not a gif").unwrap();

        let gameplay = load_scoped_background(&dir, "gameplay", PadSize::Leds25);
        assert_eq!(gameplay.len(), 1);
        assert!(gameplay[0].ref_bpm.is_none());

        // Both song_select variants load, with their reference BPMs parsed.
        let song_select = load_scoped_background(&dir, "song_select", PadSize::Leds25);
        assert_eq!(song_select.len(), 2);
        let mut bpms: Vec<f32> = song_select.iter().filter_map(|v| v.ref_bpm).collect();
        bpms.sort_by(f32::total_cmp);
        assert_eq!(bpms, vec![113.0, 225.0]);

        // Missing role and missing folder both yield an empty list.
        assert!(load_scoped_background(&dir, "results", PadSize::Leds25).is_empty());
        assert!(
            load_scoped_background(&dir.join("nope"), "gameplay", PadSize::Leds25).is_empty()
        );

        // A _25 file satisfies a _16 request (size fallback).
        assert!(!load_scoped_background(&dir, "gameplay", PadSize::Leds16).is_empty());

        fs::remove_dir_all(&dir).unwrap();
    }
}
