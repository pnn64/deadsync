//! StepMania bitmap font parser (Rust port — dependency-light, functional/procedural)
//! - SM-parity defaults for metrics and width handling (fixes tight/overlapping glyphs)
//! - Supports LINE, MAP U+XXXX / "..." / aliases (Unicode, ASCII, CP1252, numbers)
//! - SM extra-pixels quirk (+1/+1, left forced even) to avoid stroke clipping
//! - Canonical texture keys (assets-relative, forward slashes) so lookups match
//! - Parses "(res WxH)" from sheet filenames and scales INI-authored metrics like StepMania
//! - Applies inverse draw scale so on-screen size matches StepMania's authored size
//! - No regex/glob/configparser/once_cell; pure std + image + log
//! - VERBOSE TRACE logging for troubleshooting: enable with RUST_LOG=new_engine::core::font=trace

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use image;
use log::{debug, info, trace, warn};

use crate::assets;

const FONT_DEFAULT_CHAR: char = '\u{F8FF}'; // SM default glyph (private use)
const INTERNAL_ALIAS_START: u32 = 0xE000;
const M_SKIP_CODEPOINT: u32 = 0xFEFF;

#[derive(Clone, Copy)]
enum AliasValue {
    Codepoint(u32),
    Internal,
}

static FONT_CHAR_ALIAS_TABLE: &[(&str, AliasValue)] = &[
    ("ha", AliasValue::Codepoint(0x3042)),
    ("hi", AliasValue::Codepoint(0x3044)),
    ("hu", AliasValue::Codepoint(0x3046)),
    ("he", AliasValue::Codepoint(0x3048)),
    ("ho", AliasValue::Codepoint(0x304a)),
    ("hka", AliasValue::Codepoint(0x304b)),
    ("hki", AliasValue::Codepoint(0x304d)),
    ("hku", AliasValue::Codepoint(0x304f)),
    ("hke", AliasValue::Codepoint(0x3051)),
    ("hko", AliasValue::Codepoint(0x3053)),
    ("hga", AliasValue::Codepoint(0x304c)),
    ("hgi", AliasValue::Codepoint(0x304e)),
    ("hgu", AliasValue::Codepoint(0x3050)),
    ("hge", AliasValue::Codepoint(0x3052)),
    ("hgo", AliasValue::Codepoint(0x3054)),
    ("hza", AliasValue::Codepoint(0x3056)),
    ("hzi", AliasValue::Codepoint(0x3058)),
    ("hzu", AliasValue::Codepoint(0x305a)),
    ("hze", AliasValue::Codepoint(0x305c)),
    ("hzo", AliasValue::Codepoint(0x305e)),
    ("hta", AliasValue::Codepoint(0x305f)),
    ("hti", AliasValue::Codepoint(0x3061)),
    ("htu", AliasValue::Codepoint(0x3064)),
    ("hte", AliasValue::Codepoint(0x3066)),
    ("hto", AliasValue::Codepoint(0x3068)),
    ("hda", AliasValue::Codepoint(0x3060)),
    ("hdi", AliasValue::Codepoint(0x3062)),
    ("hdu", AliasValue::Codepoint(0x3065)),
    ("hde", AliasValue::Codepoint(0x3067)),
    ("hdo", AliasValue::Codepoint(0x3069)),
    ("hna", AliasValue::Codepoint(0x306a)),
    ("hni", AliasValue::Codepoint(0x306b)),
    ("hnu", AliasValue::Codepoint(0x306c)),
    ("hne", AliasValue::Codepoint(0x306d)),
    ("hno", AliasValue::Codepoint(0x306e)),
    ("hha", AliasValue::Codepoint(0x306f)),
    ("hhi", AliasValue::Codepoint(0x3072)),
    ("hhu", AliasValue::Codepoint(0x3075)),
    ("hhe", AliasValue::Codepoint(0x3078)),
    ("hho", AliasValue::Codepoint(0x307b)),
    ("hba", AliasValue::Codepoint(0x3070)),
    ("hbi", AliasValue::Codepoint(0x3073)),
    ("hbu", AliasValue::Codepoint(0x3076)),
    ("hbe", AliasValue::Codepoint(0x3079)),
    ("hbo", AliasValue::Codepoint(0x307c)),
    ("hpa", AliasValue::Codepoint(0x3071)),
    ("hpi", AliasValue::Codepoint(0x3074)),
    ("hpu", AliasValue::Codepoint(0x3077)),
    ("hpe", AliasValue::Codepoint(0x307a)),
    ("hpo", AliasValue::Codepoint(0x307d)),
    ("hma", AliasValue::Codepoint(0x307e)),
    ("hmi", AliasValue::Codepoint(0x307f)),
    ("hmu", AliasValue::Codepoint(0x3080)),
    ("hme", AliasValue::Codepoint(0x3081)),
    ("hmo", AliasValue::Codepoint(0x3082)),
    ("hya", AliasValue::Codepoint(0x3084)),
    ("hyu", AliasValue::Codepoint(0x3086)),
    ("hyo", AliasValue::Codepoint(0x3088)),
    ("hra", AliasValue::Codepoint(0x3089)),
    ("hri", AliasValue::Codepoint(0x308a)),
    ("hru", AliasValue::Codepoint(0x308b)),
    ("hre", AliasValue::Codepoint(0x308c)),
    ("hro", AliasValue::Codepoint(0x308d)),
    ("hwa", AliasValue::Codepoint(0x308f)),
    ("hwi", AliasValue::Codepoint(0x3090)),
    ("hwe", AliasValue::Codepoint(0x3091)),
    ("hwo", AliasValue::Codepoint(0x3092)),
    ("hn", AliasValue::Codepoint(0x3093)),
    ("hvu", AliasValue::Codepoint(0x3094)),
    ("has", AliasValue::Codepoint(0x3041)),
    ("his", AliasValue::Codepoint(0x3043)),
    ("hus", AliasValue::Codepoint(0x3045)),
    ("hes", AliasValue::Codepoint(0x3047)),
    ("hos", AliasValue::Codepoint(0x3049)),
    ("hkas", AliasValue::Codepoint(0x3095)),
    ("hkes", AliasValue::Codepoint(0x3096)),
    ("hsa", AliasValue::Codepoint(0x3055)),
    ("hsi", AliasValue::Codepoint(0x3057)),
    ("hsu", AliasValue::Codepoint(0x3059)),
    ("hse", AliasValue::Codepoint(0x305b)),
    ("hso", AliasValue::Codepoint(0x305d)),
    ("hyas", AliasValue::Codepoint(0x3083)),
    ("hyus", AliasValue::Codepoint(0x3085)),
    ("hyos", AliasValue::Codepoint(0x3087)),
    ("hwas", AliasValue::Codepoint(0x308e)),
    ("hq", AliasValue::Codepoint(0x3063)),
    ("ka", AliasValue::Codepoint(0x30a2)),
    ("ki", AliasValue::Codepoint(0x30a4)),
    ("ku", AliasValue::Codepoint(0x30a6)),
    ("ke", AliasValue::Codepoint(0x30a8)),
    ("ko", AliasValue::Codepoint(0x30aa)),
    ("kka", AliasValue::Codepoint(0x30ab)),
    ("kki", AliasValue::Codepoint(0x30ad)),
    ("kku", AliasValue::Codepoint(0x30af)),
    ("kke", AliasValue::Codepoint(0x30b1)),
    ("kko", AliasValue::Codepoint(0x30b3)),
    ("kga", AliasValue::Codepoint(0x30ac)),
    ("kgi", AliasValue::Codepoint(0x30ae)),
    ("kgu", AliasValue::Codepoint(0x30b0)),
    ("kge", AliasValue::Codepoint(0x30b2)),
    ("kgo", AliasValue::Codepoint(0x30b4)),
    ("kza", AliasValue::Codepoint(0x30b6)),
    ("kzi", AliasValue::Codepoint(0x30b8)),
    ("kji", AliasValue::Codepoint(0x30b8)),
    ("kzu", AliasValue::Codepoint(0x30ba)),
    ("kze", AliasValue::Codepoint(0x30bc)),
    ("kzo", AliasValue::Codepoint(0x30be)),
    ("kta", AliasValue::Codepoint(0x30bf)),
    ("kti", AliasValue::Codepoint(0x30c1)),
    ("ktu", AliasValue::Codepoint(0x30c4)),
    ("kte", AliasValue::Codepoint(0x30c6)),
    ("kto", AliasValue::Codepoint(0x30c8)),
    ("kda", AliasValue::Codepoint(0x30c0)),
    ("kdi", AliasValue::Codepoint(0x30c2)),
    ("kdu", AliasValue::Codepoint(0x30c5)),
    ("kde", AliasValue::Codepoint(0x30c7)),
    ("kdo", AliasValue::Codepoint(0x30c9)),
    ("kna", AliasValue::Codepoint(0x30ca)),
    ("kni", AliasValue::Codepoint(0x30cb)),
    ("knu", AliasValue::Codepoint(0x30cc)),
    ("kne", AliasValue::Codepoint(0x30cd)),
    ("kno", AliasValue::Codepoint(0x30ce)),
    ("kha", AliasValue::Codepoint(0x30cf)),
    ("khi", AliasValue::Codepoint(0x30d2)),
    ("khu", AliasValue::Codepoint(0x30d5)),
    ("khe", AliasValue::Codepoint(0x30d8)),
    ("kho", AliasValue::Codepoint(0x30db)),
    ("kba", AliasValue::Codepoint(0x30d0)),
    ("kbi", AliasValue::Codepoint(0x30d3)),
    ("kbu", AliasValue::Codepoint(0x30d6)),
    ("kbe", AliasValue::Codepoint(0x30d9)),
    ("kbo", AliasValue::Codepoint(0x30dc)),
    ("kpa", AliasValue::Codepoint(0x30d1)),
    ("kpi", AliasValue::Codepoint(0x30d4)),
    ("kpu", AliasValue::Codepoint(0x30d7)),
    ("kpe", AliasValue::Codepoint(0x30da)),
    ("kpo", AliasValue::Codepoint(0x30dd)),
    ("kma", AliasValue::Codepoint(0x30de)),
    ("kmi", AliasValue::Codepoint(0x30df)),
    ("kmu", AliasValue::Codepoint(0x30e0)),
    ("kme", AliasValue::Codepoint(0x30e1)),
    ("kmo", AliasValue::Codepoint(0x30e2)),
    ("kya", AliasValue::Codepoint(0x30e4)),
    ("kyu", AliasValue::Codepoint(0x30e6)),
    ("kyo", AliasValue::Codepoint(0x30e8)),
    ("kra", AliasValue::Codepoint(0x30e9)),
    ("kri", AliasValue::Codepoint(0x30ea)),
    ("kru", AliasValue::Codepoint(0x30eb)),
    ("kre", AliasValue::Codepoint(0x30ec)),
    ("kro", AliasValue::Codepoint(0x30ed)),
    ("kwa", AliasValue::Codepoint(0x30ef)),
    ("kwi", AliasValue::Codepoint(0x30f0)),
    ("kwe", AliasValue::Codepoint(0x30f1)),
    ("kwo", AliasValue::Codepoint(0x30f2)),
    ("kn", AliasValue::Codepoint(0x30f3)),
    ("kvu", AliasValue::Codepoint(0x30f4)),
    ("kas", AliasValue::Codepoint(0x30a1)),
    ("kis", AliasValue::Codepoint(0x30a3)),
    ("kus", AliasValue::Codepoint(0x30a5)),
    ("kes", AliasValue::Codepoint(0x30a7)),
    ("kos", AliasValue::Codepoint(0x30a9)),
    ("kkas", AliasValue::Codepoint(0x30f5)),
    ("kkes", AliasValue::Codepoint(0x30f6)),
    ("ksa", AliasValue::Codepoint(0x30b5)),
    ("ksi", AliasValue::Codepoint(0x30b7)),
    ("ksu", AliasValue::Codepoint(0x30b9)),
    ("kse", AliasValue::Codepoint(0x30bb)),
    ("kso", AliasValue::Codepoint(0x30bd)),
    ("kyas", AliasValue::Codepoint(0x30e3)),
    ("kyus", AliasValue::Codepoint(0x30e5)),
    ("kyos", AliasValue::Codepoint(0x30e7)),
    ("kwas", AliasValue::Codepoint(0x30ee)),
    ("kq", AliasValue::Codepoint(0x30c3)),
    ("kdot", AliasValue::Codepoint(0x30FB)),
    ("kdash", AliasValue::Codepoint(0x30FC)),
    ("nbsp", AliasValue::Codepoint(0x00a0)),
    ("delta", AliasValue::Codepoint(0x0394)),
    ("sigma", AliasValue::Codepoint(0x03a3)),
    ("omega", AliasValue::Codepoint(0x03a9)),
    ("angle", AliasValue::Codepoint(0x2220)),
    ("whiteheart", AliasValue::Codepoint(0x2661)),
    ("blackstar", AliasValue::Codepoint(0x2605)),
    ("whitestar", AliasValue::Codepoint(0x2606)),
    ("flipped-a", AliasValue::Codepoint(0x2200)),
    ("squared", AliasValue::Codepoint(0x00b2)),
    ("cubed", AliasValue::Codepoint(0x00b3)),
    ("oq", AliasValue::Codepoint(0x201c)),
    ("cq", AliasValue::Codepoint(0x201d)),
    ("leftarrow", AliasValue::Codepoint(0x2190)),
    ("uparrow", AliasValue::Codepoint(0x2191)),
    ("rightarrow", AliasValue::Codepoint(0x2192)),
    ("downarrow", AliasValue::Codepoint(0x2193)),
    ("4thnote", AliasValue::Codepoint(0x2669)),
    ("8thnote", AliasValue::Codepoint(0x266A)),
    ("b8thnote", AliasValue::Codepoint(0x266B)),
    ("b16thnote", AliasValue::Codepoint(0x266C)),
    ("flat", AliasValue::Codepoint(0x266D)),
    ("natural", AliasValue::Codepoint(0x266E)),
    ("sharp", AliasValue::Codepoint(0x266F)),
    ("up", AliasValue::Internal),
    ("down", AliasValue::Internal),
    ("left", AliasValue::Internal),
    ("right", AliasValue::Internal),
    ("downleft", AliasValue::Internal),
    ("downright", AliasValue::Internal),
    ("upleft", AliasValue::Internal),
    ("upright", AliasValue::Internal),
    ("center", AliasValue::Internal),
    ("menuup", AliasValue::Internal),
    ("menudown", AliasValue::Internal),
    ("menuleft", AliasValue::Internal),
    ("menuright", AliasValue::Internal),
    ("start", AliasValue::Internal),
    ("doublezeta", AliasValue::Internal),
    ("planet", AliasValue::Internal),
    ("back", AliasValue::Internal),
    ("ok", AliasValue::Internal),
    ("nextrow", AliasValue::Internal),
    ("select", AliasValue::Internal),
    ("auxx", AliasValue::Internal),
    ("auxtriangle", AliasValue::Internal),
    ("auxsquare", AliasValue::Internal),
    ("auxcircle", AliasValue::Internal),
    ("auxl1", AliasValue::Internal),
    ("auxl2", AliasValue::Internal),
    ("auxl3", AliasValue::Internal),
    ("auxr1", AliasValue::Internal),
    ("auxr2", AliasValue::Internal),
    ("auxr3", AliasValue::Internal),
    ("auxselect", AliasValue::Internal),
    ("auxstart", AliasValue::Internal),
    ("auxa", AliasValue::Internal),
    ("auxb", AliasValue::Internal),
    ("auxc", AliasValue::Internal),
    ("auxd", AliasValue::Internal),
    ("auxy", AliasValue::Internal),
    ("auxz", AliasValue::Internal),
    ("auxl", AliasValue::Internal),
    ("auxr", AliasValue::Internal),
    ("auxwhite", AliasValue::Internal),
    ("auxblack", AliasValue::Internal),
    ("auxlb", AliasValue::Internal),
    ("auxrb", AliasValue::Internal),
    ("auxlt", AliasValue::Internal),
    ("auxrt", AliasValue::Internal),
    ("auxback", AliasValue::Internal),
];

