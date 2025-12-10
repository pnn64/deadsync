//! StepMania bitmap font parser (Rust port — dependency-light, functional/procedural)
//! - SM-parity defaults for metrics and width handling (fixes tight/overlapping glyphs)
//! - Supports LINE, MAP U+XXXX / "..." (Unicode, ASCII, CP1252, numbers)
//! - SM extra-pixels quirk (+1/+1, left forced even) to avoid stroke clipping
//! - Canonical texture keys (assets-relative, forward slashes) so lookups match
//! - Parses "(res WxH)" from sheet filenames and scales INI-authored metrics like StepMania
//! - Applies inverse draw scale so on-screen size matches StepMania's authored size
//! - No regex/glob/configparser/once_cell; pure std + image + log
//! - VERBOSE TRACE logging for troubleshooting: enable with RUST_LOG=new_engine::core::font=trace

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use image;
use log::{debug, info, trace, warn};

use crate::assets;

const FONT_DEFAULT_CHAR: char = '\u{F8FF}'; // SM default glyph (private use)

/* ======================= TYPES ======================= */

#[derive(Debug, Clone)]
pub struct Glyph {
    pub texture_key: String,
    pub tex_rect: [f32; 4], // px: [x0, y0, x1, y1] (texture space)
    pub size: [f32; 2],     // draw units (SM authored units)
    pub offset: [f32; 2],   // draw units: [x_off_from_pen, y_off_from_baseline]
    pub advance: f32,       // draw units: pen advance
}

#[derive(Debug, Clone)]
pub struct Font {
    pub glyph_map: HashMap<char, Glyph>,
    pub default_glyph: Option<Glyph>,
    pub line_spacing: i32, // draw units (from main/default page)
    pub height: i32,       // draw units (baseline - top)
    pub fallback_font_name: Option<&'static str>,
}

pub struct FontLoadData {
    pub font: Font,
    pub required_textures: Vec<PathBuf>,
}

#[derive(Debug)]
struct FontPageSettings {
    pub(crate) draw_extra_pixels_left: i32,
    pub(crate) draw_extra_pixels_right: i32,
    pub(crate) add_to_all_widths: i32,
    pub(crate) scale_all_widths_by: f32,
    pub(crate) line_spacing: i32,         // -1 = “use frame height”
    pub(crate) top: i32,                  // -1 = “center – line_spacing/2”
    pub(crate) baseline: i32,             // -1 = “center + line_spacing/2”
    pub(crate) default_width: i32,        // -1 = “use frame width”
    pub(crate) advance_extra_pixels: i32, // SM default is 0
    pub(crate) glyph_widths: HashMap<usize, i32>,
}

impl Default for FontPageSettings {
    #[inline(always)]
    fn default() -> Self {
        Self {
            draw_extra_pixels_left: 0,
            draw_extra_pixels_right: 0,
            add_to_all_widths: 0,
            scale_all_widths_by: 1.0,
            line_spacing: -1,
            top: -1,
            baseline: -1,
            default_width: -1,
            advance_extra_pixels: 1, // SM default
            glyph_widths: HashMap::new(),
        }
    }
}

/* ======================= SMALL PARSERS (NO REGEX) ======================= */

#[inline(always)]
fn strip_bom(mut s: String) -> String {
    if s.starts_with('\u{FEFF}') {
        s.drain(..1);
    }
    s
}

#[inline(always)]
fn is_full_line_comment(s: &str) -> bool {
    let t = s.trim_start();
    t.starts_with(';') || t.starts_with('#') || t.starts_with("//")
}

#[inline(always)]
fn as_lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Parse [Section] lines (returns section name) — whitespace tolerant.
/// Allocation-free; returns borrowed slice.
#[inline(always)]
#[must_use]
fn parse_section_header(raw: &str) -> Option<&str> {
    let t = raw.trim();
    if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
        let name = &t[1..t.len() - 1];
        Some(name.trim())
    } else {
        None
    }
}

/// Parse key=value (trimmed key & value). Returns (key_lower, value_string).
#[inline(always)]
fn parse_kv_trimmed(raw: &str) -> Option<(String, String)> {
    let mut split = raw.splitn(2, '=');
    let k = split.next()?.trim();
    let v = split.next()?.trim();
    if k.is_empty() {
        return None;
    }
    Some((as_lower(k), v.to_string()))
}

