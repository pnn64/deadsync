//! StepManiaX pad GIF decoding and the preloaded animation registry.
//!
//! deadsync owns GIF decode for pad lighting (no SDK involvement): this module
//! turns GIF bytes into per-panel LED frame sequences and preloads every GIF
//! under `assets/smx-pad-lights/` (full-pad backgrounds) and
//! `assets/smx-judge-lights/` (per-panel judgements) into an immutable
//! registry, built once on first use. Each root holds pack directories
//! under `common/` (shipped; `common/common` is the default set) and `dance/`
//! (user packs). The lighting worker only ever holds `Arc` handles into the
//! registry, so no filesystem access or decoding can happen on the gameplay
//! hot path.
//!
//! Every pack (background or judgement) automatically falls back to the
//! default pack (`common/common`) for any name it doesn't supply, so a pack
//! only has to author what it wants to customize. Each pack directory may
//! also contain an optional `gifpack.ini` that declares pack-level metadata:
//!
//! ```ini
//! Fallback = "otherpack"       # try this pack before falling back to common
//! CanBeEmpty = "miss, ok, bad" # these names never fall back to anything
//! ```
//!
//! (Keys use CamelCase, matching `deadsync.ini`'s convention.)
//!
//! `Fallback` inserts an extra pack to try (both sizes) between the selected
//! pack and the automatic default-pack fallback. `Fallback = "none"` opts the
//! whole pack out of the automatic default-pack fallback (every missing name
//! in that pack resolves to nothing rather than pulling from `common`).
//!
//! `CanBeEmpty` is a comma-separated list of names (judgement names or
//! background roles) that should resolve to nothing rather than chase any
//! fallback, declared or automatic, when this pack doesn't supply them —
//! useful for a pack that intentionally wants some events to show no gif at
//! all instead of borrowing one from `common`.
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