static FONT_CHAR_ALIAS_MAP: OnceLock<HashMap<String, char>> = OnceLock::new();

#[inline(always)]
fn font_char_alias_map() -> &'static HashMap<String, char> {
    FONT_CHAR_ALIAS_MAP.get_or_init(|| {
        let mut map = HashMap::with_capacity(FONT_CHAR_ALIAS_TABLE.len() + 2);
        map.insert("default".to_string(), FONT_DEFAULT_CHAR);
        map.insert("invalid".to_string(), char::REPLACEMENT_CHARACTER);

        let mut next_internal = INTERNAL_ALIAS_START;
        for &(name, value) in FONT_CHAR_ALIAS_TABLE {
            let cp = match value {
                AliasValue::Codepoint(cp) => cp,
                AliasValue::Internal => {
                    let cp = next_internal;
                    next_internal = next_internal.saturating_add(1);
                    cp
                }
            };
            if let Some(ch) = char::from_u32(cp) {
                map.insert(name.to_ascii_lowercase(), ch);
            }
        }
        map
    })
}

#[inline(always)]
fn lookup_font_char_alias(spec: &str) -> Option<char> {
    let key = spec.trim();
    if key.is_empty() {
        return None;
    }
    font_char_alias_map()
        .get(&key.to_ascii_lowercase())
        .copied()
}

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