/// Parse LINE row with *raw* RHS preserved (no trim). Case-insensitive line.
/// Allocation-free; returns borrowed rhs slice.
#[inline(always)]
#[must_use]
fn parse_line_entry_raw(raw: &str) -> Option<(u32, &str)> {
    let eq = raw.find('=')?;
    let (lhs, rhs0) = raw.split_at(eq);
    let rhs = &rhs0[1..]; // skip '='

    // Skip leading spaces on LHS
    let bytes = lhs.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    // Expect ascii "line"
    if i + 4 > bytes.len() {
        return None;
    }
    #[inline(always)]
    fn low(b: u8) -> u8 {
        b | 0x20
    } // ascii lowercase
    if !(low(bytes[i]) == b'l'
        && low(bytes[i + 1]) == b'i'
        && low(bytes[i + 2]) == b'n'
        && low(bytes[i + 3]) == b'e')
    {
        return None;
    }
    i += 4;

    // Skip spaces, then parse digits
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let num_str = lhs[i..].trim();
    let row: u32 = num_str.parse().ok()?;
    Some((row, rhs))
}

#[inline(always)]
fn is_doubleres_in_name(name: &str) -> bool {
    let b = name.as_bytes();
    // search for "doubleres" case-insensitively without allocation
    for w in b.windows(9) {
        #[inline(always)]
        fn low(x: u8) -> u8 {
            x | 0x20
        }
        if low(w[0]) == b'd'
            && low(w[1]) == b'o'
            && low(w[2]) == b'u'
            && low(w[3]) == b'b'
            && low(w[4]) == b'l'
            && low(w[5]) == b'e'
            && low(w[6]) == b'r'
            && low(w[7]) == b'e'
            && low(w[8]) == b's'
        {
            return true;
        }
    }
    false
}

/// [section]->{key->val} (both section/key lowercased, value trimmed). Only std.
/// Allocation-free per line (borrows &str).
#[inline(always)]
fn parse_ini_trimmed_map(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = String::from("common");
    for raw in text.lines() {
        let mut line = raw;
        if let Some(s) = line.strip_suffix('\r') {
            line = s;
        }
        if is_full_line_comment(line) {
            continue;
        }
        if let Some(sec) = parse_section_header(line) {
            section = as_lower(sec.trim());
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some((k, v)) = parse_kv_trimmed(t) {
            out.entry(section.clone()).or_default().insert(k, v);
        }
    }
    out
}

/// Harvest raw line N=... entries, keeping RHS verbatim (no trim).
/// Allocation-free per line (borrows &str).
#[inline(always)]
fn harvest_raw_line_entries_from_text(text: &str) -> HashMap<(String, u32), String> {
    let mut out: HashMap<(String, u32), String> = HashMap::new();
    let mut section = String::from("common");
    for raw in text.lines() {
        let mut line = raw;
        if let Some(s) = line.strip_suffix('\r') {
            line = s;
        }
        if is_full_line_comment(line) {
            continue;
        }
        if let Some(sec) = parse_section_header(line) {
            section = as_lower(sec.trim());
            continue;
        }
        if let Some((row, rhs)) = parse_line_entry_raw(line) {
            out.insert((section.clone(), row), rhs.to_string());
        }
    }
    out
}

/// Page name from filename stem: takes text inside first pair of [...], else "main".
#[inline(always)]
fn get_page_name_from_path(path: &Path) -> String {
    let filename = path.file_stem().unwrap_or_default().to_string_lossy();
    if let (Some(s), Some(e)) = (filename.find('['), filename.find(']'))
        && s < e
    {
        return filename[s + 1..e].to_string();
    }
    "main".to_string()
}

/// List PNG textures adjacent to INI where name starts with prefix (no glob).
fn list_texture_pages(font_dir: &Path, prefix: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for entry in fs::read_dir(font_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.to_ascii_lowercase().ends_with(".png") {
            continue;
        }
        if !name.starts_with(prefix) {
            continue;
        }
        if name.contains("-stroke") {
            continue;
        }
        v.push(path);
    }
    v.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    Ok(v)
}

/// Parse range <codeset> [#start-end] from a key (key only).
#[inline(always)]
fn parse_range_key(key: &str) -> Option<(String, Option<(u32, u32)>)> {
    let k = key.trim_start();
    if !k.to_ascii_lowercase().starts_with("range ") {
        return None;
    }
    let rest = k[6..].trim_start();
    // codeset token ends at whitespace or '#'
    let mut cs_end = rest.len();
    for (i, ch) in rest.char_indices() {
        if ch.is_whitespace() || ch == '#' {
            cs_end = i;
            break;
        }
    }
    if cs_end == 0 {
        return None;
    }
    let codeset = &rest[..cs_end];
    let tail = rest[cs_end..].trim_start();
    if tail.is_empty() {
        return Some((codeset.to_string(), None));
    }
    if !tail.starts_with('#') {
        return Some((codeset.to_string(), None));
    }
    let tail = &tail[1..];
    let dash = tail.find('-')?;
    let (a, b) = tail.split_at(dash);
    let b = &b[1..];
    let start = u32::from_str_radix(a.trim(), 16).ok()?;
    let end = u32::from_str_radix(b.trim(), 16).ok()?;
    if end < start {
        return None;
    }
    Some((codeset.to_string(), Some((start, end))))
}

/* ======================= LOG HELPERS ======================= */

#[inline(always)]
fn fmt_char(ch: char) -> String {
    match ch {
        ' ' => "SPACE (U+0020)".to_string(),
        '\u{00A0}' => "NBSP (U+00A0)".to_string(),
        '\n' => "\\n (U+000A)".to_string(),
        '\r' => "\\r (U+000D)".to_string(),
        '\t' => "\\t (U+0009)".to_string(),
        _ if ch.is_control() => format!("U+{:04X}", ch as u32),
        _ => format!("'{}' (U+{:04X})", ch, ch as u32),
    }
}

/* ======================= STEPMania SHEET SCALE HELPERS ======================= */

/// Parse "(res WxH)" from a filename or path (case-insensitive). Returns sheet base res.
#[inline(always)]
fn parse_base_res_from_filename(path_or_name: &str) -> Option<(u32, u32)> {
    let s = path_or_name.to_ascii_lowercase();
    let bytes = s.as_bytes();
    let needle = b"(res";
    let mut i = 0usize;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // skip whitespace
            let mut k = i + needle.len();
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            // parse W
            let mut w: u32 = 0;
            let mut h: u32 = 0;
            let mut have_w = false;
            while k < bytes.len() && bytes[k].is_ascii_digit() {
                have_w = true;
                w = w.saturating_mul(10) + (bytes[k] - b'0') as u32;
                k += 1;
            }
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            // expect 'x'
            if k >= bytes.len() || bytes[k] != b'x' {
                i += 1;
                continue;
            }
            k += 1;
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            // parse H
            let mut have_h = false;
            while k < bytes.len() && bytes[k].is_ascii_digit() {
                have_h = true;
                h = h.saturating_mul(10) + (bytes[k] - b'0') as u32;
                k += 1;
            }
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            if have_w && have_h && k < bytes.len() && bytes[k] == b')' && w > 0 && h > 0 {
                return Some((w, h));
            }
        }
        i += 1;
    }
    None
}