use std::collections::{HashMap, HashSet};
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
pub fn select_variant(
    variants: &[BackgroundVariant],
    song_bpm: Option<f32>,
) -> Option<Arc<FullPadAnim>> {
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

/// Append `source`'s variants to `merged`, skipping any whose `ref_bpm`
/// (including the untagged `None` "tag") already has an entry in `merged`.
/// Used to pool BPM variants from multiple packs (`MergeCommonBPMVariants` /
/// `MergeFallbackBPMVariants`) while keeping the higher-priority pack's
/// variant on an exact BPM-tag collision: call this with sources in priority
/// order (own pack first, since it's pre-seeded into `merged`, then whichever
/// lower-priority sources follow).
fn push_unique_by_bpm(merged: &mut Vec<BackgroundVariant>, source: &[BackgroundVariant]) {
    for v in source {
        if !merged.iter().any(|m| m.ref_bpm == v.ref_bpm) {
            merged.push(v.clone());
        }
    }
}

/// Recolor every RGB triplet in `anim` by multiplying it against `target_rgb`
/// (each channel scaled to `0.0..=1.0`), producing a new animation. Intended
/// for a `MatchColorToDifficulty`-flagged gif authored in grayscale (R=G=B per
/// pixel): white becomes `target_rgb` exactly, black stays black, and grays
/// become dimmed versions of the target color. A non-grayscale source still
/// gets multiplied the same way per channel, which generally will not look
/// like a clean recolor — packs using this feature are expected to author
/// grayscale source art.
pub fn tint_full_pad(anim: &FullPadAnim, target_rgb: [u8; 3]) -> FullPadAnim {
    let scale = target_rgb.map(|c| c as f32 / 255.0);
    let tint_frame = |frame: &PanelFrame| -> PanelFrame {
        let mut out = *frame;
        for px in out.chunks_exact_mut(3) {
            px[0] = (px[0] as f32 * scale[0]).round() as u8;
            px[1] = (px[1] as f32 * scale[1]).round() as u8;
            px[2] = (px[2] as f32 * scale[2]).round() as u8;
        }
        out
    };
    FullPadAnim {
        panels: std::array::from_fn(|i| anim.panels[i].iter().map(tint_frame).collect()),
        durations: anim.durations.clone(),
        loop_frame: anim.loop_frame,
        beats_per_loop: anim.beats_per_loop,
    }
}

/// Exponent for `saturate_for_leds`. 2.2 undoes standard sRGB encoding;
/// raise it to push tint colors more vivid still, lower toward 1.0 for the
/// palette's on-screen pastels.
const LED_SATURATION_GAMMA: f32 = 2.2;

/// Adapt a screen (sRGB) color for the pad LEDs, which are linear in the byte
/// value. An sRGB byte encodes far less light than its raw duty cycle (0x57
/// means ~10%, not 34%), so pushing palette bytes straight to the LEDs
/// overdrives the low channels and pastels like the theme pinks read as
/// off-white. Gamma-expands each channel relative to the brightest one
/// (`c' = max * (c/max)^GAMMA`), so the peak channel keeps its brightness and
/// the floor channels drop to the light level the color actually encodes.
/// Grays and pure primaries pass through unchanged.
pub fn saturate_for_leds(rgb: [u8; 3]) -> [u8; 3] {
    let max = rgb.into_iter().max().unwrap_or(0) as f32;
    if max <= 0.0 {
        return rgb;
    }
    rgb.map(|c| (max * (c as f32 / max).powf(LED_SATURATION_GAMMA)).round() as u8)
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
    //   - tag starts with anything else (letter, …): role modifier (e.g. a
    //     grade suffix: @S+, @B+, @star5); appended to the role name.
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
    let looks_like_bpm =
        matches!(tag.chars().next(), Some(c) if c.is_ascii_digit() || c == '-' || c == '.');
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

/// The shipped default pack (`common/common`), the end of every fallback chain.
pub const DEFAULT_PACK: &str = "common";

/// Pack groups under each asset tree, scanned in shadowing order: `common/`
/// holds shipped packs, `dance/` user packs, and a `dance/` pack shadows a
/// shipped pack of the same name.
pub const GROUPS: [&str; 2] = ["common", "dance"];

/// Identity of one GIF in the registry. Every GIF lives in a named pack
/// directory under `common/` (shipped) or `dance/` (user); a user pack
/// shadows a shipped pack of the same name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    pack: String,
    name: String,
    size: PadSize,
}

/// A pack's declared `fallback` behaviour from its `gifpack.ini`.
#[derive(Default, Clone, PartialEq, Debug)]
enum PackFallback {
    /// No `fallback` key (or no `gifpack.ini` at all): the pack still gets the
    /// automatic default-pack fallback for any name it doesn't supply.
    #[default]
    Auto,
    /// `Fallback = "none"`: opts the whole pack out of the automatic
    /// default-pack fallback for every name.
    None,
    /// `Fallback = "<pack>"`: try `<pack>` (both sizes) before falling back to
    /// the default pack.
    Pack(String),
}

/// Per-pack metadata parsed from a `gifpack.ini` file.
#[derive(Default, Clone, PartialEq, Debug)]
struct PackMeta {
    fallback: PackFallback,
    /// Names declared via `CanBeEmpty`: when missing from this pack, resolve
    /// to nothing rather than chasing `fallback` or the default pack.
    can_be_empty: HashSet<String>,
    /// Base role names (e.g. `"results"`) declared via `MatchColorToDifficulty`:
    /// whatever background actually resolves for that role should be recolored
    /// to the played chart's difficulty color (see `tint_full_pad`).
    /// Backgrounds only; the app layer applies the actual tint (this crate just
    /// carries the declaration).
    match_color_to_difficulty: HashSet<String>,
    /// Base role names (background packs only) declared via
    /// `MergeCommonBPMVariants`: pool this pack's own BPM-tagged variants for
    /// that role with `common`'s, rather than using only this pack's own list.
    /// Independent of `merge_fallback_bpm_variants`; see `background()`.
    merge_common_bpm_variants: HashSet<String>,
    /// Same as `merge_common_bpm_variants`, but pools with the declared
    /// `Fallback` pack's variants instead of (or as well as) `common`'s.
    /// A no-op if this pack has no `Fallback` pack declared.
    merge_fallback_bpm_variants: HashSet<String>,
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
    /// Per-pack metadata declared in `smx-pad-lights/<pack>/gifpack.ini`. Packs
    /// with no `gifpack.ini` have no entry here (equivalent to `PackMeta::default()`:
    /// no extra fallback pack, every name still eligible for the automatic
    /// default-pack fallback). Kept separate from `judgement_pack_meta` so a
    /// pack name reused across both asset trees (e.g. a "senpi-basic"
    /// background pack and an unrelated "senpi-basic" judgement pack) can
    /// declare independent metadata.
    background_pack_meta: HashMap<String, PackMeta>,
    /// Same as `background_pack_meta`, but for `smx-judge-lights/<pack>/gifpack.ini`.
    judgement_pack_meta: HashMap<String, PackMeta>,
}

impl GifRegistry {
    /// Scan and decode everything under `root` (the `assets` directory):
    /// full-pad backgrounds from `smx-pad-lights/` and per-panel judgements
    /// from `smx-judge-lights/`. Unreadable or malformed files are logged and
    /// skipped; missing directories just yield an empty category.
    ///
    /// `gifpack.ini` files are also read from each pack directory. `dance/`
    /// packs shadow `common/` packs of the same name for both gifs and
    /// metadata: a `dance/` pack that supplies any file for a (name, size)
    /// replaces the shipped pack's entry for it outright (BPM variants only
    /// pool across files within one directory, never across the two trees).
    pub fn load(root: &Path) -> Self {
        let mut reg = Self::default();
        for (pack, meta) in load_pack_meta(&root.join("smx-pad-lights")) {
            reg.background_pack_meta.entry(pack).or_insert(meta);
        }
        for (pack, meta) in load_pack_meta(&root.join("smx-judge-lights")) {
            reg.judgement_pack_meta.entry(pack).or_insert(meta);
        }
        for group in GROUPS {
            // Stage each group's backgrounds separately so a later group
            // replaces (not appends to) an earlier group's variant list.
            let mut staged: HashMap<Key, Vec<BackgroundVariant>> = HashMap::new();
            for_each_gif(&root.join("smx-pad-lights"), group, |pack, path| {
                load_background_into(&mut staged, pack, path);
            });
            reg.backgrounds.extend(staged);
            // Judgements hold one gif per key, so plain insertion already
            // shadows across groups.
            for_each_gif(&root.join("smx-judge-lights"), group, |pack, path| {
                reg.load_judgement(pack, path);
            });
        }
        log::info!(
            "SMX gifs: loaded {} backgrounds, {} judgements from {}",
            reg.backgrounds.len(),
            reg.judgements.len(),
            root.display()
        );
        reg
    }

    /// Resolve a full-pad background by role (`default`, `song_select`,
    /// `gameplay`, ...). Tries the selected pack first (both sizes), then its
    /// declared `gifpack.ini` fallback pack if any, then the default pack
    /// (`common/common`) unless the pack opts out via `Fallback = "none"` or
    /// lists `role` under `CanBeEmpty`. Picks the variant best fitting
    /// `song_bpm` among those found (see `select_variant`).
    ///
    /// If the selected pack lists `role` under `MergeCommonBPMVariants` and/or
    /// `MergeFallbackBPMVariants`, this pack's own BPM-tagged variants for
    /// `role` are pooled with `common`'s and/or the declared `Fallback`
    /// pack's before picking the best fit, rather than considering only this
    /// pack's own variants. On an exact BPM-tag collision between sources,
    /// this pack's own variant always wins, then the `Fallback` pack's, then
    /// `common`'s — the same precedence as the regular (non-merged) chain.
    pub fn background(
        &self,
        pack: Option<&str>,
        role: &str,
        size: PadSize,
        song_bpm: Option<f32>,
    ) -> Option<Arc<FullPadAnim>> {
        if let Some(p) = pack.filter(|p| *p != DEFAULT_PACK)
            && let Some(meta) = self.background_pack_meta.get(p)
            && (meta.merge_common_bpm_variants.contains(role)
                || meta.merge_fallback_bpm_variants.contains(role))
        {
            return self.merged_background(p, meta, role, size, song_bpm);
        }
        resolve(
            &self.backgrounds,
            &self.background_pack_meta,
            pack,
            role,
            size,
        )
        .and_then(|v| select_variant(v, song_bpm))
    }

    /// The `MergeCommonBPMVariants`/`MergeFallbackBPMVariants` path: pools
    /// `pack`'s own BPM-tagged variants for `role` with the declared
    /// `Fallback` pack's (if `merge_fallback_bpm_variants` lists `role`)
    /// and/or `common`'s (if `merge_common_bpm_variants` lists `role`), each
    /// tried at the requested size then the other size, same as the
    /// non-merged chain. Falls back to the regular `resolve` chain if `pack`
    /// (and any merge sources) have nothing at all for `role`, so `CanBeEmpty`
    /// and `Fallback = "none"` still behave correctly in that edge case.
    fn merged_background(
        &self,
        pack: &str,
        meta: &PackMeta,
        role: &str,
        size: PadSize,
        song_bpm: Option<f32>,
    ) -> Option<Arc<FullPadAnim>> {
        let variants_for = |p: &str| -> Option<&Vec<BackgroundVariant>> {
            self.backgrounds
                .get(&key(p, role, size))
                .or_else(|| self.backgrounds.get(&key(p, role, size.other())))
        };
        let mut merged: Vec<BackgroundVariant> = Vec::new();
        if let Some(v) = variants_for(pack) {
            merged.extend(v.iter().cloned());
        }
        if meta.merge_fallback_bpm_variants.contains(role)
            && let PackFallback::Pack(fb) = &meta.fallback
            && let Some(v) = variants_for(fb)
        {
            push_unique_by_bpm(&mut merged, v);
        }
        if meta.merge_common_bpm_variants.contains(role)
            && let Some(v) = variants_for(DEFAULT_PACK)
        {
            push_unique_by_bpm(&mut merged, v);
        }
        if merged.is_empty() {
            return resolve(
                &self.backgrounds,
                &self.background_pack_meta,
                Some(pack),
                role,
                size,
            )
            .and_then(|v| select_variant(v, song_bpm));
        }
        select_variant(&merged, song_bpm)
    }

    /// Resolve a per-panel judgement by name (`bad`, `freeze`, ...), with the
    /// same resolution order as `background`.
    pub fn judgement(
        &self,
        pack: Option<&str>,
        name: &str,
        size: PadSize,
    ) -> Option<Arc<PanelAnim>> {
        resolve(
            &self.judgements,
            &self.judgement_pack_meta,
            pack,
            name,
            size,
        )
        .cloned()
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

    /// Whether `pack`'s own `gifpack.ini` declares `role` (a base role name
    /// like `"results"`, not a grade/difficulty-qualified one) under
    /// `MatchColorToDifficulty`. Checks only the selected pack itself, not any
    /// `Fallback` chain — a pack should only flag roles it authors itself.
    /// `None` or the default pack never want tinting (the shipped pack isn't
    /// meant to imply any particular player's theme color).
    pub fn background_wants_difficulty_tint(&self, pack: Option<&str>, role: &str) -> bool {
        let Some(p) = pack.filter(|p| *p != DEFAULT_PACK) else {
            return false;
        };
        self.background_pack_meta
            .get(p)
            .is_some_and(|m| m.match_color_to_difficulty.contains(role))
    }

    pub fn is_empty(&self) -> bool {
        self.backgrounds.is_empty() && self.judgements.is_empty()
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

/// Decode one background gif into `backgrounds`, appending it to the variant
/// list for its (pack, name, size) key. Malformed files are logged and skipped.
fn load_background_into(
    backgrounds: &mut HashMap<Key, Vec<BackgroundVariant>>,
    pack: &str,
    path: &Path,
) {
    let Some((bytes, name, size, beats)) = read_named_gif(path) else {
        return;
    };
    match decode_full_pad(&bytes) {
        Ok((mut anim, decoded_size)) if decoded_size == size => {
            anim.beats_per_loop = beats.map(|b| b.beats);
            backgrounds
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

/// Outcome of looking a name up in one specific pack (not yet considering the
/// automatic default-pack fallback).
enum Lookup<'a, V> {
    /// Found in the selected pack or its declared `gifpack.ini` fallback pack.
    Found(&'a V),
    /// Explicitly opted out (via `CanBeEmpty` or `Fallback = "none"`): do not
    /// chase the automatic default-pack fallback either.
    Empty,
    /// Not found and not opted out: the automatic default-pack fallback still
    /// applies.
    Missing,
}

/// Look up a gif in one pack, respecting the size fallback and that pack's
/// `gifpack.ini` metadata (`fallback` and `CanBeEmpty`). Does not itself reach
/// for the automatic default-pack fallback; see `resolve`.
fn lookup<'a, V>(
    map: &'a HashMap<Key, V>,
    meta: &HashMap<String, PackMeta>,
    pack: Option<&str>,
    name: &str,
    size: PadSize,
) -> Lookup<'a, V> {
    let get = |p: &str, s: PadSize| map.get(&key(p, name, s));
    let Some(p) = pack.filter(|p| *p != DEFAULT_PACK) else {
        // No pack selected (or selected pack is the default): use the default pack.
        return match get(DEFAULT_PACK, size).or_else(|| get(DEFAULT_PACK, size.other())) {
            Some(v) => Lookup::Found(v),
            None => Lookup::Missing,
        };
    };
    if let Some(v) = get(p, size).or_else(|| get(p, size.other())) {
        return Lookup::Found(v);
    }
    let pack_meta = meta.get(p);
    if pack_meta.is_some_and(|m| m.can_be_empty.contains(name)) {
        return Lookup::Empty;
    }
    match pack_meta.map(|m| &m.fallback) {
        Some(PackFallback::Pack(fb)) => {
            if let Some(v) = get(fb, size).or_else(|| get(fb, size.other())) {
                return Lookup::Found(v);
            }
            Lookup::Missing
        }
        Some(PackFallback::None) => Lookup::Empty,
        Some(PackFallback::Auto) | None => Lookup::Missing,
    }
}

/// Resolve a name through `lookup`, then the automatic default-pack fallback
/// when `lookup` came back `Missing` (not found, and not explicitly opted out
/// via `CanBeEmpty` or `Fallback = "none"`).
fn resolve<'a, V>(
    map: &'a HashMap<Key, V>,
    meta: &HashMap<String, PackMeta>,
    pack: Option<&str>,
    name: &str,
    size: PadSize,
) -> Option<&'a V> {
    match lookup(map, meta, pack, name, size) {
        Lookup::Found(v) => Some(v),
        Lookup::Empty => None,
        Lookup::Missing => match lookup(map, meta, None, name, size) {
            Lookup::Found(v) => Some(v),
            Lookup::Empty | Lookup::Missing => None,
        },
    }
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

/// Parse the contents of a `gifpack.ini` file into its declared metadata.
/// Recognised keys: `Fallback` (a pack name, or `"none"` to opt the whole pack
/// out of the automatic default-pack fallback), `CanBeEmpty` (a
/// comma-separated list of names that never fall back to anything),
/// `MatchColorToDifficulty` (background packs only: a comma-separated list of
/// base role names whose resolved gif should be recolored to the played
/// chart's difficulty color), and `MergeCommonBPMVariants` /
/// `MergeFallbackBPMVariants` (background packs only: comma-separated lists of
/// base role names whose BPM-tagged variants should be pooled with `common`'s
/// and/or the declared `Fallback` pack's, instead of using only this pack's
/// own variants). Unrecognised keys and comment lines are ignored.
fn parse_gifpack_ini(content: &str) -> PackMeta {
    let mut meta = PackMeta::default();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"');
        match k.trim() {
            "Fallback" => {
                meta.fallback = if v.is_empty() || v.eq_ignore_ascii_case("none") {
                    PackFallback::None
                } else {
                    PackFallback::Pack(v.to_owned())
                };
            }
            "CanBeEmpty" => {
                meta.can_be_empty = v
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            "MatchColorToDifficulty" => {
                meta.match_color_to_difficulty = v
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            "MergeCommonBPMVariants" => {
                meta.merge_common_bpm_variants = v
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            "MergeFallbackBPMVariants" => {
                meta.merge_fallback_bpm_variants = v
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    meta
}

/// Scan `<dir>/common/*/gifpack.ini` and `<dir>/dance/*/gifpack.ini` and
/// return the per-pack metadata map. `dance/` packs override `common/` packs
/// of the same name, matching the gif shadowing rule.
fn load_pack_meta(dir: &Path) -> HashMap<String, PackMeta> {
    let mut metas = HashMap::new();
    for group in GROUPS {
        let Ok(entries) = fs::read_dir(dir.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(pack) = path.file_name().and_then(|n| n.to_str())
            {
                let ini_path = path.join("gifpack.ini");
                if let Ok(content) = fs::read_to_string(&ini_path) {
                    metas.insert(pack.to_owned(), parse_gifpack_ini(&content));
                }
            }
        }
    }
    metas
}

/// Sorted, deduplicated pack directory names under `<tree>/common/` and
/// `<tree>/dance/`, excluding the default pack. Used by the options screens to
/// build the pack pickers, so it scans directory names only (no gif decode).
pub fn discover_packs(tree: &Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for group in GROUPS {
        let Ok(entries) = fs::read_dir(tree.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name != DEFAULT_PACK
            {
                names.push(name.to_owned());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Visit every `.gif` in `<dir>/<group>/<pack>/` for one pack group.
/// `common/` holds shipped packs (`common` at minimum), `dance/` user packs;
/// the caller scans `common` first so a user pack shadows a shipped one of
/// the same name.
fn for_each_gif(dir: &Path, group: &str, mut visit: impl FnMut(&str, &Path)) {
    let Ok(entries) = fs::read_dir(dir.join(group)) else {
        return;
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
            Some((
                s("song_select"),
                PadSize::Leds25,
                Some(spec(1.0, Some(120.0)))
            ))
        );
        assert_eq!(
            parse_stem("song_select_25@0.5B90"),
            Some((
                s("song_select"),
                PadSize::Leds25,
                Some(spec(0.5, Some(90.0)))
            ))
        );
        // Bare beat counts (no bpm) are accepted too.
        assert_eq!(
            parse_stem("song_select_25@2b"),
            Some((s("song_select"), PadSize::Leds25, Some(spec(2.0, None))))
        );
        assert_eq!(
            parse_stem("bad_16"),
            Some((s("bad"), PadSize::Leds16, None))
        );
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
            parse_stem("results_25@star5"),
            Some((s("results@star5"), PadSize::Leds25, None))
        );
        // Stacked difficulty+grade tags: everything after the first '@' becomes
        // part of the name verbatim, so a second '@' just carries straight through.
        assert_eq!(
            parse_stem("results_25@hard@S+"),
            Some((s("results@hard@S+"), PadSize::Leds25, None))
        );
        assert_eq!(
            parse_stem("results_25@edit@star5"),
            Some((s("results@edit@star5"), PadSize::Leds25, None))
        );
        // Difficulty tag alone (no grade).
        assert_eq!(
            parse_stem("results_25@hard"),
            Some((s("results@hard"), PadSize::Leds25, None))
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

    fn dummy_panel() -> Arc<PanelAnim> {
        Arc::new(PanelAnim {
            frames: vec![[0u8; PANEL_RGB_BYTES]],
            durations: vec![1.0 / 30.0],
            loop_frame: 0,
            loop_end: 0,
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
    fn background_falls_back_to_default_pack_even_without_gifpack_ini() {
        let mut reg = GifRegistry::default();
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "default", PadSize::Leds25),
            dummy_variants(),
        );
        // "foo" pack exists but has no gifpack.ini at all: still resolves through common.
        let got = reg
            .background(Some("foo"), "default", PadSize::Leds25, None)
            .unwrap();
        let common = reg.backgrounds[&key(DEFAULT_PACK, "default", PadSize::Leds25)][0]
            .anim
            .clone();
        assert!(Arc::ptr_eq(&got, &common));

        // "foo" does have a gif: own gif wins over the default-pack fallback.
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
    fn pack_with_gifpack_ini_fallback_reaches_declared_pack_before_default() {
        let mut reg = GifRegistry::default();
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "default", PadSize::Leds25),
            dummy_variants(),
        );
        reg.backgrounds
            .insert(key("other", "default", PadSize::Leds25), dummy_variants());
        // "foo" pack declares fallback = "other" (not common).
        reg.background_pack_meta.insert(
            "foo".to_owned(),
            PackMeta {
                fallback: PackFallback::Pack("other".to_owned()),
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );

        // "foo" has no "default" gif, so it falls through to "other", not common.
        let got = reg
            .background(Some("foo"), "default", PadSize::Leds25, None)
            .unwrap();
        let other = reg.backgrounds[&key("other", "default", PadSize::Leds25)][0]
            .anim
            .clone();
        assert!(Arc::ptr_eq(&got, &other));

        // "foo" does have a gif: own gif wins over the declared fallback.
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
    fn fallback_none_opts_a_pack_out_of_the_automatic_default_fallback() {
        let mut reg = GifRegistry::default();
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "default", PadSize::Leds25),
            dummy_variants(),
        );
        reg.background_pack_meta.insert(
            "foo".to_owned(),
            PackMeta {
                fallback: PackFallback::None,
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );
        assert!(
            reg.background(Some("foo"), "default", PadSize::Leds25, None)
                .is_none(),
            "Fallback = \"none\" should suppress the automatic default-pack fallback"
        );
    }

    #[test]
    fn can_be_empty_suppresses_fallback_only_for_listed_names() {
        let mut reg = GifRegistry::default();
        reg.judgements
            .insert(key(DEFAULT_PACK, "miss", PadSize::Leds25), dummy_panel());
        reg.judgements
            .insert(key(DEFAULT_PACK, "ok", PadSize::Leds25), dummy_panel());
        reg.judgement_pack_meta.insert(
            "foo".to_owned(),
            PackMeta {
                fallback: PackFallback::Auto,
                can_be_empty: ["miss".to_owned()].into_iter().collect(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );

        // "miss" is listed under CanBeEmpty: no fallback, resolves to nothing.
        assert!(
            reg.judgement(Some("foo"), "miss", PadSize::Leds25)
                .is_none()
        );
        // "ok" is not listed: still gets the automatic default-pack fallback.
        assert!(reg.judgement(Some("foo"), "ok", PadSize::Leds25).is_some());
    }

    #[test]
    fn judgement_falls_back_to_default_pack_even_without_gifpack_ini() {
        let mut reg = GifRegistry::default();
        reg.judgements
            .insert(key(DEFAULT_PACK, "miss", PadSize::Leds25), dummy_panel());
        // "foo" has no gifpack.ini at all: a missing judgement gif still
        // resolves through the automatic default-pack fallback so the
        // judgement shows something on the pad.
        let got = reg.judgement(Some("foo"), "miss", PadSize::Leds25).unwrap();
        let common = reg.judgements[&key(DEFAULT_PACK, "miss", PadSize::Leds25)].clone();
        assert!(Arc::ptr_eq(&got, &common));

        // "foo" does have its own gif: own gif wins over the default-pack fallback.
        reg.judgements
            .insert(key("foo", "miss", PadSize::Leds25), dummy_panel());
        let got2 = reg.judgement(Some("foo"), "miss", PadSize::Leds25).unwrap();
        let foo = reg.judgements[&key("foo", "miss", PadSize::Leds25)].clone();
        assert!(Arc::ptr_eq(&got2, &foo));

        // Default pack has nothing for this name either: still None.
        assert!(
            reg.judgement(Some("foo"), "nope", PadSize::Leds25)
                .is_none()
        );
    }

    #[test]
    fn no_pack_and_basic_by_name_both_use_the_default_pack() {
        let mut reg = GifRegistry::default();
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "default", PadSize::Leds25),
            dummy_variants(),
        );
        assert!(
            reg.background(None, "default", PadSize::Leds25, None)
                .is_some()
        );
        // Selecting common by name is the same as selecting nothing.
        assert!(
            reg.background(Some(DEFAULT_PACK), "default", PadSize::Leds25, None)
                .is_some()
        );
        // Unknown pack still gets the automatic default-pack fallback.
        assert!(
            reg.background(Some("nope"), "default", PadSize::Leds25, None)
                .is_some()
        );
        // But an unknown role that the default pack doesn't have either: nothing.
        assert!(
            reg.background(Some("nope"), "no_such_role", PadSize::Leds25, None)
                .is_none()
        );
    }

    // gifpack.ini parsing

    #[test]
    fn parse_gifpack_ini_extracts_fallback() {
        assert_eq!(
            parse_gifpack_ini("Fallback = \"basic\"").fallback,
            PackFallback::Pack("basic".to_owned())
        );
        // Whitespace and comment lines are tolerated.
        assert_eq!(
            parse_gifpack_ini("# my pack\nFallback = \"basic\"\n").fallback,
            PackFallback::Pack("basic".to_owned())
        );
        // "none" and absent key both yield PackFallback::None/Auto respectively.
        assert_eq!(
            parse_gifpack_ini("Fallback = \"none\"").fallback,
            PackFallback::None
        );
        assert_eq!(parse_gifpack_ini("").fallback, PackFallback::Auto);
        assert_eq!(
            parse_gifpack_ini("# just a comment").fallback,
            PackFallback::Auto
        );
        // Unknown keys are ignored.
        assert_eq!(
            parse_gifpack_ini("name = \"cool pack\"\nFallback = \"basic\"").fallback,
            PackFallback::Pack("basic".to_owned())
        );
    }

    #[test]
    fn parse_gifpack_ini_extracts_can_be_empty() {
        assert_eq!(
            parse_gifpack_ini("CanBeEmpty = \"miss, ok, bad\"").can_be_empty,
            ["miss", "ok", "bad"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        // Absent key yields an empty set.
        assert!(
            parse_gifpack_ini("Fallback = \"basic\"")
                .can_be_empty
                .is_empty()
        );
        // Both keys together.
        let meta = parse_gifpack_ini("Fallback = \"basic\"\nCanBeEmpty = \"miss\"");
        assert_eq!(meta.fallback, PackFallback::Pack("basic".to_owned()));
        assert_eq!(meta.can_be_empty, ["miss".to_owned()].into_iter().collect());
    }

    #[test]
    fn parse_gifpack_ini_extracts_match_color_to_difficulty() {
        assert_eq!(
            parse_gifpack_ini("MatchColorToDifficulty = \"results\"").match_color_to_difficulty,
            ["results".to_owned()].into_iter().collect()
        );
        assert_eq!(
            parse_gifpack_ini("MatchColorToDifficulty = \"results, gameplay\"")
                .match_color_to_difficulty,
            ["results", "gameplay"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        // Absent key yields an empty set.
        assert!(
            parse_gifpack_ini("Fallback = \"basic\"")
                .match_color_to_difficulty
                .is_empty()
        );
    }

    #[test]
    fn parse_gifpack_ini_extracts_merge_bpm_variant_keys() {
        let meta = parse_gifpack_ini(
            "MergeCommonBPMVariants = \"song_select\"\nMergeFallbackBPMVariants = \"song_select, gameplay\"",
        );
        assert_eq!(
            meta.merge_common_bpm_variants,
            ["song_select".to_owned()].into_iter().collect()
        );
        assert_eq!(
            meta.merge_fallback_bpm_variants,
            ["song_select", "gameplay"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        // Absent keys yield empty sets.
        let meta = parse_gifpack_ini("Fallback = \"basic\"");
        assert!(meta.merge_common_bpm_variants.is_empty());
        assert!(meta.merge_fallback_bpm_variants.is_empty());
    }

    #[test]
    fn background_wants_difficulty_tint_checks_only_the_selected_pack() {
        let mut reg = GifRegistry::default();
        reg.background_pack_meta.insert(
            "foo".to_owned(),
            PackMeta {
                fallback: PackFallback::Auto,
                can_be_empty: Default::default(),
                match_color_to_difficulty: ["results".to_owned()].into_iter().collect(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );
        assert!(reg.background_wants_difficulty_tint(Some("foo"), "results"));
        // Not the flagged role.
        assert!(!reg.background_wants_difficulty_tint(Some("foo"), "gameplay"));
        // No pack selected, or the default pack: never wants tinting.
        assert!(!reg.background_wants_difficulty_tint(None, "results"));
        assert!(!reg.background_wants_difficulty_tint(Some(DEFAULT_PACK), "results"));
        // A pack with no declaration at all.
        assert!(!reg.background_wants_difficulty_tint(Some("bar"), "results"));
    }

    #[test]
    fn merge_bpm_variants_pools_sources_with_own_pack_precedence() {
        let tagged = |bpm: f32| BackgroundVariant {
            anim: dummy_full_pad(),
            ref_bpm: Some(bpm),
        };
        let mut reg = GifRegistry::default();
        // "mine" has a 120bpm variant; declares fallback="other" and merges both.
        reg.backgrounds.insert(
            key("mine", "song_select", PadSize::Leds25),
            vec![tagged(120.0)],
        );
        // "other" (the declared Fallback pack) has 90bpm and a colliding 120bpm.
        let other_90 = tagged(90.0);
        let other_120 = tagged(120.0);
        reg.backgrounds.insert(
            key("other", "song_select", PadSize::Leds25),
            vec![other_90.clone(), other_120.clone()],
        );
        // common has 200bpm and a colliding 120bpm.
        let common_200 = tagged(200.0);
        let common_120 = tagged(120.0);
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "song_select", PadSize::Leds25),
            vec![common_200.clone(), common_120.clone()],
        );
        reg.background_pack_meta.insert(
            "mine".to_owned(),
            PackMeta {
                fallback: PackFallback::Pack("other".to_owned()),
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: ["song_select".to_owned()].into_iter().collect(),
                merge_fallback_bpm_variants: ["song_select".to_owned()].into_iter().collect(),
            },
        );

        // A song at 90bpm: only "other" has that exact tag (own pack's 120 doesn't
        // qualify as "at or above" a slower song, so the merge must have pulled
        // "other"'s 90bpm in for this to resolve at all).
        let got = reg
            .background(Some("mine"), "song_select", PadSize::Leds25, Some(90.0))
            .unwrap();
        assert!(Arc::ptr_eq(&got, &other_90.anim));

        // A very fast song: only common's 200bpm variant covers it.
        let got = reg
            .background(Some("mine"), "song_select", PadSize::Leds25, Some(500.0))
            .unwrap();
        assert!(Arc::ptr_eq(&got, &common_200.anim));

        // At the collision point (120bpm exists in all three sources), own
        // pack's variant wins, not the fallback's or common's.
        let got = reg
            .background(Some("mine"), "song_select", PadSize::Leds25, Some(120.0))
            .unwrap();
        assert!(!Arc::ptr_eq(&got, &other_120.anim));
        assert!(!Arc::ptr_eq(&got, &common_120.anim));
    }

    #[test]
    fn merge_bpm_variants_only_pools_the_flagged_source() {
        let tagged = |bpm: f32| BackgroundVariant {
            anim: dummy_full_pad(),
            ref_bpm: Some(bpm),
        };
        let mut reg = GifRegistry::default();
        reg.backgrounds.insert(
            key("mine", "song_select", PadSize::Leds25),
            vec![tagged(120.0)],
        );
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "song_select", PadSize::Leds25),
            vec![tagged(200.0)],
        );
        // No merge flags at all: common's variant is never considered, even
        // though it would be a better BPM fit.
        assert!(
            reg.background(Some("mine"), "song_select", PadSize::Leds25, Some(500.0))
                .is_some(),
            "still resolves to mine's own (worse-fit) variant"
        );
        let without_merge = reg
            .background(Some("mine"), "song_select", PadSize::Leds25, Some(500.0))
            .unwrap();
        let mine_only = &reg.backgrounds[&key("mine", "song_select", PadSize::Leds25)][0].anim;
        assert!(Arc::ptr_eq(&without_merge, mine_only));

        // Flip on MergeCommonBPMVariants: now common's better-fitting variant wins.
        reg.background_pack_meta.insert(
            "mine".to_owned(),
            PackMeta {
                fallback: PackFallback::Auto,
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: ["song_select".to_owned()].into_iter().collect(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );
        let with_merge = reg
            .background(Some("mine"), "song_select", PadSize::Leds25, Some(500.0))
            .unwrap();
        assert!(!Arc::ptr_eq(&with_merge, mine_only));
    }

    #[test]
    fn tint_full_pad_multiplies_channels_by_target_color() {
        // Panel 0, frame 0: white, mid-gray, black (rest of the panel unused/zeroed).
        let mut frame = [0u8; PANEL_RGB_BYTES];
        frame[0..9].copy_from_slice(&[255, 255, 255, 128, 128, 128, 0, 0, 0]);
        let anim = FullPadAnim {
            panels: std::array::from_fn(|i| {
                if i == 0 {
                    vec![frame]
                } else {
                    vec![[0u8; PANEL_RGB_BYTES]]
                }
            }),
            durations: vec![1.0 / 30.0],
            loop_frame: 0,
            beats_per_loop: None,
        };
        let tinted = tint_full_pad(&anim, [200, 100, 50]);
        let px = tinted.panels[0][0];
        // White -> exact target color.
        assert_eq!(&px[0..3], &[200, 100, 50]);
        // Mid-gray -> roughly half the target color.
        assert_eq!(&px[3..6], &[100, 50, 25]);
        // Black -> stays black regardless of target.
        assert_eq!(&px[6..9], &[0, 0, 0]);
    }

    #[test]
    fn saturate_for_leds_expands_floor_channels_and_keeps_extremes() {
        // Theme pink #FF577E: peak channel stays, floor channels gamma-expand
        // to the light level the sRGB bytes encode (0x57 = 34% duty but ~9%
        // light), so the LEDs show pink instead of off-white.
        let [r, g, b] = saturate_for_leds([0xFF, 0x57, 0x7E]);
        assert_eq!(r, 0xFF);
        assert!(g < 0x57 / 2, "green floor should drop sharply, got {g:#x}");
        assert!(b < 0x7E / 2, "blue floor should drop sharply, got {b:#x}");
        assert!(b > g, "channel ordering (hue) must be preserved");
        // Grays, primaries, and black pass through unchanged.
        assert_eq!(saturate_for_leds([128, 128, 128]), [128, 128, 128]);
        assert_eq!(saturate_for_leds([255, 0, 0]), [255, 0, 0]);
        assert_eq!(saturate_for_leds([0, 0, 0]), [0, 0, 0]);
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
        let bg_basic = root.join("smx-pad-lights/common/common");
        let bg_user = root.join("smx-pad-lights/dance/mypack");
        let j_basic = root.join("smx-judge-lights/common/common");
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
        // gifpack.ini: mypack declares common as its fallback.
        fs::write(bg_user.join("gifpack.ini"), b"Fallback = \"common\"\n").unwrap();

        let reg = GifRegistry::load(&root);
        fs::remove_dir_all(&root).unwrap();

        assert!(
            reg.background(None, "default", PadSize::Leds25, None)
                .is_some()
        );
        let song = reg
            .background(Some("mypack"), "song_select", PadSize::Leds25, None)
            .unwrap();
        assert_eq!(song.beats_per_loop, Some(1.0));
        assert!(reg.judgement(None, "bad", PadSize::Leds25).is_some());
        // The 16-LED request falls back to the only (_25) asset.
        assert!(reg.judgement(None, "bad", PadSize::Leds16).is_some());

        assert!(
            reg.background(None, "broken", PadSize::Leds25, None)
                .is_none()
        );
        assert!(reg.judgement(None, "badname", PadSize::Leds25).is_none());
        assert_eq!(reg.background_packs(), vec!["mypack".to_owned()]);
        assert!(reg.judgement_packs().is_empty());

        // mypack declares fallback = common, so a missing role falls through.
        assert!(
            reg.background(Some("mypack"), "default", PadSize::Leds25, None)
                .is_some(),
            "mypack fallback to common should resolve 'default'"
        );
        // A pack with no gifpack.ini still gets the automatic default-pack fallback.
        assert!(
            reg.background(Some("nofallback"), "default", PadSize::Leds25, None)
                .is_some()
        );
    }

    #[test]
    fn dance_pack_shadows_a_same_name_shipped_pack_instead_of_pooling() {
        let root =
            std::env::temp_dir().join(format!("deadsync-smx-shadow-test-{}", std::process::id()));
        let shipped = root.join("smx-pad-lights/common/shadowpack");
        let user = root.join("smx-pad-lights/dance/shadowpack");
        fs::create_dir_all(&shipped).unwrap();
        fs::create_dir_all(&user).unwrap();
        let full_pad = encode_gif((23, 24), &[([0, 0, 255], 100)]);
        // The shipped copy has a 100bpm variant; the user copy only a 200bpm one.
        fs::write(shipped.join("song_select_25@1b100.gif"), &full_pad).unwrap();
        fs::write(user.join("song_select_25@2b200.gif"), &full_pad).unwrap();

        let reg = GifRegistry::load(&root);
        fs::remove_dir_all(&root).unwrap();

        // If the two trees pooled, the shipped 100bpm variant would be the
        // exact fit here; shadowing means only the user pack's file exists.
        let got = reg
            .background(
                Some("shadowpack"),
                "song_select",
                PadSize::Leds25,
                Some(100.0),
            )
            .unwrap();
        assert_eq!(
            got.beats_per_loop,
            Some(2.0),
            "user pack must replace the shipped entry"
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
        let song_select = load_scoped_background(&dir, "song_select", PadSize::Leds25);
        let missing_role = load_scoped_background(&dir, "results", PadSize::Leds25);
        let missing_dir = load_scoped_background(&dir.join("nope"), "gameplay", PadSize::Leds25);
        let other_size = load_scoped_background(&dir, "gameplay", PadSize::Leds16);
        // Clean up before asserting so a failure can't leak the temp dir.
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(gameplay.len(), 1);
        assert!(gameplay[0].ref_bpm.is_none());

        // Both song_select variants load, with their reference BPMs parsed.
        assert_eq!(song_select.len(), 2);
        let mut bpms: Vec<f32> = song_select.iter().filter_map(|v| v.ref_bpm).collect();
        bpms.sort_by(f32::total_cmp);
        assert_eq!(bpms, vec![113.0, 225.0]);

        // Missing role and missing folder both yield an empty list.
        assert!(missing_role.is_empty());
        assert!(missing_dir.is_empty());

        // A _25 file satisfies a _16 request (size fallback).
        assert!(!other_size.is_empty());
    }

    #[test]
    fn judgement_pack_can_chain_to_a_non_default_pack_via_gifpack_ini() {
        let mut reg = GifRegistry::default();
        reg.judgements
            .insert(key("other", "miss", PadSize::Leds25), dummy_panel());
        reg.judgements
            .insert(key(DEFAULT_PACK, "miss", PadSize::Leds25), dummy_panel());
        reg.judgement_pack_meta.insert(
            "mine".to_owned(),
            PackMeta {
                fallback: PackFallback::Pack("other".to_owned()),
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );

        let got = reg
            .judgement(Some("mine"), "miss", PadSize::Leds25)
            .unwrap();
        let other = reg.judgements[&key("other", "miss", PadSize::Leds25)].clone();
        let common = reg.judgements[&key(DEFAULT_PACK, "miss", PadSize::Leds25)].clone();
        assert!(
            Arc::ptr_eq(&got, &other),
            "should chain to declared fallback, not common"
        );
        assert!(!Arc::ptr_eq(&got, &common));
    }

    #[test]
    fn background_and_judgement_fallbacks_are_independent_for_the_same_pack_name() {
        // A pack named "senpi-basic" can exist under both smx-pad-lights/ and
        // smx-judge-lights/ with unrelated (or absent) gifpack.ini fallback
        // declarations in each; one category's chain must not leak into the other.
        let mut reg = GifRegistry::default();
        reg.backgrounds.insert(
            key(DEFAULT_PACK, "default", PadSize::Leds25),
            dummy_variants(),
        );
        reg.judgements
            .insert(key("bg-fallback", "miss", PadSize::Leds25), dummy_panel());
        reg.background_pack_meta.insert(
            "senpi-basic".to_owned(),
            PackMeta {
                fallback: PackFallback::Pack(DEFAULT_PACK.to_owned()),
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );
        reg.judgement_pack_meta.insert(
            "senpi-basic".to_owned(),
            PackMeta {
                fallback: PackFallback::Pack("bg-fallback".to_owned()),
                can_be_empty: Default::default(),
                match_color_to_difficulty: Default::default(),
                merge_common_bpm_variants: Default::default(),
                merge_fallback_bpm_variants: Default::default(),
            },
        );

        // Background side resolves through its own declared fallback (common).
        assert!(
            reg.background(Some("senpi-basic"), "default", PadSize::Leds25, None)
                .is_some()
        );
        // Judgement side resolves through its own declared fallback ("bg-fallback"),
        // not the background category's chain.
        let got = reg
            .judgement(Some("senpi-basic"), "miss", PadSize::Leds25)
            .unwrap();
        let bg_fallback = reg.judgements[&key("bg-fallback", "miss", PadSize::Leds25)].clone();
        assert!(Arc::ptr_eq(&got, &bg_fallback));
    }
}