const MAP_ASCII: &[u32] = &[
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, 0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025,
    0x0026, 0x0027, 0x0028, 0x0029, 0x002A, 0x002B, 0x002C, 0x002D, 0x002E, 0x002F,
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037, 0x0038, 0x0039,
    0x003A, 0x003B, 0x003C, 0x003D, 0x003E, 0x003F, 0x0040, 0x0041, 0x0042, 0x0043,
    0x0044, 0x0045, 0x0046, 0x0047, 0x0048, 0x0049, 0x004A, 0x004B, 0x004C, 0x004D,
    0x004E, 0x004F, 0x0050, 0x0051, 0x0052, 0x0053, 0x0054, 0x0055, 0x0056, 0x0057,
    0x0058, 0x0059, 0x005A, 0x005B, 0x005C, 0x005D, 0x005E, 0x005F, 0x0060, 0x0061,
    0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067, 0x0068, 0x0069, 0x006A, 0x006B,
    0x006C, 0x006D, 0x006E, 0x006F, 0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075,
    0x0076, 0x0077, 0x0078, 0x0079, 0x007A, 0x007B, 0x007C, 0x007D, 0x007E,
    M_SKIP_CODEPOINT,
];

const MAP_ISO_8859_1: &[u32] = &[
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, 0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025,
    0x0026, 0x0027, 0x0028, 0x0029, 0x002a, 0x002b, 0x002c, 0x002d, 0x002e, 0x002f,
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037, 0x0038, 0x0039,
    0x003a, 0x003b, 0x003c, 0x003d, 0x003e, 0x003f, 0x0040, 0x0041, 0x0042, 0x0043,
    0x0044, 0x0045, 0x0046, 0x0047, 0x0048, 0x0049, 0x004a, 0x004b, 0x004c, 0x004d,
    0x004e, 0x004f, 0x0050, 0x0051, 0x0052, 0x0053, 0x0054, 0x0055, 0x0056, 0x0057,
    0x0058, 0x0059, 0x005a, 0x005b, 0x005c, 0x005d, 0x005e, 0x005f, 0x0060, 0x0061,
    0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067, 0x0068, 0x0069, 0x006a, 0x006b,
    0x006c, 0x006d, 0x006e, 0x006f, 0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075,
    0x0076, 0x0077, 0x0078, 0x0079, 0x007a, 0x007b, 0x007c, 0x007d, 0x007e, 0x007f,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, 0x00a0, 0x00a1, 0x00a2, 0x00a3, 0x00a4, 0x00a5,
    0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x00aa, 0x00ab, 0x00ac, 0x00ad, 0x00ae, 0x00af,
    0x00b0, 0x00b1, 0x00b2, 0x00b3, 0x00b4, 0x00b5, 0x00b6, 0x00b7, 0x00b8, 0x00b9,
    0x00ba, 0x00bb, 0x00bc, 0x00bd, 0x00be, 0x00bf, 0x00c0, 0x00c1, 0x00c2, 0x00c3,
    0x00c4, 0x00c5, 0x00c6, 0x00c7, 0x00c8, 0x00c9, 0x00ca, 0x00cb, 0x00cc, 0x00cd,
    0x00ce, 0x00cf, 0x00d0, 0x00d1, 0x00d2, 0x00d3, 0x00d4, 0x00d5, 0x00d6, 0x00d7,
    0x00d8, 0x00d9, 0x00da, 0x00db, 0x00dc, 0x00dd, 0x00de, 0x00df, 0x00e0, 0x00e1,
    0x00e2, 0x00e3, 0x00e4, 0x00e5, 0x00e6, 0x00e7, 0x00e8, 0x00e9, 0x00ea, 0x00eb,
    0x00ec, 0x00ed, 0x00ee, 0x00ef, 0x00f0, 0x00f1, 0x00f2, 0x00f3, 0x00f4, 0x00f5,
    0x00f6, 0x00f7, 0x00f8, 0x00f9, 0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x00fe, 0x00ff,
];