/// Round-to-nearest with ties-to-even (banker's rounding), like C's lrint with FE_TONEAREST.
#[inline(always)]
#[must_use]
fn round_half_to_even_i32(v: f32) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let floor = v.floor();
    let frac = v - floor;
    if frac < 0.5 {
        floor as i32
    } else if frac > 0.5 {
        (floor + 1.0) as i32
    } else {
        let f = floor as i32;
        if (f & 1) == 0 { f } else { f + 1 }
    }
}

/* ======================= RANGE APPLY ======================= */

#[inline(always)]
fn cp1252_to_unicode(byte: u8) -> u32 {
    match byte {
        0x80 => 0x20AC,   // €
        0x81 => 0x0081,   // undefined (C1)
        0x82 => 0x201A,   // ‚
        0x83 => 0x0192,   // ƒ
        0x84 => 0x201E,   // „
        0x85 => 0x2026,   // …
        0x86 => 0x2020,   // †
        0x87 => 0x2021,   // ‡
        0x88 => 0x02C6,   // ˆ
        0x89 => 0x2030,   // ‰
        0x8A => 0x0160,   // Š
        0x8B => 0x2039,   // ‹
        0x8C => 0x0152,   // Œ
        0x8D => 0x008D,   // undefined (C1)
        0x8E => 0x017D,   // Ž
        0x8F => 0x008F,   // undefined (C1)
        0x90 => 0x0090,   // undefined (C1)
        0x91 => 0x2018,   // ‘
        0x92 => 0x2019,   // ’
        0x93 => 0x201C,   // “
        0x94 => 0x201D,   // ”
        0x95 => 0x2022,   // •
        0x96 => 0x2013,   // –
        0x97 => 0x2014,   // —
        0x98 => 0x02DC,   // ˜
        0x99 => 0x2122,   // ™
        0x9A => 0x0161,   // š
        0x9B => 0x203A,   // ›
        0x9C => 0x0153,   // œ
        0x9D => 0x009D,   // undefined (C1)
        0x9E => 0x017E,   // ž
        0x9F => 0x0178,   // Ÿ
        _ => byte as u32, // 0x00..0x7F and 0xA0..0xFF map 1:1 to Unicode
    }
}