const MAP_CP1252: &[u32] = &[
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, 0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025,
    0x0026, 0x0027, 0x0028, 0x0029, 0x002A, 0x002B, 0x002C, 0x002D, 0x002E, 0x002F,
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037, 0x0038, 0x0039,
    0x003A, 0x003B, 0x003C, 0x003D, 0x003E, 0x003F, 0x0040, 0x0041, 0x0042, 0x0043,
    0x0044, 0x0045, 0x0046, 0x0047, 0x0048, 0x0049, 0x004A, 0x004B, 0x004C, 0x004D,
    0x004E, 0x004F, 0x0050, 0x0051, 0x0052, 0x0053, 0x0054, 0x0055, 0x0056, 0x0057,
    0x0058, 0x0059, 0x005A, 0x005B, 0x005C, 0x005D, 0x005E, 0x005F, 0x0060, 0x0061,
    0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067, 0x0068, 0x0069, 0x006A, 0x006B,
    0x006C, 0x006D, 0x006E, 0x006F, 0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075,
    0x0076, 0x0077, 0x0078, 0x0079, 0x007A, 0x007B, 0x007C, 0x007D, 0x007E,
    M_SKIP_CODEPOINT, 0x20AC, M_SKIP_CODEPOINT, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020,
    0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, M_SKIP_CODEPOINT, 0x017D,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
    0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x017E, M_SKIP_CODEPOINT, 0x0178,
    0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9,
    0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3,
    0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD,
    0x00BE, 0x00BF, 0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7,
    0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD, 0x00CE, 0x00CF, 0x00D0, 0x00D1,
    0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7, 0x00D8, 0x00D9, 0x00DA, 0x00DB,
    0x00DC, 0x00DD, 0x00DE, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5,
    0x00E6, 0x00E7, 0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF,
    0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, 0x00F8, 0x00F9,
    0x00FA, 0x00FB, 0x00FC, 0x00FD, 0x00FE, 0x00FF,
];