#[inline(always)]
fn apply_range_mapping(
    map: &mut HashMap<char, usize>,
    codeset: &str,
    hex_range: Option<(u32, u32)>,
    first_frame: usize,
) {
    match codeset.to_ascii_lowercase().as_str() {
        "unicode" => {
            if let Some((start, end)) = hex_range {
                let count = end - start + 1;
                for i in 0..count {
                    if let Some(ch) = char::from_u32(start + i) {
                        map.insert(ch, first_frame + i as usize);
                    }
                }
            } else {
                warn!("range Unicode without #start-end ignored");
            }
        }
        "ascii" => {
            let (start, end) = hex_range.unwrap_or((0, 0x7F));
            let mut ff = first_frame;
            for cp in start..=end {
                if let Some(ch) = char::from_u32(cp) {
                    map.insert(ch, ff);
                }
                ff += 1;
            }
        }
        "cp1252" => {
            let (start, end) = hex_range.unwrap_or((0, 0xFF));
            let mut ff = first_frame;
            for cp in start..=end {
                if cp <= 0xFF {
                    let u = cp1252_to_unicode(cp as u8);
                    if let Some(ch) = char::from_u32(u) {
                        map.insert(ch, ff);
                    }
                    ff += 1;
                }
            }
        }
        "basic-japanese" => {
            // This charmap corresponds to the Unicode block U+3000..=U+30FF.
            // StepMania hardcodes this mapping. It does not use the optional hex_range.
            let start_cp = 0x3000;
            let end_cp = 0x30FF;
            for i in 0..=(end_cp - start_cp) {
                if let Some(ch) = char::from_u32(start_cp + i) {
                    // Skip the ZERO WIDTH NO-BREAK SPACE character, which is M_SKIP in StepMania's charmap system.
                    if ch != '\u{FEFF}' {
                        map.insert(ch, first_frame + i as usize);
                    }
                }
            }
        }
        "numbers" => {
            // Include both 'x' and 'X'; many SM fonts expect upper-case as well.
            // Also include the multiplication sign × for completeness.
            let numbers_map: &[char] = &[
                '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', ':', '-', '+', '/', 'x',
                'X', '×', '%', ' ',
            ];
            let (start, end) = hex_range.unwrap_or((0, (numbers_map.len() as u32) - 1));
            let mut ff = first_frame;
            for idx in start..=end {
                let i = idx as usize;
                if i < numbers_map.len() {
                    map.insert(numbers_map[i], ff);
                    ff += 1;
                }
            }
        }
        other => warn!("Unsupported codeset '{}' in RANGE; skipping.", other),
    }
}

/* ======================= PARSE ======================= */

pub fn parse(ini_path_str: &str) -> Result<FontLoadData, Box<dyn std::error::Error>> {
    use std::collections::{HashMap, HashSet};

    fn resolve_import_path(base_ini: &Path, spec: &str) -> Option<PathBuf> {
        // Accept either "Folder/Name" or ".../Name.ini"
        let mut rel = PathBuf::from(spec);
        if rel.extension().is_none() {
            rel.set_extension("ini");
        }

        // Try Fonts root (parent of the font dir), then sibling of current ini
        let font_dir = base_ini.parent()?;
        let fonts_root = font_dir.parent();

        let candidates = [fonts_root.map(|r| r.join(&rel)), Some(font_dir.join(&rel))];
        for c in candidates.iter().flatten() {
            if c.is_file() {
                return Some(c.clone());
            }
        }
        None
    }

    fn gather_import_specs(
        ini_map_lower: &HashMap<String, HashMap<String, String>>,
    ) -> Vec<String> {
        let mut specs: Vec<String> = Vec::new();
        // SM implicitly seeds "Common default". We'll add it first; failure is non-fatal.
        specs.push("Common default".to_string());
        for map in ini_map_lower.values() {
            if let Some(v) = map.get("import") {
                // allow comma/semicolon separated or single value
                for s in v
                    .split(&[',', ';'][..])
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    specs.push(s.to_string());
                }
            }
            if let Some(v) = map.get("_imports") {
                for s in v
                    .split(&[',', ';'][..])
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    specs.push(s.to_string());
                }
            }
        }
        specs
    }

    // ---- original parse begins
    let ini_path = Path::new(ini_path_str);
    let font_dir = ini_path.parent().ok_or("Could not find font directory")?;
    let mut ini_text = fs::read_to_string(ini_path_str)?;
    ini_text = strip_bom(ini_text);

    let ini_map_lower = parse_ini_trimmed_map(&ini_text);
    let raw_line_map = harvest_raw_line_entries_from_text(&ini_text);

    let prefix = ini_path.file_stem().unwrap().to_str().unwrap();
    let texture_paths = list_texture_pages(font_dir, prefix)?;
    if texture_paths.is_empty() {
        return Err(format!("No texture pages found for font '{}'", ini_path_str).into());
    }

    // ---- NEW: import merge (before local pages)
    let mut required_textures: Vec<PathBuf> = Vec::new();
    let mut all_glyphs: HashMap<char, Glyph> = HashMap::new();
    let mut imported_once: HashSet<String> = HashSet::new();

    for spec in gather_import_specs(&ini_map_lower) {
        if !imported_once.insert(spec.clone()) {
            continue;
        }
        if let Some(import_ini) = resolve_import_path(ini_path, &spec) {
            match parse(import_ini.to_string_lossy().as_ref()) {
                Ok(imported) => {
                    // Merge textures
                    required_textures.extend(imported.required_textures.into_iter());
                    // Merge glyphs: imported -> base; local pages will override later
                    for (ch, g) in imported.font.glyph_map.into_iter() {
                        all_glyphs.entry(ch).or_insert(g);
                    }
                    debug!("Imported font '{}' merged.", spec);
                }
                Err(e) => {
                    warn!("Failed to import font '{}': {}", spec, e);
                }
            }
        } else {
            warn!("Import '{}' not found relative to '{}'", spec, ini_path_str);
        }
    }

    // Keep track of default metrics from our main/first page (not from imports)
    let mut default_page_metrics = (0, 0);

    // ---- local pages loop (unchanged logic; our pages override imported glyphs)
    for (page_idx, tex_path) in texture_paths.iter().enumerate() {
        let page_name = get_page_name_from_path(tex_path);
        let tex_dims = image::image_dimensions(tex_path)?;
        let texture_key = assets::canonical_texture_key(tex_path);
        required_textures.push(tex_path.to_path_buf());

        let (num_frames_wide, num_frames_high) = assets::parse_sprite_sheet_dims(&texture_key);
        let has_doubleres = is_doubleres_in_name(&texture_key);
        let total_frames = (num_frames_wide * num_frames_high) as usize;

        let (base_tex_w, base_tex_h) =
            parse_base_res_from_filename(&texture_key).unwrap_or((tex_dims.0, tex_dims.1));

        // authored metrics parity w/ StepMania
        let mut authored_tex_w = base_tex_w;
        let mut authored_tex_h = base_tex_h;
        if has_doubleres {
            authored_tex_w = (authored_tex_w / 2).max(1);
            authored_tex_h = (authored_tex_h / 2).max(1);
        }
        let frame_w_i = (authored_tex_w / num_frames_wide) as i32;
        let frame_h_i = (authored_tex_h / num_frames_high) as i32;

        info!(
            " Page '{}', Texture: '{}' -> Authored Grid: {}x{} (frame {}x{} px)",
            page_name, texture_key, num_frames_wide, num_frames_high, frame_w_i, frame_h_i
        );

        // settings: common → page → legacy
        let mut settings = FontPageSettings::default();
        let mut sections_to_check = vec!["common".to_string(), page_name.clone()];
        if page_name == "main" {
            sections_to_check.push("char widths".to_string());
        }
        for section in &sections_to_check {
            if let Some(map) = ini_map_lower.get(section) {
                let get_int = |k: &str| -> Option<i32> { map.get(k).and_then(|s| s.parse().ok()) };
                let get_f32 = |k: &str| -> Option<f32> { map.get(k).and_then(|s| s.parse().ok()) };

                if let Some(n) = get_int("drawextrapixelsleft") {
                    settings.draw_extra_pixels_left = n;
                }
                if let Some(n) = get_int("drawextrapixelsright") {
                    settings.draw_extra_pixels_right = n;
                }
                if let Some(n) = get_int("addtoallwidths") {
                    settings.add_to_all_widths = n;
                }
                if let Some(n) = get_f32("scaleallwidthsby") {
                    settings.scale_all_widths_by = n;
                }
                if let Some(n) = get_int("linespacing") {
                    settings.line_spacing = n;
                }
                if let Some(n) = get_int("top") {
                    settings.top = n;
                }
                if let Some(n) = get_int("baseline") {
                    settings.baseline = n;
                }
                if let Some(n) = get_int("defaultwidth") {
                    settings.default_width = n;
                }
                if let Some(n) = get_int("advanceextrapixels") {
                    settings.advance_extra_pixels = n;
                }

                for (key, val) in map {
                    if let Ok(frame_idx) = key.parse::<usize>()
                        && let Ok(w) = val.parse::<i32>()
                    {
                        settings.glyph_widths.insert(frame_idx, w);
                    }
                }
            }
        }

        trace!(
            " [{}] settings(authored): draw_extra L={} R={}, add_to_all_widths={}, scale_all_widths_by={:.3}, \
             line_spacing={}, top={}, baseline={}, default_width={}, advance_extra_pixels={}",
            page_name,
            settings.draw_extra_pixels_left,
            settings.draw_extra_pixels_right,
            settings.add_to_all_widths,
            settings.scale_all_widths_by,
            settings.line_spacing,
            settings.top,
            settings.baseline,
            settings.default_width,
            settings.advance_extra_pixels
        );
        trace!(
            " [{}] frames: {}x{} (frame_w={} frame_h={}), total_frames={}",
            page_name, num_frames_wide, num_frames_high, frame_w_i, frame_h_i, total_frames
        );

        // vertical metrics (authored)
        let line_spacing_authored = if settings.line_spacing != -1 {
            settings.line_spacing
        } else {
            frame_h_i
        };
        let baseline_authored = if settings.baseline != -1 {
            settings.baseline
        } else {
            (frame_h_i as f32 * 0.5 + line_spacing_authored as f32 * 0.5) as i32
        };
        let top_authored = if settings.top != -1 {
            settings.top
        } else {
            (frame_h_i as f32 * 0.5 - line_spacing_authored as f32 * 0.5) as i32
        };
        let height_authored = baseline_authored - top_authored;
        let vshift_authored = -(baseline_authored as f32);

        if page_idx == 0 || page_name == "main" {
            default_page_metrics = (height_authored, line_spacing_authored);
        }

        trace!(
            " VMetrics(authored): line_spacing={}, baseline={}, top={}, height={}, vshift={:.1}",
            line_spacing_authored,
            baseline_authored,
            top_authored,
            height_authored,
            vshift_authored
        );

        // mapping char → frame (SM spill across row up to total_frames)
        let mut char_to_frame: HashMap<char, usize> = HashMap::new();
        for section_name in &sections_to_check {
            let sec_lc = section_name.to_string();
            if let Some(map) = ini_map_lower.get(&sec_lc) {
                for (raw_key_lc, val_str) in map {
                    let key_lc = raw_key_lc.as_str();
                    if key_lc.starts_with("line ") {
                        if let Ok(row) = key_lc[5..].trim().parse::<u32>() {
                            if row >= num_frames_high {
                                continue;
                            }
                            let first_frame = (row * num_frames_wide) as usize;

                            let line_val = raw_line_map
                                .get(&(sec_lc.clone(), row))
                                .map_or(val_str.as_str(), |s| s.as_str());

                            for (i, ch) in line_val.chars().enumerate() {
                                let idx = first_frame + i;
                                if idx < total_frames {
                                    char_to_frame.insert(ch, idx);
                                } else {
                                    break;
                                }
                            }
                        }
                    } else if key_lc.starts_with("map ") {
                        if let Ok(frame_index) = val_str.parse::<usize>() {
                            let spec = raw_key_lc[4..].trim();
                            if let Some(hex) =
                                spec.strip_prefix("U+").or_else(|| spec.strip_prefix("u+"))
                            {
                                if let Ok(cp) = u32::from_str_radix(hex, 16)
                                    && let Some(ch) = char::from_u32(cp)
                                    && frame_index < total_frames
                                {
                                    char_to_frame.insert(ch, frame_index);
                                }
                            } else if spec.starts_with('"')
                                && spec.ends_with('"')
                                && spec.len() >= 2
                            {
                                for ch in spec[1..spec.len() - 1].chars() {
                                    if frame_index < total_frames {
                                        char_to_frame.insert(ch, frame_index);
                                    }
                                }
                            }
                        }
                    } else if key_lc.starts_with("range ")
                        && let Ok(first_frame) = val_str.parse::<usize>()
                        && let Some((codeset, hex)) = parse_range_key(raw_key_lc)
                    {
                        apply_range_mapping(&mut char_to_frame, &codeset, hex, first_frame);
                    }
                }
            }
        }

        apply_space_nbsp_symmetry(&mut char_to_frame);

        if page_name != "common" && char_to_frame.is_empty() {
            match total_frames {
                128 => apply_range_mapping(&mut char_to_frame, "ascii", None, 0),
                256 => apply_range_mapping(&mut char_to_frame, "cp1252", None, 0),
                15 | 16 => apply_range_mapping(&mut char_to_frame, "numbers", None, 0),
                _ => {}
            }
        }

        debug!(
            "Page '{}' mapped {} chars (frames={}).",
            page_name,
            char_to_frame.len(),
            total_frames
        );

        // SM extra pixels (+1/+1, left forced even)
        let mut draw_left = settings.draw_extra_pixels_left + 1;
        let draw_right = settings.draw_extra_pixels_right + 1;
        if draw_left % 2 != 0 {
            draw_left += 1;
        }

        for i in 0..total_frames {
            let base_w_ini = if let Some(&w) = settings.glyph_widths.get(&i) {
                w
            } else if settings.default_width != -1 {
                settings.default_width
            } else {
                frame_w_i
            };
            let base_w_scaled = round_half_to_even_i32(
                (base_w_ini + settings.add_to_all_widths) as f32 * settings.scale_all_widths_by,
            );
            let hadvance = base_w_scaled + settings.advance_extra_pixels;

            let mut width_i = base_w_scaled;
            let mut chop_i = frame_w_i - width_i;
            if chop_i < 0 {
                chop_i = 0;
            }
            if (chop_i & 1) != 0 {
                chop_i -= 1;
                width_i += 1; // odd-chop quirk
            }

            let width_f = width_i as f32;
            let chop_f = chop_i as f32;
            let pad_f = (chop_f * 0.5).max(0.0);

            let mut extra_left = (draw_left as f32).min(pad_f);
            let mut extra_right = (draw_right as f32).min(pad_f);
            if width_i <= 0 {
                extra_left = 0.0;
                extra_right = 0.0;
            }

            let glyph_size = [width_f + extra_left + extra_right, frame_h_i as f32];
            let glyph_offset = [-extra_left, vshift_authored];
            let advance = hadvance as f32;

            // texture rect in actual pixels (retain SM float precision)
            let actual_frame_w = (tex_dims.0 as f32) / (num_frames_wide as f32);
            let actual_frame_h = (tex_dims.1 as f32) / (num_frames_high as f32);
            let col = (i as u32 % num_frames_wide) as f32;
            let row = (i as u32 / num_frames_wide) as f32;

            let authored_to_actual_ratio = if frame_w_i > 0 {
                actual_frame_w / frame_w_i as f32
            } else {
                1.0
            };
            let tex_chop_off = chop_f * authored_to_actual_ratio;
            let tex_extra_left = extra_left * authored_to_actual_ratio;
            let tex_extra_right = extra_right * authored_to_actual_ratio;

            let frame_left_px = col * actual_frame_w;
            let frame_top_px = row * actual_frame_h;
            let tex_rect_left = frame_left_px + 0.5 * tex_chop_off - tex_extra_left;
            let tex_rect_right =
                frame_left_px + actual_frame_w - 0.5 * tex_chop_off + tex_extra_right;
            let tex_rect = [
                tex_rect_left,
                frame_top_px,
                tex_rect_right,
                frame_top_px + actual_frame_h,
            ];

            let glyph = Glyph {
                texture_key: texture_key.clone(),
                tex_rect,
                size: glyph_size,
                offset: glyph_offset,
                advance,
            };

            for (&ch, &frame_idx) in &char_to_frame {
                if frame_idx == i {
                    trace!(
                        " [{}] GLYPH {} -> frame {} | width_i={} hadv={} chop={} extraL={} extraR={} \
                         size=[{:.3}x{:.3}] offset=[{:.3},{:.3}] advance={:.3} \
                         tex_rect=[{:.1},{:.1},{:.1},{:.1}]",
                        page_name,
                        fmt_char(ch),
                        i,
                        width_i,
                        hadvance,
                        chop_i,
                        extra_left,
                        extra_right,
                        glyph.size[0],
                        glyph.size[1],
                        glyph.offset[0],
                        glyph.offset[1],
                        glyph.advance,
                        tex_rect[0],
                        tex_rect[1],
                        tex_rect[2],
                        tex_rect[3],
                    );
                    // local page overrides any previously-imported glyph
                    all_glyphs.insert(ch, glyph.clone());
                }
            }

            // default glyph from our first page only (not from imports)
            if page_idx == 0 && i == 0 {
                all_glyphs
                    .entry(FONT_DEFAULT_CHAR)
                    .or_insert_with(|| glyph.clone());
            }
        }
    }

    synthesize_space_from_nbsp(&mut all_glyphs);

    let default_glyph = all_glyphs.get(&FONT_DEFAULT_CHAR).cloned();
    let font = Font {
        glyph_map: all_glyphs,
        default_glyph,
        height: default_page_metrics.0,
        line_spacing: default_page_metrics.1,
        fallback_font_name: None,
    };

    if !font.glyph_map.contains_key(&' ') {
        let adv = font
            .default_glyph
            .as_ref()
            .map(|g| g.advance)
            .unwrap_or(0.0);
        warn!(
            "Font '{}' is missing SPACE (U+0020). Falling back to default glyph (advance {:.1}).",
            ini_path_str, adv
        );
    } else if let Some(g) = font.glyph_map.get(&' ') {
        trace!(
            "SPACE metrics (draw): advance={:.3} size=[{:.3}x{:.3}] offset=[{:.3},{:.3}]",
            g.advance, g.size[0], g.size[1], g.offset[0], g.offset[1]
        );
        debug!(
            "SPACE mapped: draw advance {:.3} (texture='{}')",
            g.advance, g.texture_key
        );
    }

    info!(
        "--- FINISHED Parsing font '{}' with {} glyphs and {} textures. ---\n",
        ini_path_str,
        font.glyph_map.len(),
        required_textures.len()
    );

    Ok(FontLoadData {
        font,
        required_textures,
    })
}