const MAP_ISO_8859_2: &[u32] = &[
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, M_SKIP_CODEPOINT,
    M_SKIP_CODEPOINT, M_SKIP_CODEPOINT, 0x00A0, 0x0104, 0x02D8, 0x0141, 0x00A4, 0x013D,
    0x015A, 0x00A7, 0x00A8, 0x0160, 0x015E, 0x0164, 0x0179, 0x00AD, 0x017D, 0x017B,
    0x00B0, 0x0105, 0x02DB, 0x0142, 0x00B4, 0x013E, 0x015B, 0x02C7, 0x00B8, 0x0161,
    0x015F, 0x0165, 0x017A, 0x02DD, 0x017E, 0x017C, 0x0154, 0x00C1, 0x00C2, 0x0102,
    0x00C4, 0x0139, 0x0106, 0x00C7, 0x010C, 0x00C9, 0x0118, 0x00CB, 0x011A, 0x00CD,
    0x00CE, 0x010E, 0x0110, 0x0143, 0x0147, 0x00D3, 0x00D4, 0x0150, 0x00D6, 0x00D7,
    0x0158, 0x016E, 0x00DA, 0x0170, 0x00DC, 0x00DD, 0x0162, 0x00DF, 0x0155, 0x00E1,
    0x00E2, 0x0103, 0x00E4, 0x013A, 0x0107, 0x00E7, 0x010D, 0x00E9, 0x0119, 0x00EB,
    0x011B, 0x00ED, 0x00EE, 0x010F, 0x0111, 0x0144, 0x0148, 0x00F3, 0x00F4, 0x0151,
    0x00F6, 0x00F7, 0x0159, 0x016F, 0x00FA, 0x0171, 0x00FC, 0x00FD, 0x0163, 0x02D9,
];