/* ======================= API ======================= */

/// Traverses the font fallback chain to find a glyph for a given character.
pub fn find_glyph<'a>(
    start_font: &'a Font,
    c: char,
    all_fonts: &'a HashMap<&'static str, Font>,
) -> Option<&'a Glyph> {
    let mut current_font = Some(start_font);
    while let Some(font) = current_font {
        // Check the current font's glyph map.
        if let Some(glyph) = font.glyph_map.get(&c) {
            return Some(glyph);
        }
        // If not found, move to the next font in the chain.
        current_font = font.fallback_font_name.and_then(|name| all_fonts.get(name));
    }
    // If the character was not found in any font in the chain,
    // return the default glyph of the *original* starting font.
    start_font.default_glyph.as_ref()
}

/// StepMania parity: calculates the logical width of a line by summing the integer advances.
#[inline(always)]
pub fn measure_line_width_logical(
    font: &Font,
    text: &str,
    all_fonts: &HashMap<&'static str, Font>,
) -> i32 {
    text.chars()
        .map(|c| {
            let g = find_glyph(font, c, all_fonts);
            g.map_or(0, |glyph| glyph.advance as i32)
        })
        .sum()
}

/* ======================= LAYOUT HELPERS USED BY UI ======================= */

#[inline(always)]
fn apply_space_nbsp_symmetry(char_to_frame: &mut std::collections::HashMap<char, usize>) {
    // If SPACE exists but NBSP doesn't, map NBSP -> SPACE frame.
    if let Some(&space_idx) = char_to_frame.get(&' ') {
        char_to_frame.entry('\u{00A0}').or_insert(space_idx);
    }
    // If NBSP exists but SPACE doesn't, map SPACE -> NBSP frame. (Wendy relies on this)
    if let Some(&nbsp_idx) = char_to_frame.get(&'\u{00A0}') {
        char_to_frame.entry(' ').or_insert(nbsp_idx);
    }
}

#[inline(always)]
fn synthesize_space_from_nbsp(all_glyphs: &mut std::collections::HashMap<char, Glyph>) {
    if !all_glyphs.contains_key(&' ')
        && let Some(nbsp) = all_glyphs.get(&'\u{00A0}').cloned()
    {
        all_glyphs.insert(' ', nbsp);
        debug!("SPACE synthesized from NBSP glyph at font level (SM parity).");
    }
}