const MAP_BASIC_JAPANESE: &[u32] = &[
    0x3000, 0x3001, 0x3002, 0x3003, 0x3004, 0x3005, 0x3006, 0x3007, 0x3008, 0x3009,
    0x300a, 0x300b, 0x300c, 0x300d, 0x300e, 0x300f, 0x3010, 0x3011, 0x3012, 0x3013,
    0x3014, 0x3015, 0x3016, 0x3017, 0x3018, 0x3019, 0x301a, 0x301b, 0x301c, 0x301d,
    0x301e, 0x301f, 0x3020, 0x3021, 0x3022, 0x3023, 0x3024, 0x3025, 0x3026, 0x3027,
    0x3028, 0x3029, 0x302a, 0x302b, 0x302c, 0x302d, 0x302e, 0x302f, 0x3030, 0x3031,
    0x3032, 0x3033, 0x3034, 0x3035, 0x3036, 0x3037, 0x3038, 0x3039, 0x303a, 0x303b,
    0x303c, 0x303d, 0x303e, 0x303f, 0x3040, 0x3041, 0x3042, 0x3043, 0x3044, 0x3045,
    0x3046, 0x3047, 0x3048, 0x3049, 0x304a, 0x304b, 0x304c, 0x304d, 0x304e, 0x304f,
    0x3050, 0x3051, 0x3052, 0x3053, 0x3054, 0x3055, 0x3056, 0x3057, 0x3058, 0x3059,
    0x305a, 0x305b, 0x305c, 0x305d, 0x305e, 0x305f, 0x3060, 0x3061, 0x3062, 0x3063,
    0x3064, 0x3065, 0x3066, 0x3067, 0x3068, 0x3069, 0x306a, 0x306b, 0x306c, 0x306d,
    0x306e, 0x306f, 0x3070, 0x3071, 0x3072, 0x3073, 0x3074, 0x3075, 0x3076, 0x3077,
    0x3078, 0x3079, 0x307a, 0x307b, 0x307c, 0x307d, 0x307e, 0x307f, 0x3080, 0x3081,
    0x3082, 0x3083, 0x3084, 0x3085, 0x3086, 0x3087, 0x3088, 0x3089, 0x308a, 0x308b,
    0x308c, 0x308d, 0x308e, 0x308f, 0x3090, 0x3091, 0x3092, 0x3093, 0x3094, 0x3095,
    0x3096, 0x3097, 0x3098, 0x3099, 0x309a, 0x309b, 0x309c, 0x309d, 0x309e, 0x309f,
    0x30a0, 0x30a1, 0x30a2, 0x30a3, 0x30a4, 0x30a5, 0x30a6, 0x30a7, 0x30a8, 0x30a9,
    0x30aa, 0x30ab, 0x30ac, 0x30ad, 0x30ae, 0x30af, 0x30b0, 0x30b1, 0x30b2, 0x30b3,
    0x30b4, 0x30b5, 0x30b6, 0x30b7, 0x30b8, 0x30b9, 0x30ba, 0x30bb, 0x30bc, 0x30bd,
    0x30be, 0x30bf, 0x30c0, 0x30c1, 0x30c2, 0x30c3, 0x30c4, 0x30c5, 0x30c6, 0x30c7,
    0x30c8, 0x30c9, 0x30ca, 0x30cb, 0x30cc, 0x30cd, 0x30ce, 0x30cf, 0x30d0, 0x30d1,
    0x30d2, 0x30d3, 0x30d4, 0x30d5, 0x30d6, 0x30d7, 0x30d8, 0x30d9, 0x30da, 0x30db,
    0x30dc, 0x30dd, 0x30de, 0x30df, 0x30e0, 0x30e1, 0x30e2, 0x30e3, 0x30e4, 0x30e5,
    0x30e6, 0x30e7, 0x30e8, 0x30e9, 0x30ea, 0x30eb, 0x30ec, 0x30ed, 0x30ee, 0x30ef,
    0x30f0, 0x30f1, 0x30f2, 0x30f3, 0x30f4, 0x30f5, 0x30f6, 0x30f7, 0x30f8, 0x30f9,
    0x30fa, 0x30fb, 0x30fc, 0x30fd, 0x30fe, 0x30ff,
];

const MAP_KOREAN_JAMO: &[u32] = &[
    0x1100, 0x1110, 0x1120, 0x1130, 0x1140, 0x1150, 0x1160, 0x1170, 0x1180, 0x1190,
    0x11a0, 0x11b0, 0x11c0, 0x11d0, 0x11e0, 0x11f0, 0x1101, 0x1111, 0x1121, 0x1131,
    0x1141, 0x1151, 0x1161, 0x1171, 0x1181, 0x1191, 0x11a1, 0x11b1, 0x11c1, 0x11d1,
    0x11e1, 0x11f1, 0x1102, 0x1112, 0x1122, 0x1132, 0x1142, 0x1152, 0x1162, 0x1172,
    0x1182, 0x1192, 0x11a2, 0x11b2, 0x11c2, 0x11d2, 0x11e2, 0x11f2, 0x1103, 0x1113,
    0x1123, 0x1133, 0x1143, 0x1153, 0x1163, 0x1173, 0x1183, 0x1193, 0x11a3, 0x11b3,
    0x11c3, 0x11d3, 0x11e3, 0x11f3, 0x1104, 0x1114, 0x1124, 0x1134, 0x1144, 0x1154,
    0x1164, 0x1174, 0x1184, 0x1194, 0x11a4, 0x11b4, 0x11c4, 0x11d4, 0x11e4, 0x11f4,
    0x1105, 0x1115, 0x1125, 0x1135, 0x1145, 0x1155, 0x1165, 0x1175, 0x1185, 0x1195,
    0x11a5, 0x11b5, 0x11c5, 0x11d5, 0x11e5, 0x11f5, 0x1106, 0x1116, 0x1126, 0x1136,
    0x1146, 0x1156, 0x1166, 0x1176, 0x1186, 0x1196, 0x11a6, 0x11b6, 0x11c6, 0x11d6,
    0x11e6, 0x11f6, 0x1107, 0x1117, 0x1127, 0x1137, 0x1147, 0x1157, 0x1167, 0x1177,
    0x1187, 0x1197, 0x11a7, 0x11b7, 0x11c7, 0x11d7, 0x11e7, 0x11f7, 0x1108, 0x1118,
    0x1128, 0x1138, 0x1148, 0x1158, 0x1168, 0x1178, 0x1188, 0x1198, 0x11a8, 0x11b8,
    0x11c8, 0x11d8, 0x11e8, 0x11f8, 0x1109, 0x1119, 0x1129, 0x1139, 0x1149, 0x1159,
    0x1169, 0x1179, 0x1189, 0x1199, 0x11a9, 0x11b9, 0x11c9, 0x11d9, 0x11e9, 0x11f9,
    0x110a, 0x111a, 0x112a, 0x113a, 0x114a, 0x115a, 0x116a, 0x117a, 0x118a, 0x119a,
    0x11aa, 0x11ba, 0x11ca, 0x11da, 0x11ea, 0x11fa, 0x110b, 0x111b, 0x112b, 0x113b,
    0x114b, 0x115b, 0x116b, 0x117b, 0x118b, 0x119b, 0x11ab, 0x11bb, 0x11cb, 0x11db,
    0x11eb, 0x11fb, 0x110c, 0x111c, 0x112c, 0x113c, 0x114c, 0x115c, 0x116c, 0x117c,
    0x118c, 0x119c, 0x11ac, 0x11bc, 0x11cc, 0x11dc, 0x11ec, 0x11fc, 0x110d, 0x111d,
    0x112d, 0x113d, 0x114d, 0x115d, 0x116d, 0x117d, 0x118d, 0x119d, 0x11ad, 0x11bd,
    0x11cd, 0x11dd, 0x11ed, 0x11fd, 0x110e, 0x111e, 0x112e, 0x113e, 0x114e, 0x115e,
    0x116e, 0x117e, 0x118e, 0x119e, 0x11ae, 0x11be, 0x11ce, 0x11de, 0x11ee, 0x11fe,
    0x110f, 0x111f, 0x112f, 0x113f, 0x114f, 0x115f, 0x116f, 0x117f, 0x118f, 0x119f,
    0x11af, 0x11bf, 0x11cf, 0x11df, 0x11ef, 0x11ff,
];

const MAP_NUMBERS: &[u32] = &[
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037, 0x0038, 0x0039,
    0x0025, 0x002E, 0x0020, 0x003A, 0x0078,
];

#[inline(always)]
fn apply_charmap_range(
    map: &mut HashMap<char, usize>,
    charmap: &[u32],
    map_offset: u32,
    first_frame: usize,
    count: Option<u32>,
) {
    if charmap.is_empty() {
        return;
    }
    let len = charmap.len() as u32;
    if map_offset >= len {
        warn!(
            "range map offset {} exceeds charmap length {}; skipping.",
            map_offset, len
        );
        return;
    }

    let requested = count.unwrap_or(len - map_offset);
    let mut remaining = requested;
    let mut idx = map_offset;
    let mut frame = first_frame;

    while idx < len && remaining > 0 {
        let cp = charmap[idx as usize];
        if cp != M_SKIP_CODEPOINT {
            if let Some(ch) = char::from_u32(cp) {
                map.insert(ch, frame);
            }
        }
        idx += 1;
        frame += 1;
        remaining -= 1;
    }

    if remaining > 0 {
        warn!(
            "range map overflow (offset={}, count={}, len={})",
            map_offset, requested, len
        );
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
                let count = end.saturating_sub(start).saturating_add(1);
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
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_ASCII, map_offset, first_frame, count);
        }
        "cp1252" => {
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_CP1252, map_offset, first_frame, count);
        }
        "iso-8859-1" => {
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_ISO_8859_1, map_offset, first_frame, count);
        }
        "iso-8859-2" => {
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_ISO_8859_2, map_offset, first_frame, count);
        }
        "korean-jamo" => {
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_KOREAN_JAMO, map_offset, first_frame, count);
        }
        "basic-japanese" => {
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_BASIC_JAPANESE, map_offset, first_frame, count);
        }
        "numbers" => {
            let (map_offset, count) = hex_range
                .map(|(start, end)| (start, Some(end.saturating_sub(start).saturating_add(1))))
                .unwrap_or((0, None));
            apply_charmap_range(map, MAP_NUMBERS, map_offset, first_frame, count);
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
                            } else if spec.chars().count() == 1 {
                                if let Some(ch) = spec.chars().next()
                                    && frame_index < total_frames
                                {
                                    char_to_frame.insert(ch, frame_index);
                                }
                            } else if let Some(ch) = lookup_font_char_alias(spec)
                                && frame_index < total_frames
                            {
                                char_to_frame.insert(ch, frame_index);
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
